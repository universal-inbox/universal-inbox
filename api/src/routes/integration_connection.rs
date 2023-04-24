use std::sync::Arc;

use actix_http::body::BoxBody;
use actix_identity::Identity;
use actix_web::{web, HttpResponse, Scope};
use anyhow::Context;
use serde_json::json;
use tokio::sync::RwLock;

use universal_inbox::{
    integration_connection::{
        IntegrationConnection, IntegrationConnectionCreation, IntegrationConnectionId,
    },
    user::UserId,
};

use crate::{
    routes::option_wildcard,
    universal_inbox::{
        integration_connection::service::IntegrationConnectionService, UniversalInboxError,
        UpdateStatus,
    },
};

pub fn scope() -> Scope {
    web::scope("/integration-connections")
        .service(
            web::resource("")
                .name("integration-connections")
                .route(web::get().to(list_integration_connections))
                .route(web::post().to(create_integration_connection))
                .route(web::method(http::Method::OPTIONS).to(option_wildcard)),
        )
        .service(
            web::resource("/{integration_connection_id}")
                .route(web::delete().to(disconnect_integration_connection))
                .route(web::method(http::Method::OPTIONS).to(option_wildcard)),
        )
        .service(
            web::resource("/{integration_connection_id}/status")
                .route(web::patch().to(verify_integration_connection))
                .route(web::method(http::Method::OPTIONS).to(option_wildcard)),
        )
}

#[tracing::instrument(level = "debug", skip(integration_connection_service, identity))]
pub async fn list_integration_connections(
    integration_connection_service: web::Data<Arc<RwLock<IntegrationConnectionService>>>,
    identity: Identity,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = identity
        .id()
        .context("No user ID found in identity")?
        .parse::<UserId>()
        .context("User ID has wrong format")?;
    let service = integration_connection_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while listing integration connections")?;
    let result: Vec<IntegrationConnection> = service
        .fetch_all_integration_connections(&mut transaction, user_id)
        .await?;

    Ok(HttpResponse::Ok().content_type("application/json").body(
        serde_json::to_string(&result)
            .context("Cannot serialize integration connections list result")?,
    ))
}

#[tracing::instrument(level = "debug", skip(integration_connection_service, identity))]
pub async fn create_integration_connection(
    integration_connection_creation: web::Json<IntegrationConnectionCreation>,
    integration_connection_service: web::Data<Arc<RwLock<IntegrationConnectionService>>>,
    identity: Identity,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = identity
        .id()
        .context("No user ID found in identity")?
        .parse::<UserId>()
        .context("User ID has wrong format")?;
    let service = integration_connection_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while creating integration connection")?;

    let created_integration_connection = service
        .create_integration_connection(
            &mut transaction,
            integration_connection_creation.provider_kind,
            user_id,
        )
        .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit while creating integration connection")?;

    Ok(HttpResponse::Ok().content_type("application/json").body(
        serde_json::to_string(&created_integration_connection)
            .context("Cannot serialize integration connection")?,
    ))
}

#[tracing::instrument(level = "debug", skip(integration_connection_service, identity))]
pub async fn verify_integration_connection(
    path: web::Path<IntegrationConnectionId>,
    integration_connection_service: web::Data<Arc<RwLock<IntegrationConnectionService>>>,
    identity: Identity,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = identity
        .id()
        .context("No user ID found in identity")?
        .parse::<UserId>()
        .context("User ID has wrong format")?;
    let integration_connection_id = path.into_inner();
    let service = integration_connection_service.read().await;
    let mut transaction = service.begin().await.context(format!(
        "Failed to verify integration connection {integration_connection_id}"
    ))?;

    let updated_integration_connection = service
        .verify_integration_connection(&mut transaction, integration_connection_id, user_id)
        .await?;

    transaction.commit().await.context(format!(
        "Failed to commit while verifying integration connection {integration_connection_id}"
    ))?;

    match updated_integration_connection {
        UpdateStatus {
            updated: _,
            result: Some(integration_connection),
        } => Ok(HttpResponse::Ok().content_type("application/json").body(
            serde_json::to_string(&integration_connection)
                .context("Cannot serialize integration connection")?,
        )),
        UpdateStatus {
            updated: _,
            result: None,
        } => Ok(HttpResponse::NotFound()
            .content_type("application/json")
            .body(BoxBody::new(
                json!({
                    "message":
                    format!(
                        "Cannot update unknown integration_connection {integration_connection_id}"
                    )
                })
                .to_string(),
            ))),
    }
}

#[tracing::instrument(level = "debug", skip(integration_connection_service, identity))]
pub async fn disconnect_integration_connection(
    path: web::Path<IntegrationConnectionId>,
    integration_connection_service: web::Data<Arc<RwLock<IntegrationConnectionService>>>,
    identity: Identity,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = identity
        .id()
        .context("No user ID found in identity")?
        .parse::<UserId>()
        .context("User ID has wrong format")?;
    let integration_connection_id = path.into_inner();
    let service = integration_connection_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context(format!("Failed to create new transaction while disconnecting integration connection {integration_connection_id}"))?;

    let disconnected_integration_connection = service
        .disconnect_integration_connection(&mut transaction, integration_connection_id, user_id)
        .await?;

    transaction.commit().await.context(format!(
        "Failed to commit while disconnecting integration connection {integration_connection_id}"
    ))?;

    match disconnected_integration_connection {
        UpdateStatus {
            updated: _,
            result: Some(integration_connection),
        } => Ok(HttpResponse::Ok().content_type("application/json").body(
            serde_json::to_string(&integration_connection)
                .context("Cannot serialize integration connection")?,
        )),
        UpdateStatus {
            updated: _,
            result: None,
        } => Ok(HttpResponse::NotFound()
            .content_type("application/json")
            .body(BoxBody::new(
                json!({
                    "message":
                    format!(
                        "Cannot update unknown integration_connection {integration_connection_id}"
                    )
                })
                .to_string(),
            ))),
    }
}
