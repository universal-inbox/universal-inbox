//! Realtime events pushed from the API to the web app.
//!
//! An event only tells a client that something changed for its user: the client refetches the
//! changed data through the regular REST endpoints. It carries no item content.

use serde::{Deserialize, Serialize};
use strum::Display;
use uuid::Uuid;

use crate::user::UserId;

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
pub struct RealtimeEvent {
    pub user_id: UserId,
    pub kind: RealtimeEventKind,
    /// Ids of the changed items, as a hint. `None` when unknown or too many: refetch everything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ids: Option<Vec<Uuid>>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Copy, Clone, Eq, Hash, Display)]
pub enum RealtimeEventKind {
    #[serde(rename = "notifications.changed")]
    #[strum(serialize = "notifications.changed")]
    NotificationsChanged,
    #[serde(rename = "tasks.changed")]
    #[strum(serialize = "tasks.changed")]
    TasksChanged,
    #[serde(rename = "integration_connection.changed")]
    #[strum(serialize = "integration_connection.changed")]
    IntegrationConnectionChanged,
    #[serde(rename = "user.changed")]
    #[strum(serialize = "user.changed")]
    UserChanged,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    #[test]
    fn test_realtime_event_wire_format() {
        let user_id = UserId(Uuid::nil());
        let event = RealtimeEvent {
            user_id,
            kind: RealtimeEventKind::IntegrationConnectionChanged,
            ids: Some(vec![Uuid::nil()]),
        };

        assert_eq!(
            serde_json::to_value(&event).unwrap(),
            json!({
                "user_id": Uuid::nil(),
                "kind": "integration_connection.changed",
                "ids": [Uuid::nil()]
            })
        );
        assert_eq!(
            serde_json::to_value(RealtimeEvent { ids: None, ..event }).unwrap(),
            json!({ "user_id": Uuid::nil(), "kind": "integration_connection.changed" })
        );
        assert_eq!(
            RealtimeEventKind::NotificationsChanged.to_string(),
            "notifications.changed"
        );
    }
}
