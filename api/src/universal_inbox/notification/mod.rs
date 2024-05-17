use async_trait::async_trait;
use sqlx::{Postgres, Transaction};

use universal_inbox::{notification::Notification, user::UserId};

use crate::universal_inbox::UniversalInboxError;

pub mod event;
pub mod service;

#[async_trait]
pub trait NotificationEventService<T> {
    async fn save_notification_from_event<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        event: T,
        user_id: UserId,
    ) -> Result<Option<Notification>, UniversalInboxError>;
}
