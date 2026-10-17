//! Server-sent event stream of the realtime events of a user.

use std::{
    convert::Infallible,
    sync::Arc,
    time::{Duration, Instant},
};

use actix_web::web::Bytes;
use async_trait::async_trait;
use chrono::Utc;
use futures::Stream;
use serde_json::json;
use tokio::{
    sync::broadcast::error::RecvError,
    time::{Interval, MissedTickBehavior, interval_at, timeout},
};
use tracing::{info, warn};

use universal_inbox::{
    realtime::{RESYNC_EVENT, RealtimeEvent},
    user::UserId,
};

use crate::{
    observability::attr,
    universal_inbox::{
        UniversalInboxError,
        realtime::{
            hub::{EventHub, HubMessage, Subscription},
            metrics::ResyncReason,
        },
        user::service::UserService,
    },
};

/// Delay before the browser reconnects a closed stream.
pub const RECONNECT_DELAY_IN_MILLISECONDS: u64 = 5000;
/// Session checks failing in a row before the stream is closed: a single store hiccup does not
/// make every stream of the pod reconnect at once.
const MAX_CONSECUTIVE_SESSION_CHECK_FAILURES: u32 = 2;

/// Tells whether a session is still valid. The authentication middleware only checked it when
/// the stream was opened.
#[async_trait]
pub trait SessionValidator: Send + Sync {
    async fn is_session_active(
        &self,
        user_id: UserId,
        token_id: &str,
        issued_at: i64,
    ) -> Result<bool, UniversalInboxError>;
}

#[async_trait]
impl SessionValidator for UserService {
    async fn is_session_active(
        &self,
        user_id: UserId,
        token_id: &str,
        issued_at: i64,
    ) -> Result<bool, UniversalInboxError> {
        UserService::is_session_active(self, user_id, token_id, issued_at).await
    }
}

/// The session a stream was opened with.
pub struct StreamSession {
    pub user_id: UserId,
    pub token_id: String,
    pub issued_at: i64,
    pub expires_at: i64,
    pub validator: Arc<dyn SessionValidator>,
}

/// Open the event stream of the user of `session`, or fail with `TooManyRequests` when the
/// user has too many streams open on this pod. The stream sends a heartbeat every
/// `heartbeat_interval`, checking the session, and ends when it is no longer valid.
pub fn open_event_stream(
    event_hub: &Arc<EventHub>,
    session: StreamSession,
    heartbeat_interval: Duration,
) -> Result<impl Stream<Item = Result<Bytes, Infallible>> + 'static, UniversalInboxError> {
    let user_id = session.user_id;
    let Some(subscription) = event_hub.subscribe(user_id) else {
        return Err(UniversalInboxError::TooManyRequests {
            retry_after_seconds: RECONNECT_DELAY_IN_MILLISECONDS / 1000,
        });
    };
    info!(
        { attr::USER_ID } = %user_id,
        { attr::REALTIME_STREAMS_COUNT } = event_hub.streams_count(user_id),
        "Realtime event stream opened"
    );

    let mut heartbeat = interval_at(
        tokio::time::Instant::now() + heartbeat_interval,
        heartbeat_interval,
    );
    heartbeat.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let stream = EventStream {
        subscription,
        heartbeat,
        heartbeat_interval,
        session,
        session_check_failures: 0,
        connected: false,
        stats: StreamStats {
            user_id,
            opened_at: Instant::now(),
            close_reason: "client_disconnected",
            delivered_events: 0,
            resyncs: 0,
            lagged_events: 0,
        },
    };
    Ok(futures::stream::unfold(stream, EventStream::next))
}

struct EventStream {
    subscription: Subscription,
    heartbeat: Interval,
    heartbeat_interval: Duration,
    session: StreamSession,
    session_check_failures: u32,
    connected: bool,
    stats: StreamStats,
}

impl EventStream {
    /// The next chunk of the stream, or `None` to close it.
    async fn next(mut self) -> Option<(Result<Bytes, Infallible>, Self)> {
        if !self.connected {
            self.connected = true;
            // Announce the stream once the pod receives the events: one published before would
            // be lost. Never wait past a heartbeat, the client refetches on connect anyway.
            let hub = self.subscription.hub().clone();
            if timeout(self.heartbeat_interval, hub.wait_subscribed())
                .await
                .is_err()
            {
                warn!(
                    { attr::USER_ID } = %self.stats.user_id,
                    "Realtime event stream opened while the pod is not subscribed to the events"
                );
            }
            return Some((
                Ok(Bytes::from(format!(
                    "retry: {RECONNECT_DELAY_IN_MILLISECONDS}\n: connected\n\n"
                ))),
                self,
            ));
        }

        let chunk = tokio::select! {
            message = self.subscription.recv() => match message {
                Ok(HubMessage::Event(event)) => {
                    self.stats.delivered_events += 1;
                    self.subscription.hub().metrics().events_delivered(1);
                    format_event(&event)
                }
                // Events were lost: the client refetches everything
                Ok(HubMessage::Resync) => self.resync(0),
                Err(RecvError::Lagged(lagged_events)) => self.resync(lagged_events),
                Err(RecvError::Closed) => {
                    self.stats.close_reason = "hub_closed";
                    return None;
                }
            },
            _ = self.heartbeat.tick() => match self.check_session().await {
                Ok(()) => Bytes::from_static(b": heartbeat\n\n"),
                Err(reason) => {
                    self.stats.close_reason = reason;
                    return None;
                }
            },
        };
        Some((Ok(chunk), self))
    }

    fn resync(&mut self, lagged_events: u64) -> Bytes {
        self.stats.resyncs += 1;
        self.stats.lagged_events += lagged_events;
        if lagged_events > 0 {
            // Resyncs of all the streams of the pod are counted by the hub
            let metrics = self.subscription.hub().metrics();
            metrics.events_dropped(lagged_events);
            metrics.resynced(1, ResyncReason::Lagged);
        }
        Bytes::from(format!("event: {RESYNC_EVENT}\ndata: {{}}\n\n"))
    }

    /// Fails closed with the reason to close the stream. The client then reconnects, and the
    /// authentication middleware tells whether it is still logged in.
    async fn check_session(&mut self) -> Result<(), &'static str> {
        let session = &self.session;
        if session.expires_at <= Utc::now().timestamp() {
            return Err("session_expired");
        }
        match session
            .validator
            .is_session_active(session.user_id, &session.token_id, session.issued_at)
            .await
        {
            Ok(true) => {
                self.session_check_failures = 0;
                Ok(())
            }
            Ok(false) => Err("session_revoked"),
            Err(err) => {
                self.session_check_failures += 1;
                warn!(
                    { attr::USER_ID } = %session.user_id,
                    { attr::ERROR_MESSAGE } = %err,
                    "Failed to check the session of a realtime event stream"
                );
                if self.session_check_failures >= MAX_CONSECUTIVE_SESSION_CHECK_FAILURES {
                    Err("session_check_failed")
                } else {
                    Ok(())
                }
            }
        }
    }
}

fn format_event(event: &RealtimeEvent) -> Bytes {
    let data = match &event.ids {
        Some(ids) => json!({ "ids": ids }),
        None => json!({}),
    };
    Bytes::from(format!("event: {}\ndata: {data}\n\n", event.kind))
}

/// Log the end of a stream when it is dropped, whatever ended it. Stands for the stream
/// metrics until the API has a metrics pipeline (universal-inbox-gr35.1).
struct StreamStats {
    user_id: UserId,
    opened_at: Instant,
    close_reason: &'static str,
    delivered_events: u64,
    resyncs: u64,
    lagged_events: u64,
}

impl Drop for StreamStats {
    fn drop(&mut self) {
        info!(
            { attr::USER_ID } = %self.user_id,
            { attr::REALTIME_STREAM_DURATION_MS } = self.opened_at.elapsed().as_millis() as u64,
            { attr::REALTIME_STREAM_CLOSE_REASON } = self.close_reason,
            { attr::REALTIME_STREAM_DELIVERED_EVENTS } = self.delivered_events,
            { attr::REALTIME_STREAM_RESYNCS } = self.resyncs,
            { attr::REALTIME_STREAM_LAGGED_EVENTS } = self.lagged_events,
            "Realtime event stream closed"
        );
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use futures::StreamExt;
    use pretty_assertions::assert_eq;
    use uuid::Uuid;

    use universal_inbox::realtime::RealtimeEventKind;

    use super::{super::hub::STREAM_CHANNEL_CAPACITY, *};

    /// Session validator answering from a list, then `Ok(true)`.
    #[derive(Default)]
    struct FakeValidator(Mutex<Vec<Result<bool, UniversalInboxError>>>);

    #[async_trait]
    impl SessionValidator for FakeValidator {
        async fn is_session_active(
            &self,
            _user_id: UserId,
            _token_id: &str,
            _issued_at: i64,
        ) -> Result<bool, UniversalInboxError> {
            let mut answers = self.0.lock().unwrap();
            if answers.is_empty() {
                Ok(true)
            } else {
                answers.remove(0)
            }
        }
    }

    fn user() -> UserId {
        UserId(Uuid::from_u128(1))
    }

    fn session(answers: Vec<Result<bool, UniversalInboxError>>) -> StreamSession {
        StreamSession {
            user_id: user(),
            token_id: "jti".to_string(),
            issued_at: 0,
            expires_at: Utc::now().timestamp() + 3600,
            validator: Arc::new(FakeValidator(Mutex::new(answers))),
        }
    }

    fn subscribed_hub() -> Arc<EventHub> {
        let hub = EventHub::new();
        hub.set_subscribed(true);
        hub
    }

    async fn next_chunk(
        stream: &mut (impl Stream<Item = Result<Bytes, Infallible>> + Unpin),
    ) -> Option<String> {
        stream
            .next()
            .await
            .map(|chunk| String::from_utf8(chunk.unwrap().to_vec()).unwrap())
    }

    #[tokio::test]
    async fn test_stream_sends_events_and_a_resync_when_lagging() {
        let hub = subscribed_hub();
        let stream = open_event_stream(&hub, session(vec![]), Duration::from_secs(60)).unwrap();
        let mut stream = Box::pin(stream);
        assert_eq!(
            next_chunk(&mut stream).await.as_deref(),
            Some("retry: 5000\n: connected\n\n")
        );

        hub.dispatch(RealtimeEvent {
            user_id: user(),
            kind: RealtimeEventKind::TasksChanged,
            ids: Some(vec![Uuid::nil()]),
        });
        assert_eq!(
            next_chunk(&mut stream).await,
            Some(format!(
                "event: tasks.changed\ndata: {{\"ids\":[\"{}\"]}}\n\n",
                Uuid::nil()
            ))
        );

        for _ in 0..=STREAM_CHANNEL_CAPACITY {
            hub.dispatch(RealtimeEvent {
                user_id: user(),
                kind: RealtimeEventKind::TasksChanged,
                ids: None,
            });
        }
        assert_eq!(
            next_chunk(&mut stream).await.as_deref(),
            Some("event: resync\ndata: {}\n\n")
        );
    }

    #[tokio::test]
    async fn test_stream_is_refused_above_the_streams_cap() {
        let hub = subscribed_hub();
        let _streams: Vec<_> = (0..super::super::hub::MAX_STREAMS_PER_USER)
            .map(|_| open_event_stream(&hub, session(vec![]), Duration::from_secs(60)).unwrap())
            .collect();

        assert!(matches!(
            open_event_stream(&hub, session(vec![]), Duration::from_secs(60)),
            Err(UniversalInboxError::TooManyRequests { .. })
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn test_stream_tolerates_one_failed_session_check() {
        let hub = subscribed_hub();
        let failure = || {
            Err(UniversalInboxError::SessionStoreUnavailable(
                anyhow::anyhow!("Redis is down"),
            ))
        };
        let stream = open_event_stream(
            &hub,
            session(vec![failure(), Ok(true), failure(), failure()]),
            Duration::from_secs(1),
        )
        .unwrap();
        let mut stream = Box::pin(stream);
        next_chunk(&mut stream).await;

        for _ in 0..3 {
            assert_eq!(
                next_chunk(&mut stream).await.as_deref(),
                Some(": heartbeat\n\n")
            );
        }
        assert_eq!(next_chunk(&mut stream).await, None);
    }

    #[tokio::test(start_paused = true)]
    async fn test_stream_ends_when_the_session_is_revoked() {
        let hub = subscribed_hub();
        let stream =
            open_event_stream(&hub, session(vec![Ok(false)]), Duration::from_secs(1)).unwrap();
        let mut stream = Box::pin(stream);
        next_chunk(&mut stream).await;

        assert_eq!(next_chunk(&mut stream).await, None);
        assert_eq!(hub.streams_count(user()), 0);
    }

    #[tokio::test]
    async fn test_stream_records_delivered_and_dropped_events() {
        use crate::{
            observability::attr,
            universal_inbox::realtime::metrics::test_support::{TestMetrics, attribute},
        };

        let test_metrics = TestMetrics::new();
        let hub = EventHub::with_metrics(test_metrics.metrics);
        hub.set_subscribed(true);
        let mut stream =
            Box::pin(open_event_stream(&hub, session(vec![]), Duration::from_secs(60)).unwrap());
        next_chunk(&mut stream).await;

        hub.dispatch(RealtimeEvent {
            user_id: user(),
            kind: RealtimeEventKind::TasksChanged,
            ids: None,
        });
        next_chunk(&mut stream).await;
        // One more than the channel holds: the oldest is lost
        for _ in 0..=STREAM_CHANNEL_CAPACITY {
            hub.dispatch(RealtimeEvent {
                user_id: user(),
                kind: RealtimeEventKind::TasksChanged,
                ids: None,
            });
        }
        assert_eq!(
            next_chunk(&mut stream).await.as_deref(),
            Some("event: resync\ndata: {}\n\n")
        );

        assert_eq!(
            test_metrics.sum("realtime.events.delivered"),
            vec![(vec![], 1)]
        );
        assert_eq!(
            test_metrics.sum("realtime.events.dropped"),
            vec![(vec![], 1)]
        );
        assert_eq!(
            test_metrics.sum("realtime.streams.resyncs"),
            vec![(attribute(attr::REALTIME_RESYNC_REASON, "lagged"), 1)]
        );
    }
}
