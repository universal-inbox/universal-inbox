use std::str::FromStr;

use chrono::Utc;
use cucumber::{Parameter, given, then, when};
use pretty_assertions::assert_eq;
use serde_json::json;
use slack_morphism::prelude::*;
use uuid::Uuid;

use universal_inbox::notification::NotificationId;
use universal_inbox::slack_bridge::{
    SlackBridgeActionStatus, SlackBridgeActionType, SlackBridgePendingAction,
    SlackBridgePendingActionId,
};
use universal_inbox_api::repository::{slack_bridge::SlackBridgeRepository, user::UserRepository};

use crate::{
    helpers::slack_bridge::{
        count_actions, delete_notification_row, notification_reference, read_action_state,
    },
    state::Deletion,
    world::{ApiWorld, Response},
};

#[derive(Clone, Debug)]
pub struct ActionStatus(SlackBridgeActionStatus);

impl Parameter for ActionStatus {
    const REGEX: &'static str = "Pending|Failed|PermanentlyFailed|Completed";
    const NAME: &'static str = "action_status";
}

impl ActionStatus {
    /// The text the status is stored as: spelled out, so that a rename of the Rust variant
    /// cannot silently change what the scenarios compare against.
    fn sql_text(self) -> &'static str {
        match self.0 {
            SlackBridgeActionStatus::Pending => "Pending",
            SlackBridgeActionStatus::Failed => "Failed",
            SlackBridgeActionStatus::PermanentlyFailed => "PermanentlyFailed",
            SlackBridgeActionStatus::Completed => "Completed",
        }
    }
}

impl FromStr for ActionStatus {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "Pending" => Ok(Self(SlackBridgeActionStatus::Pending)),
            "Failed" => Ok(Self(SlackBridgeActionStatus::Failed)),
            "PermanentlyFailed" => Ok(Self(SlackBridgeActionStatus::PermanentlyFailed)),
            "Completed" => Ok(Self(SlackBridgeActionStatus::Completed)),
            other => Err(format!("Unknown action status {other:?}")),
        }
    }
}

fn the_action_id(world: &ApiWorld) -> SlackBridgePendingActionId {
    world
        .slack_bridge
        .action_id
        .expect("The scenario has no Slack bridge action")
}

async fn seed_action(world: &mut ApiWorld, status: SlackBridgeActionStatus) {
    seed_action_about(world, status, None).await;
}

async fn seed_action_about(
    world: &mut ApiWorld,
    status: SlackBridgeActionStatus,
    notification_id: Option<NotificationId>,
) {
    let user_id = world.user().id;
    let now = Utc::now();
    let action = SlackBridgePendingAction {
        id: Uuid::new_v4().into(),
        user_id,
        notification_id,
        action_type: SlackBridgeActionType::MarkAsRead,
        slack_team_id: SlackTeamId::new("T1234".to_string()),
        slack_channel_id: SlackChannelId::new("C1234".to_string()),
        slack_thread_ts: SlackTs::new("1700000000.000100".to_string()),
        slack_last_message_ts: SlackTs::new("1700000000.000200".to_string()),
        status,
        failure_message: None,
        retry_count: 0,
        created_at: now,
        updated_at: now,
        completed_at: None,
    };
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let created = app
        .repository
        .create_pending_action(&mut transaction, &action)
        .await
        .expect("Failed to seed the Slack bridge pending action");
    transaction.commit().await.unwrap();
    world.slack_bridge.action_id = Some(created.id);
    world.slack_bridge.notification_id = notification_id;
}

#[given(expr = "I have a {action_status} Slack bridge action")]
async fn i_have_an_action(world: &mut ApiWorld, status: ActionStatus) {
    seed_action(world, status.0).await;
}

#[given("I have a Pending Slack bridge action about that notification")]
async fn i_have_an_action_about_the_notification(world: &mut ApiWorld) {
    let notification_id = world
        .integrations
        .github_notification
        .as_ref()
        .expect("The account has no GitHub notification")
        .id;
    seed_action_about(
        world,
        SlackBridgeActionStatus::Pending,
        Some(notification_id),
    )
    .await;
}

async fn record(world: &mut ApiWorld, request: reqwest::RequestBuilder) {
    let response = request.send().await.unwrap();
    world.http.record(Response::read(response).await);
}

#[given("the Slack extension reports that action as completed")]
#[when("the Slack extension reports that action as completed")]
async fn the_extension_reports_completed(world: &mut ApiWorld) {
    let id = the_action_id(world);
    let client = world.client();
    let app = world.app.get().await;
    let request = client.post(format!(
        "{}slack-bridge/actions/{}/complete",
        app.api_address, id.0
    ));
    record(world, request).await;
}

#[when(expr = "the Slack extension reports that action as failed, with the error {string}")]
async fn the_extension_reports_failed(world: &mut ApiWorld, error: String) {
    let id = the_action_id(world);
    let client = world.client();
    let app = world.app.get().await;
    let request = client
        .post(format!(
            "{}slack-bridge/actions/{}/fail",
            app.api_address, id.0
        ))
        .json(&json!({ "error": error }));
    record(world, request).await;
}

#[when(
    expr = "the Slack extension reports that action as failed {int} times, with the error {string}"
)]
async fn the_extension_reports_failed_many_times(world: &mut ApiWorld, times: u32, error: String) {
    for _ in 0..times {
        the_extension_reports_failed(world, error.clone()).await;
        assert_eq!(world.http.last().status, 200);
    }
}

#[when("I remember the state of that action")]
#[given("I remember the state of that action")]
async fn i_remember_the_action(world: &mut ApiWorld) {
    let id = the_action_id(world);
    let app = world.app.get().await;
    world.slack_bridge.remembered = Some(read_action_state(app, id).await);
}

#[then(expr = "that action is {action_status}")]
async fn that_action_is(world: &mut ApiWorld, status: ActionStatus) {
    let id = the_action_id(world);
    let app = world.app.get().await;
    let state = read_action_state(app, id).await;
    assert_eq!(state.status, status.sql_text());
}

#[then("that action has a completion date")]
async fn that_action_has_a_completion_date(world: &mut ApiWorld) {
    let id = the_action_id(world);
    let app = world.app.get().await;
    assert!(read_action_state(app, id).await.completed_at.is_some());
}

#[then("that action has no completion date")]
async fn that_action_has_no_completion_date(world: &mut ApiWorld) {
    let id = the_action_id(world);
    let app = world.app.get().await;
    assert!(read_action_state(app, id).await.completed_at.is_none());
}

#[then("that action is exactly as I remembered it")]
async fn that_action_is_as_remembered(world: &mut ApiWorld) {
    let id = the_action_id(world);
    let app = world.app.get().await;
    let remembered = world
        .slack_bridge
        .remembered
        .clone()
        .expect("No state was remembered");
    assert_eq!(read_action_state(app, id).await, remembered);
}

#[then(expr = "that action was retried {int} times")]
async fn that_action_was_retried(world: &mut ApiWorld, retries: i32) {
    let id = the_action_id(world);
    let app = world.app.get().await;
    assert_eq!(read_action_state(app, id).await.retry_count, retries);
}

#[then(expr = "that action keeps the failure message {string}")]
async fn that_action_keeps_the_failure_message(world: &mut ApiWorld, message: String) {
    let id = the_action_id(world);
    let app = world.app.get().await;
    assert_eq!(
        read_action_state(app, id).await.failure_message.as_deref(),
        Some(message.as_str())
    );
}

#[when("my user is deleted")]
async fn my_user_is_deleted(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let deleted = app
        .repository
        .delete_user(&mut transaction, user_id)
        .await
        .expect("Deleting a user with pending Slack bridge actions must succeed");
    transaction.commit().await.unwrap();
    world.slack_bridge.deletion = Some(if deleted {
        Deletion::Deleted
    } else {
        Deletion::NothingToDelete
    });
}

#[then("that user is deleted, with that action")]
async fn that_user_is_deleted_with_the_action(world: &mut ApiWorld) {
    assert_eq!(world.slack_bridge.deletion, Some(Deletion::Deleted));
    let id = the_action_id(world);
    let app = world.app.get().await;
    assert_eq!(count_actions(app, id).await, 0);
}

#[when("that notification is deleted from the database")]
async fn that_notification_is_deleted(world: &mut ApiWorld) {
    let id = world
        .slack_bridge
        .notification_id
        .expect("The action has no notification");
    let app = world.app.get().await;
    delete_notification_row(app, id).await;
}

#[then("that action is kept, without any reference to that notification")]
async fn that_action_is_kept_without_reference(world: &mut ApiWorld) {
    let id = the_action_id(world);
    let app = world.app.get().await;
    assert_eq!(count_actions(app, id).await, 1);
    assert_eq!(notification_reference(app, id).await, None);
}

async fn report_as_the_other_user(
    world: &mut ApiWorld,
    verb: &str,
    body: Option<serde_json::Value>,
) {
    let id = the_action_id(world);
    let client = world
        .other_session
        .as_ref()
        .expect("There is no other user")
        .client
        .clone();
    let app = world.app.get().await;
    let mut request = client.post(format!(
        "{}slack-bridge/actions/{}/{verb}",
        app.api_address, id.0
    ));
    if let Some(body) = body {
        request = request.json(&body);
    }
    record(world, request).await;
}

#[when("the Slack extension of another user reports that action as completed")]
async fn another_user_reports_completed(world: &mut ApiWorld) {
    report_as_the_other_user(world, "complete", None).await;
}

#[when(
    expr = "the Slack extension of another user reports that action as failed, with the error {string}"
)]
async fn another_user_reports_failed(world: &mut ApiWorld, error: String) {
    report_as_the_other_user(world, "fail", Some(json!({ "error": error }))).await;
}
