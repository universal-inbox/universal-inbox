use std::default::Default;

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone, Default)]
pub struct LinearConfig {
    pub sync_notifications_enabled: bool,
}

impl LinearConfig {
    pub fn enabled() -> Self {
        Self {
            sync_notifications_enabled: true,
        }
    }
}
