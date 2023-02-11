use std::sync::Arc;

use actix_http::body::BoxBody;
use actix_web::{web, HttpResponse, Scope};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::RwLock;

use universal_inbox::{
    notification::{
        Notification, NotificationId, NotificationPatch, NotificationStatus, NotificationWithTask,
    },
    task::{TaskCreation, TaskId},
};

use crate::{
    integrations::notification::NotificationSyncSourceKind,
    universal_inbox::{
        notification::service::NotificationService, UniversalInboxError, UpdateStatus,
    },
};

use super::option_wildcard;

pub fn scope() -> Scope {
    web::scope("/notifications")
        .route("/sync", web::post().to(sync_notifications))
        .service(
            web::resource("")
                .name("notifications")
                .route(web::get().to(list_notifications))
                .route(web::post().to(create_notification))
                .route(web::method(http::Method::OPTIONS).to(option_wildcard)),
        )
        .service(
            web::resource("/{notification_id}")
                .route(web::get().to(get_notification))
                .route(web::patch().to(patch_notification))
                .route(web::method(http::Method::OPTIONS).to(option_wildcard)),
        )
        .service(
            web::resource("/{notification_id}/task")
                .route(web::post().to(create_task_from_notification))
                .route(web::method(http::Method::OPTIONS).to(option_wildcard)),
        )
}

#[derive(Debug, Deserialize)]
pub struct ListNotificationRequest {
    status: NotificationStatus,
    include_snoozed_notifications: Option<bool>,
    task_id: Option<TaskId>,
}

#[tracing::instrument(level = "debug", skip(notification_service))]
pub async fn list_notifications(
    list_notification_request: web::Query<ListNotificationRequest>,
    notification_service: web::Data<Arc<RwLock<NotificationService>>>,
) -> Result<HttpResponse, UniversalInboxError> {
    let service = notification_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while listing notifications")?;
    let result: Vec<NotificationWithTask> = service
        .list_notifications(
            &mut transaction,
            list_notification_request.status,
            list_notification_request
                .include_snoozed_notifications
                .unwrap_or(false),
            list_notification_request.task_id,
        )
        .await?;

    Ok(HttpResponse::Ok().content_type("application/json").body(
        serde_json::to_string(&result).context("Cannot serialize notifications list result")?,
    ))
}

#[tracing::instrument(level = "debug", skip(notification_service))]
pub async fn get_notification(
    path: web::Path<NotificationId>,
    notification_service: web::Data<Arc<RwLock<NotificationService>>>,
) -> Result<HttpResponse, UniversalInboxError> {
    let notification_id = path.into_inner();
    let service = notification_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while getting notification")?;

    match service
        .get_notification(&mut transaction, notification_id)
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

#[tracing::instrument(level = "debug", skip(notification_service))]
pub async fn create_notification(
    notification: web::Json<Box<Notification>>,
    notification_service: web::Data<Arc<RwLock<NotificationService>>>,
) -> Result<HttpResponse, UniversalInboxError> {
    let service = notification_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while creating notification")?;

    let created_notification = service
        .create_notification(&mut transaction, notification.into_inner())
        .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit while creating notification")?;

    Ok(HttpResponse::Ok().content_type("application/json").body(
        serde_json::to_string(&created_notification).context("Cannot serialize notification")?,
    ))
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SyncNotificationsParameters {
    source: Option<NotificationSyncSourceKind>,
}

#[tracing::instrument(level = "debug", skip(notification_service))]
pub async fn sync_notifications(
    params: web::Json<SyncNotificationsParameters>,
    notification_service: web::Data<Arc<RwLock<NotificationService>>>,
) -> Result<HttpResponse, UniversalInboxError> {
    let service = notification_service.read().await;
    let mut transaction = service.begin().await.context(format!(
        "Failed to create new transaction while syncing {:?}",
        &params.source
    ))?;

    let notifications: Vec<Notification> = service
        .sync_notifications(&mut transaction, &params.source)
        .await?;

    transaction.commit().await.context(format!(
        "Failed to commit while syncing {:?}",
        &params.source
    ))?;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(serde_json::to_string(&notifications).context("Cannot serialize notifications")?))
}

#[tracing::instrument(level = "debug", skip(notification_service))]
pub async fn patch_notification(
    path: web::Path<NotificationId>,
    patch: web::Json<NotificationPatch>,
    notification_service: web::Data<Arc<RwLock<NotificationService>>>,
) -> Result<HttpResponse, UniversalInboxError> {
    let notification_id = path.into_inner();
    let notification_patch = patch.into_inner();
    let service = notification_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context(format!("Failed to patch notification {notification_id}"))?;

    let updated_notification = service
        .patch_notification(&mut transaction, notification_id, &notification_patch)
        .await?;

    transaction.commit().await.context(format!(
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

#[tracing::instrument(level = "debug", skip(notification_service))]
pub async fn create_task_from_notification(
    path: web::Path<NotificationId>,
    task_creation: web::Json<TaskCreation>,
    notification_service: web::Data<Arc<RwLock<NotificationService>>>,
) -> Result<HttpResponse, UniversalInboxError> {
    let notification_id = path.into_inner();
    let task_creation = task_creation.into_inner();
    let service = notification_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context(format!("Failed to create task from {notification_id}"))?;

    let notification_with_task = service
        .create_task_from_notification(&mut transaction, notification_id, &task_creation)
        .await?;

    transaction.commit().await.context(format!(
        "Failed to commit while creating task from notification {notification_id}"
    ))?;

    Ok(HttpResponse::Ok().content_type("application/json").body(
        serde_json::to_string(&notification_with_task).context("Cannot serialize created task")?,
    ))
}
