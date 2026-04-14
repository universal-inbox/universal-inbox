use std::sync::Arc;

use anyhow::Context;
use apalis::prelude::Data;
use serde::{Deserialize, Serialize};
use slack_morphism::prelude::*;
use tokio::sync::RwLock;

use tracing_opentelemetry::OpenTelemetrySpanExt;

use crate::{
    integrations::slack::SlackService,
    jobs::slack::{
        slack_message::handle_slack_message_push_event,
        slack_reaction::handle_slack_reaction_push_event,
    },
    universal_inbox::{
        UniversalInboxError, integration_connection::service::IntegrationConnectionService,
        notification::service::NotificationService, task::service::TaskService,
        third_party::service::ThirdPartyItemService,
    },
};

pub mod slack_message;
pub mod slack_reaction;

#[derive(Debug, Serialize, Deserialize)]
pub struct SlackPushEventCallbackJob(pub SlackPushEventCallback);

pub fn fail_if_needed<T>(
    result: Result<T, UniversalInboxError>,
) -> Result<Option<T>, UniversalInboxError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(UniversalInboxError::UnsupportedAction(_)) => Ok(None),
        Err(error) => Err(error),
    }
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn handle_slack_push_event(
    job: SlackPushEventCallbackJob,
    notification_service: Data<Arc<RwLock<NotificationService>>>,
    task_service: Data<Arc<RwLock<TaskService>>>,
    integration_connection_service: Data<Arc<RwLock<IntegrationConnectionService>>>,
    third_party_item_service: Data<Arc<RwLock<ThirdPartyItemService>>>,
    slack_service: Data<Arc<SlackService>>,
) -> Result<(), UniversalInboxError> {
    let current_span = tracing::Span::current();
    current_span.set_attribute("slack.team_id", job.0.team_id.to_string());
    current_span.set_attribute("slack.event_id", job.0.event_id.to_string());
    let event_type = match &job.0.event {
        SlackEventCallbackBody::ReactionAdded(_) => "reaction_added",
        SlackEventCallbackBody::ReactionRemoved(_) => "reaction_removed",
        SlackEventCallbackBody::Message(_) => "message",
        _ => "unknown",
    };
    current_span.set_attribute("slack.event_type", event_type);

    let service = notification_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while handling a Slack event")?;

    match &job.0 {
        event @ SlackPushEventCallback {
            event: SlackEventCallbackBody::ReactionAdded(SlackReactionAddedEvent { user, .. }),
            ..
        }
        | event @ SlackPushEventCallback {
            event: SlackEventCallbackBody::ReactionRemoved(SlackReactionRemovedEvent { user, .. }),
            ..
        } => {
            handle_slack_reaction_push_event(
                &mut transaction,
                event,
                user.to_string(),
                (*notification_service).clone(),
                (*task_service).clone(),
                (*integration_connection_service).clone(),
            )
            .await?
        }
        event @ SlackPushEventCallback {
            event: SlackEventCallbackBody::Message(_),
            ..
        } => {
            handle_slack_message_push_event(
                &mut transaction,
                event,
                (*notification_service).clone(),
                (*integration_connection_service).clone(),
                (*third_party_item_service).clone(),
                (*slack_service).clone(),
            )
            .await?
        }
        event => {
            return Err(UniversalInboxError::UnsupportedAction(format!(
                "Unsupported Slack event {event:?}"
            )));
        }
    };

    transaction
        .commit()
        .await
        .context("Failed to commit while handling a Slack event")?;

    Ok(())
}
