use std::{num::NonZeroU32, sync::Arc};

use actix_jwt_authc::Authenticated;
use actix_session::Session;
use actix_web::{HttpResponse, Scope, web};
use anyhow::{Context, anyhow};
use chrono::{DateTime, TimeDelta, Utc};
use governor::Quota;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use universal_inbox::{
    auth::oauth2::{
        OAuth2ConsentDecision, OAuth2ConsentRequest, OAuth2ConsentResponse, OAuth2ConsentSubmission,
    },
    user::UserId,
};

use crate::{
    configuration::Settings,
    universal_inbox::{
        UniversalInboxError,
        oauth2::service::{
            OAuth2Service, is_loopback_redirect_uri, is_origin_in_allowlist,
            redirect_uri_matches_registered, validate_redirect_uri,
        },
    },
    utils::{jwt::Claims, rate_limit::IpRateLimiter},
};

const OAUTH2_RATE_LIMIT_PER_MINUTE: u32 = 30;
const OAUTH2_PENDING_CONSENT_SESSION_KEY: &str = "oauth2_pending_consent";
const PENDING_CONSENT_TTL_SECS: i64 = 300;

type OAuth2RateLimiter = IpRateLimiter;

pub fn build_rate_limiter() -> Arc<OAuth2RateLimiter> {
    let quota = Quota::per_minute(
        NonZeroU32::new(OAUTH2_RATE_LIMIT_PER_MINUTE).expect("rate limit must be non-zero"),
    );
    Arc::new(OAuth2RateLimiter::keyed(quota))
}

pub fn scope(rate_limiter: Arc<OAuth2RateLimiter>) -> Scope {
    web::scope("/oauth2")
        .app_data(web::Data::new(rate_limiter))
        .route("/register", web::post().to(register))
        .route("/authorize", web::get().to(authorize))
        .route("/authorize/consent", web::get().to(consent_get))
        .route("/authorize/consent", web::post().to(consent_post))
        .route("/token", web::post().to(token))
}

#[derive(Debug, Deserialize)]
pub struct RegisterClientRequest {
    pub client_name: Option<String>,
    pub redirect_uris: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct AuthorizeParams {
    pub response_type: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub scope: Option<String>,
    pub state: Option<String>,
    pub code_challenge: String,
    pub code_challenge_method: String,
    pub resource: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TokenParams {
    pub grant_type: String,
    pub code: Option<String>,
    pub redirect_uri: Option<String>,
    pub code_verifier: Option<String>,
    pub client_id: String,
    pub refresh_token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ConsentRequestQuery {
    pub request_id: String,
}

/// Pending consent state held in the user's session between GET /authorize
/// and POST /authorize/consent. Carries every parameter required to either
/// issue an authorization code (on allow) or build an error redirect (on deny).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingConsentRequest {
    request_id: String,
    csrf_token: String,
    user_id: UserId,
    client_id: String,
    redirect_uri: String,
    scope: Option<String>,
    state: Option<String>,
    code_challenge: String,
    code_challenge_method: String,
    resource: Option<String>,
    expires_at: DateTime<Utc>,
}

impl PendingConsentRequest {
    fn is_expired(&self) -> bool {
        self.expires_at < Utc::now()
    }
}

pub async fn register(
    oauth2_service: web::Data<Arc<OAuth2Service>>,
    body: web::Json<RegisterClientRequest>,
    req: actix_web::HttpRequest,
    rate_limiter: web::Data<Arc<OAuth2RateLimiter>>,
    settings: web::Data<Settings>,
    authenticated: Option<Authenticated<Claims>>,
) -> Result<HttpResponse, UniversalInboxError> {
    if let Err(response) = crate::utils::rate_limit::check_ip_rate_limit(&req, &rate_limiter) {
        return Ok(response);
    }

    // Validate redirect_uri schemes (https://, or http:// loopback only).
    // `OAuth2Service::register_client` performs the same validation again,
    // but doing it here lets the handler return 400 directly.
    if body.redirect_uris.is_empty() {
        return Err(UniversalInboxError::InvalidInputData {
            source: None,
            user_error: "redirect_uris must contain at least one URI".to_string(),
        });
    }
    for uri in &body.redirect_uris {
        validate_redirect_uri(uri)?;
    }

    // Unauthenticated dynamic client registration is permitted when every
    // redirect_uri is either loopback (the MCP-spec public DCR path for
    // desktop/CLI clients — Claude Code, MCP Inspector, …) OR has an
    // https origin present in `mcp_extra_allowed_origins` (the hosted MCP
    // clients we already trust enough to grant CORS + MCP-Origin acceptance).
    // The IP rate limiter on /api/oauth2/* and the explicit consent screen
    // remain in place; the gate just keeps a remote attacker from seeding
    // clients pointing at arbitrary attacker-controlled hosts.
    let allowed_origins = &settings.application.security.mcp_extra_allowed_origins;
    if authenticated.is_none()
        && !body.redirect_uris.iter().all(|uri| {
            is_loopback_redirect_uri(uri) || is_origin_in_allowlist(uri, allowed_origins)
        })
    {
        return Err(UniversalInboxError::Unauthorized(anyhow!(
            "Unauthenticated OAuth2 client registration requires a loopback or \
             trusted-origin redirect_uri: {:?}",
            body.redirect_uris
        )));
    }

    let service = oauth2_service.clone();
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while registering OAuth2 client")?;

    let client = service
        .register_client(
            &mut transaction,
            body.client_name.clone(),
            body.redirect_uris.clone(),
        )
        .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit while registering OAuth2 client")?;

    Ok(HttpResponse::Created()
        .content_type("application/json")
        .body(serde_json::to_string(&client).context("Cannot serialize OAuth2 client")?))
}

pub async fn authorize(
    oauth2_service: web::Data<Arc<OAuth2Service>>,
    authenticated: Option<Authenticated<Claims>>,
    params: web::Query<AuthorizeParams>,
    req: actix_web::HttpRequest,
    settings: web::Data<Settings>,
    session: Session,
) -> Result<HttpResponse, UniversalInboxError> {
    if params.response_type != "code" {
        return Err(UniversalInboxError::InvalidInputData {
            source: None,
            user_error: "Only response_type=code is supported".to_string(),
        });
    }

    // If user is not authenticated, redirect to login with a return URL
    let authenticated = match authenticated {
        Some(auth) => auth,
        None => {
            let authorize_url = req.uri().to_string();
            let login_url = format!(
                "{}login?redirect={}",
                settings.application.front_base_url,
                urlencoding::encode(&authorize_url)
            );
            return Ok(HttpResponse::Found()
                .insert_header(("Location", login_url))
                .finish());
        }
    };

    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;

    let service = oauth2_service.clone();
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while creating authorization code")?;

    // Verify the client exists (DCR row OR CIMD metadata document) and the
    // redirect_uri is registered before doing anything else (mirrors what
    // create_authorization_code would have done).
    let client = service
        .resolve_client(&mut transaction, &params.client_id)
        .await?
        .ok_or_else(|| UniversalInboxError::InvalidInputData {
            source: None,
            user_error: format!("Unknown client_id: {}", params.client_id),
        })?;
    if !redirect_uri_matches_registered(&client.redirect_uris, &params.redirect_uri) {
        return Err(UniversalInboxError::InvalidInputData {
            source: None,
            user_error: format!("Invalid redirect_uri: {}", params.redirect_uri),
        });
    }

    // If the user has already consented to this client with at least the
    // requested scope, skip the consent screen and issue the code immediately.
    let existing_consent = service
        .get_user_consent(&mut transaction, user_id, &params.client_id)
        .await?;
    let consent_covers = existing_consent.as_ref().is_some_and(|consent| {
        OAuth2Service::consent_covers_scope(&consent.scope, params.scope.as_deref())
    });

    if consent_covers {
        let code = service
            .create_authorization_code(
                &mut transaction,
                &params.client_id,
                user_id,
                &params.redirect_uri,
                params.scope.as_deref(),
                &params.code_challenge,
                &params.code_challenge_method,
                params.resource.as_deref(),
            )
            .await?;

        transaction
            .commit()
            .await
            .context("Failed to commit while creating authorization code")?;

        let redirect_url = build_redirect_with_code(&params.redirect_uri, &code, &params.state)?;
        return Ok(HttpResponse::Found()
            .insert_header(("Location", redirect_url))
            .finish());
    }

    transaction
        .commit()
        .await
        .context("Failed to commit while checking OAuth2 consent")?;

    // No prior consent — store a pending consent request in the session and
    // redirect to the frontend consent screen.
    let pending = PendingConsentRequest {
        request_id: Uuid::new_v4().to_string(),
        csrf_token: generate_csrf_token(),
        user_id,
        client_id: params.client_id.clone(),
        redirect_uri: params.redirect_uri.clone(),
        scope: params.scope.clone(),
        state: params.state.clone(),
        code_challenge: params.code_challenge.clone(),
        code_challenge_method: params.code_challenge_method.clone(),
        resource: params.resource.clone(),
        expires_at: Utc::now()
            + TimeDelta::try_seconds(PENDING_CONSENT_TTL_SECS).unwrap_or_else(|| {
                panic!("Invalid PENDING_CONSENT_TTL_SECS value: {PENDING_CONSENT_TTL_SECS}")
            }),
    };

    session
        .insert(OAUTH2_PENDING_CONSENT_SESSION_KEY, &pending)
        .context("Failed to insert OAuth2 pending consent into the session")?;

    let consent_url = format!(
        "{}oauth2/consent?request_id={}",
        settings.application.front_base_url,
        urlencoding::encode(&pending.request_id)
    );
    Ok(HttpResponse::Found()
        .insert_header(("Location", consent_url))
        .finish())
}

pub async fn consent_get(
    oauth2_service: web::Data<Arc<OAuth2Service>>,
    authenticated: Authenticated<Claims>,
    query: web::Query<ConsentRequestQuery>,
    session: Session,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;

    let pending = read_pending_consent(&session, &query.request_id, user_id)?;

    let service = oauth2_service.clone();
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while loading OAuth2 consent request")?;

    let client = service
        .resolve_client(&mut transaction, &pending.client_id)
        .await?
        .ok_or_else(|| UniversalInboxError::InvalidInputData {
            source: None,
            user_error: format!("Unknown client_id: {}", pending.client_id),
        })?;

    transaction
        .commit()
        .await
        .context("Failed to commit while loading OAuth2 consent request")?;

    let response = OAuth2ConsentRequest {
        request_id: pending.request_id.clone(),
        csrf_token: pending.csrf_token.clone(),
        client_id: pending.client_id.clone(),
        client_name: client.client_name,
        redirect_uri: pending.redirect_uri.clone(),
        scope: pending.scope.clone(),
    };
    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(serde_json::to_string(&response).context("Cannot serialize OAuth2 consent request")?))
}

pub async fn consent_post(
    oauth2_service: web::Data<Arc<OAuth2Service>>,
    authenticated: Authenticated<Claims>,
    body: web::Json<OAuth2ConsentSubmission>,
    session: Session,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;

    let pending = read_pending_consent(&session, &body.request_id, user_id)?;

    if pending.csrf_token != body.csrf_token {
        // Don't leak whether the request_id existed — same generic error.
        session.remove(OAUTH2_PENDING_CONSENT_SESSION_KEY);
        return Err(UniversalInboxError::Forbidden(
            "Invalid OAuth2 consent request".to_string(),
        ));
    }

    // Consume the pending entry regardless of the decision.
    session.remove(OAUTH2_PENDING_CONSENT_SESSION_KEY);

    match body.decision {
        OAuth2ConsentDecision::Deny => {
            let redirect_url =
                build_redirect_with_error(&pending.redirect_uri, "access_denied", &pending.state)?;
            Ok(HttpResponse::Ok().content_type("application/json").body(
                serde_json::to_string(&OAuth2ConsentResponse { redirect_url })
                    .context("Cannot serialize OAuth2 consent response")?,
            ))
        }
        OAuth2ConsentDecision::Allow => {
            let service = oauth2_service.clone();
            let mut transaction = service
                .begin()
                .await
                .context("Failed to create new transaction while recording OAuth2 consent")?;

            service
                .record_user_consent(
                    &mut transaction,
                    user_id,
                    &pending.client_id,
                    pending.scope.as_deref().unwrap_or(""),
                )
                .await?;

            let code = service
                .create_authorization_code(
                    &mut transaction,
                    &pending.client_id,
                    user_id,
                    &pending.redirect_uri,
                    pending.scope.as_deref(),
                    &pending.code_challenge,
                    &pending.code_challenge_method,
                    pending.resource.as_deref(),
                )
                .await?;

            transaction
                .commit()
                .await
                .context("Failed to commit while recording OAuth2 consent")?;

            let redirect_url =
                build_redirect_with_code(&pending.redirect_uri, &code, &pending.state)?;
            Ok(HttpResponse::Ok().content_type("application/json").body(
                serde_json::to_string(&OAuth2ConsentResponse { redirect_url })
                    .context("Cannot serialize OAuth2 consent response")?,
            ))
        }
    }
}

pub async fn token(
    oauth2_service: web::Data<Arc<OAuth2Service>>,
    form: web::Form<TokenParams>,
    req: actix_web::HttpRequest,
    rate_limiter: web::Data<Arc<OAuth2RateLimiter>>,
) -> Result<HttpResponse, UniversalInboxError> {
    if let Err(response) = crate::utils::rate_limit::check_ip_rate_limit(&req, &rate_limiter) {
        return Ok(response);
    }
    let service = oauth2_service.clone();
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while exchanging token")?;

    let token_response = match form.grant_type.as_str() {
        "authorization_code" => {
            let code =
                form.code
                    .as_deref()
                    .ok_or_else(|| UniversalInboxError::InvalidInputData {
                        source: None,
                        user_error: "Missing 'code' parameter for authorization_code grant"
                            .to_string(),
                    })?;
            let redirect_uri = form.redirect_uri.as_deref().ok_or_else(|| {
                UniversalInboxError::InvalidInputData {
                    source: None,
                    user_error: "Missing 'redirect_uri' parameter for authorization_code grant"
                        .to_string(),
                }
            })?;
            let code_verifier = form.code_verifier.as_deref().ok_or_else(|| {
                UniversalInboxError::InvalidInputData {
                    source: None,
                    user_error: "Missing 'code_verifier' parameter for authorization_code grant"
                        .to_string(),
                }
            })?;

            service
                .exchange_code(
                    &mut transaction,
                    code,
                    &form.client_id,
                    redirect_uri,
                    code_verifier,
                )
                .await?
        }
        "refresh_token" => {
            let refresh_token = form.refresh_token.as_deref().ok_or_else(|| {
                UniversalInboxError::InvalidInputData {
                    source: None,
                    user_error: "Missing 'refresh_token' parameter for refresh_token grant"
                        .to_string(),
                }
            })?;

            service
                .refresh_token(&mut transaction, refresh_token, &form.client_id)
                .await?
        }
        _ => {
            return Err(UniversalInboxError::InvalidInputData {
                source: None,
                user_error: format!("Unsupported grant_type: {}", form.grant_type),
            });
        }
    };

    transaction
        .commit()
        .await
        .context("Failed to commit while exchanging token")?;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .insert_header(("Cache-Control", "no-store"))
        .insert_header(("Pragma", "no-cache"))
        .body(serde_json::to_string(&token_response).context("Cannot serialize token response")?))
}

fn read_pending_consent(
    session: &Session,
    request_id: &str,
    user_id: UserId,
) -> Result<PendingConsentRequest, UniversalInboxError> {
    let Some(pending) = session
        .get::<PendingConsentRequest>(OAUTH2_PENDING_CONSENT_SESSION_KEY)
        .context("Failed to read pending OAuth2 consent from session")?
    else {
        return Err(UniversalInboxError::Forbidden(
            "No pending OAuth2 consent request".to_string(),
        ));
    };

    if pending.request_id != request_id || pending.user_id != user_id {
        session.remove(OAUTH2_PENDING_CONSENT_SESSION_KEY);
        return Err(UniversalInboxError::Forbidden(
            "Invalid OAuth2 consent request".to_string(),
        ));
    }

    if pending.is_expired() {
        session.remove(OAUTH2_PENDING_CONSENT_SESSION_KEY);
        return Err(UniversalInboxError::Forbidden(
            "OAuth2 consent request has expired".to_string(),
        ));
    }

    Ok(pending)
}

fn build_redirect_with_code(
    redirect_uri: &str,
    code: &str,
    state: &Option<String>,
) -> Result<String, UniversalInboxError> {
    let mut url =
        url::Url::parse(redirect_uri).map_err(|_| UniversalInboxError::InvalidInputData {
            source: None,
            user_error: format!("Invalid redirect_uri: {redirect_uri}"),
        })?;
    url.query_pairs_mut().append_pair("code", code);
    if let Some(state) = state {
        url.query_pairs_mut().append_pair("state", state);
    }
    Ok(url.to_string())
}

fn build_redirect_with_error(
    redirect_uri: &str,
    error: &str,
    state: &Option<String>,
) -> Result<String, UniversalInboxError> {
    let mut url =
        url::Url::parse(redirect_uri).map_err(|_| UniversalInboxError::InvalidInputData {
            source: None,
            user_error: format!("Invalid redirect_uri: {redirect_uri}"),
        })?;
    url.query_pairs_mut().append_pair("error", error);
    if let Some(state) = state {
        url.query_pairs_mut().append_pair("state", state);
    }
    Ok(url.to_string())
}

fn generate_csrf_token() -> String {
    use base64::prelude::*;
    use rand::RngExt;
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    BASE64_URL_SAFE_NO_PAD.encode(bytes)
}
