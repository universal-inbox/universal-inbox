//! In-memory fan-out of the realtime events received by an API pod to the event streams its
//! users have open on it.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use tokio::sync::{
    broadcast::{self, error::RecvError},
    watch,
};

use universal_inbox::{realtime::RealtimeEvent, user::UserId};

use super::metrics::{RealtimeMetrics, ResyncReason, realtime_metrics};

/// Events buffered per user before a slow stream lags and is told to resync.
pub const STREAM_CHANNEL_CAPACITY: usize = 64;
/// Open streams allowed per user on one pod.
pub const MAX_STREAMS_PER_USER: usize = 10;

/// What a stream receives: an event, or the order to refetch everything because events may
/// have been lost.
#[derive(Debug, Clone, PartialEq)]
pub enum HubMessage {
    Event(Arc<RealtimeEvent>),
    Resync,
}

pub struct EventHub {
    senders: Mutex<HashMap<UserId, broadcast::Sender<HubMessage>>>,
    /// Whether the pod's subscriber currently receives the published events
    subscribed: watch::Sender<bool>,
    metrics: &'static RealtimeMetrics,
}

impl EventHub {
    pub fn new() -> Arc<Self> {
        Self::with_metrics(realtime_metrics())
    }

    /// A hub recording its metrics, and those of its streams, with `metrics`.
    pub fn with_metrics(metrics: &'static RealtimeMetrics) -> Arc<Self> {
        Arc::new(Self {
            senders: Mutex::default(),
            subscribed: watch::Sender::new(false),
            metrics,
        })
    }

    pub fn metrics(&self) -> &'static RealtimeMetrics {
        self.metrics
    }

    /// Record whether the pod's subscriber is subscribed to the published events.
    pub fn set_subscribed(&self, subscribed: bool) {
        self.subscribed.send_replace(subscribed);
    }

    /// Wait until the pod's subscriber is subscribed: an event published before is lost.
    pub async fn wait_subscribed(&self) {
        let _ = self
            .subscribed
            .subscribe()
            .wait_for(|subscribed| *subscribed)
            .await;
    }

    /// Open a subscription to the events of `user_id`, or `None` when the user already has
    /// [`MAX_STREAMS_PER_USER`] streams open on this pod.
    pub fn subscribe(self: &Arc<Self>, user_id: UserId) -> Option<Subscription> {
        let mut senders = self.senders.lock().unwrap();
        let sender = senders
            .entry(user_id)
            .or_insert_with(|| broadcast::channel(STREAM_CHANNEL_CAPACITY).0);
        if sender.receiver_count() >= MAX_STREAMS_PER_USER {
            return None;
        }
        self.metrics.stream_opened();
        Some(Subscription {
            receiver: Some(sender.subscribe()),
            user_id,
            hub: self.clone(),
        })
    }

    /// Send `event` to the open streams of its user, if any.
    pub fn dispatch(&self, event: RealtimeEvent) {
        let senders = self.senders.lock().unwrap();
        if let Some(sender) = senders.get(&event.user_id) {
            // No receiver left is not an error: the entry is removed with the last subscription
            // Counted as delivered once written to a stream: a slow one may lose it
            let _ = sender.send(HubMessage::Event(Arc::new(event)));
        }
    }

    /// Tell every open stream to refetch everything, as events may have been lost.
    pub fn resync_all(&self, reason: ResyncReason) {
        for sender in self.senders.lock().unwrap().values() {
            if let Ok(streams_count) = sender.send(HubMessage::Resync) {
                self.metrics.resynced(streams_count as u64, reason);
            }
        }
    }

    /// The users with at least one stream open on this pod.
    pub fn connected_users(&self) -> Vec<UserId> {
        self.senders
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, sender)| sender.receiver_count() > 0)
            .map(|(user_id, _)| *user_id)
            .collect()
    }

    /// Number of open streams of `user_id` on this pod.
    pub fn streams_count(&self, user_id: UserId) -> usize {
        self.senders
            .lock()
            .unwrap()
            .get(&user_id)
            .map_or(0, broadcast::Sender::receiver_count)
    }

    fn unsubscribe(&self, user_id: UserId) {
        let mut senders = self.senders.lock().unwrap();
        // The receiver of the subscription being dropped is already gone
        if senders
            .get(&user_id)
            .is_some_and(|sender| sender.receiver_count() == 0)
        {
            senders.remove(&user_id);
        }
    }
}

pub struct Subscription {
    /// Always `Some` until the subscription is dropped
    receiver: Option<broadcast::Receiver<HubMessage>>,
    user_id: UserId,
    hub: Arc<EventHub>,
}

impl Subscription {
    pub async fn recv(&mut self) -> Result<HubMessage, RecvError> {
        match &mut self.receiver {
            Some(receiver) => receiver.recv().await,
            None => Err(RecvError::Closed),
        }
    }

    pub fn hub(&self) -> &Arc<EventHub> {
        &self.hub
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        // Drop the receiver first so that it is not counted any more
        self.receiver.take();
        self.hub.unsubscribe(self.user_id);
        self.hub.metrics.stream_closed();
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use tokio::sync::broadcast::error::{RecvError, TryRecvError};
    use tokio::time::{Duration, timeout};
    use uuid::Uuid;

    use universal_inbox::realtime::RealtimeEventKind;

    use super::*;

    fn user(n: u128) -> UserId {
        UserId(Uuid::from_u128(n))
    }

    fn event(user_id: UserId) -> RealtimeEvent {
        RealtimeEvent {
            user_id,
            kind: RealtimeEventKind::NotificationsChanged,
            ids: None,
        }
    }

    #[tokio::test]
    async fn test_dispatch_reaches_only_the_streams_of_the_user() {
        let hub = EventHub::new();
        let mut first = hub.subscribe(user(1)).unwrap();
        let mut second = hub.subscribe(user(1)).unwrap();
        let mut other = hub.subscribe(user(2)).unwrap();

        hub.dispatch(event(user(1)));

        let expected = HubMessage::Event(Arc::new(event(user(1))));
        assert_eq!(first.recv().await.unwrap(), expected);
        assert_eq!(second.recv().await.unwrap(), expected);
        assert_eq!(
            other.receiver.as_mut().unwrap().try_recv(),
            Err(TryRecvError::Empty)
        );
    }

    #[tokio::test]
    async fn test_last_subscription_drop_removes_the_user() {
        let hub = EventHub::new();
        let first = hub.subscribe(user(1)).unwrap();
        let second = hub.subscribe(user(1)).unwrap();
        assert_eq!(hub.streams_count(user(1)), 2);

        drop(first);
        assert_eq!(hub.streams_count(user(1)), 1);
        assert!(hub.senders.lock().unwrap().contains_key(&user(1)));

        drop(second);
        assert_eq!(hub.streams_count(user(1)), 0);
        assert!(!hub.senders.lock().unwrap().contains_key(&user(1)));
    }

    #[tokio::test]
    async fn test_subscribe_is_refused_above_the_streams_cap() {
        let hub = EventHub::new();
        let subscriptions: Vec<_> = (0..MAX_STREAMS_PER_USER)
            .map(|_| hub.subscribe(user(1)).unwrap())
            .collect();

        assert!(hub.subscribe(user(1)).is_none());
        assert!(hub.subscribe(user(2)).is_some());

        drop(subscriptions);
        assert!(hub.subscribe(user(1)).is_some());
    }

    #[tokio::test]
    async fn test_slow_stream_lags_and_all_streams_can_be_resynced() {
        let hub = EventHub::new();
        let mut subscription = hub.subscribe(user(1)).unwrap();

        for _ in 0..=STREAM_CHANNEL_CAPACITY {
            hub.dispatch(event(user(1)));
        }
        assert_eq!(subscription.recv().await, Err(RecvError::Lagged(1)));

        let mut subscription = hub.subscribe(user(2)).unwrap();
        hub.resync_all(ResyncReason::Resubscribed);
        assert_eq!(subscription.recv().await, Ok(HubMessage::Resync));
    }

    #[tokio::test]
    async fn test_wait_subscribed_waits_for_the_subscriber() {
        let hub = EventHub::new();
        assert!(
            timeout(Duration::from_millis(50), hub.wait_subscribed())
                .await
                .is_err()
        );

        hub.set_subscribed(true);
        timeout(Duration::from_millis(50), hub.wait_subscribed())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_connected_users_are_the_users_with_an_open_stream() {
        let hub = EventHub::new();
        assert_eq!(hub.connected_users(), vec![]);

        let first = hub.subscribe(user(1)).unwrap();
        let second = hub.subscribe(user(1)).unwrap();
        assert_eq!(hub.connected_users(), vec![user(1)]);

        drop(first);
        assert_eq!(hub.connected_users(), vec![user(1)]);
        drop(second);
        assert_eq!(hub.connected_users(), vec![]);
    }

    #[tokio::test]
    async fn test_open_streams_metric_stays_balanced() {
        let test_metrics =
            crate::universal_inbox::realtime::metrics::test_support::TestMetrics::new();
        let hub = EventHub::with_metrics(test_metrics.metrics);

        let subscriptions: Vec<_> = (0..MAX_STREAMS_PER_USER)
            .map(|_| hub.subscribe(user(1)).unwrap())
            .collect();
        // Refused above the cap: not counted
        assert!(hub.subscribe(user(1)).is_none());
        assert_eq!(
            test_metrics.sum("realtime.streams.open"),
            vec![(vec![], MAX_STREAMS_PER_USER as i64)]
        );

        drop(subscriptions);
        assert_eq!(test_metrics.sum("realtime.streams.open"), vec![(vec![], 0)]);
    }
}
