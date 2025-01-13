use async_trait::async_trait;
use sqlx::{Postgres, Transaction};

use universal_inbox::{
    integration_connection::IntegrationConnection, notification::Notification,
    third_party::item::ThirdPartyItem, user::UserId,
};

use crate::universal_inbox::UniversalInboxError;

pub mod event;
pub mod service;

#[async_trait]
pub trait NotificationEventService<T> {
    async fn save_notification_from_event<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        event: &T,
        existing_third_party_item: Option<&ThirdPartyItem>,
        integration_connection: IntegrationConnection,
        user_id: UserId,
    ) -> Result<Option<Notification>, UniversalInboxError>;
}
