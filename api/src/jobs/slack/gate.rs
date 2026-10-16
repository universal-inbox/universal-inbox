//! The Slack webhook gate: which pushed events are worth a job.
//!
//! The webhook receives every message of every channel the app is in: the gate runs before any
//! Slack API call, and only queues events that may notify someone.

use std::{borrow::Cow, sync::Arc};

use anyhow::Context;

use slack_morphism::prelude::*;
use tokio::sync::RwLock;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::slack::{SlackConfig, SlackReactionConfig},
        provider::IntegrationProviderKind,
    },
    third_party::item::ThirdPartyItemKind,
};

use crate::{
    integrations::slack::{edited_slack_message_event, has_slack_references_in_message},
    universal_inbox::{
        UniversalInboxError, integration_connection::service::IntegrationConnectionService,
        third_party::service::ThirdPartyItemService,
    },
};

/// What the webhook does with an event, with a low-cardinality reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlackGateDecision {
    Queued(&'static str),
    Discarded(&'static str),
}

/// A gate decision, with the event it is about.
#[derive(Debug)]
pub struct SlackGatedEvent<'a> {
    pub decision: SlackGateDecision,
    /// The event to queue: for an edit (`message_changed`), the message event of the edited
    /// message (see [`edited_slack_message_event`]), otherwise the pushed event.
    pub event: Cow<'a, SlackPushEventCallback>,
}

/// Decide whether a Slack event callback becomes a job. The callbacks the webhook handles
/// itself (app uninstall, token revocation) never reach the gate.
///
/// `raw_payload` is the webhook body, read for `message_changed` events only.
pub async fn decide_slack_event_gate<'a>(
    event: &'a SlackPushEventCallback,
    raw_payload: &[u8],
    integration_connection_service: &Arc<RwLock<IntegrationConnectionService>>,
    third_party_item_service: &Arc<RwLock<ThirdPartyItemService>>,
) -> Result<SlackGatedEvent<'a>, UniversalInboxError> {
    if let SlackEventCallbackBody::Message(
        message @ SlackMessageEvent {
            subtype: Some(SlackMessageEventType::MessageChanged),
            ..
        },
    ) = &event.event
    {
        let raw_payload: serde_json::Value = serde_json::from_slice(raw_payload)
            .context("Failed to deserialize Slack message_changed event")?;
        return Ok(match edited_slack_message_event(message, &raw_payload) {
            Ok(edited_message) => SlackGatedEvent {
                decision: decide_message_gate(&edited_message, third_party_item_service).await?,
                event: Cow::Owned(SlackPushEventCallback {
                    event: SlackEventCallbackBody::Message(edited_message),
                    ..event.clone()
                }),
            },
            Err(discard_reason) => SlackGatedEvent {
                decision: SlackGateDecision::Discarded(discard_reason),
                event: Cow::Borrowed(event),
            },
        });
    }

    let decision = decide_event_gate(
        event,
        integration_connection_service,
        third_party_item_service,
    )
    .await?;
    Ok(SlackGatedEvent {
        decision,
        event: Cow::Borrowed(event),
    })
}

async fn decide_event_gate(
    event: &SlackPushEventCallback,
    integration_connection_service: &Arc<RwLock<IntegrationConnectionService>>,
    third_party_item_service: &Arc<RwLock<ThirdPartyItemService>>,
) -> Result<SlackGateDecision, UniversalInboxError> {
    match &event.event {
        SlackEventCallbackBody::ReactionAdded(SlackReactionAddedEvent {
            user, reaction, ..
        })
        | SlackEventCallbackBody::ReactionRemoved(SlackReactionRemovedEvent {
            user,
            reaction,
            ..
        }) => decide_reaction_gate(user, reaction, integration_connection_service).await,
        SlackEventCallbackBody::Message(message) => {
            decide_message_gate(message, third_party_item_service).await
        }
        _ => Ok(SlackGateDecision::Discarded("unknown_event_type")),
    }
}

async fn decide_reaction_gate(
    user: &SlackUserId,
    reaction: &SlackReactionName,
    integration_connection_service: &Arc<RwLock<IntegrationConnectionService>>,
) -> Result<SlackGateDecision, UniversalInboxError> {
    let service = integration_connection_service.read().await;
    // No `.context()`: keep a pool timeout as `DatabaseUnavailable` (503).
    let mut transaction = service.begin().await?;

    let config = service
        .get_integration_connection_config_for_provider_user_id(
            &mut transaction,
            IntegrationProviderKind::Slack,
            user.to_string(),
        )
        .await?;

    Ok(match config {
        Some(IntegrationConnectionConfig::Slack(SlackConfig {
            reaction_config:
                SlackReactionConfig {
                    sync_enabled: true,
                    reaction_name,
                    ..
                },
            ..
        })) if reaction_name == *reaction => SlackGateDecision::Queued("reaction_sync_enabled"),
        Some(IntegrationConnectionConfig::Slack(SlackConfig {
            reaction_config:
                SlackReactionConfig {
                    sync_enabled: false,
                    ..
                },
            ..
        })) => SlackGateDecision::Discarded("reaction_sync_disabled"),
        Some(IntegrationConnectionConfig::Slack(_)) => {
            SlackGateDecision::Discarded("reaction_name_mismatch")
        }
        _ => SlackGateDecision::Discarded("no_slack_config"),
    })
}

async fn decide_message_gate(
    message: &SlackMessageEvent,
    third_party_item_service: &Arc<RwLock<ThirdPartyItemService>>,
) -> Result<SlackGateDecision, UniversalInboxError> {
    let thread_ts = match decide_message_gate_without_db(message) {
        MessageGate::Decided(decision) => return Ok(decision),
        MessageGate::LookUpThread(thread_ts) => thread_ts,
    };
    let service = third_party_item_service.read().await;
    // A single read: use a plain connection (no BEGIN/ROLLBACK), returned to the pool before
    // the Redis job push. Pool exhaustion surfaces as `DatabaseUnavailable` (503) so Slack
    // retries the event.
    let mut connection = service.connect().await?;
    let is_known_thread = service
        .has_third_party_item_for_source_id(
            &mut connection,
            ThirdPartyItemKind::SlackThread,
            &thread_ts.0,
        )
        .await?;

    Ok(if is_known_thread {
        SlackGateDecision::Queued("known_thread")
    } else {
        SlackGateDecision::Discarded("no_references_no_known_thread")
    })
}

#[derive(Debug, PartialEq)]
enum MessageGate<'a> {
    Decided(SlackGateDecision),
    /// A thread reply without references: only the known thread lookup can decide it.
    LookUpThread(&'a SlackTs),
}

/// The cheap, DB-free part of the message gate.
fn decide_message_gate_without_db(message: &SlackMessageEvent) -> MessageGate<'_> {
    let Some(content) = &message.content else {
        return MessageGate::Decided(SlackGateDecision::Discarded("no_message_content"));
    };
    if has_slack_references_in_message(content) {
        return MessageGate::Decided(SlackGateDecision::Queued("has_references"));
    }
    match &message.origin.thread_ts {
        Some(thread_ts) => MessageGate::LookUpThread(thread_ts),
        None => MessageGate::Decided(SlackGateDecision::Discarded(
            "no_references_no_known_thread",
        )),
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;

    fn message_event(event: serde_json::Value) -> SlackMessageEvent {
        serde_json::from_value(event).unwrap()
    }

    // The known thread lookup is covered by `api/tests/api/test_slack_webhook_message.rs`.
    #[test]
    fn test_a_message_without_content_is_discarded() {
        let message = SlackMessageEvent {
            content: None,
            ..message_event(json!({
                "type": "message",
                "subtype": "message_deleted",
                "channel": "C01",
                "ts": "1700000002.000300",
                "deleted_ts": "1700000001.000200"
            }))
        };

        assert_eq!(
            decide_message_gate_without_db(&message),
            MessageGate::Decided(SlackGateDecision::Discarded("no_message_content"))
        );
    }

    #[test]
    fn test_a_message_with_references_is_queued() {
        let message = message_event(json!({
            "type": "message",
            "channel": "C01",
            "user": "U01",
            "text": "hi <@U02>",
            "ts": "1700000001.000200",
            "blocks": [{
                "type": "rich_text",
                "elements": [{
                    "type": "rich_text_section",
                    "elements": [{ "type": "user", "user_id": "U02" }]
                }]
            }]
        }));

        assert_eq!(
            decide_message_gate_without_db(&message),
            MessageGate::Decided(SlackGateDecision::Queued("has_references"))
        );
    }

    #[test]
    fn test_a_channel_message_without_references_is_discarded() {
        let message = message_event(json!({
            "type": "message",
            "channel": "C01",
            "user": "U01",
            "text": "hi",
            "ts": "1700000001.000200"
        }));

        assert_eq!(
            decide_message_gate_without_db(&message),
            MessageGate::Decided(SlackGateDecision::Discarded(
                "no_references_no_known_thread"
            ))
        );
    }

    #[test]
    fn test_a_thread_reply_without_references_needs_the_known_thread_lookup() {
        let message = message_event(json!({
            "type": "message",
            "channel": "C01",
            "user": "U01",
            "text": "hi",
            "ts": "1700000001.000200",
            "thread_ts": "1700000000.000100"
        }));

        assert_eq!(
            decide_message_gate_without_db(&message),
            MessageGate::LookUpThread(&SlackTs("1700000000.000100".to_string()))
        );
    }
}
