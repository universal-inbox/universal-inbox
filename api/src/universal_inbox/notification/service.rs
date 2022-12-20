use std::{fmt::Debug, sync::Arc};

use anyhow::anyhow;
use duplicate::duplicate_item;
use futures::stream::{self, StreamExt};
use uuid::Uuid;

use universal_inbox::notification::{
    Notification, NotificationMetadata, NotificationPatch, NotificationStatus,
};

use crate::{
    integrations::{
        github::GithubService,
        notification::{
            NotificationSourceKind, NotificationSourceService, NotificationSyncSourceKind,
        },
    },
    repository::{
        notification::NotificationRepository, ConnectedRepository, Repository,
        TransactionalRepository,
    },
    universal_inbox::{UniversalInboxError, UpdateStatus},
};

pub struct NotificationService {
    repository: Arc<Repository>,
    github_service: GithubService,
}

impl NotificationService {
    pub fn new(
        repository: Arc<Repository>,
        github_service: GithubService,
    ) -> Result<NotificationService, UniversalInboxError> {
        Ok(NotificationService {
            repository,
            github_service,
        })
    }

    pub async fn connect(&self) -> Result<Box<ConnectedNotificationService>, UniversalInboxError> {
        Ok(Box::new(ConnectedNotificationService {
            repository: self.repository.connect().await?,
            service: self,
        }))
    }

    pub fn connected_with(
        &self,
        repository: Arc<ConnectedRepository>,
    ) -> Box<ConnectedNotificationService> {
        Box::new(ConnectedNotificationService {
            repository,
            service: self,
        })
    }

    pub async fn begin(
        &self,
    ) -> Result<Box<TransactionalNotificationService>, UniversalInboxError> {
        Ok(Box::new(TransactionalNotificationService {
            repository: self.repository.begin().await?,
            service: self,
        }))
    }

    pub fn transactional_with<'a>(
        self: &'a NotificationService,
        repository: Arc<TransactionalRepository<'a>>,
    ) -> Box<TransactionalNotificationService> {
        Box::new(TransactionalNotificationService {
            repository,
            service: self,
        })
    }
}

pub struct ConnectedNotificationService<'a> {
    repository: Arc<ConnectedRepository>,
    service: &'a NotificationService,
}

pub struct TransactionalNotificationService<'a> {
    pub repository: Arc<TransactionalRepository<'a>>,
    service: &'a NotificationService,
}

impl<'a> TransactionalNotificationService<'a> {
    pub async fn commit(self) -> Result<(), UniversalInboxError> {
        let repository = Arc::try_unwrap(self.repository)
            .map_err(|_| {
                UniversalInboxError::Unexpected(anyhow!(
                    "Cannot extract repository to commit transaction it as it has other references using it"
                ))
            })?;

        repository.commit().await
    }
}

#[duplicate_item(notification_service; [ConnectedNotificationService]; [TransactionalNotificationService];)]
impl<'a> notification_service<'a> {
    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn list_notifications(
        &self,
        status: NotificationStatus,
        include_snoozed_notifications: bool,
        task_id: Option<Uuid>,
    ) -> Result<Vec<Notification>, UniversalInboxError> {
        self.repository
            .fetch_all_notifications(status, include_snoozed_notifications, task_id)
            .await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn get_notification(
        &self,
        notification_id: Uuid,
    ) -> Result<Option<Notification>, UniversalInboxError> {
        self.repository.get_one_notification(notification_id).await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn create_notification(
        &self,
        notification: Box<Notification>,
    ) -> Result<Box<Notification>, UniversalInboxError> {
        self.repository.create_notification(notification).await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn create_or_update_notification(
        &self,
        notification: Box<Notification>,
    ) -> Result<Box<Notification>, UniversalInboxError> {
        self.repository
            .create_or_update_notification(notification)
            .await
            .map(Box::new)
    }

    async fn sync_source_notifications<T: Debug>(
        &self,
        notification_source_service: &dyn NotificationSourceService<T>,
    ) -> Result<Vec<Notification>, UniversalInboxError>
    where
        Notification: From<T>,
    {
        let source_notifications = notification_source_service
            .fetch_all_notifications()
            .await?;
        self.save_notifications_from_source(
            notification_source_service.get_notification_source_kind(),
            source_notifications,
        )
        .await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn save_notifications_from_source<T: Debug>(
        &self,
        notification_source_kind: NotificationSourceKind,
        source_items: Vec<T>,
    ) -> Result<Vec<Notification>, UniversalInboxError>
    where
        Notification: From<T>,
    {
        let notifications = stream::iter(source_items)
            .then(|source_item| {
                let notification: Notification = source_item.into();
                self.repository
                    .create_or_update_notification(Box::new(notification))
            })
            .collect::<Vec<Result<Notification, UniversalInboxError>>>()
            .await
            .into_iter()
            .collect::<Result<Vec<Notification>, UniversalInboxError>>()?;

        let source_notification_ids = notifications
            .iter()
            .map(|notification| notification.source_id.clone())
            .collect::<Vec<String>>();

        self.repository
            .update_stale_notifications_status_from_source_ids(
                source_notification_ids,
                notification_source_kind,
                NotificationStatus::Deleted,
            )
            .await?;

        Ok(notifications)
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn sync_notifications(
        &self,
        source: &Option<NotificationSyncSourceKind>,
    ) -> Result<Vec<Notification>, UniversalInboxError> {
        match source {
            Some(NotificationSyncSourceKind::Github) => {
                self.sync_source_notifications(&self.service.github_service)
                    .await
            }
            None => {
                let notifications_from_github = self
                    .sync_source_notifications(&self.service.github_service)
                    .await?;
                Ok(notifications_from_github)
            }
        }
    }

    async fn apply_updated_notification_side_effect<T>(
        &self,
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

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn patch_notification(
        &self,
        notification_id: Uuid,
        patch: &NotificationPatch,
    ) -> Result<UpdateStatus<Box<Notification>>, UniversalInboxError> {
        let updated_notification = self
            .repository
            .update_notification(notification_id, patch)
            .await?;

        if let UpdateStatus {
            updated: true,
            result: Some(ref notification),
        } = updated_notification
        {
            match notification.metadata {
                NotificationMetadata::Github(_) => {
                    self.apply_updated_notification_side_effect(
                        &self.service.github_service,
                        patch,
                        notification.clone(),
                    )
                    .await?
                }
                NotificationMetadata::Todoist => {
                    return Err(UniversalInboxError::UnsupportedAction(format!(
                        "Cannot update the status of Todoist notification {notification_id}, update task's project"
                    )))
                }
            };
        }

        Ok(updated_notification)
    }
}
