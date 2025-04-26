use std::sync::Arc;

use actix_http::body::BoxBody;
use actix_jwt_authc::{Authenticated, MaybeAuthenticated};
use actix_web::{web, HttpResponse, Scope};
use anyhow::Context;
use apalis_redis::RedisStorage;
use serde::Deserialize;
use serde_json::json;
use serde_with::{formats::CommaSeparator, serde_as, StringWithSeparator};
use tokio::sync::RwLock;

use universal_inbox::{
    notification::{
        service::{InvitationPatch, NotificationPatch, SyncNotificationsParameters},
        NotificationId, NotificationListOrder, NotificationSourceKind, NotificationStatus,
        NotificationWithTask,
    },
    task::{TaskCreation, TaskId},
    user::UserId,
    utils::base64::decode_base64,
    Page, PageToken,
};

use crate::{
    jobs::UniversalInboxJob,
    universal_inbox::{
        integration_connection::service::IntegrationConnectionService,
        notification::service::NotificationService, UniversalInboxError, UpdateStatus,
    },
    utils::jwt::Claims,
};

pub fn scope() -> Scope {
    web::scope("/notifications")
        .route("/sync", web::post().to(sync_notifications))
        .service(
            web::resource("")
                .name("notifications")
                .route(web::get().to(list_notifications)),
        )
        .service(
            web::resource("/{notification_id}")
                .route(web::get().to(get_notification))
                .route(web::patch().to(patch_notification)),
        )
        .service(
            web::resource("/{notification_id}/task")
                .route(web::post().to(create_task_from_notification)),
        )
        .service(
            web::resource("/{notification_id}/invitation")
                .route(web::patch().to(update_invitation_from_notification)),
        )
}

#[serde_as]
#[derive(Debug, Deserialize)]
pub struct ListNotificationRequest {
    #[serde_as(as = "Option<StringWithSeparator::<CommaSeparator, NotificationStatus>>")]
    status: Option<Vec<NotificationStatus>>,
    include_snoozed_notifications: Option<bool>,
    task_id: Option<TaskId>,
    trigger_sync: Option<bool>,
    order_by: Option<NotificationListOrder>,
    #[serde_as(as = "Option<StringWithSeparator::<CommaSeparator, NotificationSourceKind>>")]
    sources: Option<Vec<NotificationSourceKind>>,
    page_token: Option<String>,
}

pub async fn list_notifications(
    list_notification_request: web::Query<ListNotificationRequest>,
    notification_service: web::Data<Arc<RwLock<NotificationService>>>,
    authenticated: Authenticated<Claims>,
    job_storage: web::Data<RedisStorage<UniversalInboxJob>>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;
    let page_token: Option<PageToken> = if let Some(token) = &list_notification_request.page_token {
        let Ok(decoded_token) = decode_base64(token) else {
            return Ok(HttpResponse::BadRequest()
                .content_type("application/json")
                .body(json!({"error": "Invalid page token format"}).to_string()));
        };

        let Ok(token) = serde_json::from_str(&decoded_token) else {
            return Ok(HttpResponse::BadRequest()
                .content_type("application/json")
                .body(json!({"error": "Invalid page token structure"}).to_string()));
        };
        Some(token)
    } else {
        None
    };

    let service = notification_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while listing notifications")?;

    let result: Page<NotificationWithTask> = service
        .list_notifications(
            &mut transaction,
            list_notification_request.status.clone().unwrap_or_default(),
            list_notification_request
                .include_snoozed_notifications
                .unwrap_or(false),
            list_notification_request.task_id,
            list_notification_request.order_by.unwrap_or_default(),
            list_notification_request
                .sources
                .clone()
                .unwrap_or_default(),
            page_token,
            user_id,
            list_notification_request
                .trigger_sync
                .unwrap_or(true)
                .then(|| job_storage.as_ref().clone()),
        )
        .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit while listing notifications")?;

    Ok(HttpResponse::Ok().content_type("application/json").body(
        serde_json::to_string(&result).context("Cannot serialize notifications list result")?,
    ))
}

pub async fn get_notification(
    path: web::Path<NotificationId>,
    notification_service: web::Data<Arc<RwLock<NotificationService>>>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;
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

pub async fn sync_notifications(
    params: web::Json<SyncNotificationsParameters>,
    notification_service: web::Data<Arc<RwLock<NotificationService>>>,
    integration_connection_service: web::Data<Arc<RwLock<IntegrationConnectionService>>>,
    maybe_authenticated: MaybeAuthenticated<Claims>,
    storage: web::Data<RedisStorage<UniversalInboxJob>>,
) -> Result<HttpResponse, UniversalInboxError> {
    let source = params.source;
    let mut storage = storage.as_ref().clone();

    if let Some(authenticated) = maybe_authenticated.into_option() {
        let user_id = authenticated
            .claims
            .sub
            .parse::<UserId>()
            .context("Wrong user ID format")?;

        if params.asynchronous.unwrap_or(true) {
            let service = integration_connection_service.read().await;
            let mut transaction = service
                .begin()
                .await
                .context("Failed to create new transaction while triggering notifications sync")?;
            service
                .trigger_sync_notifications(&mut transaction, source, Some(user_id), &mut storage)
                .await?;
            transaction
                .commit()
                .await
                .context("Failed to commit while triggering notifications sync")?;
            Ok(HttpResponse::Created().finish())
        } else {
            let service = notification_service.read().await;

            let notifications = if let Some(source) = source {
                service
                    .sync_notifications_with_transaction(source, user_id, false)
                    .await?
            } else {
                service.sync_all_notifications(user_id, false).await?
            };
            Ok(HttpResponse::Ok().content_type("application/json").body(
                serde_json::to_string(&notifications).context("Cannot serialize notifications")?,
            ))
        }
    } else {
        let service = integration_connection_service.read().await;
        let mut transaction = service
            .begin()
            .await
            .context("Failed to create new transaction while triggering notifications sync")?;
        service
            .trigger_sync_notifications(&mut transaction, source, None, &mut storage)
            .await?;
        transaction
            .commit()
            .await
            .context("Failed to commit while triggering notifications sync")?;
        Ok(HttpResponse::Created().finish())
    }
}

pub async fn patch_notification(
    path: web::Path<NotificationId>,
    patch: web::Json<NotificationPatch>,
    notification_service: web::Data<Arc<RwLock<NotificationService>>>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;
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
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;
    let notification_id = path.into_inner();
    let task_creation = task_creation.into_inner();
    let service = notification_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context(format!("Failed to create task from {notification_id}"))?;

    let notification_with_task = service
        .create_task_from_notification(
            &mut transaction,
            notification_id,
            &task_creation,
            true,
            user_id,
        )
        .await?;

    transaction.commit().await.context(format!(
        "Failed to commit while creating task from notification {notification_id}"
    ))?;

    Ok(HttpResponse::Ok().content_type("application/json").body(
        serde_json::to_string(&notification_with_task).context("Cannot serialize created task")?,
    ))
}

pub async fn update_invitation_from_notification(
    path: web::Path<NotificationId>,
    patch: web::Json<InvitationPatch>,
    notification_service: web::Data<Arc<RwLock<NotificationService>>>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;
    let notification_id = path.into_inner();
    let invitation_patch = patch.into_inner();
    let service = notification_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while updating invitation")?;

    let updated_notification = service
        .update_invitation_from_notification(
            &mut transaction,
            notification_id,
            &invitation_patch,
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
