use chrono::Utc;
use cucumber::given;
use slack_morphism::prelude::{SlackChannelId, SlackTeamId, SlackTs};
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::github::GithubConfig,
    },
    slack_bridge::{SlackBridgeActionStatus, SlackBridgeActionType, SlackBridgePendingAction},
};
use universal_inbox_api::repository::slack_bridge::SlackBridgeRepository;

use crate::{
    common::settings,
    helpers::{
        integration_connection::{create_and_mock_integration_connection, github_oauth_credential},
        notification::github::{create_notification_from_github_notification, github_notification},
    },
    world::ApiWorld,
};

#[given("I am connected to GitHub")]
async fn i_am_connected_to_github(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let connection = create_and_mock_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        &settings(),
        github_oauth_credential(),
        None,
        None,
    )
    .await;
    world.integrations.github_connection = Some(connection);
}

#[given("I have a GitHub notification")]
async fn i_have_a_github_notification(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let connection_id = world
        .integrations
        .github_connection
        .as_ref()
        .expect("The account is not connected to GitHub")
        .id;
    let app = world.app.get().await;
    let notification = create_notification_from_github_notification(
        app,
        &github_notification(),
        user_id,
        connection_id,
    )
    .await;
    world.integrations.github_notification = Some(notification);
}

#[given("a Slack bridge action is waiting for that notification")]
async fn a_slack_bridge_action_is_waiting(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let notification_id = world
        .integrations
        .github_notification
        .as_ref()
        .expect("The account has no notification")
        .id;
    let app = world.app.get().await;
    let now = Utc::now();
    let mut transaction = app.repository.begin().await.unwrap();
    app.repository
        .create_pending_action(
            &mut transaction,
            &SlackBridgePendingAction {
                id: Uuid::new_v4().into(),
                user_id,
                notification_id: Some(notification_id),
                action_type: SlackBridgeActionType::MarkAsRead,
                slack_team_id: SlackTeamId::new("T1234".to_string()),
                slack_channel_id: SlackChannelId::new("C1234".to_string()),
                slack_thread_ts: SlackTs::new("1700000000.000100".to_string()),
                slack_last_message_ts: SlackTs::new("1700000000.000200".to_string()),
                status: SlackBridgeActionStatus::Pending,
                failure_message: None,
                retry_count: 0,
                created_at: now,
                updated_at: now,
                completed_at: None,
            },
        )
        .await
        .unwrap();
    transaction.commit().await.unwrap();
}
