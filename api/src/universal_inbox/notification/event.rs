use async_trait::async_trait;
use slack_morphism::prelude::*;
use sqlx::{Postgres, Transaction};

use universal_inbox::{
    integration_connection::provider::IntegrationProviderKind,
    notification::{
        integrations::slack::SlackPushEventCallbackExt, Notification, NotificationSourceKind,
    },
};

use crate::universal_inbox::{
    notification::{service::NotificationService, NotificationEventService},
    UniversalInboxError, UpsertStatus,
};

#[async_trait]
impl NotificationEventService<SlackPushEventCallback> for NotificationService {
    #[tracing::instrument(level = "debug", skip(self, executor), err)]
    async fn save_notification_from_event<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        event: SlackPushEventCallback,
    ) -> Result<Vec<UpsertStatus<Box<Notification>>>, UniversalInboxError> {
        let provider_user_id = match &event {
            SlackPushEventCallback {
                event: SlackEventCallbackBody::StarAdded(SlackStarAddedEvent { user, .. }),
                ..
            } => user.to_string(),
            SlackPushEventCallback {
                event: SlackEventCallbackBody::StarRemoved(SlackStarRemovedEvent { user, .. }),
                ..
            } => user.to_string(),
            _ => {
                return Err(UniversalInboxError::UnsupportedAction(format!(
                    "Unsupported Slack event {event:?}"
                )))
            }
        };

        let integration_connection = self
            .integration_connection_service
            .read()
            .await
            .get_integration_connection_per_provider_user_id(
                executor,
                IntegrationProviderKind::Slack,
                provider_user_id.clone(),
            )
            .await?
            .ok_or_else(|| {
                UniversalInboxError::UnsupportedAction(format!(
                    "Integration connection not found for Slack user id {provider_user_id}"
                ))
            })?;

        let notification = event.into_notification(integration_connection.user_id)?;

        self.save_notifications_from_source(
            executor,
            NotificationSourceKind::Slack,
            vec![notification],
            false,
            false,
        )
        .await
    }
}
