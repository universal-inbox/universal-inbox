use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use universal_inbox::{notification::NotificationId, slack_bridge::SlackBridgePendingActionId};

use crate::helpers::TestedApp;

/// The whole row of a pending action: a change of any column shows.
#[derive(Debug, Clone, PartialEq, FromRow)]
pub struct ActionState {
    pub id: Uuid,
    pub user_id: Uuid,
    pub notification_id: Option<Uuid>,
    pub action_type: String,
    pub slack_team_id: String,
    pub slack_channel_id: String,
    pub slack_thread_ts: String,
    pub slack_last_message_ts: String,
    pub status: String,
    pub failure_message: Option<String>,
    pub retry_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

pub async fn read_action_state(app: &TestedApp, id: SlackBridgePendingActionId) -> ActionState {
    sqlx::query_as::<_, ActionState>(
        r#"
            SELECT *
            FROM slack_bridge_pending_action
            WHERE id = $1
        "#,
    )
    .bind(id.0)
    .fetch_one(&*app.repository.pool)
    .await
    .expect("Failed to read slack_bridge_pending_action state")
}

pub async fn count_actions(app: &TestedApp, id: SlackBridgePendingActionId) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT count(*) FROM slack_bridge_pending_action WHERE id = $1")
        .bind(id.0)
        .fetch_one(&*app.repository.pool)
        .await
        .expect("Failed to count slack_bridge_pending_action rows")
}

pub async fn delete_notification_row(app: &TestedApp, id: NotificationId) {
    sqlx::query("DELETE FROM notification WHERE id = $1")
        .bind(id.0)
        .execute(&*app.repository.pool)
        .await
        .expect("Deleting a notification referenced by a pending action must succeed");
}

pub async fn notification_reference(
    app: &TestedApp,
    id: SlackBridgePendingActionId,
) -> Option<Uuid> {
    sqlx::query_scalar("SELECT notification_id FROM slack_bridge_pending_action WHERE id = $1")
        .bind(id.0)
        .fetch_one(&*app.repository.pool)
        .await
        .expect("The pending action should still exist")
}

pub async fn pending_action_ids(
    app: &TestedApp,
    user_id: universal_inbox::user::UserId,
) -> Vec<Uuid> {
    sqlx::query_scalar("SELECT id FROM slack_bridge_pending_action WHERE user_id = $1")
        .bind(user_id.0)
        .fetch_all(&*app.repository.pool)
        .await
        .expect("Failed to list the pending actions")
}
