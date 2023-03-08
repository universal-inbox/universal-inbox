use std::{
    fmt::Debug,
    sync::{Arc, Weak},
};

use anyhow::{anyhow, Context};
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;

use universal_inbox::{
    notification::{
        Notification, NotificationId, NotificationMetadata, NotificationPatch, NotificationStatus,
        NotificationWithTask,
    },
    task::{TaskCreation, TaskId, TaskPatch, TaskStatus},
};

use crate::{
    integrations::{
        github::GithubService,
        notification::{
            NotificationSourceKind, NotificationSourceService, NotificationSyncSourceKind,
        },
    },
    repository::{notification::NotificationRepository, Repository},
    universal_inbox::{task::service::TaskService, UniversalInboxError, UpdateStatus},
};

#[derive(Debug)]
pub struct NotificationService {
    repository: Arc<Repository>,
    github_service: GithubService,
    task_service: Weak<RwLock<TaskService>>,
}

impl NotificationService {
    pub fn new(
        repository: Arc<Repository>,
        github_service: GithubService,
        task_service: Weak<RwLock<TaskService>>,
    ) -> NotificationService {
        NotificationService {
            repository,
            github_service,
            task_service,
        }
    }

    pub fn set_task_service(&mut self, task_service: Weak<RwLock<TaskService>>) {
        self.task_service = task_service;
    }

    pub async fn begin(&self) -> Result<Transaction<Postgres>, UniversalInboxError> {
        self.repository.begin().await
    }

    #[tracing::instrument(level = "debug", skip(notification_source_service))]
    pub async fn apply_updated_notification_side_effect<T>(
        notification_source_service: &dyn NotificationSourceService<T>,
        patch: &NotificationPatch,
        notification: Box<Notification>,
    ) -> Result<(), UniversalInboxError> {
        match patch.status {
            Some(NotificationStatus::Deleted) => {
                notification_source_service
                    .delete_notification_from_source(&notification.source_id)
                    .await
            }
            Some(NotificationStatus::Unsubscribed) => {
                notification_source_service
                    .unsubscribe_notification_from_source(&notification.source_id)
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
    ) -> Result<Vec<NotificationWithTask>, UniversalInboxError> {
        self.repository
            .fetch_all_notifications(executor, status, include_snoozed_notifications, task_id)
            .await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn get_notification<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification_id: NotificationId,
    ) -> Result<Option<Notification>, UniversalInboxError> {
        self.repository
            .get_one_notification(executor, notification_id)
            .await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn create_notification<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification: Box<Notification>,
    ) -> Result<Box<Notification>, UniversalInboxError> {
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

    async fn sync_source_notifications<'a, T: Debug>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification_source_service: &dyn NotificationSourceService<T>,
    ) -> Result<Vec<Notification>, UniversalInboxError>
    where
        Notification: From<T>,
    {
        let source_notifications = notification_source_service
            .fetch_all_notifications()
            .await?;
        self.save_notifications_from_source(
            executor,
            notification_source_service.get_notification_source_kind(),
            source_notifications,
        )
        .await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn save_notifications_from_source<'a, T: Debug>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification_source_kind: NotificationSourceKind,
        source_items: Vec<T>,
    ) -> Result<Vec<Notification>, UniversalInboxError>
    where
        Notification: From<T>,
    {
        let mut notifications = vec![];
        for source_item in source_items {
            let notification = source_item.into();
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
    ) -> Result<Vec<Notification>, UniversalInboxError> {
        match source {
            Some(NotificationSyncSourceKind::Github) => {
                self.sync_source_notifications(executor, &self.github_service)
                    .await
            }
            None => {
                let notifications_from_github = self
                    .sync_source_notifications(executor, &self.github_service)
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
    ) -> Result<UpdateStatus<Box<Notification>>, UniversalInboxError> {
        let updated_notification = self
            .repository
            .update_notification(executor, notification_id, patch)
            .await?;

        if let UpdateStatus {
            updated: true,
            result: Some(ref notification),
        } = updated_notification
        {
            match notification.metadata {
                NotificationMetadata::Github(_) => {
                    NotificationService::apply_updated_notification_side_effect(
                        &self.github_service,
                        patch,
                        notification.clone(),
                    )
                    .await?;
                }
                NotificationMetadata::Todoist => {
                    if let Some(NotificationStatus::Deleted) = patch.status {
                        if let Some(task_id) = notification.task_id {
                            self.task_service
                                .upgrade()
                                .context("Unable to access task_service from notification_service")?
                                .read()
                                .await
                                .patch_task(
                                    executor,
                                    task_id,
                                    &TaskPatch {
                                        status: Some(TaskStatus::Deleted),
                                        ..Default::default()
                                    },
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
                    .associate_notification_with_task(executor, notification, task_id)
                    .await?;
            }
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
    ) -> Result<Option<NotificationWithTask>, UniversalInboxError> {
        let delete_patch = NotificationPatch {
            status: Some(NotificationStatus::Deleted),
            ..Default::default()
        };
        let updated_notification = self
            .patch_notification(executor, notification_id, &delete_patch)
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
                .create_task_from_notification(executor, task_creation, notification)
                .await?;

            return Ok(Some(NotificationWithTask::build(notification, Some(*task))));
        }

        Ok(None)
    }
}
