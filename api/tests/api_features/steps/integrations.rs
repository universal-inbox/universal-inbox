use chrono::Utc;
use cucumber::given;
use reqwest::StatusCode;
use slack_morphism::prelude::{SlackChannelId, SlackTeamId, SlackTs};
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::{
            github::GithubConfig,
            linear::{LinearConfig, LinearSyncTaskConfig},
            todoist::TodoistConfig,
        },
    },
    slack_bridge::{SlackBridgeActionStatus, SlackBridgeActionType, SlackBridgePendingAction},
    task::ProjectSummary,
    third_party::integrations::linear::LinearIssue,
};
use universal_inbox_api::repository::slack_bridge::SlackBridgeRepository;

use crate::{
    common::settings,
    helpers::{
        auth::authenticate_user,
        integration_connection::{
            create_and_mock_integration_connection, github_oauth_credential,
            linear_oauth_credential, todoist_oauth_credential,
        },
        notification::{
            github::{create_notification_from_github_notification, github_notification},
            linear::sync_linear_tasks_response,
        },
        task::{
            linear::create_linear_task,
            todoist::{mock_todoist_sync_resources_service, sync_todoist_projects_response},
        },
    },
    steps::reauthentication::create_api_token,
    world::{ApiWorld, Session},
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

#[given("I am connected to Todoist")]
async fn i_am_connected_to_todoist(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let connection = create_and_mock_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings(),
        todoist_oauth_credential(),
        None,
        None,
    )
    .await;
    world.integrations.todoist_connection = Some(connection);
}

#[given(expr = "I am connected to Linear, syncing its tasks into the project {string}")]
async fn i_am_connected_to_linear(world: &mut ApiWorld, project_name: String) {
    let user_id = world.user().id;
    let project = ProjectSummary {
        name: project_name,
        source_id: "1111".into(),
    };
    let app = world.app.get().await;
    let connection = create_and_mock_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::Linear(LinearConfig {
            sync_notifications_enabled: true,
            sync_task_config: LinearSyncTaskConfig {
                enabled: true,
                target_project: Some(project.clone()),
                ..Default::default()
            },
        }),
        &settings(),
        linear_oauth_credential(),
        None,
        None,
    )
    .await;
    world.integrations.linear_connection = Some(connection);
    world.integrations.linear_project = Some(project);
}

#[given("I have a task synced from Linear")]
async fn i_have_a_task_synced_from_linear(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let linear_id = world
        .integrations
        .linear_connection
        .as_ref()
        .expect("The account is not connected to Linear")
        .id;
    let todoist_id = world
        .integrations
        .todoist_connection
        .as_ref()
        .expect("The account is not connected to Todoist")
        .id;
    let project = world.integrations.linear_project.clone().unwrap();
    let issues: Vec<LinearIssue> = sync_linear_tasks_response()
        .data
        .unwrap()
        .try_into()
        .unwrap();
    let app = world.app.get().await;
    let task = create_linear_task(
        app,
        &issues[0],
        project,
        user_id,
        linear_id,
        todoist_id,
        "todoist_source_id".to_string(),
    )
    .await;
    world.integrations.linear_task = Some(task);
}

#[given("another user is signed in")]
async fn another_user_is_signed_in(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let (client, user) = authenticate_user(app, "5678", "Jane", "Doe", "jane@example.com").await;
    world.other_session = Some(Session { client, user });
}

#[given("that other user has an API token")]
async fn that_other_user_has_an_api_token(world: &mut ApiWorld) {
    let client = world.other_session.as_ref().unwrap().client.clone();
    let app = world.app.get().await;
    let response = create_api_token(&client, app).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[given("that other user is connected to GitHub")]
async fn that_other_user_is_connected_to_github(world: &mut ApiWorld) {
    let user_id = world.other_session.as_ref().unwrap().user.id;
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
    world.integrations.other_github_connection = Some(connection);
}

#[given("Todoist can list my projects")]
async fn todoist_can_list_my_projects(world: &mut ApiWorld) {
    let app = world.app.get().await;
    mock_todoist_sync_resources_service(
        &app.todoist_mock_server,
        "projects",
        &sync_todoist_projects_response(),
        None,
    )
    .await;
}
