use std::sync::Arc;

use actix_http::body::BoxBody;
use actix_identity::Identity;
use actix_web::{web, HttpMessage, HttpRequest, HttpResponse, Scope};
use anyhow::Context;
use serde_json::json;
use tokio::sync::RwLock;

use universal_inbox::{
    auth::{CloseSessionResponse, SessionAuthValidationParameters},
    user::UserId,
};

use crate::{
    routes::option_wildcard,
    universal_inbox::{user::service::UserService, UniversalInboxError},
};

pub fn scope() -> Scope {
    web::scope("/auth")
        .service(
            web::resource("session")
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

pub async fn authenticate_session(
    request: HttpRequest,
    params: web::Json<SessionAuthValidationParameters>,
    user_service: web::Data<Arc<RwLock<UserService>>>,
) -> Result<HttpResponse, UniversalInboxError> {
    let bearer_access_token = request
        .headers()
        .get("Authorization")
        .context("Missing `Authorization` request header")?
        .to_str()
        .context("Failed to convert `Authorization` request header to a string")?
        .split(' ')
        .nth(1)
        .context("Failed to extract the access token from the `Authorization` request header")?;

    let service = user_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while authenticating user")?;

    let user = service
        .authenticate_and_create_user_if_not_exists(
            &mut transaction,
            bearer_access_token,
            params.auth_id_token.clone(),
        )
        .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit while authenticating user")?;

    Identity::login(&request.extensions(), user.id.to_string())?;

    Ok(HttpResponse::Ok().finish())
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
