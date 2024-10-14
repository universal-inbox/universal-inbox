use async_trait::async_trait;
use slack_morphism::prelude::*;
use sqlx::{Postgres, Transaction};

use universal_inbox::{
    notification::{
        integrations::slack::SlackPushEventCallbackExt, Notification, NotificationDetails,
        NotificationSourceKind,
    },
    user::UserId,
    utils::truncate::truncate_with_ellipse,
};

use crate::{
    repository::notification::NotificationRepository,
    universal_inbox::{
        notification::{service::NotificationService, NotificationEventService},
        UniversalInboxError,
    },
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
        let Some(mut notification) = saved_notifications.pop() else {
            return Ok(None);
        };

        // This is a temporary workaround for Slack reaction as the message content is not
        // embedded in the event stored in notification.metadata and thus, the title of the
        // notification cannot be computed. To workaround it, here, we use the notification details
        // It should be fixed when moving NotificationDetails to ThirdPartyItem as there will
        // not be any `notification.metadata` anymore
        if let Some(NotificationDetails::SlackMessage(message)) = &notification.details {
            let message_content = message.content();
            notification.title = truncate_with_ellipse(&message_content, 50, "...", true);
            let upsert_result = self
                .repository
                .create_or_update_notification(
                    executor,
                    Box::new(notification),
                    NotificationSourceKind::Slack,
                    false,
                )
                .await?;
            return Ok(Some(*upsert_result.value()));
        }

        Ok(Some(notification))
    }
}
