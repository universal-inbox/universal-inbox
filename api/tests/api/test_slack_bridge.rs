use chrono::Utc;
use pretty_assertions::assert_eq;
use rstest::*;
use serde_json::json;
use slack_morphism::prelude::*;
use sqlx::FromRow;
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::github::GithubConfig,
    },
    notification::NotificationId,
    slack_bridge::{
        SlackBridgeActionStatus, SlackBridgeActionType, SlackBridgePendingAction,
        SlackBridgePendingActionId,
    },
    third_party::integrations::github::GithubNotification,
};
use universal_inbox_api::{
    configuration::Settings,
    repository::{slack_bridge::SlackBridgeRepository, user::UserRepository},
};

use crate::helpers::{
    TestedApp,
    auth::{AuthenticatedApp, authenticated_app},
    integration_connection::{
        OAuthCredentialFixture, create_and_mock_integration_connection, github_oauth_credential,
    },
    notification::github::{create_notification_from_github_notification, github_notification},
    settings,
};

#[derive(Debug, FromRow)]
struct ActionState {
    status: String,
    retry_count: i32,
    completed_at: Option<chrono::DateTime<Utc>>,
    failure_message: Option<String>,
}

async fn read_action_state(app: &TestedApp, id: SlackBridgePendingActionId) -> ActionState {
    sqlx::query_as::<_, ActionState>(
        r#"
            SELECT status, retry_count, completed_at, failure_message
            FROM slack_bridge_pending_action
            WHERE id = $1
        "#,
    )
    .bind(id.0)
    .fetch_one(&*app.repository.pool)
    .await
    .expect("Failed to read slack_bridge_pending_action state")
}

async fn seed_action(
    app: &AuthenticatedApp,
    status: SlackBridgeActionStatus,
) -> SlackBridgePendingActionId {
    seed_action_for_notification(app, status, None).await
}

async fn seed_action_for_notification(
    app: &AuthenticatedApp,
    status: SlackBridgeActionStatus,
    notification_id: Option<NotificationId>,
) -> SlackBridgePendingActionId {
    let now = Utc::now();
    let action = SlackBridgePendingAction {
        id: Uuid::new_v4().into(),
        user_id: app.user.id,
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

    let mut tx = app.app.repository.begin().await.unwrap();
    let created = app
        .app
        .repository
        .create_pending_action(&mut tx, &action)
        .await
        .expect("Failed to seed slack bridge pending action");
    tx.commit().await.unwrap();
    created.id
}

async fn post_complete(app: &AuthenticatedApp, id: SlackBridgePendingActionId) -> u16 {
    app.client
        .post(format!(
            "{}slack-bridge/actions/{}/complete",
            app.app.api_address, id.0
        ))
        .send()
        .await
        .unwrap()
        .status()
        .as_u16()
}

async fn post_fail(app: &AuthenticatedApp, id: SlackBridgePendingActionId, error: &str) -> u16 {
    app.client
        .post(format!(
            "{}slack-bridge/actions/{}/fail",
            app.app.api_address, id.0
        ))
        .json(&json!({ "error": error }))
        .send()
        .await
        .unwrap()
        .status()
        .as_u16()
}

#[rstest]
#[tokio::test]
async fn test_complete_pending_action_succeeds(#[future] authenticated_app: AuthenticatedApp) {
    let app = authenticated_app.await;
    let action_id = seed_action(&app, SlackBridgeActionStatus::Pending).await;

    assert_eq!(post_complete(&app, action_id).await, 200);

    let state = read_action_state(&app.app, action_id).await;
    assert_eq!(state.status, "Completed");
    assert!(state.completed_at.is_some());
}

#[rstest]
#[tokio::test]
async fn test_complete_failed_action_succeeds(#[future] authenticated_app: AuthenticatedApp) {
    let app = authenticated_app.await;
    let action_id = seed_action(&app, SlackBridgeActionStatus::Failed).await;

    assert_eq!(post_complete(&app, action_id).await, 200);

    let state = read_action_state(&app.app, action_id).await;
    assert_eq!(state.status, "Completed");
    assert!(state.completed_at.is_some());
}

#[rstest]
#[tokio::test]
async fn test_fail_after_complete_is_noop(#[future] authenticated_app: AuthenticatedApp) {
    let app = authenticated_app.await;
    let action_id = seed_action(&app, SlackBridgeActionStatus::Pending).await;

    assert_eq!(post_complete(&app, action_id).await, 200);
    let after_complete = read_action_state(&app.app, action_id).await;
    assert_eq!(after_complete.status, "Completed");

    // A stale failure report from the extension must not revert the action.
    assert_eq!(post_fail(&app, action_id, "stale error").await, 200);

    let state = read_action_state(&app.app, action_id).await;
    assert_eq!(state.status, "Completed");
    assert_eq!(state.retry_count, 0);
    assert!(state.failure_message.is_none());
    assert_eq!(state.completed_at, after_complete.completed_at);
}

#[rstest]
#[tokio::test]
async fn test_complete_after_permanently_failed_is_noop(
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    let action_id = seed_action(&app, SlackBridgeActionStatus::PermanentlyFailed).await;

    assert_eq!(post_complete(&app, action_id).await, 200);

    let state = read_action_state(&app.app, action_id).await;
    assert_eq!(state.status, "PermanentlyFailed");
    assert!(state.completed_at.is_none());
}

#[rstest]
#[tokio::test]
async fn test_fail_transitions_to_permanently_failed_after_max_retries(
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    let action_id = seed_action(&app, SlackBridgeActionStatus::Pending).await;

    // MAX_RETRIES = 5: first 4 failures stay in Failed, 5th transitions to PermanentlyFailed.
    for _ in 0..4 {
        assert_eq!(post_fail(&app, action_id, "transient").await, 200);
    }
    let state = read_action_state(&app.app, action_id).await;
    assert_eq!(state.status, "Failed");
    assert_eq!(state.retry_count, 4);

    assert_eq!(post_fail(&app, action_id, "terminal").await, 200);
    let state = read_action_state(&app.app, action_id).await;
    assert_eq!(state.status, "PermanentlyFailed");
    assert_eq!(state.retry_count, 5);
    assert_eq!(state.failure_message.as_deref(), Some("terminal"));
}

async fn count_actions(app: &TestedApp, id: SlackBridgePendingActionId) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT count(*) FROM slack_bridge_pending_action WHERE id = $1")
        .bind(id.0)
        .fetch_one(&*app.repository.pool)
        .await
        .expect("Failed to count slack_bridge_pending_action rows")
}

async fn seed_notification(
    app: &AuthenticatedApp,
    settings: &Settings,
    github_notification: &GithubNotification,
    github_oauth_credential: OAuthCredentialFixture,
) -> NotificationId {
    let github_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        settings,
        github_oauth_credential,
        None,
        None,
    )
    .await;

    create_notification_from_github_notification(
        &app.app,
        github_notification,
        app.user.id,
        github_integration_connection.id,
    )
    .await
    .id
}

#[rstest]
#[tokio::test]
async fn test_delete_user_with_pending_actions_succeeds(
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    let action_id = seed_action(&app, SlackBridgeActionStatus::Pending).await;

    let mut transaction = app.app.repository.begin().await.unwrap();
    let deleted = app
        .app
        .repository
        .delete_user(&mut transaction, app.user.id)
        .await
        .expect("Deleting a user with pending Slack bridge actions must succeed");
    transaction.commit().await.unwrap();

    assert!(deleted);
    assert_eq!(count_actions(&app.app, action_id).await, 0);
}

#[rstest]
#[tokio::test]
async fn test_delete_notification_referenced_by_pending_action_succeeds(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    github_notification: Box<GithubNotification>,
    github_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    let notification_id = seed_notification(
        &app,
        &settings,
        &github_notification,
        github_oauth_credential,
    )
    .await;
    let action_id = seed_action_for_notification(
        &app,
        SlackBridgeActionStatus::Pending,
        Some(notification_id),
    )
    .await;

    sqlx::query("DELETE FROM notification WHERE id = $1")
        .bind(notification_id.0)
        .execute(&*app.app.repository.pool)
        .await
        .expect("Deleting a notification referenced by a pending action must succeed");

    // The action is kept (the extension replays it from its Slack coordinates),
    // only the reference to the deleted notification is dropped.
    let notification_ref: Option<Uuid> =
        sqlx::query_scalar("SELECT notification_id FROM slack_bridge_pending_action WHERE id = $1")
            .bind(action_id.0)
            .fetch_one(&*app.app.repository.pool)
            .await
            .expect("The pending action should still exist");
    assert_eq!(notification_ref, None);
    assert_eq!(
        read_action_state(&app.app, action_id).await.status,
        "Pending"
    );
}
