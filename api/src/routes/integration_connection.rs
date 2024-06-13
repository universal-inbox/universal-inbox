use std::sync::Arc;

use actix_http::body::BoxBody;
use actix_jwt_authc::Authenticated;
use actix_web::{web, HttpResponse, Scope};
use anyhow::Context;
use serde_json::json;
use tokio::sync::RwLock;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, IntegrationConnection, IntegrationConnectionCreation,
        IntegrationConnectionId,
    },
    user::UserId,
};

use crate::{
    universal_inbox::{
        integration_connection::service::IntegrationConnectionService, UniversalInboxError,
        UpdateStatus,
    },
    utils::jwt::Claims,
};

pub fn scope() -> Scope {
    web::scope("/integration-connections")
        .service(
            web::resource("")
                .name("integration-connections")
                .route(web::get().to(list_integration_connections))
                .route(web::post().to(create_integration_connection)),
        )
        .service(
            web::resource("/{integration_connection_id}")
                .route(web::delete().to(disconnect_integration_connection)),
        )
        .service(
            web::resource("/{integration_connection_id}/config")
                .route(web::put().to(update_integration_connection_config)),
        )
        .service(
            web::resource("/{integration_connection_id}/status")
                .route(web::patch().to(verify_integration_connection)),
        )
}

pub async fn list_integration_connections(
    integration_connection_service: web::Data<Arc<RwLock<IntegrationConnectionService>>>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;
    let service = integration_connection_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while listing integration connections")?;
    let result: Vec<IntegrationConnection> = service
        .fetch_all_integration_connections(&mut transaction, user_id, None)
        .await?;

    Ok(HttpResponse::Ok().content_type("application/json").body(
        serde_json::to_string(&result)
            .context("Cannot serialize integration connections list result")?,
    ))
}

pub async fn create_integration_connection(
    integration_connection_creation: web::Json<IntegrationConnectionCreation>,
    integration_connection_service: web::Data<Arc<RwLock<IntegrationConnectionService>>>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;
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

pub async fn update_integration_connection_config(
    path: web::Path<IntegrationConnectionId>,
    integration_connection_config: web::Json<IntegrationConnectionConfig>,
    integration_connection_service: web::Data<Arc<RwLock<IntegrationConnectionService>>>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;
    let integration_connection_id = path.into_inner();
    let service = integration_connection_service.read().await;
    let mut transaction = service.begin().await.context(
        "Failed to create new transaction while updating integration connection configuration",
    )?;

    let updated_config = service
        .update_integration_connection_config(
            &mut transaction,
            integration_connection_id,
            integration_connection_config.into_inner(),
            user_id,
        )
        .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit while updating integration connection configuration")?;

    match updated_config {
        UpdateStatus {
            updated: true,
            result: Some(config),
        } => Ok(HttpResponse::Ok()
                .content_type("application/json")
                .body(serde_json::to_string(&config).context("Cannot serialize integration connection configuration")?)),
        UpdateStatus {
            updated: false,
            result: Some(_),
        } => Ok(HttpResponse::NotModified().finish()),
        UpdateStatus {
            updated: _,
            result: None,
        } => Ok(HttpResponse::NotFound()
                .content_type("application/json")
                .body(BoxBody::new(
                    json!({
                        "message": format!("Cannot update unknown integration connection {integration_connection_id}")
                    })
                        .to_string(),
                ))),
    }
}

pub async fn verify_integration_connection(
    path: web::Path<IntegrationConnectionId>,
    integration_connection_service: web::Data<Arc<RwLock<IntegrationConnectionService>>>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;
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

pub async fn disconnect_integration_connection(
    path: web::Path<IntegrationConnectionId>,
    integration_connection_service: web::Data<Arc<RwLock<IntegrationConnectionService>>>,
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
