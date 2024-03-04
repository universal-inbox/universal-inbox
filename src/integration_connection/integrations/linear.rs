use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct LinearConfig {
    pub sync_notifications_enabled: bool,
}

impl Default for LinearConfig {
    fn default() -> Self {
        Self {
            sync_notifications_enabled: true,
        }
    }
}

impl LinearConfig {
    pub fn enabled() -> Self {
        Self {
            sync_notifications_enabled: true,
        }
    }

    pub fn disabled() -> Self {
        Self {
            sync_notifications_enabled: false,
        }
    }
}
