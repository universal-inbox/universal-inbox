use chrono::{DateTime, Utc};

use universal_inbox::integration_connection::{IntegrationConnection, IntegrationConnectionId};
use universal_inbox_api::commands::slack::BackfillTeamIdReport;

#[derive(Debug, PartialEq)]
pub struct Remembered {
    pub connection: IntegrationConnection,
    pub credential: Option<StoredTokens>,
}

#[derive(Debug, PartialEq)]
pub struct StoredTokens {
    pub encrypted_access_token: Vec<u8>,
    pub encrypted_refresh_token: Option<Vec<u8>>,
    pub access_token_expires_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Default)]
pub struct SlackBackfill {
    pub mine: Option<IntegrationConnectionId>,
    pub others: Option<IntegrationConnectionId>,
    pub report: Option<BackfillTeamIdReport>,
    pub remembered: Option<Remembered>,
}
