//! Redis pub/sub subscriber of an API pod, feeding the [`EventHub`] with the realtime events
//! published by every process.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use futures::StreamExt;
use tokio::time::{MissedTickBehavior, interval_at, timeout};
use tracing::{info, warn};

use universal_inbox::realtime::RealtimeEvent;

use crate::{
    configuration::RedisSettings,
    observability::attr,
    universal_inbox::realtime::{hub::EventHub, metrics::ResyncReason},
};

const MIN_RECONNECT_DELAY: Duration = Duration::from_secs(1);
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(30);
/// A connection that stayed up this long resets the reconnect backoff: a flapping one would
/// otherwise make every client refetch everything each second.
const STABLE_CONNECTION_DURATION: Duration = Duration::from_secs(60);
/// The subscription is pinged this often: Redis sends nothing on an idle channel, and a
/// half-open connection (failover, idle NAT drop) would otherwise never end.
const PING_INTERVAL: Duration = Duration::from_secs(30);
const PING_TIMEOUT: Duration = Duration::from_secs(5);

/// Subscribe to `channel` and dispatch its events to `hub`, forever. A lost connection is
/// retried with an exponential backoff, then every open stream is told to resync: events
/// published meanwhile were lost.
pub async fn run_event_subscriber(
    redis_settings: RedisSettings,
    channel: String,
    hub: Arc<EventHub>,
) {
    let mut reconnect_delay = MIN_RECONNECT_DELAY;
    let mut was_connected = false;
    loop {
        match subscribe(&redis_settings, &channel).await {
            Ok(pubsub) => {
                info!(
                    { attr::MESSAGING_DESTINATION_NAME } = %channel,
                    "Subscribed to realtime events"
                );
                hub.set_subscribed(true);
                if was_connected {
                    hub.resync_all(ResyncReason::Resubscribed);
                }
                was_connected = true;

                let connected_at = Instant::now();
                let reason = dispatch_events(pubsub, &channel, &hub).await;
                hub.set_subscribed(false);
                if connected_at.elapsed() >= STABLE_CONNECTION_DURATION {
                    reconnect_delay = MIN_RECONNECT_DELAY;
                }
                warn!(
                    { attr::MESSAGING_DESTINATION_NAME } = %channel,
                    { attr::ERROR_MESSAGE } = reason,
                    "Realtime events subscription lost, reconnecting in {reconnect_delay:?}"
                );
            }
            Err(err) => warn!(
                { attr::MESSAGING_DESTINATION_NAME } = %channel,
                { attr::ERROR_MESSAGE } = %err,
                "Failed to subscribe to realtime events, retrying in {reconnect_delay:?}"
            ),
        }

        tokio::time::sleep(reconnect_delay).await;
        reconnect_delay = (reconnect_delay * 2).min(MAX_RECONNECT_DELAY);
    }
}

/// Dispatch the events of the subscription until it is lost, returning why.
async fn dispatch_events(
    pubsub: redis::aio::PubSub,
    channel: &str,
    hub: &EventHub,
) -> &'static str {
    let (mut sink, mut messages) = pubsub.split();
    let mut ping = interval_at(tokio::time::Instant::now() + PING_INTERVAL, PING_INTERVAL);
    ping.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            message = messages.next() => {
                let Some(message) = message else {
                    return "connection closed";
                };
                match message
                    .get_payload::<String>()
                    .map_err(anyhow::Error::from)
                    .and_then(|payload| {
                        serde_json::from_str::<RealtimeEvent>(&payload).map_err(Into::into)
                    }) {
                    Ok(event) => hub.dispatch(event),
                    Err(err) => warn!(
                        { attr::MESSAGING_DESTINATION_NAME } = %channel,
                        { attr::ERROR_MESSAGE } = %err,
                        "Skipping an invalid realtime event"
                    ),
                }
            }
            _ = ping.tick() => {
                match timeout(PING_TIMEOUT, sink.ping::<()>()).await {
                    Ok(Ok(())) => {}
                    Ok(Err(_)) => return "ping failed",
                    Err(_) => return "ping timed out",
                }
            }
        }
    }
}

async fn subscribe(
    redis_settings: &RedisSettings,
    channel: &str,
) -> Result<redis::aio::PubSub, redis::RedisError> {
    let client = redis::Client::open(redis_settings.connection_string())?;
    let mut pubsub = client.get_async_pubsub().await?;
    pubsub.subscribe(channel).await?;
    Ok(pubsub)
}
