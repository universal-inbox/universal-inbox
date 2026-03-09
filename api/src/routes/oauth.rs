use std::sync::Arc;

use actix_jwt_authc::Authenticated;
use actix_web::{HttpResponse, Scope, web};
use anyhow::Context;
use redis::AsyncCommands;
use ring::rand::SecureRandom;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;
use tracing::error;

use universal_inbox::{
    integration_connection::{
        IntegrationConnectionId, IntegrationConnectionStatus, provider::IntegrationProviderKind,
    },
    user::UserId,
};

use crate::{
    configuration::Settings,
    repository::oauth_credential::OAuthCredentialRepository,
    universal_inbox::{
        UniversalInboxError, integration_connection::service::IntegrationConnectionService,
    },
    utils::{cache::Cache, crypto::encrypt_token, jwt::Claims},
};

const OAUTH_STATE_PREFIX: &str = "universal-inbox::oauth-state::";
const OAUTH_STATE_TTL_SECONDS: u64 = 600;

pub fn authorize_scope() -> Scope {
    web::scope("/oauth").service(
        web::resource("/authorize/{integration_connection_id}")
            .route(web::get().to(oauth_authorize)),
    )
}

#[derive(Debug, Serialize, Deserialize)]
struct OAuthStateData {
    integration_connection_id: IntegrationConnectionId,
    pkce_verifier: Option<String>,
    provider_kind: IntegrationProviderKind,
}

fn generate_pkce_verifier_and_challenge() -> Result<(String, String), UniversalInboxError> {
    let rng = ring::rand::SystemRandom::new();
    let mut verifier_bytes = [0u8; 32];
    rng.fill(&mut verifier_bytes).map_err(|_| {
        UniversalInboxError::Unexpected(anyhow::anyhow!(
            "Failed to generate PKCE verifier random bytes"
        ))
    })?;

    let verifier = base64_url_encode(&verifier_bytes);

    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let challenge_bytes = hasher.finalize();
    let challenge = base64_url_encode(&challenge_bytes);

    Ok((verifier, challenge))
}

fn generate_state() -> Result<String, UniversalInboxError> {
    let rng = ring::rand::SystemRandom::new();
    let mut state_bytes = [0u8; 32];
    rng.fill(&mut state_bytes).map_err(|_| {
        UniversalInboxError::Unexpected(anyhow::anyhow!(
            "Failed to generate OAuth state random bytes"
        ))
    })?;
    Ok(hex::encode(state_bytes))
}

fn base64_url_encode(bytes: &[u8]) -> String {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    URL_SAFE_NO_PAD.encode(bytes)
}

#[derive(Deserialize)]
pub struct OAuthCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

pub async fn oauth_authorize(
    path: web::Path<IntegrationConnectionId>,
    integration_connection_service: web::Data<Arc<RwLock<IntegrationConnectionService>>>,
    cache: web::Data<Cache>,
    _settings: web::Data<Settings>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;
    let integration_connection_id = path.into_inner();

    let service = integration_connection_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while starting OAuth authorization")?;

    // Validate the integration connection exists, belongs to user, has status Created
    let integration_connection = service
        .get_integration_connection(&mut transaction, integration_connection_id)
        .await?
        .ok_or_else(|| {
            UniversalInboxError::ItemNotFound(format!(
                "Integration connection {integration_connection_id} not found"
            ))
        })?;

    if integration_connection.user_id != user_id {
        return Err(UniversalInboxError::Forbidden(format!(
            "Integration connection {integration_connection_id} does not belong to user {user_id}"
        )));
    }

    if integration_connection.status != IntegrationConnectionStatus::Created {
        return Err(UniversalInboxError::UnsupportedAction(format!(
            "Integration connection {integration_connection_id} is not in Created status"
        )));
    }

    let provider_kind = integration_connection.provider.kind();

    // Look up the OAuth2Provider for this provider kind
    let provider = service.get_oauth2_provider(&provider_kind).ok_or_else(|| {
        UniversalInboxError::UnsupportedAction(format!(
            "No OAuth2 provider configured for {provider_kind:?}"
        ))
    })?;

    let oauth2_flow_service = service.get_oauth2_flow_service().ok_or_else(|| {
        UniversalInboxError::Unexpected(anyhow::anyhow!("OAuth2 flow service not configured"))
    })?;

    let redirect_uri = service.get_oauth_redirect_uri().ok_or_else(|| {
        UniversalInboxError::Unexpected(anyhow::anyhow!("OAuth redirect URI not configured"))
    })?;

    // Generate PKCE if supported
    let (pkce_verifier, pkce_challenge, pkce_method) = if provider.supports_pkce() {
        let (verifier, challenge) = generate_pkce_verifier_and_challenge()?;
        (Some(verifier), Some(challenge), Some("S256".to_string()))
    } else {
        (None, None, None)
    };

    // Generate state
    let state = generate_state()?;

    // Store state in Redis
    let state_data = OAuthStateData {
        integration_connection_id,
        pkce_verifier: pkce_verifier.clone(),
        provider_kind,
    };
    let state_json =
        serde_json::to_string(&state_data).context("Failed to serialize OAuth state data")?;

    let redis_key = format!("{OAUTH_STATE_PREFIX}{state}");
    let mut conn = cache.connection_manager.clone();
    conn.set_ex::<_, _, ()>(&redis_key, &state_json, OAUTH_STATE_TTL_SECONDS)
        .await
        .context("Failed to store OAuth state in Redis")?;

    // Generate authorization URL
    let authorization_url = oauth2_flow_service.generate_authorization_url(
        provider,
        redirect_uri,
        &state,
        pkce_challenge.as_deref(),
        pkce_method.as_deref(),
    );

    Ok(HttpResponse::Found()
        .insert_header(("Location", authorization_url.as_str()))
        .finish())
}

pub async fn oauth_callback(
    query: web::Query<OAuthCallbackQuery>,
    integration_connection_service: web::Data<Arc<RwLock<IntegrationConnectionService>>>,
    cache: web::Data<Cache>,
    settings: web::Data<Settings>,
) -> HttpResponse {
    let front_base_url = settings
        .application
        .front_base_url
        .as_str()
        .trim_end_matches('/');

    // Handle error from provider
    if let Some(ref error) = query.error {
        return build_error_redirect(front_base_url, error);
    }

    let Some(ref code) = query.code else {
        return build_error_redirect(front_base_url, "missing_code");
    };

    let Some(ref state) = query.state else {
        return build_error_redirect(front_base_url, "missing_state");
    };

    match handle_callback(
        code,
        state,
        &integration_connection_service,
        &cache,
        &settings,
    )
    .await
    {
        Ok(()) => build_success_redirect(front_base_url),
        Err(err) => {
            error!("OAuth callback error: {err:?}");
            let error_message = format!("{err}");
            build_error_redirect(front_base_url, &error_message)
        }
    }
}

async fn handle_callback(
    code: &str,
    state: &str,
    integration_connection_service: &web::Data<Arc<RwLock<IntegrationConnectionService>>>,
    cache: &web::Data<Cache>,
    _settings: &web::Data<Settings>,
) -> Result<(), UniversalInboxError> {
    // Look up and delete state from Redis (single-use)
    let redis_key = format!("{OAUTH_STATE_PREFIX}{state}");
    let mut conn = cache.connection_manager.clone();
    let state_json: Option<String> = conn
        .get_del(&redis_key)
        .await
        .context("Failed to retrieve OAuth state from Redis")?;

    let state_json = state_json.ok_or_else(|| {
        UniversalInboxError::Unauthorized(anyhow::anyhow!("Invalid or expired OAuth state"))
    })?;

    let state_data: OAuthStateData =
        serde_json::from_str(&state_json).context("Failed to deserialize OAuth state data")?;

    let service = integration_connection_service.read().await;

    let provider = service
        .get_oauth2_provider(&state_data.provider_kind)
        .ok_or_else(|| {
            UniversalInboxError::Unexpected(anyhow::anyhow!(
                "No OAuth2 provider configured for {:?}",
                state_data.provider_kind
            ))
        })?;

    let oauth2_flow_service = service.get_oauth2_flow_service().ok_or_else(|| {
        UniversalInboxError::Unexpected(anyhow::anyhow!("OAuth2 flow service not configured"))
    })?;

    let redirect_uri = service.get_oauth_redirect_uri().ok_or_else(|| {
        UniversalInboxError::Unexpected(anyhow::anyhow!("OAuth redirect URI not configured"))
    })?;

    let token_encryption_key = service.get_token_encryption_key().ok_or_else(|| {
        UniversalInboxError::Unexpected(anyhow::anyhow!("Token encryption key not configured"))
    })?;

    // Exchange code for tokens
    let token_response = oauth2_flow_service
        .exchange_code_for_token(
            provider,
            code,
            redirect_uri,
            state_data.pkce_verifier.as_deref(),
        )
        .await?;

    // Encrypt tokens (bind ciphertext to this specific connection via AAD)
    let aad_context = state_data.integration_connection_id.0.as_bytes();
    let encrypted_access_token = encrypt_token(
        &token_response.access_token,
        aad_context,
        token_encryption_key,
    )?;
    let encrypted_refresh_token = token_response
        .refresh_token
        .as_ref()
        .map(|rt| encrypt_token(rt, aad_context, token_encryption_key))
        .transpose()?;

    let expires_at = token_response.expires_at();

    // Extract registered scopes from the raw response, then strip sensitive fields
    let mut raw_response = serde_json::to_value(&token_response)
        .context("Failed to serialize token response to Value")?;
    let registered_scopes = provider.extract_registered_scopes(&raw_response)?;
    if let Some(obj) = raw_response.as_object_mut() {
        obj.remove("access_token");
        obj.remove("refresh_token");
    }

    // Store credential and update integration connection status
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create transaction for storing OAuth credential")?;

    // Get the integration connection and verify it is still in Created status.
    // This prevents a late callback from a duplicate authorize flow from overwriting
    // credentials stored by an earlier successful callback.
    let integration_connection = service
        .get_integration_connection(&mut transaction, state_data.integration_connection_id)
        .await?
        .ok_or_else(|| {
            UniversalInboxError::Unexpected(anyhow::anyhow!(
                "Integration connection {} not found",
                state_data.integration_connection_id
            ))
        })?;

    if integration_connection.status != IntegrationConnectionStatus::Created {
        return Err(UniversalInboxError::UnsupportedAction(format!(
            "Integration connection {} is no longer in Created status (current: {:?}), ignoring stale OAuth callback",
            state_data.integration_connection_id, integration_connection.status
        )));
    }

    // Store the OAuth credential
    service
        .repository()
        .store_oauth_credential(
            &mut transaction,
            state_data.integration_connection_id,
            encrypted_access_token,
            encrypted_refresh_token,
            expires_at,
            raw_response,
        )
        .await?;

    // Update the integration connection status to Validated
    service
        .update_integration_connection_status(
            &mut transaction,
            state_data.integration_connection_id,
            integration_connection.user_id,
            IntegrationConnectionStatus::Validated,
            registered_scopes,
        )
        .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit OAuth credential storage")?;

    Ok(())
}

fn build_success_redirect(front_base_url: &str) -> HttpResponse {
    let redirect_url = format!("{front_base_url}/settings?oauth_success=true");
    HttpResponse::Found()
        .insert_header(("Location", redirect_url.as_str()))
        .finish()
}

fn build_error_redirect(front_base_url: &str, error: &str) -> HttpResponse {
    let encoded_error = urlencoding::encode(error);
    let redirect_url = format!("{front_base_url}/settings?oauth_error={encoded_error}");
    HttpResponse::Found()
        .insert_header(("Location", redirect_url.as_str()))
        .finish()
}
