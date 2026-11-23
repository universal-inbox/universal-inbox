use std::collections::HashMap;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use universal_inbox::notification::{Notification, NotificationId, NotificationWithTask};

#[derive(Default)]
pub struct Notifications {
    pub by_name: HashMap<String, Notification>,
    pub created: usize,
    pub listed: Vec<Notification>,
    pub snoozed_listed: Vec<NotificationWithTask>,
    pub tied: Vec<NotificationId>,
    pub tied_updated_at: Option<DateTime<Utc>>,
    pub patched: Option<Notification>,
    pub bulk_changed: Vec<Notification>,
    pub unknown_id: Option<Uuid>,
}

impl Notifications {
    pub fn named(&self, name: &str) -> &Notification {
        self.by_name
            .get(name)
            .unwrap_or_else(|| panic!("No notification is named {name:?}"))
    }
}
