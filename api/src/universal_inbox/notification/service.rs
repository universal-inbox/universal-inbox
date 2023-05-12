use std::{
    fmt::Debug,
    sync::{Arc, Weak},
};

use anyhow::{anyhow, Context};
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;

use universal_inbox::{
    notification::{
        IntoNotification, Notification, NotificationId, NotificationMetadata, NotificationPatch,
        NotificationStatus, NotificationWithTask,
    },
    task::{TaskCreation, TaskId, TaskPatch, TaskStatus},
    user::UserId,
};

use crate::{
    integrations::{
        github::GithubService,
        notification::{
            NotificationSourceKind, NotificationSourceService, NotificationSyncSourceKind,
        },
    },
    repository::{notification::NotificationRepository, Repository},
    universal_inbox::{
        integration_connection::service::IntegrationConnectionService, task::service::TaskService,
        UniversalInboxError, UpdateStatus,
    },
};

#[derive(Debug)]
pub struct NotificationService {
    repository: Arc<Repository>,
    github_service: GithubService,
    task_service: Weak<RwLock<TaskService>>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
}

impl NotificationService {
    pub fn new(
        repository: Arc<Repository>,
        github_service: GithubService,
        task_service: Weak<RwLock<TaskService>>,
        integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    ) -> NotificationService {
        NotificationService {
            repository,
            github_service,
            task_service,
            integration_connection_service,
        }
    }

    pub fn set_task_service(&mut self, task_service: Weak<RwLock<TaskService>>) {
        self.task_service = task_service;
    }

    pub async fn begin(&self) -> Result<Transaction<Postgres>, UniversalInboxError> {
        self.repository.begin().await
    }

    #[tracing::instrument(level = "debug", skip(notification_source_service))]
    pub async fn apply_updated_notification_side_effect<'a, T>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification_source_service: &dyn NotificationSourceService<T>,
        patch: &NotificationPatch,
        notification: Box<Notification>,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let access_token = self
            .integration_connection_service
            .read()
            .await
            .get_access_token(
                executor,
                notification_source_service.get_integration_provider_kind(),
                user_id,
            )
            .await?;

        match patch.status {
            Some(NotificationStatus::Deleted) => {
                let access_token = access_token.ok_or_else(|| {
                    anyhow!(
                        "Cannot delete notification {} without an access token",
                        notification.id
                    )
                })?;
                notification_source_service
                    .delete_notification_from_source(&notification.source_id, &access_token)
                    .await
            }
            Some(NotificationStatus::Unsubscribed) => {
                let access_token = access_token.ok_or_else(|| {
                    anyhow!(
                        "Cannot unsubscribe from notification {} without an access token",
                        notification.id
                    )
                })?;
                notification_source_service
                    .unsubscribe_notification_from_source(&notification.source_id, &access_token)
                    .await
            }
            _ => Ok(()),
        }
    }
}

impl NotificationService {
    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn list_notifications<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        status: NotificationStatus,
        include_snoozed_notifications: bool,
        task_id: Option<TaskId>,
        user_id: UserId,
    ) -> Result<Vec<NotificationWithTask>, UniversalInboxError> {
        self.repository
            .fetch_all_notifications(
                executor,
                status,
                include_snoozed_notifications,
                task_id,
                user_id,
            )
            .await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn get_notification<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification_id: NotificationId,
        for_user_id: UserId,
    ) -> Result<Option<Notification>, UniversalInboxError> {
        let notification = self
            .repository
            .get_one_notification(executor, notification_id)
            .await?;

        if let Some(ref notif) = notification {
            if notif.user_id != for_user_id {
                return Err(UniversalInboxError::Forbidden(format!(
                    "Only the owner of the notification {notification_id} can access it"
                )));
            }
        }

        Ok(notification)
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn create_notification<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification: Box<Notification>,
        for_user_id: UserId,
    ) -> Result<Box<Notification>, UniversalInboxError> {
        if notification.user_id != for_user_id {
            return Err(UniversalInboxError::Forbidden(format!(
                "A notification can only be created for {for_user_id}"
            )));
        }

        self.repository
            .create_notification(executor, notification)
            .await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn create_or_update_notification<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification: Box<Notification>,
    ) -> Result<Box<Notification>, UniversalInboxError> {
        self.repository
            .create_or_update_notification(executor, notification)
            .await
            .map(Box::new)
    }

    async fn sync_source_notifications<'a, T: Debug + IntoNotification>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification_source_service: &dyn NotificationSourceService<T>,
        user_id: UserId,
    ) -> Result<Vec<Notification>, UniversalInboxError> {
        let access_token = self
            .integration_connection_service
            .read()
            .await
            .get_access_token(
                executor,
                notification_source_service.get_integration_provider_kind(),
                user_id,
            )
            .await?;

        if let Some(access_token) = access_token {
            let source_notifications = notification_source_service
                .fetch_all_notifications(&access_token)
                .await?;
            self.save_notifications_from_source(
                executor,
                notification_source_service.get_notification_source_kind(),
                source_notifications,
                user_id,
            )
            .await
        } else {
            Ok(vec![])
        }
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn save_notifications_from_source<'a, T: Debug + IntoNotification>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification_source_kind: NotificationSourceKind,
        source_items: Vec<T>,
        user_id: UserId,
    ) -> Result<Vec<Notification>, UniversalInboxError> {
        let mut notifications = vec![];
        for source_item in source_items {
            let notification = source_item.into_notification(user_id);
            let uptodate_notification = self
                .repository
                .create_or_update_notification(executor, Box::new(notification))
                .await?;
            notifications.push(uptodate_notification);
        }

        let source_notification_ids = notifications
            .iter()
            .map(|notification| notification.source_id.clone())
            .collect::<Vec<String>>();

        self.repository
            .update_stale_notifications_status_from_source_ids(
                executor,
                source_notification_ids,
                notification_source_kind,
                NotificationStatus::Deleted,
            )
            .await?;

        Ok(notifications)
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn sync_notifications<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        source: &Option<NotificationSyncSourceKind>,
        user_id: UserId,
    ) -> Result<Vec<Notification>, UniversalInboxError> {
        match source {
            Some(NotificationSyncSourceKind::Github) => {
                self.sync_source_notifications(executor, &self.github_service, user_id)
                    .await
            }
            None => {
                let notifications_from_github = self
                    .sync_source_notifications(executor, &self.github_service, user_id)
                    .await?;
                Ok(notifications_from_github)
            }
        }
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn patch_notification<'a, 'b>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification_id: NotificationId,
        patch: &'b NotificationPatch,
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<Notification>>, UniversalInboxError> {
        let updated_notification = self
            .repository
            .update_notification(executor, notification_id, patch, for_user_id)
            .await?;

        match updated_notification {
            UpdateStatus {
                updated: true,
                result: Some(ref notification),
            } => {
                match notification.metadata {
                    NotificationMetadata::Github(_) => {
                        self.apply_updated_notification_side_effect(
                            executor,
                            &self.github_service,
                            patch,
                            notification.clone(),
                            for_user_id,
                        )
                        .await?;
                    }
                    NotificationMetadata::Todoist => {
                        if let Some(NotificationStatus::Deleted) = patch.status {
                            if let Some(task_id) = notification.task_id {
                                self.task_service
                                    .upgrade()
                                    .context(
                                        "Unable to access task_service from notification_service",
                                    )?
                                    .read()
                                    .await
                                    .patch_task(
                                        executor,
                                        task_id,
                                        &TaskPatch {
                                            status: Some(TaskStatus::Deleted),
                                            ..Default::default()
                                        },
                                        for_user_id,
                                    )
                                    .await?;
                            } else {
                                return Err(UniversalInboxError::Unexpected(anyhow!(
                                "Todoist notification {notification_id} is expected to be linked to a task"
                            )));
                            }
                        } else {
                            return Err(UniversalInboxError::UnsupportedAction(format!(
                        "Cannot update the status of Todoist notification {notification_id}, update task's project"
                    )));
                        }
                    }
                };

                if let Some(task_id) = patch.task_id {
                    self.task_service
                        .upgrade()
                        .context("Unable to access task_service from notification_service")?
                        .read()
                        .await
                        .associate_notification_with_task(
                            executor,
                            notification,
                            task_id,
                            for_user_id,
                        )
                        .await?;
                }
            }
            UpdateStatus {
                updated: false,
                result: None,
            } => {
                if self
                    .repository
                    .does_notification_exist(executor, notification_id)
                    .await?
                {
                    return Err(UniversalInboxError::Forbidden(format!(
                        "Only the owner of the notification {notification_id} can patch it"
                    )));
                }
            }
            _ => {}
        }

        Ok(updated_notification)
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn patch_notifications_for_task<'a, 'b>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        task_id: TaskId,
        notification_kind: Option<NotificationSourceKind>,
        patch: &'b NotificationPatch,
    ) -> Result<Vec<UpdateStatus<Notification>>, UniversalInboxError> {
        self.repository
            .update_notifications_for_task(executor, task_id, notification_kind, patch)
            .await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn create_task_from_notification<'a, 'b>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification_id: NotificationId,
        task_creation: &'b TaskCreation,
        for_user_id: UserId,
    ) -> Result<Option<NotificationWithTask>, UniversalInboxError> {
        let delete_patch = NotificationPatch {
            status: Some(NotificationStatus::Deleted),
            ..Default::default()
        };
        let updated_notification = self
            .patch_notification(executor, notification_id, &delete_patch, for_user_id)
            .await?;

        if let UpdateStatus {
            updated: true,
            result: Some(ref notification),
        } = updated_notification
        {
            let task = self
                .task_service
                .upgrade()
                .context("Unable to access task_service from notification_service")?
                .read()
                .await
                .create_task_from_notification(executor, task_creation, notification, for_user_id)
                .await?;

            return Ok(Some(NotificationWithTask::build(notification, Some(*task))));
        }

        Ok(None)
    }
}
