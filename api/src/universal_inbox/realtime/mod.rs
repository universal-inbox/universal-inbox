//! Realtime events: tell the web app of a user that their notifications, tasks or integration
//! connections changed, so it refetches them.
//!
//! Services call [`record_event`] when they change something. The events are buffered in the
//! task-local scope of the entry point (HTTP request, job, MCP tool call, CLI command, see
//! [`with_event_scope`]) and published when it succeeds, ie. after the transactions it
//! committed; they are discarded when it fails. Long-running entry points (syncs) also publish
//! right after their own commits with [`flush_events`].
//!
//! Publishing is best-effort: a failure is logged and never fails the write. A missed event
//! costs the client one safety-net poll, and an event published for a change that was rolled
//! back (a dropped entry point) costs it one refetch.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use tracing::{Span, warn};

use universal_inbox::realtime::RealtimeEvent;

use crate::{
    configuration::RealtimeSettings,
    observability::{attr, instrument_client_call, redis_client_span},
    utils::cache::Cache,
};

pub mod scope;

pub use scope::{flush_events, record_event, with_event_scope, with_event_scope_if};

/// Bound on a publish, so that a slow or unreachable Redis does not stall the writes.
const PUBLISH_TIMEOUT: Duration = Duration::from_millis(200);

#[async_trait]
pub trait EventPublisher: Send + Sync {
    /// Publish `events`. Never fails: a failure is logged by the implementation.
    async fn publish(&self, events: Vec<RealtimeEvent>);
}

/// Publish the events on a Redis pub/sub channel, read by every API pod.
pub struct RedisEventPublisher {
    connection_manager: redis::aio::ConnectionManager,
    channel: String,
}

impl RedisEventPublisher {
    pub fn new(cache: &Cache, channel: String) -> Self {
        Self {
            connection_manager: cache.connection_manager.clone(),
            channel,
        }
    }
}

fn record_span_error(span: &Span, error_type: &str) {
    span.record(attr::ERROR_TYPE, error_type);
    span.record(attr::OTEL_STATUS_CODE, "ERROR");
}

#[async_trait]
impl EventPublisher for RedisEventPublisher {
    async fn publish(&self, events: Vec<RealtimeEvent>) {
        if events.is_empty() {
            return;
        }

        let span = redis_client_span("PUBLISH", &self.channel);
        let mut pipeline = redis::pipe();
        for event in &events {
            match serde_json::to_string(event) {
                Ok(payload) => {
                    pipeline
                        .cmd("PUBLISH")
                        .arg(&self.channel)
                        .arg(payload)
                        .ignore();
                }
                Err(err) => {
                    record_span_error(&span, "serialization");
                    warn!(
                        { attr::MESSAGING_DESTINATION_NAME } = %self.channel,
                        { attr::USER_ID } = %event.user_id,
                        "Failed to serialize {} realtime event: {err}",
                        event.kind
                    );
                }
            }
        }

        let mut connection = self.connection_manager.clone();
        let publish = async {
            match tokio::time::timeout(PUBLISH_TIMEOUT, pipeline.query_async::<()>(&mut connection))
                .await
            {
                Ok(result) => result.map_err(|err| ("redis", err.to_string())),
                Err(_) => Err(("timeout", format!("timed out after {PUBLISH_TIMEOUT:?}"))),
            }
        };
        if let Err((_, err)) =
            instrument_client_call(span, publish, |(error_type, _)| error_type.to_string()).await
        {
            warn!(
                { attr::MESSAGING_DESTINATION_NAME } = %self.channel,
                { attr::REALTIME_EVENTS_COUNT } = events.len(),
                "Failed to publish realtime events: {err}"
            );
        }
    }
}

/// Publisher used when realtime events are disabled.
pub struct NoopEventPublisher;

#[async_trait]
impl EventPublisher for NoopEventPublisher {
    async fn publish(&self, _events: Vec<RealtimeEvent>) {}
}

/// Keep the published events in memory, for tests.
#[cfg(test)]
#[derive(Default)]
pub struct InMemoryEventPublisher {
    published: std::sync::Mutex<Vec<Vec<RealtimeEvent>>>,
}

#[cfg(test)]
impl InMemoryEventPublisher {
    /// Published events, one entry per `publish` call.
    pub fn published(&self) -> Vec<Vec<RealtimeEvent>> {
        self.published.lock().unwrap().clone()
    }
}

#[cfg(test)]
#[async_trait]
impl EventPublisher for InMemoryEventPublisher {
    async fn publish(&self, events: Vec<RealtimeEvent>) {
        if !events.is_empty() {
            self.published.lock().unwrap().push(events);
        }
    }
}

pub fn build_event_publisher(
    settings: &RealtimeSettings,
    cache: &Cache,
) -> Arc<dyn EventPublisher> {
    if settings.enabled {
        Arc::new(RedisEventPublisher::new(cache, settings.channel.clone()))
    } else {
        Arc::new(NoopEventPublisher)
    }
}
