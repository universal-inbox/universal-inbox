use std::{str::FromStr, time::Duration};

use chrono::{DateTime, Utc};
use cucumber::{Parameter, given, then, when};
use pretty_assertions::assert_eq;
use reqwest::Client;
use secrecy::ExposeSecret;
use serde_json::json;

use universal_inbox::{
    integration_connection::provider::IntegrationProviderKind,
    realtime::{RealtimeEvent, RealtimeEventKind},
    user::UserId,
};
use universal_inbox_api::{
    configuration::RealtimeSettings,
    repository::integration_connection::IntegrationConnectionRepository,
    universal_inbox::realtime::{EventPublisher, RedisEventPublisher, hub::MAX_STREAMS_PER_USER},
    utils::cache::Cache,
};

use crate::{
    common::settings,
    state::EventStream,
    steps::api_tokens::api_token,
    world::{ApiWorld, Response},
};

#[derive(Clone, Copy, Debug)]
pub struct EventKind(RealtimeEventKind);

impl Parameter for EventKind {
    const REGEX: &'static str = r"tasks\.changed|integration_connection\.changed";
    const NAME: &'static str = "event_kind";
}

impl FromStr for EventKind {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "tasks.changed" => Ok(Self(RealtimeEventKind::TasksChanged)),
            "integration_connection.changed" => {
                Ok(Self(RealtimeEventKind::IntegrationConnectionChanged))
            }
            other => Err(format!("Unknown event kind {other:?}")),
        }
    }
}

async fn publish_event(user_id: UserId, kind: RealtimeEventKind) {
    let cache = Cache::new(&settings().redis).await.unwrap();
    RedisEventPublisher::new(&cache, RealtimeSettings::default().channel)
        .publish(vec![RealtimeEvent {
            user_id,
            kind,
            ids: None,
        }])
        .await;
}

fn the_stream(world: &mut ApiWorld) -> &mut EventStream {
    world
        .realtime_stream
        .own
        .last_mut()
        .expect("The scenario has no open event stream")
}

fn the_other_stream(world: &mut ApiWorld) -> &mut EventStream {
    world
        .realtime_stream
        .other
        .as_mut()
        .expect("The other user has no open event stream")
}

#[given("the app does not serve realtime events")]
fn the_app_does_not_serve_realtime_events(world: &mut ApiWorld) {
    world.app.configure().settings.application.realtime.enabled = false;
}

#[given("the app schedules the syncs of the users with an open stream every second")]
fn the_app_schedules_the_syncs(world: &mut ApiWorld) {
    world
        .app
        .configure()
        .settings
        .application
        .realtime
        .sync_scheduling_interval_in_seconds = 1;
}

#[given("I have the event stream open")]
#[when("I open the event stream")]
async fn i_have_the_event_stream_open(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    let stream = EventStream::open(&client, &app.api_address).await;
    world.realtime_stream.own.push(stream);
}

#[given("that other user has the event stream open")]
async fn that_other_user_has_the_event_stream_open(world: &mut ApiWorld) {
    let client = world
        .other_session
        .as_ref()
        .expect("There is no other user")
        .client
        .clone();
    let app = world.app.get().await;
    let stream = EventStream::open(&client, &app.api_address).await;
    world.realtime_stream.other = Some(stream);
}

async fn record_the_answer(world: &mut ApiWorld, request: reqwest::RequestBuilder) {
    let response = request.send().await.unwrap();
    world.http.record(Response {
        status: response.status(),
        headers: response.headers().clone(),
        cookies: vec![],
        body: vec![],
    });
}

#[when("I request the event stream")]
async fn i_request_the_event_stream(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    let request = client.get(format!("{}events", app.api_address));
    record_the_answer(world, request).await;
}

#[when(expr = "I request the event stream, accepting the encodings {string}")]
async fn i_request_the_event_stream_accepting(world: &mut ApiWorld, encodings: String) {
    let client = world.client();
    let app = world.app.get().await;
    let request = client
        .get(format!("{}events", app.api_address))
        .header(reqwest::header::ACCEPT_ENCODING, encodings);
    record_the_answer(world, request).await;
}

#[when("I request the event stream with my API token, instead of my session")]
async fn i_request_the_event_stream_with_my_api_token(world: &mut ApiWorld) {
    let jwt = api_token(world).jwt_token.expose_secret().0.clone();
    let app = world.app.get().await;
    let request = Client::new()
        .get(format!("{}events", app.api_address))
        .bearer_auth(jwt);
    record_the_answer(world, request).await;
}

#[when("I open as many event streams as a user may")]
async fn i_open_as_many_streams_as_a_user_may(world: &mut ApiWorld) {
    for _ in 0..MAX_STREAMS_PER_USER {
        i_have_the_event_stream_open(world).await;
    }
}

#[when(expr = "a worker publishes the event {event_kind} for me")]
async fn a_worker_publishes_for_me(world: &mut ApiWorld, kind: EventKind) {
    publish_event(world.user().id, kind.0).await;
}

#[when(expr = "a worker publishes the event {event_kind} for that other user")]
async fn a_worker_publishes_for_the_other_user(world: &mut ApiWorld, kind: EventKind) {
    let user_id = world
        .other_session
        .as_ref()
        .expect("There is no other user")
        .user
        .id;
    publish_event(user_id, kind.0).await;
}

#[when("I wait two seconds, as long as two scheduling intervals")]
async fn i_wait_two_scheduling_intervals(_world: &mut ApiWorld) {
    tokio::time::sleep(Duration::from_secs(2)).await;
}

#[then(expr = "my stream delivers the event {event_kind}, with no ids")]
async fn my_stream_delivers_with_no_ids(world: &mut ApiWorld, kind: EventKind) {
    let expected = format!("event: {}\ndata: {{}}", kind_name(kind));
    assert_eq!(the_stream(world).next_event().await, Some(expected));
}

#[then(expr = "the stream of that other user delivers the event {event_kind}, with no ids")]
async fn the_other_stream_delivers_with_no_ids(world: &mut ApiWorld, kind: EventKind) {
    let expected = format!("event: {}\ndata: {{}}", kind_name(kind));
    assert_eq!(the_other_stream(world).next_event().await, Some(expected));
}

fn kind_name(kind: EventKind) -> &'static str {
    match kind.0 {
        RealtimeEventKind::TasksChanged => "tasks.changed",
        RealtimeEventKind::IntegrationConnectionChanged => "integration_connection.changed",
        other => panic!("No name for {other:?}"),
    }
}

#[then(expr = "my stream delivers the event notifications.changed, for {string} only")]
async fn my_stream_delivers_for_a_notification(world: &mut ApiWorld, name: String) {
    let id = world.notifications.named(&name).id;
    let expected = format!(
        "event: notifications.changed\ndata: {}",
        json!({ "ids": [id.0] })
    );
    assert_eq!(the_stream(world).next_event().await, Some(expected));
}

#[then(
    "my stream delivers the event integration_connection.changed, for my GitHub connection only"
)]
async fn my_stream_delivers_for_my_connection(world: &mut ApiWorld) {
    let id = world
        .integrations
        .github_connection
        .as_ref()
        .expect("The account is not connected to GitHub")
        .id;
    let expected = format!(
        "event: integration_connection.changed\ndata: {}",
        json!({ "ids": [id.0] })
    );
    assert_eq!(the_stream(world).next_event().await, Some(expected));
}

#[then("my stream sends two heartbeats in a row")]
async fn my_stream_sends_two_heartbeats(world: &mut ApiWorld) {
    let stream = the_stream(world);
    assert_eq!(stream.next_message().await.as_deref(), Some(": heartbeat"));
    assert_eq!(stream.next_message().await.as_deref(), Some(": heartbeat"));
}

#[then("my stream ends")]
async fn my_stream_ends(world: &mut ApiWorld) {
    assert_eq!(the_stream(world).next_event().await, None);
}

async fn github_scheduled_at(world: &mut ApiWorld) -> Option<DateTime<Utc>> {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let connection = app
        .repository
        .get_integration_connection_per_provider(
            &mut transaction,
            user_id,
            IntegrationProviderKind::Github,
            None,
            None,
        )
        .await
        .unwrap()
        .expect("The account is not connected to GitHub");
    transaction.commit().await.unwrap();
    connection.last_notifications_sync_scheduled_at
}

#[given("my GitHub connection was never scheduled for a sync")]
#[then("my GitHub connection was never scheduled for a sync")]
async fn my_connection_was_never_scheduled(world: &mut ApiWorld) {
    assert_eq!(github_scheduled_at(world).await, None);
}

#[then("my GitHub connection was scheduled for a sync")]
async fn my_connection_was_scheduled(world: &mut ApiWorld) {
    assert!(github_scheduled_at(world).await.is_some());
}

#[when("a visitor without any credential requests the event stream")]
async fn a_visitor_requests_the_event_stream(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let request = Client::new().get(format!("{}events", app.api_address));
    record_the_answer(world, request).await;
}

#[then("that other user can still open an event stream")]
async fn the_other_user_can_still_open_a_stream(world: &mut ApiWorld) {
    let client = world
        .other_session
        .as_ref()
        .expect("There is no other user")
        .client
        .clone();
    let app = world.app.get().await;
    EventStream::open(&client, &app.api_address).await;
}
