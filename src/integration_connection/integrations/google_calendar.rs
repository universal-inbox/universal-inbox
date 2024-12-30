use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GoogleCalendarConfig {
    pub sync_event_details_enabled: bool,
}

impl Default for GoogleCalendarConfig {
    fn default() -> Self {
        Self {
            sync_event_details_enabled: true,
        }
    }
}

impl GoogleCalendarConfig {
    pub fn enabled() -> Self {
        Self {
            sync_event_details_enabled: true,
        }
    }

    pub fn disabled() -> Self {
        Self {
            sync_event_details_enabled: false,
        }
    }
}
