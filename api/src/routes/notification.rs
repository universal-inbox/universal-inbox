use std::sync::Arc;

use actix_http::body::BoxBody;
use actix_web::{web, HttpResponse};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::universal_inbox::{
    notification::{service::NotificationService, source::NotificationSourceKind},
    UniversalInboxError, UpdateStatus,
};
use ::universal_inbox::{Notification, NotificationPatch, NotificationStatus};

#[derive(Debug, Deserialize)]
pub struct ListNotificationRequest {
    status: NotificationStatus,
    include_snoozed_notifications: Option<bool>,
}

#[tracing::instrument(level = "debug", skip(service))]
pub async fn list_notifications(
    list_notification_request: web::Query<ListNotificationRequest>,
    service: web::Data<Arc<NotificationService>>,
) -> Result<HttpResponse, UniversalInboxError> {
    let notifications: Vec<Notification> = service
        .connect()
        .await?
        .list_notifications(
            list_notification_request.status,
            list_notification_request
                .include_snoozed_notifications
                .unwrap_or(false),
        )
        .await?;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(serde_json::to_string(&notifications).context("Cannot serialize notifications")?))
}

#[tracing::instrument(level = "debug", skip(service))]
pub async fn get_notification(
    path: web::Path<Uuid>,
    service: web::Data<Arc<NotificationService>>,
) -> Result<HttpResponse, UniversalInboxError> {
    let notification_id = path.into_inner();

    match service
        .connect()
        .await?
        .get_notification(notification_id)
        .await?
    {
        Some(notification) => Ok(HttpResponse::Ok()
            .content_type("application/json")
            .body(serde_json::to_string(&notification).context("Cannot serialize notification")?)),
        None => Ok(HttpResponse::NotFound()
            .content_type("application/json")
            .body(BoxBody::new(
                json!({ "message": format!("Cannot find notification {}", notification_id) })
                    .to_string(),
            ))),
    }
}

#[tracing::instrument(level = "debug", skip(service))]
pub async fn create_notification(
    notification: web::Json<Box<Notification>>,
    service: web::Data<Arc<NotificationService>>,
) -> Result<HttpResponse, UniversalInboxError> {
    let transactional_service = service
        .begin()
        .await
        .context("Failed to create new transaction while creating notification")?;

    let created_notification = transactional_service
        .create_notification(notification.into_inner())
        .await?;

    transactional_service
        .commit()
        .await
        .context("Failed to commit while creating notification")?;

    Ok(HttpResponse::Ok().content_type("application/json").body(
        serde_json::to_string(&created_notification).context("Cannot serialize notification")?,
    ))
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SyncNotificationsParameters {
    source: Option<NotificationSourceKind>,
}

#[tracing::instrument(level = "debug", skip(service))]
pub async fn sync_notifications(
    params: web::Json<SyncNotificationsParameters>,
    service: web::Data<Arc<NotificationService>>,
) -> Result<HttpResponse, UniversalInboxError> {
    let transactional_service = service.begin().await.context(format!(
        "Failed to create new transaction while syncing {:?}",
        &params.source
    ))?;

    let notifications: Vec<Notification> = transactional_service
        .sync_notifications(&params.source)
        .await?;

    transactional_service.commit().await.context(format!(
        "Failed to commit while syncing {:?}",
        &params.source
    ))?;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(serde_json::to_string(&notifications).context("Cannot serialize notifications")?))
}

#[tracing::instrument(level = "debug", skip(service))]
pub async fn patch_notification(
    path: web::Path<Uuid>,
    patch: web::Json<NotificationPatch>,
    service: web::Data<Arc<NotificationService>>,
) -> Result<HttpResponse, UniversalInboxError> {
    let notification_id = path.into_inner();
    let transactional_service = service
        .begin()
        .await
        .context(format!("Failed to patch notification {notification_id}"))?;

    let updated_notification = transactional_service
        .patch_notification(notification_id, &patch.into_inner())
        .await?;

    transactional_service.commit().await.context(format!(
        "Failed to commit while patching notification {notification_id}"
    ))?;

    match updated_notification {
        UpdateStatus {
            updated: true,
            result: Some(notification),
        } => Ok(HttpResponse::Ok()
            .content_type("application/json")
            .body(serde_json::to_string(&notification).context("Cannot serialize notification")?)),
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
                    "message": format!("Cannot update unknown notification {}", notification_id)
                })
                .to_string(),
            ))),
    }
}

#[tracing::instrument(level = "debug")]
pub async fn option_wildcard() -> HttpResponse {
    HttpResponse::Ok().finish()
}
