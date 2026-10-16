//! Identity of a Slack push event, shared by the webhook gate and the job worker so that every
//! span and log line about one Slack message carries the same keys.
//!
//! Spans are sampled by the trace backend, logs are not: each decision about an event (the
//! webhook gate, then one per user in the worker) is also logged once with these fields, so
//! `@slack.message.ts:<ts>` finds the whole trail of a message. A failure on the way is logged
//! with the same fields, so the trail never ends silently.

use slack_morphism::prelude::*;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use universal_inbox::{integration_connection::IntegrationConnectionId, user::UserId};

pub use crate::jobs::slack::gate::SlackGateDecision;
use crate::{
    jobs::slack::slack_message::SlackMessageDecision, observability::attr,
    universal_inbox::UniversalInboxError,
};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SlackEventTrace {
    pub event_id: String,
    pub team_id: String,
    pub event_type: String,
    /// `message` subtype, or the `subtype` of an event slack-morphism does not model
    pub subtype: Option<String>,
    pub channel_id: Option<String>,
    pub ts: Option<String>,
    pub thread_ts: Option<String>,
    /// Sender of a message, or author of a reaction. Recorded on spans only.
    pub slack_user_id: Option<String>,
    /// Recorded on spans only.
    pub reaction_name: Option<String>,
}

/// Log `$message` at `$level` with the identity fields of `$trace`, then `$fields`.
macro_rules! log_slack_event {
    ($level:ident, $trace:expr, $message:literal, $($fields:tt)*) => {
        tracing::$level!(
            { attr::SLACK_EVENT_ID } = $trace.event_id,
            { attr::SLACK_TEAM_ID } = $trace.team_id,
            { attr::SLACK_EVENT_TYPE } = $trace.event_type,
            { attr::SLACK_MESSAGE_SUBTYPE } = $trace.message_subtype(),
            { attr::SLACK_EVENT_SUBTYPE } = $trace.event_subtype(),
            { attr::SLACK_CHANNEL_ID } = $trace.channel_id,
            { attr::SLACK_MESSAGE_TS } = $trace.ts,
            { attr::SLACK_MESSAGE_THREAD_TS } = $trace.thread_ts,
            $($fields)*
            $message
        )
    };
}

impl SlackEventTrace {
    pub fn from_event(event: &SlackPushEventCallback) -> Self {
        let mut trace = SlackEventTrace {
            event_id: event.event_id.to_string(),
            team_id: event.team_id.to_string(),
            event_type: slack_event_type(&event.event),
            ..Default::default()
        };

        match &event.event {
            SlackEventCallbackBody::Message(SlackMessageEvent {
                origin,
                subtype,
                sender,
                ..
            }) => {
                trace.subtype = subtype.as_ref().and_then(serialized_str);
                trace.channel_id = origin.channel.as_ref().map(ToString::to_string);
                trace.ts = Some(origin.ts.to_string());
                trace.thread_ts = origin.thread_ts.as_ref().map(ToString::to_string);
                trace.slack_user_id = sender.user.as_ref().map(ToString::to_string);
            }
            SlackEventCallbackBody::ReactionAdded(SlackReactionAddedEvent {
                item,
                user,
                reaction,
                ..
            })
            | SlackEventCallbackBody::ReactionRemoved(SlackReactionRemovedEvent {
                item,
                user,
                reaction,
                ..
            }) => {
                if let SlackReactionsItem::Message(SlackHistoryMessage { origin, .. }) = item {
                    trace.channel_id = origin.channel.as_ref().map(ToString::to_string);
                    trace.ts = Some(origin.ts.to_string());
                    trace.thread_ts = origin.thread_ts.as_ref().map(ToString::to_string);
                }
                trace.slack_user_id = Some(user.to_string());
                trace.reaction_name = Some(reaction.to_string());
            }
            SlackEventCallbackBody::Unknown(body) => {
                trace.subtype = body
                    .get("subtype")
                    .and_then(|subtype| subtype.as_str())
                    .map(ToString::to_string);
            }
            _ => {}
        }

        trace
    }

    /// The trace of the message event rebuilt from a `message_changed` event: the edited
    /// message's identity, keeping the `message_changed` subtype of the pushed event.
    pub fn for_edited_event(&self, edited_event: &SlackPushEventCallback) -> Self {
        SlackEventTrace {
            subtype: self.subtype.clone(),
            ..SlackEventTrace::from_event(edited_event)
        }
    }

    fn is_message(&self) -> bool {
        self.event_type == "message"
    }

    fn message_subtype(&self) -> Option<&str> {
        self.subtype.as_deref().filter(|_| self.is_message())
    }

    fn event_subtype(&self) -> Option<&str> {
        self.subtype.as_deref().filter(|_| !self.is_message())
    }

    pub fn record_on(&self, span: &Span) {
        span.set_attribute(attr::SLACK_EVENT_ID, self.event_id.clone());
        span.set_attribute(attr::SLACK_TEAM_ID, self.team_id.clone());
        span.set_attribute(attr::SLACK_EVENT_TYPE, self.event_type.clone());
        let optional_attributes = [
            (attr::SLACK_MESSAGE_SUBTYPE, self.message_subtype()),
            (attr::SLACK_EVENT_SUBTYPE, self.event_subtype()),
            (attr::SLACK_CHANNEL_ID, self.channel_id.as_deref()),
            (attr::SLACK_MESSAGE_TS, self.ts.as_deref()),
            (attr::SLACK_MESSAGE_THREAD_TS, self.thread_ts.as_deref()),
            (attr::SLACK_USER_ID, self.slack_user_id.as_deref()),
            (attr::SLACK_REACTION_NAME, self.reaction_name.as_deref()),
        ];
        for (key, value) in optional_attributes {
            if let Some(value) = value {
                span.set_attribute(key, value.to_string());
            }
        }
    }

    /// Record the webhook gate decision on `span` and log it.
    pub fn record_gate_decision(&self, span: &Span, decision: SlackGateDecision) {
        let (outcome, queue_reason, discard_reason) = match decision {
            SlackGateDecision::Queued(reason) => ("queued", Some(reason), None),
            SlackGateDecision::Discarded(reason) => ("discarded", None, Some(reason)),
        };
        span.set_attribute(attr::SLACK_EVENT_OUTCOME, outcome);
        if let Some(reason) = queue_reason {
            span.set_attribute(attr::SLACK_EVENT_QUEUE_REASON, reason);
        }
        if let Some(reason) = discard_reason {
            span.set_attribute(attr::SLACK_EVENT_DISCARD_REASON, reason);
        }
        log_slack_event!(
            info,
            self,
            "Slack event gate decision",
            { attr::SLACK_EVENT_OUTCOME } = outcome,
            { attr::SLACK_EVENT_QUEUE_REASON } = queue_reason,
            { attr::SLACK_EVENT_DISCARD_REASON } = discard_reason,
        );
    }

    /// The webhook failed before deciding or queueing the event: Slack retries it on a 5xx.
    pub fn log_gate_failure(&self, error: &UniversalInboxError) {
        log_slack_event!(
            warn,
            self,
            "Slack event gate failed",
            { attr::SLACK_EVENT_OUTCOME } = "error",
            { attr::ERROR_TYPE } = error.error_type(),
        );
    }

    /// The worker decision for one user (or for the event, when no user is involved).
    pub fn log_user_decision(
        &self,
        decision: SlackMessageDecision,
        user_id: Option<UserId>,
        integration_connection_id: Option<IntegrationConnectionId>,
    ) {
        log_slack_event!(
            info,
            self,
            "Slack message notification decision",
            { attr::SLACK_MESSAGE_OUTCOME } = decision.as_str(),
            { attr::USER_ID } = user_id.map(|id| id.to_string()),
            { attr::INTEGRATION_CONNECTION_ID } =
                integration_connection_id.map(|id| id.to_string()),
        );
    }

    /// The worker failed on this event: the job is retried.
    pub fn log_worker_failure(&self, error: &UniversalInboxError) {
        log_slack_event!(
            warn,
            self,
            "Slack message handling failed",
            { attr::SLACK_MESSAGE_OUTCOME } = "error",
            { attr::ERROR_TYPE } = error.error_type(),
        );
    }
}

/// The Slack `type` of an event callback body, eg. `message` or `reaction_added`.
fn slack_event_type(body: &SlackEventCallbackBody) -> String {
    match body {
        SlackEventCallbackBody::Message(_) => "message".to_string(),
        SlackEventCallbackBody::ReactionAdded(_) => "reaction_added".to_string(),
        SlackEventCallbackBody::ReactionRemoved(_) => "reaction_removed".to_string(),
        SlackEventCallbackBody::Unknown(body) => body
            .get("type")
            .and_then(|event_type| event_type.as_str())
            .unwrap_or("unknown")
            .to_string(),
        // Rare events: serialize the whole event to read its `type` tag rather than listing
        // every variant. Messages and reactions, the bulk of the traffic, are matched above.
        other => serde_json::to_value(other)
            .ok()
            .and_then(|value| value.get("type")?.as_str().map(ToString::to_string))
            .unwrap_or_else(|| "unknown".to_string()),
    }
}

/// The serde name of a unit enum value, eg. `message_changed`.
fn serialized_str<T: serde::Serialize>(value: &T) -> Option<String> {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToString::to_string))
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;

    fn callback(event: serde_json::Value) -> SlackPushEventCallback {
        serde_json::from_value(json!({
            "team_id": "T01",
            "api_app_id": "A01",
            "event": event,
            "event_id": "Ev01",
            "event_time": 1700000000
        }))
        .unwrap()
    }

    #[test]
    fn test_trace_of_a_thread_reply() {
        let trace = SlackEventTrace::from_event(&callback(json!({
            "type": "message",
            "channel": "C01",
            "user": "U01",
            "text": "hello",
            "ts": "1700000001.000200",
            "thread_ts": "1700000000.000100"
        })));

        assert_eq!(
            trace,
            SlackEventTrace {
                event_id: "Ev01".to_string(),
                team_id: "T01".to_string(),
                event_type: "message".to_string(),
                subtype: None,
                channel_id: Some("C01".to_string()),
                ts: Some("1700000001.000200".to_string()),
                thread_ts: Some("1700000000.000100".to_string()),
                slack_user_id: Some("U01".to_string()),
                reaction_name: None,
            }
        );
    }

    #[test]
    fn test_trace_of_an_edited_message_has_its_subtype() {
        let trace = SlackEventTrace::from_event(&callback(json!({
            "type": "message",
            "subtype": "message_changed",
            "channel": "C01",
            "ts": "1700000002.000300",
            "message": { "text": "edited", "user": "U01", "ts": "1700000001.000200" }
        })));

        assert_eq!(trace.subtype, Some("message_changed".to_string()));
        assert_eq!(trace.ts, Some("1700000002.000300".to_string()));
    }

    #[test]
    fn test_trace_of_an_edit_is_the_edited_message_with_the_edit_subtype() {
        let edit_trace = SlackEventTrace::from_event(&callback(json!({
            "type": "message",
            "subtype": "message_changed",
            "channel": "C01",
            "ts": "1700000009.000900",
            "message": { "text": "edited", "user": "U01", "ts": "1700000001.000200" }
        })));
        let edited_event = callback(json!({
            "type": "message",
            "channel": "C01",
            "user": "U01",
            "text": "edited",
            "ts": "1700000001.000200",
            "thread_ts": "1700000000.000100"
        }));

        let trace = edit_trace.for_edited_event(&edited_event);

        assert_eq!(trace.ts, Some("1700000001.000200".to_string()));
        assert_eq!(trace.thread_ts, Some("1700000000.000100".to_string()));
        assert_eq!(trace.subtype, Some("message_changed".to_string()));
    }

    #[test]
    fn test_trace_of_a_reaction_points_to_the_reacted_message() {
        let trace = SlackEventTrace::from_event(&callback(json!({
            "type": "reaction_added",
            "user": "U01",
            "reaction": "eyes",
            "item_user": "U02",
            "item": { "type": "message", "channel": "C01", "ts": "1700000001.000200" },
            "event_ts": "1700000003.000400"
        })));

        assert_eq!(trace.event_type, "reaction_added");
        assert_eq!(trace.slack_user_id, Some("U01".to_string()));
        assert_eq!(trace.reaction_name, Some("eyes".to_string()));
        assert_eq!(trace.channel_id, Some("C01".to_string()));
        assert_eq!(trace.ts, Some("1700000001.000200".to_string()));
    }

    #[test]
    fn test_trace_of_an_unknown_event_has_its_type_and_subtype() {
        let trace = SlackEventTrace::from_event(&callback(json!({
            "type": "some_future_event",
            "subtype": "some_subtype"
        })));

        assert_eq!(trace.event_type, "some_future_event");
        assert_eq!(trace.event_subtype(), Some("some_subtype"));
        assert_eq!(trace.message_subtype(), None);
    }

    #[derive(Clone, Default)]
    struct Buffer(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for Buffer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().write(buf)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// The JSON log lines `f` writes, as the prod stdout layer formats them.
    fn json_logs(f: impl FnOnce()) -> Vec<serde_json::Value> {
        let buffer = Buffer::default();
        let writer = buffer.clone();
        let subscriber = tracing_subscriber::fmt()
            .json()
            .flatten_event(true)
            .with_writer(move || writer.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, f);
        String::from_utf8(buffer.0.lock().unwrap().clone())
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn thread_reply_trace() -> SlackEventTrace {
        SlackEventTrace::from_event(&callback(json!({
            "type": "message",
            "channel": "C01",
            "user": "U01",
            "text": "hello",
            "ts": "1700000001.000200",
            "thread_ts": "1700000000.000100"
        })))
    }

    #[test]
    fn test_gate_decision_log_carries_the_message_identity() {
        let trace = thread_reply_trace();

        let logs = json_logs(|| {
            trace.record_gate_decision(&Span::none(), SlackGateDecision::Queued("known_thread"))
        });

        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0]["level"], "INFO");
        assert_eq!(logs[0]["slack.event.id"], "Ev01");
        assert_eq!(logs[0]["slack.channel.id"], "C01");
        assert_eq!(logs[0]["slack.message.ts"], "1700000001.000200");
        assert_eq!(logs[0]["slack.message.thread_ts"], "1700000000.000100");
        assert_eq!(logs[0]["slack.event.outcome"], "queued");
        assert_eq!(logs[0]["slack.event.queue_reason"], "known_thread");
        assert!(logs[0].get("slack.event.discard_reason").is_none());
    }

    #[test]
    fn test_user_decision_log_carries_the_message_identity_and_user() {
        let trace = thread_reply_trace();
        let user_id = UserId(uuid::Uuid::new_v4());
        let integration_connection_id = IntegrationConnectionId(uuid::Uuid::new_v4());

        let logs = json_logs(|| {
            trace.log_user_decision(
                SlackMessageDecision::DeletedLastFromUser,
                Some(user_id),
                Some(integration_connection_id),
            )
        });

        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0]["level"], "INFO");
        assert_eq!(logs[0]["slack.message.ts"], "1700000001.000200");
        assert_eq!(logs[0]["slack.message.outcome"], "deleted_last_from_user");
        assert_eq!(logs[0]["user.id"], user_id.to_string());
        assert_eq!(
            logs[0]["integration_connection.id"],
            integration_connection_id.to_string()
        );
    }

    #[test]
    fn test_failure_log_carries_the_message_identity() {
        let trace = thread_reply_trace();

        let logs = json_logs(|| {
            trace.log_worker_failure(&UniversalInboxError::Unexpected(anyhow::anyhow!("boom")))
        });

        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0]["level"], "WARN");
        assert_eq!(logs[0]["slack.message.ts"], "1700000001.000200");
        assert_eq!(logs[0]["slack.message.outcome"], "error");
        assert!(logs[0]["error.type"].is_string());
    }

    #[test]
    fn test_trace_of_a_known_event_without_explicit_mapping_has_its_type() {
        let trace = SlackEventTrace::from_event(&callback(json!({
            "type": "channel_created",
            "channel": { "id": "C02", "name": "new", "created": 1700000000, "creator": "U01" }
        })));

        assert_eq!(trace.event_type, "channel_created");
    }
}
