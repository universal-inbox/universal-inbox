use std::sync::Arc;

use actix_http::body::BoxBody;
use actix_identity::Identity;
use actix_web::{web, HttpResponse, Scope};
use anyhow::Context;
use serde_json::json;
use tokio::sync::RwLock;

use universal_inbox::user::UserId;

use crate::{
    routes::option_wildcard,
    universal_inbox::{user::service::UserService, UniversalInboxError},
};

pub fn scope() -> Scope {
    web::scope("/users").service(
        web::resource("me")
            .route(web::get().to(get_user))
            .route(web::method(http::Method::OPTIONS).to(option_wildcard)),
    )
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
