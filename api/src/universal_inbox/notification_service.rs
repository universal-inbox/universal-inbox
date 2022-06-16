use super::{NotificationRepository, UniversalInboxError};
use universal_inbox::Notification;

pub struct NotificationService {
    repository: Box<dyn NotificationRepository>,
}

impl NotificationService {
    pub fn new(repository: Box<dyn NotificationRepository>) -> NotificationService {
        NotificationService { repository }
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn list_notifications(&self) -> Result<Vec<Notification>, UniversalInboxError> {
        self.repository.fetch_all().await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn get_notification(
        &self,
        id: uuid::Uuid,
    ) -> Result<Option<Notification>, UniversalInboxError> {
        self.repository.get_one(id).await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn create_notification(
        &self,
        notification: &Notification,
    ) -> Result<Notification, UniversalInboxError> {
        self.repository.create(notification).await
    }
}
