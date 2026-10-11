use chrono::{DateTime, Utc};
use email_address::EmailAddress;
use serde::{Deserialize, Serialize};

use crate::pii::Pii;

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct NotionConfig {
    /// Notify on comments in the discussions the user takes part in.
    pub sync_comments_enabled: bool,
    /// Notify when the user is @mentioned in a page body.
    pub sync_mentions_enabled: bool,
}

impl Default for NotionConfig {
    fn default() -> Self {
        Self::enabled()
    }
}

impl NotionConfig {
    pub fn enabled() -> Self {
        Self {
            sync_comments_enabled: true,
            sync_mentions_enabled: true,
        }
    }

    pub fn disabled() -> Self {
        Self {
            sync_comments_enabled: false,
            sync_mentions_enabled: false,
        }
    }
}

/// Read from the OAuth token response when the user connects Notion. The
/// human Notion user id (`owner.user.id`) is the connection's
/// `provider_user_id`; `GET /v1/users/me` would return the bot instead.
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct NotionContext {
    pub workspace_id: String,
    pub workspace_name: Option<String>,
    pub bot_id: String,
    pub user_display_name: Option<Pii<String>>,
    pub user_email_address: Option<Pii<EmailAddress>>,
    /// When the sync last re-read the tracked discussions.
    #[serde(default)]
    pub comments_reconciled_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    #[test]
    fn test_context_stored_before_comment_reconciliation_deserializes() {
        let context: NotionContext = serde_json::from_value(json!({
            "workspace_id": "workspace-id",
            "workspace_name": "Acme",
            "bot_id": "bot-id",
            "user_display_name": null,
            "user_email_address": null
        }))
        .unwrap();

        assert_eq!(context.comments_reconciled_at, None);
    }
}
