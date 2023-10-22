use std::{fmt, sync::Arc};

use actix_http::body::BoxBody;
use actix_identity::Identity;
use actix_web::{web, HttpResponse, Scope};
use anyhow::Context;
use serde::{de, Deserialize};
use serde_json::json;
use tokio::sync::RwLock;
use tracing::{error, info};

use universal_inbox::{
    notification::{
        service::{NotificationPatch, SyncNotificationsParameters},
        Notification, NotificationId, NotificationStatus, NotificationWithTask,
    },
    task::{TaskCreation, TaskId},
    user::UserId,
};

use crate::{
    routes::option_wildcard,
    universal_inbox::{
        notification::service::NotificationService, UniversalInboxError, UpdateStatus,
    },
};

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
    #[serde(deserialize_with = "deserialize_stringified_list")]
    status: Vec<NotificationStatus>,
    include_snoozed_notifications: Option<bool>,
    task_id: Option<TaskId>,
}

pub fn deserialize_stringified_list<'de, D>(
    deserializer: D,
) -> Result<Vec<NotificationStatus>, D::Error>
where
    D: de::Deserializer<'de>,
{
    struct StringVecVisitor;

    impl<'de> de::Visitor<'de> for StringVecVisitor {
        type Value = Vec<NotificationStatus>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a string containing a list of NoticationStatus")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            let mut ids = Vec::new();
            for id in v.split(',') {
                let id = id.parse::<NotificationStatus>().map_err(E::custom)?;
                ids.push(id);
            }
            Ok(ids)
        }
    }

    deserializer.deserialize_any(StringVecVisitor)
}

pub async fn list_notifications(
    list_notification_request: web::Query<ListNotificationRequest>,
    notification_service: web::Data<Arc<RwLock<NotificationService>>>,
    identity: Identity,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = identity
        .id()
        .context("No user ID found in identity")?
        .parse::<UserId>()
        .context("User ID has wrong format")?;
    let service = notification_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while listing notifications")?;
    let result: Vec<NotificationWithTask> = service
        .list_notifications(
            &mut transaction,
            list_notification_request.status.clone(),
            list_notification_request
                .include_snoozed_notifications
                .unwrap_or(false),
            list_notification_request.task_id,
            user_id,
        )
        .await?;

    Ok(HttpResponse::Ok().content_type("application/json").body(
        serde_json::to_string(&result).context("Cannot serialize notifications list result")?,
    ))
}

pub async fn get_notification(
    path: web::Path<NotificationId>,
    notification_service: web::Data<Arc<RwLock<NotificationService>>>,
    identity: Identity,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = identity
        .id()
        .context("No user ID found in identity")?
        .parse::<UserId>()
        .context("User ID has wrong format")?;
    let notification_id = path.into_inner();
    let service = notification_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while getting notification")?;

    match service
        .get_notification(&mut transaction, notification_id, user_id)
        .await?
    {
        Some(notification) => Ok(HttpResponse::Ok()
            .content_type("application/json")
            .body(serde_json::to_string(&notification).context("Cannot serialize notification")?)),
        None => Ok(HttpResponse::NotFound()
            .content_type("application/json")
            .body(BoxBody::new(
                json!({ "message": format!("Cannot find notification {notification_id}") })
                    .to_string(),
            ))),
    }
}

pub async fn create_notification(
    notification: web::Json<Box<Notification>>,
    notification_service: web::Data<Arc<RwLock<NotificationService>>>,
    identity: Identity,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = identity
        .id()
        .context("No user ID found in identity")?
        .parse::<UserId>()
        .context("User ID has wrong format")?;
    let service = notification_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while creating notification")?;

    let created_notification = service
        .create_notification(&mut transaction, notification.into_inner(), user_id)
        .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit while creating notification")?;

    Ok(HttpResponse::Ok().content_type("application/json").body(
        serde_json::to_string(&created_notification).context("Cannot serialize notification")?,
    ))
}

pub async fn sync_notifications(
    params: web::Json<SyncNotificationsParameters>,
    notification_service: web::Data<Arc<RwLock<NotificationService>>>,
    identity: Option<Identity>,
) -> Result<HttpResponse, UniversalInboxError> {
    let source = params.source;

    if let Some(identity) = identity {
        let user_id = identity
            .id()
            .context("No user ID found in identity")?
            .parse::<UserId>()
            .context("User ID has wrong format")?;

        if params.asynchronous.unwrap_or(true) {
            let notification_service = notification_service.get_ref().clone();
            tokio::spawn(async move {
                let source_kind_string = source
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "all types of".to_string());
                info!("Syncing {source_kind_string} notifications for user {user_id}");
                let service = notification_service.read().await;

                let notifications = if let Some(source) = source {
                    service
                        .sync_notifications_with_transaction(source, user_id)
                        .await
                } else {
                    service.sync_all_notifications(user_id).await
                };

                match notifications {
                    Ok(notifications) => info!(
                        "{} {source_kind_string} notifications successfully synced for user {user_id}",
                        notifications.len()
                    ),
                    Err(err) => {
                        error!("Failed to sync {source_kind_string} notifications for user {user_id}: {err:?}")
                    }
                };
            });
            Ok(HttpResponse::Created().finish())
        } else {
            let service = notification_service.read().await;

            let notifications = if let Some(source) = source {
                service
                    .sync_notifications_with_transaction(source, user_id)
                    .await?
            } else {
                service.sync_all_notifications(user_id).await?
            };
            Ok(HttpResponse::Ok().content_type("application/json").body(
                serde_json::to_string(&notifications).context("Cannot serialize notifications")?,
            ))
        }
    } else {
        let notification_service = notification_service.get_ref().clone();

        tokio::spawn(async move {
            let source_kind_string = source
                .map(|s| s.to_string())
                .unwrap_or_else(|| "all types of".to_string());
            info!("Syncing {source_kind_string} notifications for all users");
            let service = notification_service.read().await;

            let result = service.sync_notifications_for_all_users(source).await;

            match result {
                Ok(_) => info!("{source_kind_string} notifications successfully synced"),
                Err(err) => {
                    error!("Failed to sync {source_kind_string} notifications: {err:?}")
                }
            };
        });
        Ok(HttpResponse::Created().finish())
    }
}

pub async fn patch_notification(
    path: web::Path<NotificationId>,
    patch: web::Json<NotificationPatch>,
    notification_service: web::Data<Arc<RwLock<NotificationService>>>,
    identity: Identity,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = identity
        .id()
        .context("No user ID found in identity")?
        .parse::<UserId>()
        .context("User ID has wrong format")?;
    let notification_id = path.into_inner();
    let notification_patch = patch.into_inner();
    let service = notification_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context(format!("Failed to patch notification {notification_id}"))?;

    let updated_notification = service
        .patch_notification(
            &mut transaction,
            notification_id,
            &notification_patch,
            true,
            user_id,
        )
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
                    "message": format!("Cannot update unknown notification {notification_id}")
                })
                .to_string(),
            ))),
    }
}

pub async fn create_task_from_notification(
    path: web::Path<NotificationId>,
    task_creation: web::Json<TaskCreation>,
    notification_service: web::Data<Arc<RwLock<NotificationService>>>,
    identity: Identity,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = identity
        .id()
        .context("No user ID found in identity")?
        .parse::<UserId>()
        .context("User ID has wrong format")?;
    let notification_id = path.into_inner();
    let task_creation = task_creation.into_inner();
    let service = notification_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context(format!("Failed to create task from {notification_id}"))?;

    let notification_with_task = service
        .create_task_from_notification(&mut transaction, notification_id, &task_creation, user_id)
        .await?;

    transaction.commit().await.context(format!(
        "Failed to commit while creating task from notification {notification_id}"
    ))?;

    Ok(HttpResponse::Ok().content_type("application/json").body(
        serde_json::to_string(&notification_with_task).context("Cannot serialize created task")?,
    ))
}
