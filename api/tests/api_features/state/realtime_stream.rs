use std::time::Duration;

use actix_web::web::Bytes;
use futures::{StreamExt, stream::BoxStream};
use http::StatusCode;
use reqwest::Client;
use tokio::time::timeout;

const HEARTBEAT: &str = ": heartbeat";

pub struct EventStream {
    chunks: BoxStream<'static, reqwest::Result<Bytes>>,
    buffer: String,
}

impl EventStream {
    pub async fn open(client: &Client, api_address: &str) -> Self {
        let response = client
            .get(format!("{api_address}events"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let mut stream = Self {
            chunks: response.bytes_stream().boxed(),
            buffer: String::new(),
        };
        assert_eq!(
            stream.next_message().await.as_deref(),
            Some("retry: 5000\n: connected")
        );
        stream
    }

    pub async fn next_message(&mut self) -> Option<String> {
        loop {
            if let Some(end) = self.buffer.find("\n\n") {
                let message = self.buffer[..end].to_string();
                self.buffer.drain(..end + 2);
                return Some(message);
            }
            match timeout(Duration::from_secs(10), self.chunks.next())
                .await
                .expect("No event stream message in time")
            {
                Some(chunk) => self
                    .buffer
                    .push_str(std::str::from_utf8(&chunk.unwrap()).unwrap()),
                None => return None,
            }
        }
    }

    pub async fn next_event(&mut self) -> Option<String> {
        timeout(Duration::from_secs(10), async {
            loop {
                match self.next_message().await {
                    Some(message) if message == HEARTBEAT => continue,
                    message => return message,
                }
            }
        })
        .await
        .expect("No event stream event in time")
    }
}

#[derive(Default)]
pub struct RealtimeStream {
    pub own: Vec<EventStream>,
    pub other: Option<EventStream>,
}

#[derive(Default)]
pub struct RealtimePublication {
    pub subscriber: Option<crate::helpers::realtime::RealtimeEventsSubscriber>,
    pub events: Vec<universal_inbox::realtime::RealtimeEvent>,
    pub seeded_actions: Vec<uuid::Uuid>,
}
