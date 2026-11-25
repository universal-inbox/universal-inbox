use serde_json::{Value, json};

use universal_inbox::integration_connection::IntegrationConnectionId;

use crate::helpers::TestedApp;

pub fn event_callback(team_id: &str, event: Value, event_time: i64) -> Value {
    json!({
        "type": "event_callback",
        "team_id": team_id,
        "api_app_id": "A05XXX",
        "event": event,
        "event_id": "Ev05XXX",
        "event_time": event_time,
    })
}

pub async fn pause_for_inactivity(app: &TestedApp, id: IntegrationConnectionId) {
    sqlx::query(
        r#"
            UPDATE integration_connection
            SET status = 'Paused',
                paused_at = (now() at time zone 'utc'),
                paused_reason = 'Inactivity'
            WHERE id = $1
        "#,
    )
    .bind(id.0)
    .execute(&*app.repository.pool)
    .await
    .expect("Failed to pause the connection");
}
