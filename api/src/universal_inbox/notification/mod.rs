use async_trait::async_trait;
use sqlx::{Postgres, Transaction};

use universal_inbox::{
    notification::Notification, third_party::item::ThirdPartyItem, user::UserId,
};

use crate::universal_inbox::{UniversalInboxError, UpsertStatus};

pub mod event;
pub mod service;

/// What saving a notification from an event did.
#[derive(Debug)]
pub enum NotificationEventOutcome {
    /// The event is about nothing to notify (eg. a thread the user cannot see), with a
    /// low-cardinality reason.
    Skipped(&'static str),
    /// The item the event is about did not change: the notification was left as is.
    ItemUnchanged,
    Saved(UpsertStatus<Box<Notification>>),
}

#[async_trait]
pub trait NotificationEventService<T> {
    async fn save_notification_from_event(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        event: &T,
        existing_third_party_item: Option<&ThirdPartyItem>,
        user_id: UserId,
    ) -> Result<NotificationEventOutcome, UniversalInboxError>;
}
