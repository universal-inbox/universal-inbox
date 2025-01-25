use std::{str::FromStr, sync::Arc};

use actix_jwt_authc::Authenticated;
use actix_session::Session;
use actix_web::{
    web::{self, Redirect},
    HttpResponse, Scope,
};
use anyhow::{anyhow, Context};
use openidconnect::{core::CoreIdToken, AuthorizationCode, CsrfToken, Nonce};
use secrecy::ExposeSecret;
use serde::Deserialize;
use tokio::sync::RwLock;
use tracing::debug;
use url::Url;

use universal_inbox::{
    auth::{AuthorizeSessionResponse, CloseSessionResponse, SessionAuthValidationParameters},
    user::{UserAuthKind, UserId},
};

use crate::{
    configuration::{AuthenticationSettings, OIDCFlowSettings, Settings},
    universal_inbox::{
        auth_token::service::AuthenticationTokenService, user::service::UserService,
        UniversalInboxError,
    },
    utils::jwt::JWT_SESSION_KEY,
    Claims,
};

pub fn scope() -> Scope {
    web::scope("/auth")
        // Authorization code flow
        .service(web::resource("session/authorize").route(web::get().to(authorize_session)))
        .service(web::resource("session/authenticated").route(web::get().to(authenticated_session)))
        .service(
            web::resource("session")
                // Authorization code + PKCE flow
                .route(web::post().to(authenticate_session))
                .route(web::delete().to(close_session)),
        )
}

/// Authenticate a user session using the Authorization Code + PKCE flow.
/// It will verify the access token given in the `Authorization` request header:
///
/// Authorization: Bearer ACCESS_TOKEN
///
/// It will also store the auth ID token given in the request body and associate it to
/// the current user.
/// If the user is unknown, it will create a new one.
#[allow(clippy::too_many_arguments)]
pub async fn authenticate_session(
    params: web::Json<SessionAuthValidationParameters>,
    user_service: web::Data<Arc<UserService>>,
    auth_token_service: web::Data<Arc<RwLock<AuthenticationTokenService>>>,
    settings: web::Data<Settings>,
    session: Session,
) -> Result<HttpResponse, UniversalInboxError> {
    let service = user_service.clone();
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while authenticating user")?;

    let Some(AuthenticationSettings::OpenIDConnect(openid_connect_settings)) = &settings
        .application
        .security
        .authentication
        .iter()
        .find(|auth| matches!(auth, AuthenticationSettings::OpenIDConnect(_)))
    else {
        return Err(UniversalInboxError::Unexpected(anyhow!(
            "This service can only be called when OpenID Connect authentication is configured"
                .to_string()
        )));
    };
    let OIDCFlowSettings::AuthorizationCodePKCEFlow(pkce_flow_settings) =
        &openid_connect_settings.oidc_flow_settings
    else {
        return Err(UniversalInboxError::Unexpected(anyhow!(
            "This service can only be called when OpenID Connect PKCE flow is configured"
                .to_string()
        )));
    };

    let id_token = CoreIdToken::from_str(&params.auth_id_token.to_string())
        .context("Could not parse OIDC ID token")?;
    let access_token = params.access_token.clone();
    let user = service
        .authenticate_for_auth_code_pkce_flow(
            &mut transaction,
            openid_connect_settings,
            pkce_flow_settings,
            access_token,
            id_token,
        )
        .await?;

    let auth_token_service = auth_token_service.read().await;

    let auth_token = auth_token_service
        .create_auth_token(&mut transaction, true, user.id, None)
        .await?;
    session
        .insert(
            JWT_SESSION_KEY,
            auth_token.jwt_token.expose_secret().0.clone(),
        )
        .context("Failed to insert JWT token into the session")?;
    session
        .insert(
            USER_AUTH_KIND_SESSION_KEY,
            UserAuthKind::OIDCAuthorizationCodePKCE,
        )
        .context("Failed to insert authentication type into the session")?;

    transaction
        .commit()
        .await
        .context("Failed to commit while authenticating user")?;

    Ok(HttpResponse::Ok().finish())
}

pub const USER_AUTH_KIND_SESSION_KEY: &str = "user_auth_kind";
const OIDC_CSRF_TOKEN_SESSION_KEY: &str = "oidc_csrf_token";
const OIDC_NONCE_SESSION_KEY: &str = "oidc_nonce";
const OIDC_AUTHORIZATION_URL_SESSION_KEY: &str = "authorization_url";

/// Implement the Authorization Code flow and redirect the user to the OpenIDConnect
/// auth provider.
pub async fn authorize_session(
    user_service: web::Data<Arc<UserService>>,
    session: Session,
    settings: web::Data<Settings>,
) -> Result<HttpResponse, UniversalInboxError> {
    if let Some(authorization_url) = session
        .get::<Url>(OIDC_AUTHORIZATION_URL_SESSION_KEY)
        .context("Failed to extract OIDC authorization URL from the session")?
    {
        debug!("Redirecting to authorization URL found in the user's session: {authorization_url}");
        return Ok(HttpResponse::Ok().content_type("application/json").body(
            serde_json::to_string(&AuthorizeSessionResponse { authorization_url })
                .context("Failed to serialize the authorization URL")?,
        ));
    }

    let service = user_service.clone();
    let Some(AuthenticationSettings::OpenIDConnect(openid_connect_settings)) = &settings
        .application
        .security
        .authentication
        .iter()
        .find(|auth| matches!(auth, AuthenticationSettings::OpenIDConnect(_)))
    else {
        return Err(UniversalInboxError::Unexpected(anyhow!(
            "This service can only be called when OpenID Connect authentication is configured"
                .to_string()
        )));
    };
    let (authorization_url, csrf_token, nonce) =
        service.build_auth_url(openid_connect_settings).await?;

    debug!(
        "store CSRF token: {:?} & nonce: {:?}",
        csrf_token.secret(),
        nonce
    );
    session
        .insert(OIDC_CSRF_TOKEN_SESSION_KEY, csrf_token)
        .context("Failed to insert CSRF token into the session")?;
    session
        .insert(OIDC_NONCE_SESSION_KEY, nonce)
        .context("Failed to insert the Nonce into the session")?;
    session
        .insert(
            OIDC_AUTHORIZATION_URL_SESSION_KEY,
            authorization_url.clone(),
        )
        .context("Failed to insert OIDC authorization URL into the session")?;

    debug!("Redirecting to newly built authorization URL: {authorization_url}");
    Ok(HttpResponse::Ok().content_type("application/json").body(
        serde_json::to_string(&AuthorizeSessionResponse { authorization_url })
            .context("Failed to serialize the authorization URL")?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct AuthenticatedSessionRequest {
    code: AuthorizationCode,
    state: CsrfToken,
}

/// Implement the Authorization Code flow and act as the callback URL.
/// It should receive the authorization code and exchange it for an access token.
/// It should also store the auth ID token received from the auth provider and create a new
/// user if it does not exist.
/// Finally it creates a new authenticated session.
pub async fn authenticated_session(
    settings: web::Data<Settings>,
    session: Session,
    authenticated_session_request: web::Query<AuthenticatedSessionRequest>,
    user_service: web::Data<Arc<UserService>>,
    auth_token_service: web::Data<Arc<RwLock<AuthenticationTokenService>>>,
) -> Result<Redirect, UniversalInboxError> {
    session
        .remove(OIDC_AUTHORIZATION_URL_SESSION_KEY)
        .context("Failed to remove the OIDC authorization URL from the session")?;
    // 1. Get the authorization code from the request
    let csrf_token = session
        .get::<CsrfToken>(OIDC_CSRF_TOKEN_SESSION_KEY)
        .context("Failed to extract CSRF token from the session")?
        .context(format!(
            "Missing `{OIDC_CSRF_TOKEN_SESSION_KEY}` session key"
        ))?;
    debug!(
        "fetched CSRF token: {:?} vs state: {:?}",
        csrf_token.secret(),
        authenticated_session_request.state.secret()
    );
    if authenticated_session_request.state.secret() != csrf_token.secret() {
        return Err(UniversalInboxError::Unauthorized(anyhow!(
            "Invalid CSRF token"
        )));
    }

    let nonce = session
        .get::<Nonce>(OIDC_NONCE_SESSION_KEY)
        .context("Failed to extract Nonce from the session")?
        .context(format!("Missing `{OIDC_NONCE_SESSION_KEY}` session key"))?;

    let service = user_service.clone();
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while authenticating user")?;

    let Some(AuthenticationSettings::OpenIDConnect(openid_connect_settings)) = &settings
        .application
        .security
        .authentication
        .iter()
        .find(|auth| matches!(auth, AuthenticationSettings::OpenIDConnect(_)))
    else {
        return Err(UniversalInboxError::Unexpected(anyhow!(
            "This service can only be called when OpenID Connect authentication is configured"
                .to_string()
        )));
    };
    let user = service
        .authenticate_for_auth_code_flow(
            &mut transaction,
            openid_connect_settings,
            authenticated_session_request.code.clone(),
            nonce,
        )
        .await?;

    // 4. Create a new authenticated session
    let auth_token_service = auth_token_service.read().await;

    let auth_token = auth_token_service
        .create_auth_token(&mut transaction, true, user.id, None)
        .await?;
    session
        .insert(
            JWT_SESSION_KEY,
            auth_token.jwt_token.expose_secret().0.clone(),
        )
        .context("Failed to insert JWT token into the session")?;
    session
        .insert(
            USER_AUTH_KIND_SESSION_KEY,
            UserAuthKind::OIDCGoogleAuthorizationCode,
        )
        .context("Failed to insert authentication type into the session")?;

    transaction
        .commit()
        .await
        .context("Failed to commit while authenticating user")?;

    Ok(Redirect::to(
        settings.application.front_base_url.to_string(),
    ))
}

pub async fn close_session(
    user_service: web::Data<Arc<UserService>>,
    authenticated: Authenticated<Claims>,
    session: Session,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;

    let user_auth_kind = session
        .get::<UserAuthKind>(USER_AUTH_KIND_SESSION_KEY)
        .context("Failed to extract UserAuthKind from the session")?;

    let service = user_service.clone();
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while closing user session")?;

    let logout_url = service
        .close_session(&mut transaction, user_id, user_auth_kind)
        .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit while authenticating user")?;

    session.purge();

    Ok(HttpResponse::Ok().content_type("application/json").body(
        serde_json::to_string(&CloseSessionResponse { logout_url })
            .context("Cannot response to close session")?,
    ))
}
