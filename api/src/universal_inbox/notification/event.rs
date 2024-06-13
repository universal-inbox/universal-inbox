use async_trait::async_trait;
use slack_morphism::prelude::*;
use sqlx::{Postgres, Transaction};

use universal_inbox::{
    notification::{integrations::slack::SlackPushEventCallbackExt, Notification},
    user::UserId,
};

use crate::universal_inbox::{
    notification::{service::NotificationService, NotificationEventService},
    UniversalInboxError,
};

#[async_trait]
impl NotificationEventService<SlackPushEventCallback> for NotificationService {
    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(level = "debug", skip(self, executor, event))]
    async fn save_notification_from_event<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        event: SlackPushEventCallback,
        user_id: UserId,
    ) -> Result<Option<Notification>, UniversalInboxError> {
        let notification = event.into_notification(user_id)?;
        let mut saved_notifications = self
            .save_notifications_and_sync_details(
                executor,
                self.slack_service.clone(),
                vec![notification],
                user_id,
            )
            .await?;
        Ok(saved_notifications.pop())
    }
}
