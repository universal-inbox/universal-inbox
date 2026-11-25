use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use universal_inbox::{notification::NotificationId, slack_bridge::SlackBridgePendingActionId};

use crate::helpers::TestedApp;

#[derive(Debug, Clone, PartialEq, FromRow)]
pub struct ActionState {
    pub status: String,
    pub retry_count: i32,
    pub completed_at: Option<DateTime<Utc>>,
    pub failure_message: Option<String>,
}

pub async fn read_action_state(app: &TestedApp, id: SlackBridgePendingActionId) -> ActionState {
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
