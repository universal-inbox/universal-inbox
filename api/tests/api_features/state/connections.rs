use chrono::{DateTime, Utc};
use universal_inbox::integration_connection::{
    IntegrationConnection, config::IntegrationConnectionConfig,
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
    pub slack: Option<Box<IntegrationConnection>>,
    pub granted: Option<Box<IntegrationConnection>>,
    pub saved_config: Option<Box<IntegrationConnectionConfig>>,
    pub unknown_id: Option<Uuid>,
    pub guards: Vec<(String, MockGuard)>,
    pub retried: Option<(usize, usize)>,
    pub stored_token_response: Option<serde_json::Value>,
    pub linear: Option<Box<IntegrationConnection>>,
    pub reported: Option<(usize, usize)>,
    pub report: Option<PauseWithoutEmailReport>,
    pub refused: Option<bool>,
    pub recorded_activity: Option<DateTime<Utc>>,
}

impl Connections {
    pub fn guard(&self, name: &str) -> &MockGuard {
        self.guards
            .iter()
            .find(|(guard_name, _)| guard_name == name)
            .map(|(_, guard)| guard)
            .unwrap_or_else(|| panic!("No provider call was prepared as {name:?}"))
    }

    pub fn release(&mut self, name: &str) {
        self.guards.retain(|(guard_name, _)| guard_name != name);
    }
}
