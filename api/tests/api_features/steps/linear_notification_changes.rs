use std::str::FromStr;

use chrono::{DateTime, Utc};
use cucumber::{Parameter, given, then, when};
use graphql_client::Error;
use pretty_assertions::assert_eq;
use serde_json::Value;
use wiremock::{Mock, ResponseTemplate, matchers::any};

use universal_inbox::{
    notification::{NotificationStatus, service::NotificationPatch},
    third_party::integrations::linear::LinearNotification,
};

use crate::{
    helpers::{
        notification::linear::{
            create_notification_from_linear_notification, mock_linear_archive_notification_query,
            mock_linear_issue_notification_subscribers_query,
            mock_linear_project_notification_subscribers_query,
            mock_linear_update_issue_subscribers_query,
            mock_linear_update_notification_snoozed_until_at_query,
            sync_linear_notifications_response,
        },
        rest::patch_resource_response,
    },
    steps::status::UserStatus,
    world::{ApiWorld, Response},
};

#[derive(Clone, Copy, Debug)]
pub enum LinearKind {
    Issue,
    Project,
}

impl Parameter for LinearKind {
    const REGEX: &'static str = "issue|project";
    const NAME: &'static str = "linear_kind";
}

impl FromStr for LinearKind {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "issue" => Ok(Self::Issue),
            "project" => Ok(Self::Project),
            other => Err(format!("Unknown Linear notification kind {other:?}")),
        }
    }
}

fn the_notification_id(world: &ApiWorld, name: &str) -> String {
    world
        .notifications
        .named(name)
        .source_item
        .source_id
        .clone()
}

#[given(expr = "I have the Linear {linear_kind} notification {string}")]
async fn i_have_the_linear_notification(world: &mut ApiWorld, kind: LinearKind, name: String) {
    let linear_notifications: Vec<LinearNotification> = sync_linear_notifications_response()
        .data
        .unwrap()
        .try_into()
        .unwrap();
    let linear_notification = match kind {
        LinearKind::Issue => &linear_notifications[2],
        LinearKind::Project => &linear_notifications[0],
    };
    let user_id = world.user().id;
    let connection_id = world
        .integrations
        .linear_connection
        .as_ref()
        .expect("The account is not connected to Linear")
        .id;
    let app = world.app.get().await;
    let notification = create_notification_from_linear_notification(
        app,
        linear_notification,
        user_id,
        connection_id,
    )
    .await;
    world.notifications.by_name.insert(name, *notification);
}

#[given(expr = "Linear archives, once, the notification {string}")]
async fn linear_archives_the_notification(world: &mut ApiWorld, name: String) {
    let id = the_notification_id(world, &name);
    let app = world.app.get().await;
    mock_linear_archive_notification_query(&app.linear_mock_server, id, true, None).await;
}

#[given(
    expr = "Linear answers, once, that it did not archive the notification {string}, without any error"
)]
async fn linear_does_not_archive_the_notification(world: &mut ApiWorld, name: String) {
    let id = the_notification_id(world, &name);
    let app = world.app.get().await;
    mock_linear_archive_notification_query(&app.linear_mock_server, id, false, None).await;
}

#[given(
    expr = "Linear answers, once, with the error {string} when asked to archive the notification {string}"
)]
async fn linear_fails_to_archive_the_notification(
    world: &mut ApiWorld,
    message: String,
    name: String,
) {
    let id = the_notification_id(world, &name);
    let app = world.app.get().await;
    mock_linear_archive_notification_query(
        &app.linear_mock_server,
        id,
        true,
        Some(vec![Error {
            message,
            path: None,
            locations: None,
            extensions: None,
        }]),
    )
    .await;
}

#[given(
    expr = "Linear tells, once, that the user {string} and the user {string} follow the issue {string} of the notification {string}"
)]
async fn linear_tells_the_followers(
    world: &mut ApiWorld,
    me: String,
    other: String,
    issue: String,
    name: String,
) {
    let id = the_notification_id(world, &name);
    let app = world.app.get().await;
    mock_linear_issue_notification_subscribers_query(
        &app.linear_mock_server,
        id,
        me.clone(),
        issue,
        vec![me, other],
    )
    .await;
}

#[given(
    expr = "Linear updates, once, the followers of the issue {string} to the user {string} only"
)]
async fn linear_updates_the_followers(world: &mut ApiWorld, issue: String, remaining: String) {
    let app = world.app.get().await;
    mock_linear_update_issue_subscribers_query(
        &app.linear_mock_server,
        issue,
        vec![remaining],
        true,
        None,
    )
    .await;
}

#[given(
    expr = "Linear tells, once, that the notification {string} is about a project, which has no followers"
)]
async fn linear_tells_it_is_a_project(world: &mut ApiWorld, name: String) {
    let id = the_notification_id(world, &name);
    let app = world.app.get().await;
    mock_linear_project_notification_subscribers_query(&app.linear_mock_server, id).await;
}

#[given(expr = "Linear records, once, the snooze of the notification {string} until {string}")]
async fn linear_records_the_snooze(world: &mut ApiWorld, name: String, time: String) {
    let id = the_notification_id(world, &name);
    let until: DateTime<Utc> = DateTime::parse_from_rfc3339(&time)
        .unwrap()
        .with_timezone(&Utc);
    let app = world.app.get().await;
    mock_linear_update_notification_snoozed_until_at_query(&app.linear_mock_server, id, until)
        .await;
}

#[when(expr = "I try to delete {string}")]
async fn i_try_to_delete(world: &mut ApiWorld, name: String) {
    let id = world.notifications.named(&name).id;
    let client = world.client();
    let app = world.app.get().await;
    let response = patch_resource_response(
        &client,
        &app.api_address,
        "notifications",
        id.into(),
        &NotificationPatch {
            status: Some(NotificationStatus::Deleted),
            ..Default::default()
        },
    )
    .await;
    world.http.record(Response::read(response).await);
}

#[then(expr = "the answer is the public error {string}")]
fn the_answer_is_the_public_error(world: &mut ApiWorld, message: String) {
    let body: Value = serde_json::from_slice(&world.http.last().body).unwrap();
    assert_eq!(body["message"], message);
}

#[given("Linear is not asked anything")]
async fn linear_is_not_asked_anything(world: &mut ApiWorld) {
    let app = world.app.get().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&app.linear_mock_server)
        .await;
}

#[when(expr = "that other user tries to change the status of {string} to {status}")]
async fn that_other_user_tries_to_change_the_status(
    world: &mut ApiWorld,
    name: String,
    status: UserStatus,
) {
    let id = world.notifications.named(&name).id;
    let client = world
        .other_session
        .as_ref()
        .expect("There is no other user")
        .client
        .clone();
    let app = world.app.get().await;
    let response = patch_resource_response(
        &client,
        &app.api_address,
        "notifications",
        id.into(),
        &NotificationPatch {
            status: Some(status.0),
            ..Default::default()
        },
    )
    .await;
    world.http.record(Response::read(response).await);
}
