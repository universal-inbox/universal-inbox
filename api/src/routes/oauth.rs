use std::sync::Arc;

use crate::middlewares::jwt_auth::Authenticated;
use actix_web::{HttpResponse, Scope, web};
use anyhow::Context;
use secrecy::SecretBox;
use serde::Deserialize;
use tokio::sync::RwLock;
use tracing::error;

use universal_inbox::{integration_connection::IntegrationConnectionId, user::UserId};

use crate::{
    configuration::Settings,
    integrations::oauth2::AuthorizationCode,
    universal_inbox::{
        UniversalInboxError, integration_connection::service::IntegrationConnectionService,
    },
    utils::{cache::Cache, jwt::Claims},
};

/// Public, sanitized reason codes surfaced in the `oauth_error` query param of
/// the user-visible redirect after a failed OAuth callback.
///
/// The full internal error chain is logged server-side via `tracing` — only the
/// kebab-case code reaches the URL so we never leak internal context such as
/// Redis lookup failures, integration connection IDs, or upstream provider
/// error blobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OAuthCallbackErrorCode {
    /// Missing/malformed state or code, or the state was not found in Redis.
    InvalidState,
    /// The state was found but its TTL had expired, or the integration
    /// connection is no longer in the `Created` state (late/duplicate callback).
    ExpiredState,
    /// The remote OAuth provider returned an error in the callback query
    /// string (e.g. `error=access_denied`) or the token exchange failed.
    ProviderError,
    /// Catch-all for anything else — Redis/transaction/serde failures, etc.
    InternalError,
}

impl OAuthCallbackErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            OAuthCallbackErrorCode::InvalidState => "invalid-state",
            OAuthCallbackErrorCode::ExpiredState => "expired-state",
            OAuthCallbackErrorCode::ProviderError => "provider-error",
            OAuthCallbackErrorCode::InternalError => "internal-error",
        }
    }
}

impl std::fmt::Display for OAuthCallbackErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Classify a `UniversalInboxError` produced by `complete_oauth_callback` (or
/// its dependencies) into a public reason code, redacting the original chain
/// from the redirect URL.
///
/// `Unauthorized` from the service layer is what `complete_oauth_callback`
/// returns when the Redis lookup yields no state (`"Invalid or expired OAuth
/// state"`) — there is no way to distinguish "never existed" from "TTL
/// expired" once Redis has dropped the key, so it folds into `InvalidState`.
/// `UnsupportedAction` is emitted for late/duplicate callbacks against a
/// connection that is no longer in `Created` status — surface that as
/// `ExpiredState`, which is the most accurate user-facing description (the
/// authorize attempt has been superseded).
pub fn classify_oauth_callback_error(err: &UniversalInboxError) -> OAuthCallbackErrorCode {
    match err {
        UniversalInboxError::Unauthorized(_) | UniversalInboxError::InvalidInputData { .. } => {
            OAuthCallbackErrorCode::InvalidState
        }
        UniversalInboxError::UnsupportedAction(_) => OAuthCallbackErrorCode::ExpiredState,
        UniversalInboxError::OAuth2InvalidGrant(_) => OAuthCallbackErrorCode::ProviderError,
        _ => OAuthCallbackErrorCode::InternalError,
    }
}

pub fn authorize_scope() -> Scope {
    web::scope("/oauth").service(
        web::resource("/authorize/{integration_connection_id}")
            .route(web::get().to(oauth_authorize)),
    )
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

    let authorization_url = service
        .start_oauth_authorization(&mut transaction, integration_connection_id, user_id, &cache)
        .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit OAuth authorization transaction")?;

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

    if let Some(ref error) = query.error {
        // The upstream OAuth provider returned an error in the query string
        // (e.g. `error=access_denied`). Log the raw value for diagnostics and
        // surface a generic provider-error code to the user.
        error!("OAuth callback received provider error from upstream: {error}");
        return build_error_redirect(front_base_url, OAuthCallbackErrorCode::ProviderError);
    }

    let Some(code) = query.code.as_ref() else {
        error!("OAuth callback missing `code` query parameter");
        return build_error_redirect(front_base_url, OAuthCallbackErrorCode::InvalidState);
    };

    let Some(state) = query.state.as_ref() else {
        error!("OAuth callback missing `state` query parameter");
        return build_error_redirect(front_base_url, OAuthCallbackErrorCode::InvalidState);
    };

    let service = integration_connection_service.read().await;
    let transaction = service.begin().await;
    let mut transaction = match transaction {
        Ok(tx) => tx,
        Err(err) => {
            error!("Failed to create transaction for OAuth callback: {err:?}");
            return build_error_redirect(front_base_url, OAuthCallbackErrorCode::InternalError);
        }
    };

    match service
        .complete_oauth_callback(
            &mut transaction,
            &SecretBox::new(Box::new(AuthorizationCode(code.to_string()))),
            state,
            &cache,
        )
        .await
    {
        Ok(()) => {
            if let Err(err) = transaction
                .commit()
                .await
                .context("Failed to commit OAuth callback transaction")
            {
                error!("OAuth callback commit error: {err:?}");
                return build_error_redirect(front_base_url, OAuthCallbackErrorCode::InternalError);
            }
            build_success_redirect(front_base_url)
        }
        Err(err) => {
            // Log the full error chain server-side. `UniversalInboxError`'s
            // `Debug` impl walks the source chain (see
            // `universal_inbox::error_chain_fmt`), so `{err:?}` captures every
            // layer of context. Only the classified code escapes to the URL.
            error!("OAuth callback error: {err:?}");
            let code = classify_oauth_callback_error(&err);
            build_error_redirect(front_base_url, code)
        }
    }
}

fn build_success_redirect(front_base_url: &str) -> HttpResponse {
    let redirect_url = format!("{front_base_url}/settings?oauth_success=true");
    HttpResponse::Found()
        .insert_header(("Location", redirect_url.as_str()))
        .finish()
}

fn build_error_redirect(front_base_url: &str, error: OAuthCallbackErrorCode) -> HttpResponse {
    // All current codes are kebab-case ASCII and therefore URL-safe, but pass
    // through `urlencoding::encode` defensively so any future code that
    // contains reserved characters is still encoded correctly.
    let encoded_error = urlencoding::encode(error.as_str());
    let redirect_url = format!("{front_base_url}/settings?oauth_error={encoded_error}");
    HttpResponse::Found()
        .insert_header(("Location", redirect_url.as_str()))
        .finish()
}

#[cfg(test)]
mod tests {
    use anyhow::anyhow;

    use super::*;

    #[test]
    fn code_strings_are_stable_kebab_case() {
        // These strings are the public contract with the SPA — changing them
        // breaks frontend messaging.
        assert_eq!(
            OAuthCallbackErrorCode::InvalidState.as_str(),
            "invalid-state"
        );
        assert_eq!(
            OAuthCallbackErrorCode::ExpiredState.as_str(),
            "expired-state"
        );
        assert_eq!(
            OAuthCallbackErrorCode::ProviderError.as_str(),
            "provider-error"
        );
        assert_eq!(
            OAuthCallbackErrorCode::InternalError.as_str(),
            "internal-error"
        );
        // Display matches as_str.
        assert_eq!(
            format!("{}", OAuthCallbackErrorCode::InvalidState),
            "invalid-state"
        );
    }

    #[test]
    fn classifier_maps_unauthorized_to_invalid_state() {
        // What `complete_oauth_callback` raises when Redis has no row for the
        // state token — either it never existed, or its TTL expired and was
        // purged. Either way, no leak should reach the URL.
        let err = UniversalInboxError::Unauthorized(anyhow!("Invalid or expired OAuth state"));
        assert_eq!(
            classify_oauth_callback_error(&err),
            OAuthCallbackErrorCode::InvalidState
        );
    }

    #[test]
    fn classifier_maps_invalid_input_data_to_invalid_state() {
        let err = UniversalInboxError::InvalidInputData {
            source: None,
            user_error: "bad state".to_string(),
        };
        assert_eq!(
            classify_oauth_callback_error(&err),
            OAuthCallbackErrorCode::InvalidState
        );
    }

    #[test]
    fn classifier_maps_unsupported_action_to_expired_state() {
        // Emitted when the integration connection is no longer in `Created`
        // status (late/duplicate callback). The original message names the
        // integration_connection_id — must not leak.
        let err = UniversalInboxError::UnsupportedAction(
            "Integration connection 11111111-1111-1111-1111-111111111111 is no longer in Created status (current: Validated), ignoring stale OAuth callback".to_string(),
        );
        assert_eq!(
            classify_oauth_callback_error(&err),
            OAuthCallbackErrorCode::ExpiredState
        );
    }

    #[test]
    fn classifier_maps_oauth2_invalid_grant_to_provider_error() {
        let err = UniversalInboxError::OAuth2InvalidGrant("token refused".to_string());
        assert_eq!(
            classify_oauth_callback_error(&err),
            OAuthCallbackErrorCode::ProviderError
        );
    }

    #[test]
    fn classifier_maps_recoverable_to_internal_error() {
        // `Recoverable` messages reference internal integration_connection IDs
        // (see integration_connection/service.rs:792-795). Must collapse to
        // the generic internal-error code.
        let err = UniversalInboxError::Recoverable(anyhow!(
            "Access token expired for integration connection 11111111-1111-1111-1111-111111111111. Token refresh should happen via the refresh-oauth-tokens command."
        ));
        assert_eq!(
            classify_oauth_callback_error(&err),
            OAuthCallbackErrorCode::InternalError
        );
    }

    #[test]
    fn classifier_maps_unexpected_to_internal_error() {
        // Catches Redis lookup failures, serde failures, repository errors —
        // anything that gets wrapped in `Unexpected` via `From<anyhow::Error>`.
        let err =
            UniversalInboxError::Unexpected(anyhow!("Failed to retrieve OAuth state from Redis"));
        assert_eq!(
            classify_oauth_callback_error(&err),
            OAuthCallbackErrorCode::InternalError
        );
    }

    #[test]
    fn build_error_redirect_emits_only_the_code() {
        // Regression test: even when the wrapped chain mentions internal
        // details, the redirect URL must contain only the kebab-case code and
        // none of the original text.
        let response =
            build_error_redirect("https://app.test", OAuthCallbackErrorCode::InternalError);
        let location = response
            .headers()
            .get("Location")
            .expect("redirect must have a Location header")
            .to_str()
            .expect("Location header must be ASCII");
        assert_eq!(
            location,
            "https://app.test/settings?oauth_error=internal-error"
        );
        // Defensive: none of the strings we used to leak should appear here.
        assert!(!location.contains("Failed to retrieve"));
        assert!(!location.contains("Redis"));
        assert!(!location.contains("integration connection"));
    }
}
