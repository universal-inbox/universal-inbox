use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GithubConfig {
    pub sync_notifications_enabled: bool,
}

impl Default for GithubConfig {
    fn default() -> Self {
        Self {
            sync_notifications_enabled: true,
        }
    }
}

impl GithubConfig {
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
