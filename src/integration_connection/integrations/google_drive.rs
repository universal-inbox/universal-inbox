use email_address::EmailAddress;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GoogleDriveConfig {
    pub sync_notifications_enabled: bool,
}

impl Default for GoogleDriveConfig {
    fn default() -> Self {
        Self {
            sync_notifications_enabled: true,
        }
    }
}

impl GoogleDriveConfig {
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

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GoogleDriveContext {
    pub user_email_address: EmailAddress,
    pub user_display_name: String,
}
