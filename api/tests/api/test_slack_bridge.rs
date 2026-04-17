use chrono::Utc;
use pretty_assertions::assert_eq;
use rstest::*;
use serde_json::json;
use slack_morphism::prelude::*;
use sqlx::FromRow;
use uuid::Uuid;

use universal_inbox::slack_bridge::{
    SlackBridgeActionStatus, SlackBridgeActionType, SlackBridgePendingAction,
    SlackBridgePendingActionId,
};
use universal_inbox_api::repository::slack_bridge::SlackBridgeRepository;

use crate::helpers::{
    TestedApp,
    auth::{AuthenticatedApp, authenticated_app},
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
    let now = Utc::now();
    let action = SlackBridgePendingAction {
        id: Uuid::new_v4().into(),
        user_id: app.user.id,
        notification_id: None,
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
