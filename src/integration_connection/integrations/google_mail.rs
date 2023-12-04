use serde::{Deserialize, Serialize};

use crate::notification::integrations::google_mail::{
    EmailAddress, GoogleMailLabel, GOOGLE_MAIL_STARRED_LABEL,
};

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GoogleMailConfig {
    pub sync_notifications_enabled: bool,
    pub synced_label: GoogleMailLabel,
}

impl Default for GoogleMailConfig {
    fn default() -> Self {
        Self {
            sync_notifications_enabled: false,
            synced_label: GoogleMailLabel {
                id: GOOGLE_MAIL_STARRED_LABEL.to_string(),
                name: GOOGLE_MAIL_STARRED_LABEL.to_string(),
            },
        }
    }
}

impl GoogleMailConfig {
    pub fn enabled() -> Self {
        Self {
            sync_notifications_enabled: true,
            ..Default::default()
        }
    }
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GoogleMailContext {
    pub user_email_address: EmailAddress,
    pub labels: Vec<GoogleMailLabel>,
}
