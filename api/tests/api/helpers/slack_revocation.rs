use serde_json::{Value, json};

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
