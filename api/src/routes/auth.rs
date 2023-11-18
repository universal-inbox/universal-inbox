use std::{str::FromStr, sync::Arc};

use actix_http::body::BoxBody;
use actix_identity::Identity;
use actix_session::Session;
use actix_web::{
    web::{self, Redirect},
    HttpMessage, HttpRequest, HttpResponse, Scope,
};
use anyhow::Context;
use openidconnect::{core::CoreIdToken, AccessToken, AuthorizationCode, CsrfToken, Nonce};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::RwLock;

use universal_inbox::{
    auth::{AuthorizeSessionResponse, CloseSessionResponse, SessionAuthValidationParameters},
    user::UserId,
};

use crate::{
    configuration::Settings,
    routes::option_wildcard,
    universal_inbox::{user::service::UserService, UniversalInboxError},
};

pub fn scope() -> Scope {
    web::scope("/auth")
        // Authorization code flow
        .service(
            web::resource("session/authorize")
                .route(web::get().to(authorize_session))
                .route(web::method(http::Method::OPTIONS).to(option_wildcard)),
        )
        .service(
            web::resource("session/authenticated")
                .route(web::get().to(authenticated_session))
                .route(web::method(http::Method::OPTIONS).to(option_wildcard)),
        )
        .service(
            web::resource("session")
                // Authorization code + PKCE flow
                .route(web::post().to(authenticate_session))
                .route(web::delete().to(close_session))
                .route(web::method(http::Method::OPTIONS).to(option_wildcard)),
        )
        .service(
            web::resource("user")
                .route(web::get().to(get_user))
                .route(web::method(http::Method::OPTIONS).to(option_wildcard)),
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
pub async fn authenticate_session(
    request: HttpRequest,
    params: web::Json<SessionAuthValidationParameters>,
    user_service: web::Data<Arc<RwLock<UserService>>>,
) -> Result<HttpResponse, UniversalInboxError> {
    let access_token = AccessToken::new(
        request
            .headers()
            .get("Authorization")
            .context("Missing `Authorization` request header")?
            .to_str()
            .context("Failed to convert `Authorization` request header to a string")?
            .split(' ')
            .nth(1)
            .context("Failed to extract the access token from the `Authorization` request header")?
            .to_string(),
    );

    let service = user_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while authenticating user")?;

    let id_token = CoreIdToken::from_str(&params.auth_id_token.to_string())
        .context("Could not parse OIDC ID token")?;
    let user = service
        .authenticate_for_auth_code_pkce_flow(&mut transaction, access_token, id_token)
        .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit while authenticating user")?;

    Identity::login(&request.extensions(), user.id.to_string())
        .map_err(|err| UniversalInboxError::Unauthorized(err.to_string()))?;

    Ok(HttpResponse::Ok().finish())
}

const OIDC_CSRF_TOKEN_SESSION_KEY: &str = "oidc_csrf_token";
const OIDC_NONCE_SESSION_KEY: &str = "oidc_nonce";

/// Implement the Authorization Code flow and redirect the user to the OpenIDConnect
/// auth provider.
pub async fn authorize_session(
    user_service: web::Data<Arc<RwLock<UserService>>>,
    session: Session,
) -> Result<HttpResponse, UniversalInboxError> {
    let service = user_service.read().await;
    let (authorization_url, csrf_token, nonce) = service.build_auth_url().await?;

    session
        .insert(OIDC_CSRF_TOKEN_SESSION_KEY, csrf_token)
        .context("Failed to insert CSRF token into the session")?;
    session
        .insert(OIDC_NONCE_SESSION_KEY, nonce)
        .context("Failed to insert the Nonce into the session")?;

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
    request: HttpRequest,
    settings: web::Data<Settings>,
    session: Session,
    authenticated_session_request: web::Query<AuthenticatedSessionRequest>,
    user_service: web::Data<Arc<RwLock<UserService>>>,
) -> Result<Redirect, UniversalInboxError> {
    // 1. Get the authorization code from the request
    let csrf_token = session
        .get::<CsrfToken>(OIDC_CSRF_TOKEN_SESSION_KEY)
        .context("Failed to extract CSRF token from the session")?
        .context(format!(
            "Missing `{OIDC_CSRF_TOKEN_SESSION_KEY}` session key"
        ))?;
    if authenticated_session_request.state.secret() != csrf_token.secret() {
        return Err(UniversalInboxError::Unauthorized(
            "Invalid CSRF token".to_string(),
        ));
    }

    let nonce = session
        .get::<Nonce>(OIDC_NONCE_SESSION_KEY)
        .context("Failed to extract Nonce from the session")?
        .context(format!("Missing `{OIDC_NONCE_SESSION_KEY}` session key"))?;

    let service = user_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while authenticating user")?;

    let user = service
        .authenticate_for_auth_code_flow(
            &mut transaction,
            authenticated_session_request.code.clone(),
            nonce,
        )
        .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit while authenticating user")?;

    // 4. Create a new authenticated session
    Identity::login(&request.extensions(), user.id.to_string())
        .map_err(|err| UniversalInboxError::Unauthorized(err.to_string()))?;

    Ok(Redirect::to(
        settings.application.front_base_url.to_string(),
    ))
}

pub async fn get_user(
    user_service: web::Data<Arc<RwLock<UserService>>>,
    identity: Identity,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id: UserId = identity
        .id()
        .context("Missing `user_id` in session")?
        .try_into()
        .context("Wrong user ID format")?;
    let service = user_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while fetching user")?;

    match service.get_user(&mut transaction, user_id).await? {
        Some(user) => Ok(HttpResponse::Ok()
            .content_type("application/json")
            .body(serde_json::to_string(&user).context("Cannot serialize user")?)),
        None => Ok(HttpResponse::NotFound()
            .content_type("application/json")
            .body(BoxBody::new(
                json!({ "message": format!("Cannot find user {user_id}") }).to_string(),
            ))),
    }
}

pub async fn close_session(
    user_service: web::Data<Arc<RwLock<UserService>>>,
    identity: Identity,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id: UserId = identity
        .id()
        .context("Missing `user_id` in session")?
        .try_into()
        .context("Wrong user ID format")?;

    let service = user_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while closing user session")?;

    let logout_url = service.close_session(&mut transaction, user_id).await?;

    transaction
        .commit()
        .await
        .context("Failed to commit while authenticating user")?;

    identity.logout();

    Ok(HttpResponse::Ok().content_type("application/json").body(
        serde_json::to_string(&CloseSessionResponse { logout_url })
            .context("Cannot response to close session")?,
    ))
}
