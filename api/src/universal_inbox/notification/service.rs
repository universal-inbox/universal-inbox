use std::sync::Arc;

use anyhow::anyhow;
use duplicate::duplicate_item;
use futures::stream::{self, StreamExt};
use uuid::Uuid;

use crate::{
    integrations::{
        github::GithubService, todoist::TodoistService, NotificationSourceService,
        SourceNotification,
    },
    repository::notification::{
        ConnectedNotificationRepository, NotificationRepository,
        TransactionalNotificationRepository,
    },
    universal_inbox::{UniversalInboxError, UpdateStatus},
};
use universal_inbox::{Notification, NotificationMetadata, NotificationPatch, NotificationStatus};

use super::source::NotificationSourceKind;

pub struct NotificationService {
    repository: Box<NotificationRepository>,
    github_service: GithubService,
    todoist_service: TodoistService,
}

impl NotificationService {
    pub fn new(
        repository: Box<NotificationRepository>,
        github_service: GithubService,
        todoist_service: TodoistService,
    ) -> Result<NotificationService, UniversalInboxError> {
        Ok(NotificationService {
            repository,
            github_service,
            todoist_service,
        })
    }

    pub async fn connect(&self) -> Result<Box<ConnectedNotificationService>, UniversalInboxError> {
        Ok(Box::new(ConnectedNotificationService {
            repository: self.repository.connect().await?,
            service: self,
        }))
    }

    pub async fn begin(
        &self,
    ) -> Result<Box<TransactionalNotificationService>, UniversalInboxError> {
        Ok(Box::new(TransactionalNotificationService {
            repository: self.repository.begin().await?,
            service: self,
        }))
    }
}

pub struct ConnectedNotificationService<'a> {
    repository: Arc<ConnectedNotificationRepository>,
    service: &'a NotificationService,
}

pub struct TransactionalNotificationService<'a> {
    repository: Arc<TransactionalNotificationRepository<'a>>,
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
    ) -> Result<Vec<Notification>, UniversalInboxError> {
        self.repository
            .fetch_all(status, include_snoozed_notifications)
            .await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn get_notification(
        &self,
        notification_id: Uuid,
    ) -> Result<Option<Notification>, UniversalInboxError> {
        self.repository.get_one(notification_id).await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn create_notification(
        &self,
        notification: Box<Notification>,
    ) -> Result<Box<Notification>, UniversalInboxError> {
        self.repository.create(notification).await
    }

    async fn sync_source_notifications<T: SourceNotification>(
        &self,
        notification_source_service: &dyn NotificationSourceService<T>,
    ) -> Result<Vec<Notification>, UniversalInboxError> {
        let all_source_notifications = notification_source_service
            .fetch_all_notifications()
            .await?;

        let notifications = stream::iter(&all_source_notifications)
            .then(|github_notif| {
                self.repository
                    .create_or_update(notification_source_service.build_notification(github_notif))
            })
            .collect::<Vec<Result<Notification, UniversalInboxError>>>()
            .await
            .into_iter()
            .collect::<Result<Vec<Notification>, UniversalInboxError>>()?;

        let all_source_notification_ids = all_source_notifications
            .into_iter()
            .map(|source_notif| source_notif.get_id())
            .collect::<Vec<String>>();

        self.repository
            .update_stale_notifications_status_from_source_ids(
                all_source_notification_ids,
                notification_source_service.get_notification_source_kind(),
                NotificationStatus::Deleted,
            )
            .await?;

        Ok(notifications)
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn sync_notifications(
        &self,
        source: &Option<NotificationSourceKind>,
    ) -> Result<Vec<Notification>, UniversalInboxError> {
        match source {
            Some(NotificationSourceKind::Github) => {
                self.sync_source_notifications(&self.service.github_service)
                    .await
            }
            Some(NotificationSourceKind::Todoist) => {
                self.sync_source_notifications(&self.service.todoist_service)
                    .await
            }
            None => {
                let notifications_from_github = self
                    .sync_source_notifications(&self.service.github_service)
                    .await?;
                let notifications_from_todoist = self
                    .sync_source_notifications(&self.service.todoist_service)
                    .await?;
                Ok(notifications_from_github
                    .into_iter()
                    .chain(notifications_from_todoist.into_iter())
                    .collect())
            }
        }
    }

    async fn apply_updated_notification_side_effect<T: SourceNotification>(
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
        let updated_notification = self.repository.update(notification_id, patch).await?;

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
                NotificationMetadata::Todoist(_) => {
                    self.apply_updated_notification_side_effect(
                        &self.service.todoist_service,
                        patch,
                        notification.clone(),
                    )
                    .await?
                }
            };
        }

        Ok(updated_notification)
    }
}
