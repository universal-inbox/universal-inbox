use std::sync::Arc;

use actix_web::{body::BoxBody, web, HttpResponse};
use anyhow::Context;
use redis::AsyncCommands;
use serde_json::json;
use tokio::sync::RwLock;

use crate::{
    universal_inbox::{
        integration_connection::service::IntegrationConnectionService, UniversalInboxError,
    },
    utils::cache::Cache,
};

pub async fn ping(
    integration_connection_service: web::Data<Arc<RwLock<IntegrationConnectionService>>>,
    cache: web::Data<Cache>,
) -> Result<HttpResponse, UniversalInboxError> {
    let cache_result: Result<String, anyhow::Error> = cache
        .connection_manager
        .clone()
        .ping()
        .await
        .context("Failed to ping Redis");

    let service = integration_connection_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction to ping the database")?;
    let db_result = sqlx::query_scalar!("SELECT 1")
        .fetch_one(&mut *transaction)
        .await
        .map_err(|err| {
            let message = format!("Failed to ping database: {}", err);
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        });

    let mut response = if cache_result.is_err() || db_result.is_err() {
        HttpResponse::InternalServerError()
    } else {
        HttpResponse::Ok()
    };

    Ok(response.content_type("application/json").body(BoxBody::new(
        json!({
            "cache": cache_result.map(|_| "healthy").unwrap_or("unhealthy"),
            "database": db_result.map(|_| "healthy").unwrap_or("unhealthy"),
        })
        .to_string(),
    )))
}
