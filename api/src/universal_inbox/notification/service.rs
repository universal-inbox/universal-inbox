use super::{
    super::{NotificationRepository, UniversalInboxError},
    source::NotificationSource,
};
use crate::integrations::github::GithubService;
use futures::stream::{self, StreamExt, TryStreamExt};
use universal_inbox::{
    integrations::github::GithubNotification, Notification, NotificationKind, NotificationStatus,
};
use uuid::Uuid;

pub struct NotificationService {
    pub repository: Box<dyn NotificationRepository>,
    github_service: GithubService,
    page_size: usize,
}

impl NotificationService {
    pub fn new(
        repository: Box<dyn NotificationRepository>,
        github_service: GithubService,
        page_size: usize,
    ) -> Result<NotificationService, UniversalInboxError> {
        Ok(NotificationService {
            repository,
            github_service,
            page_size,
        })
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn list_notifications(&self) -> Result<Vec<Notification>, UniversalInboxError> {
        self.repository.fetch_all().await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn get_notification(
        &self,
        id: Uuid,
    ) -> Result<Option<Notification>, UniversalInboxError> {
        self.repository.get_one(id).await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn create_notification(
        &self,
        notification: &Notification,
    ) -> Result<Notification, UniversalInboxError> {
        self.repository.create(notification.clone()).await
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
                .github_service
                .fetch_notifications(page, self.page_size)
                .await;

            response.map(|github_notifs| {
                let notifs_count = github_notifs.len();
                let is_last_page = notifs_count < self.page_size;
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
                self.repository.create_or_update(Notification {
                    id: Uuid::new_v4(),
                    title: github_notif.subject.title.clone(),
                    kind: NotificationKind::Github,
                    source_id: github_notification_id,
                    status: if github_notif.unread {
                        NotificationStatus::Unread
                    } else {
                        NotificationStatus::Read
                    },
                    metadata: github_notif.clone(),
                    updated_at: github_notif.updated_at,
                    last_read_at: github_notif.last_read_at,
                })
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
                NotificationStatus::Done,
            )
            .await?;

        Ok(notifications)
    }
}