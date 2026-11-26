use std::time::Duration;

use futures::StreamExt;
use tokio::time::{Instant, timeout_at};

use universal_inbox::{
    realtime::{RealtimeEvent, RealtimeEventKind},
    user::UserId,
};
use universal_inbox_api::configuration::{RealtimeSettings, Settings};

pub struct RealtimeEventsSubscriber {
    pubsub: redis::aio::PubSub,
    user_id: UserId,
}

impl RealtimeEventsSubscriber {
    pub async fn new(settings: &Settings, user_id: UserId) -> Self {
        let client = redis::Client::open(settings.redis.connection_string()).unwrap();
        let mut pubsub = client.get_async_pubsub().await.unwrap();
        pubsub
            .subscribe(RealtimeSettings::default().channel)
            .await
            .unwrap();
        Self { pubsub, user_id }
    }

    pub async fn events_until(
        &mut self,
        until: impl Fn(&[RealtimeEvent]) -> bool,
    ) -> Vec<RealtimeEvent> {
        let user_id = self.user_id;
        let mut messages = self.pubsub.on_message();
        let mut events = vec![];
        let mut deadline = Instant::now() + Duration::from_secs(10);
        let mut done = false;
        while let Ok(Some(message)) = timeout_at(deadline, messages.next()).await {
            let payload: String = message.get_payload().unwrap();
            let event: RealtimeEvent = serde_json::from_str(&payload).unwrap();
            if event.user_id != user_id {
                continue;
            }
            events.push(event);
            if !done && until(&events) {
                done = true;
                deadline = Instant::now() + Duration::from_millis(300);
            }
        }
        assert!(
            done,
            "Expected realtime events not received, got {events:?}"
        );
        events
    }
}

pub fn events_of_kind(events: &[RealtimeEvent], kind: RealtimeEventKind) -> Vec<RealtimeEvent> {
    events
        .iter()
        .filter(|event| event.kind == kind)
        .cloned()
        .collect()
}
