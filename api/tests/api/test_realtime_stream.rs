use std::time::Duration;

use futures::{StreamExt, stream::BoxStream};
use http::StatusCode;
use pretty_assertions::assert_eq;
use reqwest::Client;
use rstest::*;
use serde_json::json;
use tokio::time::timeout;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::github::GithubConfig,
    },
    notification::service::NotificationPatch,
    realtime::{RealtimeEvent, RealtimeEventKind},
    third_party::integrations::github::GithubNotification,
};

use universal_inbox_api::{
    configuration::{RealtimeSettings, Settings},
    jobs::JobStorage,
    universal_inbox::realtime::{EventPublisher, RedisEventPublisher},
    utils::cache::Cache,
};

use crate::helpers::{
    TestDb,
    auth::{AuthenticatedApp, authenticate_user, authenticated_app},
    db_connection,
    integration_connection::{
        OAuthCredentialFixture, create_and_mock_integration_connection, github_oauth_credential,
    },
    notification::github::{create_notification_from_github_notification, github_notification},
    redis_storage,
    rest::patch_resource_response,
    settings, tested_app, tracing_setup,
    user::{front_origin_header, logout_user_response},
};

/// Reader of a server-sent event stream, one message (block ended by a blank line) at a time.
struct EventStreamReader {
    chunks: BoxStream<'static, reqwest::Result<actix_web::web::Bytes>>,
    buffer: String,
}

impl EventStreamReader {
    async fn open(client: &Client, api_address: &str) -> Self {
        let response = client
            .get(format!("{api_address}events"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        Self {
            chunks: response.bytes_stream().boxed(),
            buffer: String::new(),
        }
    }

    /// The next message, or `None` when the stream ends. Panics when none comes in time.
    async fn next_message(&mut self) -> Option<String> {
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

    /// The next message that is not a heartbeat.
    async fn next_event(&mut self) -> Option<String> {
        loop {
            match self.next_message().await {
                Some(message) if message == ": heartbeat" => continue,
                message => return message,
            }
        }
    }
}

async fn open_stream(client: &Client, api_address: &str) -> EventStreamReader {
    let mut reader = EventStreamReader::open(client, api_address).await;
    assert_eq!(
        reader.next_message().await.as_deref(),
        Some("retry: 5000\n: connected")
    );
    reader
}

#[rstest]
#[tokio::test]
async fn test_event_stream_headers(#[future] authenticated_app: AuthenticatedApp) {
    let app = authenticated_app.await;

    let response = app
        .client
        .get(format!("{}events", app.app.api_address))
        .header(reqwest::header::ACCEPT_ENCODING, "gzip, br")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let headers = response.headers();
    assert_eq!(headers["content-type"], "text/event-stream");
    assert_eq!(headers["cache-control"], "no-cache");
    assert_eq!(headers["x-accel-buffering"], "no");
    assert_eq!(headers["content-encoding"], "identity");
}

#[rstest]
#[tokio::test]
async fn test_event_stream_delivers_the_events_of_the_user_only(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    github_notification: Box<GithubNotification>,
    github_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    let github_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        &settings,
        github_oauth_credential,
        None,
        None,
    )
    .await;
    let notification = create_notification_from_github_notification(
        &app.app,
        &github_notification,
        app.user.id,
        github_integration_connection.id,
    )
    .await;
    let mut stream = open_stream(&app.client, &app.app.api_address).await;
    let (other_client, other_user) =
        authenticate_user(&app.app, "5678", "Jane", "Doe", "jane@example.com").await;
    let mut other_stream = open_stream(&other_client, &app.app.api_address).await;

    let response = patch_resource_response(
        &app.client,
        &app.app.api_address,
        "notifications",
        notification.id.into(),
        &NotificationPatch {
            snoozed_until: Some(chrono::Utc::now() + chrono::TimeDelta::hours(1)),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    assert_eq!(
        stream.next_event().await,
        Some(format!(
            "event: notifications.changed\ndata: {}",
            json!({ "ids": [notification.id.0] })
        ))
    );
    // The other user only gets their own events: publish one to tell it got nothing before
    publish_event(&settings, other_user.id, RealtimeEventKind::TasksChanged).await;
    assert_eq!(
        other_stream.next_event().await.as_deref(),
        Some("event: tasks.changed\ndata: {}")
    );
}

/// Publish an event the way another process (an async worker) does.
async fn publish_event(
    settings: &Settings,
    user_id: universal_inbox::user::UserId,
    kind: RealtimeEventKind,
) {
    let cache = Cache::new(&settings.redis).await.unwrap();
    RedisEventPublisher::new(&cache, RealtimeSettings::default().channel)
        .publish(vec![RealtimeEvent {
            user_id,
            kind,
            ids: None,
        }])
        .await;
}

#[rstest]
#[tokio::test]
async fn test_event_stream_delivers_events_published_by_another_process(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    let mut stream = open_stream(&app.client, &app.app.api_address).await;

    publish_event(
        &settings,
        app.user.id,
        RealtimeEventKind::IntegrationConnectionChanged,
    )
    .await;

    assert_eq!(
        stream.next_event().await.as_deref(),
        Some("event: integration_connection.changed\ndata: {}")
    );
}

#[rstest]
#[tokio::test]
async fn test_event_stream_sends_heartbeats(#[future] authenticated_app: AuthenticatedApp) {
    let app = authenticated_app.await;
    let mut stream = open_stream(&app.client, &app.app.api_address).await;

    // The test configuration sends a heartbeat every second
    assert_eq!(stream.next_message().await.as_deref(), Some(": heartbeat"));
    assert_eq!(stream.next_message().await.as_deref(), Some(": heartbeat"));
}

#[rstest]
#[tokio::test]
async fn test_event_stream_ends_when_the_session_is_revoked(
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    let mut stream = open_stream(&app.client, &app.app.api_address).await;

    let response = logout_user_response(&app.client, &app.app.api_address).await;
    assert!(response.status().is_success());

    // Checked on the next heartbeat
    assert_eq!(stream.next_event().await, None);
}

#[rstest]
#[tokio::test]
async fn test_event_stream_rejects_api_keys(#[future] authenticated_app: AuthenticatedApp) {
    let app = authenticated_app.await;
    let api_key: universal_inbox::auth::auth_token::AuthenticationToken = app
        .client
        .post(format!(
            "{}users/me/authentication-tokens",
            app.app.api_address
        ))
        .header(reqwest::header::ORIGIN, front_origin_header(&app.app))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let response = Client::new()
        .get(format!("{}events", app.app.api_address))
        .bearer_auth(
            secrecy::ExposeSecret::expose_secret(&api_key.jwt_token)
                .0
                .clone(),
        )
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[fixture]
async fn authenticated_app_without_realtime(
    mut settings: Settings,
    #[allow(unused, clippy::let_unit_value)] tracing_setup: (),
    #[future] db_connection: TestDb,
    #[future] redis_storage: JobStorage,
) -> AuthenticatedApp {
    settings.application.realtime.enabled = false;
    let app = tested_app(settings, (), db_connection, redis_storage).await;
    let (client, user) = authenticate_user(&app, "1234", "John", "Doe", "test@example.com").await;
    AuthenticatedApp { client, app, user }
}

#[rstest]
#[tokio::test]
async fn test_event_stream_is_not_found_when_realtime_is_disabled(
    #[future] authenticated_app_without_realtime: AuthenticatedApp,
) {
    let app = authenticated_app_without_realtime.await;

    let response = app
        .client
        .get(format!("{}events", app.app.api_address))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[rstest]
#[tokio::test]
async fn test_event_stream_is_refused_above_the_streams_cap(
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    let mut streams = vec![];
    for _ in 0..universal_inbox_api::universal_inbox::realtime::hub::MAX_STREAMS_PER_USER {
        streams.push(open_stream(&app.client, &app.app.api_address).await);
    }

    let response = app
        .client
        .get(format!("{}events", app.app.api_address))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}
