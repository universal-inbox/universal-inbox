use std::sync::Arc;

use anyhow::anyhow;
use slack_morphism::prelude::*;
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;
use tracing::warn;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use universal_inbox::integration_connection::{
    integrations::slack::{SlackConfig, SlackReactionConfig, SlackSyncType},
    provider::{IntegrationProvider, IntegrationProviderKind},
};

use crate::universal_inbox::{
    UniversalInboxError,
    integration_connection::service::IntegrationConnectionService,
    notification::{NotificationEventService, service::NotificationService},
    task::{TaskEventService, service::TaskService},
};

#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn handle_slack_reaction_push_event(
    executor: &mut Transaction<'_, Postgres>,
    event: &SlackPushEventCallback,
    provider_user_id: String,
    notification_service: Arc<RwLock<NotificationService>>,
    task_service: Arc<RwLock<TaskService>>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
) -> Result<(), UniversalInboxError> {
    let current_span = tracing::Span::current();
    current_span.set_attribute("slack.provider_user_id", provider_user_id.clone());

    let Some(integration_connection) = integration_connection_service
        .read()
        .await
        .get_integration_connection_per_provider_user_id(
            executor,
            IntegrationProviderKind::Slack,
            provider_user_id.clone(),
        )
        .await?
    else {
        current_span.set_attribute("slack.reaction.outcome", "discarded");
        current_span.set_attribute("slack.reaction.discard_reason", "no_integration_connection");
        warn!("Validated integration connection not found for Slack user id {provider_user_id}");
        return Ok(());
    };

    let IntegrationProvider::Slack {
        config: slack_config,
        ..
    } = &integration_connection.provider
    else {
        return Err(UniversalInboxError::Unexpected(anyhow!(
            "Integration connection {} provider is supposed to be Slack",
            integration_connection.id
        )));
    };

    let user_id = integration_connection.user_id;
    let integration_connection_id = integration_connection.id;
    current_span.set_attribute("user.id", user_id.to_string());
    current_span.set_attribute(
        "slack.integration_connection_id",
        integration_connection_id.to_string(),
    );

    match slack_config {
        SlackConfig {
            reaction_config:
                SlackReactionConfig {
                    sync_enabled: true,
                    sync_type: SlackSyncType::AsTasks(_),
                    ..
                },
            ..
        } => {
            current_span.set_attribute("slack.sync_type", "as_tasks");
            current_span.set_attribute("slack.reaction.outcome", "processed");
            task_service
                .read()
                .await
                .save_task_from_event(executor, event, user_id)
                .await
                .map(|_| ())
        }

        SlackConfig {
            reaction_config:
                SlackReactionConfig {
                    sync_enabled: true,
                    sync_type: SlackSyncType::AsNotifications,
                    ..
                },
            ..
        } => {
            current_span.set_attribute("slack.sync_type", "as_notifications");
            current_span.set_attribute("slack.reaction.outcome", "processed");
            notification_service
                .read()
                .await
                .save_notification_from_event(executor, event, None, user_id)
                .await
                .map(|_| ())
        }

        _ => {
            current_span.set_attribute("slack.reaction.outcome", "discarded");
            current_span.set_attribute("slack.reaction.discard_reason", "sync_disabled");
            warn!(
                "Slack reaction sync was not enabled for integration connection {integration_connection_id}"
            );
            Ok(())
        }
    }
}
