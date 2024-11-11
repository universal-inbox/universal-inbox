use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use url::Url;

use universal_inbox::third_party::integrations::github::{
    GithubNotificationSubject, GithubRepository,
};

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct RawGithubNotification {
    pub id: String,
    pub repository: GithubRepository,
    pub subject: GithubNotificationSubject,
    pub reason: String,
    pub unread: bool,
    pub updated_at: DateTime<Utc>,
    pub last_read_at: Option<DateTime<Utc>>,
    #[serde_as(as = "DisplayFromStr")]
    pub url: Url,
    #[serde_as(as = "DisplayFromStr")]
    pub subscription_url: Url,
}
