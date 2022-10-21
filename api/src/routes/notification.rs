use std::sync::Arc;

use crate::universal_inbox::{
    self,
    notification::{service::NotificationService, source::NotificationSource},
    UniversalInboxError, UpdateStatus,
};
use ::universal_inbox::{Notification, NotificationPatch};
use actix_http::{body::BoxBody, header::TryIntoHeaderValue};
use actix_web::{
    http::{
        header::{self, ContentType},
        StatusCode,
    },
    web, HttpResponse, ResponseError,
};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

impl ResponseError for UniversalInboxError {
    fn status_code(&self) -> StatusCode {
        match self {
            UniversalInboxError::InvalidData { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            UniversalInboxError::MissingInputData(_) => StatusCode::BAD_REQUEST,
            UniversalInboxError::AlreadyExists { .. } => StatusCode::BAD_REQUEST,
            UniversalInboxError::Unexpected(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> HttpResponse<BoxBody> {
        let mut res = HttpResponse::new(self.status_code());

        res.headers_mut().insert(
            header::CONTENT_TYPE,
            ContentType::json().try_into_value().unwrap(),
        );

        res.set_body(BoxBody::new(
            json!({ "message": format!("{}", self) }).to_string(),
        ))
    }
}

#[tracing::instrument(level = "debug", skip(service))]
pub async fn list_notifications(
    service: web::Data<Arc<NotificationService>>,
) -> Result<HttpResponse, universal_inbox::UniversalInboxError> {
    let notifications: Vec<Notification> = service.list_notifications().await?;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(serde_json::to_string(&notifications).context("Cannot serialize notifications")?))
}

#[tracing::instrument(level = "debug", skip(service))]
pub async fn get_notification(
    path: web::Path<Uuid>,
    service: web::Data<Arc<NotificationService>>,
) -> Result<HttpResponse, universal_inbox::UniversalInboxError> {
    let notification_id = path.into_inner();

    match service.get_notification(notification_id).await? {
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
    notification: web::Json<Notification>,
    service: web::Data<Arc<NotificationService>>,
) -> Result<HttpResponse, universal_inbox::UniversalInboxError> {
    let created_notification = service
        .create_notification(&notification.into_inner())
        .await?;

    Ok(HttpResponse::Ok().content_type("application/json").body(
        serde_json::to_string(&created_notification).context("Cannot serialize notification")?,
    ))
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SyncNotificationsParameters {
    source: NotificationSource,
}

#[tracing::instrument(level = "debug", skip(service))]
pub async fn sync_notifications(
    params: web::Json<SyncNotificationsParameters>,
    service: web::Data<Arc<NotificationService>>,
) -> Result<HttpResponse, universal_inbox::UniversalInboxError> {
    let transaction = service.repository.begin().await.context(format!(
        "Failed to create new transaction while syncing {:?}",
        &params.source
    ))?;

    let notifications: Vec<Notification> = service.sync_notifications(&Some(params.source)).await?;

    transaction.commit().await.context(format!(
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
) -> Result<HttpResponse, universal_inbox::UniversalInboxError> {
    let notification_id = path.into_inner();
    let transaction = service
        .repository
        .begin()
        .await
        .context(format!("Failed to patch notification {notification_id}"))?;

    let updated_notification = service
        .patch_notification(notification_id, &patch.into_inner())
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

#[tracing::instrument(level = "debug")]
pub async fn option_wildcard() -> HttpResponse {
    HttpResponse::Ok().finish()
}
