use std::sync::Arc;

use actix_jwt_authc::Authenticated;
use actix_web::{web, HttpResponse, Scope};
use anyhow::Context;
use tokio::sync::RwLock;

use universal_inbox::{third_party::item::ThirdPartyItem, user::UserId};

use crate::{
    universal_inbox::{third_party::service::ThirdPartyItemService, UniversalInboxError},
    utils::jwt::Claims,
};

pub fn scope() -> Scope {
    web::scope("/third_party").service(
        web::scope("/task")
            .service(web::resource("/items").route(web::post().to(create_third_party_item))),
    )
}

pub async fn create_third_party_item(
    third_party_item: web::Json<Box<ThirdPartyItem>>,
    third_party_item_service: web::Data<Arc<RwLock<ThirdPartyItemService>>>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;
    let service = third_party_item_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while creating third party item")?;

    let created_third_party_item = service
        .create_task_item(&mut transaction, *third_party_item.into_inner(), user_id)
        .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit while creating third party item")?;

    Ok(HttpResponse::Ok().content_type("application/json").body(
        serde_json::to_string(&created_third_party_item)
            .context("Cannot serialize third party item creation result")?,
    ))
}
