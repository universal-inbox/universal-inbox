use std::collections::HashMap;

use chrono::{DateTime, Utc};
use universal_inbox::integration_connection::{
    IntegrationConnection, config::IntegrationConnectionConfig, provider::IntegrationProviderKind,
};
use universal_inbox_api::universal_inbox::{
    UniversalInboxError, integration_connection::service::PauseWithoutEmailReport,
};
use uuid::Uuid;
use wiremock::MockGuard;

#[derive(Default)]
pub struct Connections {
    pub created: Vec<IntegrationConnection>,
    pub listed: Vec<IntegrationConnection>,
    pub disconnected: Option<IntegrationConnection>,
    pub google_calendar: Option<IntegrationConnection>,
    pub token_lookup: Option<Result<String, UniversalInboxError>>,
    pub mail: Option<Box<IntegrationConnection>>,
    pub owned: HashMap<IntegrationProviderKind, Box<IntegrationConnection>>,
    pub granted: Option<Box<IntegrationConnection>>,
    pub saved_config: Option<Box<IntegrationConnectionConfig>>,
    pub unknown_id: Option<Uuid>,
    pub calls: ProviderCalls,
    pub retried: Option<(usize, usize)>,
    pub stored_token_response: Option<serde_json::Value>,
    pub reported: Option<(usize, usize)>,
    pub report: Option<PauseWithoutEmailReport>,
    pub refused: Option<bool>,
    pub recorded_activity: Option<DateTime<Utc>>,
}

impl Connections {
    pub fn keep(&mut self, kind: IntegrationProviderKind, connection: Box<IntegrationConnection>) {
        self.owned.insert(kind, connection);
    }

    pub fn owned(&self, kind: IntegrationProviderKind) -> &IntegrationConnection {
        self.owned
            .get(&kind)
            .unwrap_or_else(|| panic!("No {kind:?} connection in this scenario"))
    }
}

#[derive(Default)]
pub struct ProviderCalls {
    pub revocations: HashMap<IntegrationProviderKind, MockGuard>,
    pub failure: Option<MockGuard>,
    pub expired: Option<MockGuard>,
    pub renewal: Option<MockGuard>,
}
