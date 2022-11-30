use async_trait::async_trait;

use universal_inbox::Notification;

use crate::universal_inbox::{notification::source::NotificationSourceKind, UniversalInboxError};

pub mod github;
pub mod todoist;

pub trait SourceNotification {
    fn get_id(&self) -> String;
}

#[async_trait]
pub trait NotificationSourceService<T: SourceNotification> {
    async fn fetch_all_notifications(&self) -> Result<Vec<T>, UniversalInboxError>;
    fn build_notification(&self, source: &T) -> Box<Notification>;
    fn get_notification_source_kind(&self) -> NotificationSourceKind;
    async fn delete_notification_from_source(
        &self,
        source_id: &str,
    ) -> Result<(), UniversalInboxError>;
    async fn unsubscribe_notification_from_source(
        &self,
        source_id: &str,
    ) -> Result<(), UniversalInboxError>;
}
