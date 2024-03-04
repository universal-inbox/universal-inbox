use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct SlackConfig {
    pub sync_stars_as_notifications: bool,
}

impl Default for SlackConfig {
    fn default() -> Self {
        Self {
            sync_stars_as_notifications: true,
        }
    }
}

impl SlackConfig {
    pub fn enabled() -> Self {
        Self {
            sync_stars_as_notifications: true,
        }
    }

    pub fn disabled() -> Self {
        Self {
            sync_stars_as_notifications: false,
        }
    }
}
