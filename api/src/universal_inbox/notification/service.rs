use std::sync::Arc;

use anyhow::anyhow;
use duplicate::duplicate_item;
use futures::stream::{self, StreamExt, TryStreamExt};
use uuid::Uuid;

use crate::{
    integrations::github::{self, GithubService},
    repository::notification::{
        ConnectedNotificationRepository, NotificationRepository,
        TransactionalNotificationRepository,
    },
    universal_inbox::{UniversalInboxError, UpdateStatus},
};
use universal_inbox::{
    integrations::github::GithubNotification, Notification, NotificationKind, NotificationPatch,
    NotificationStatus,
};

use super::source::NotificationSource;

pub struct NotificationService {
    repository: Box<NotificationRepository>,
    github_service: GithubService,
    page_size: usize,
}

impl NotificationService {
    pub fn new(
        repository: Box<NotificationRepository>,
        github_service: GithubService,
        page_size: usize,
    ) -> Result<NotificationService, UniversalInboxError> {
        Ok(NotificationService {
            repository,
            github_service,
            page_size,
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
    ) -> Result<Vec<Notification>, UniversalInboxError> {
        self.repository.fetch_all(status).await
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
    ) -> Result<Notification, UniversalInboxError> {
        self.repository.create(notification).await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn sync_notifications(
        &self,
        source: &Option<NotificationSource>,
    ) -> Result<Vec<Notification>, UniversalInboxError> {
        let all_github_notifications = stream::try_unfold((1, false), |(page, stop)| async move {
            if stop {
                return Ok(None);
            }

            let response = self
                .service
                .github_service
                .fetch_notifications(page, self.service.page_size)
                .await;

            response.map(|github_notifs| {
                let notifs_count = github_notifs.len();
                let is_last_page = notifs_count < self.service.page_size;
                Some((github_notifs, (page + 1, is_last_page)))
            })
        })
        .try_collect::<Vec<Vec<GithubNotification>>>()
        .await?
        .into_iter()
        .flatten()
        .collect::<Vec<GithubNotification>>();

        let notifications = stream::iter(&all_github_notifications)
            .then(|github_notif| {
                let github_notification_id = github_notif.id.to_string();
                let source_html_url = github::get_html_url_from_api_url(&github_notif.subject.url);

                self.repository.create_or_update(Box::new(Notification {
                    id: Uuid::new_v4(),
                    title: github_notif.subject.title.clone(),
                    kind: NotificationKind::Github,
                    source_id: github_notification_id,
                    source_html_url,
                    status: if github_notif.unread {
                        NotificationStatus::Unread
                    } else {
                        NotificationStatus::Read
                    },
                    metadata: github_notif.clone(),
                    updated_at: github_notif.updated_at,
                    last_read_at: github_notif.last_read_at,
                }))
            })
            .collect::<Vec<Result<Notification, UniversalInboxError>>>()
            .await
            .into_iter()
            .collect::<Result<Vec<Notification>, UniversalInboxError>>()?;

        let all_github_notification_ids = all_github_notifications
            .into_iter()
            .map(|github_notif| github_notif.id)
            .collect::<Vec<String>>();

        self.repository
            .update_stale_notifications_status_from_source_ids(
                all_github_notification_ids,
                NotificationStatus::Deleted,
            )
            .await?;

        Ok(notifications)
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
            match patch.status {
                Some(NotificationStatus::Deleted) => {
                    self.service
                        .github_service
                        .mark_thread_as_read(&notification.source_id)
                        .await?;
                }
                Some(NotificationStatus::Unsubscribed) => {
                    self.service
                        .github_service
                        .unsubscribe_from_thread(&notification.source_id)
                        .await?;
                }
                _ => {}
            }
        }

        Ok(updated_notification)
    }
}
