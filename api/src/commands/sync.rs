use std::sync::Arc;

use anyhow::Context;
use tokio::sync::RwLock;
use tracing::{error, info};

use universal_inbox::user::UserId;

use crate::{
    integrations::{notification::NotificationSyncSourceKind, task::TaskSyncSourceKind},
    universal_inbox::{
        notification::service::NotificationService, task::service::TaskService,
        user::service::UserService, UniversalInboxError,
    },
};

pub async fn sync_notifications_for_all_users(
    user_service: Arc<RwLock<UserService>>,
    notification_service: Arc<RwLock<NotificationService>>,
    source: &Option<NotificationSyncSourceKind>,
) -> Result<(), UniversalInboxError> {
    let service = user_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while syncing notifications for all users")?;
    let users = service.fetch_all_users(&mut transaction).await?;
    for user in users {
        info!("Syncing notifications for user {}", user.id);
        match sync_notifications(notification_service.clone(), source, user.id).await {
            Ok(_) => info!("Notifications successfully synced for user {}", user.id),
            Err(err) => error!(
                "Failed to sync notifications for user {}: {:?}",
                user.id, err
            ),
        };
    }
    Ok(())
}

pub async fn sync_notifications(
    notification_service: Arc<RwLock<NotificationService>>,
    source: &Option<NotificationSyncSourceKind>,
    user_id: UserId,
) -> Result<(), UniversalInboxError> {
    let service = notification_service.read().await;
    let mut transaction = service.begin().await.context(format!(
        "Failed to create new transaction while syncing {source:?}"
    ))?;

    service
        .sync_notifications(&mut transaction, source, user_id)
        .await
        .context(format!("Failed to sync {source:?}"))?;

    transaction
        .commit()
        .await
        .context(format!("Failed to commit while syncing {source:?}"))?;

    Ok(())
}

pub async fn sync_tasks_for_all_users(
    user_service: Arc<RwLock<UserService>>,
    task_service: Arc<RwLock<TaskService>>,
    source: &Option<TaskSyncSourceKind>,
) -> Result<(), UniversalInboxError> {
    let service = user_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while syncing tasks for all users")?;
    let users = service.fetch_all_users(&mut transaction).await?;
    for user in users {
        info!("Syncing tasks for user {}", user.id);
        match sync_tasks(task_service.clone(), source, user.id).await {
            Ok(_) => info!("Tasks successfully synced for user {}", user.id),
            Err(err) => error!("Failed to sync tasks for user {}: {:?}", user.id, err),
        };
    }
    Ok(())
}

pub async fn sync_tasks(
    task_service: Arc<RwLock<TaskService>>,
    source: &Option<TaskSyncSourceKind>,
    user_id: UserId,
) -> Result<(), UniversalInboxError> {
    let service = task_service.read().await;
    let mut transaction = service.begin().await.context(format!(
        "Failed to create new transaction while syncing {:?}",
        &source
    ))?;

    service
        .sync_tasks(&mut transaction, source, user_id)
        .await
        .context(format!("Failed to sync {:?}", &source))?;

    transaction
        .commit()
        .await
        .context(format!("Failed to commit while syncing {:?}", &source))?;

    Ok(())
}
