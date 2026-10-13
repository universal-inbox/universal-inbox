use anyhow::{Result, anyhow};
use dioxus::prelude::*;
use reqwest::Method;
use uuid::Uuid;

use universal_inbox::{
    Page, PageToken,
    audit::{AuditActorKind, AuditChangeKind, AuditEntityKind, AuditEvent, AuditEventGroup},
};

use crate::{config::APP_CONFIG, model::UI_MODEL, services::api::call_api};

/// Filters of the audit log page and of the History drawer.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AuditEventsQuery {
    pub entity_kind: Option<AuditEntityKind>,
    pub entity_id: Option<Uuid>,
    pub actor_kind: Option<AuditActorKind>,
    pub change: Option<AuditChangeKind>,
    pub query: String,
    pub page_token: Option<PageToken>,
}

impl AuditEventsQuery {
    fn to_query_string(&self) -> Result<String> {
        let mut params: Vec<(&str, String)> = vec![];
        if let Some(entity_kind) = self.entity_kind {
            params.push(("entity_kind", entity_kind.to_string()));
        }
        if let Some(entity_id) = self.entity_id {
            params.push(("entity_id", entity_id.to_string()));
        }
        if let Some(actor_kind) = self.actor_kind {
            params.push(("actor_kinds", actor_kind.to_string()));
        }
        if let Some(change) = self.change {
            params.push(("change", change.to_string()));
        }
        if !self.query.trim().is_empty() {
            params.push(("query", self.query.trim().to_string()));
        }
        if let Some(page_token) = &self.page_token {
            params.push(("page_token", page_token.to_url_parameter()?));
        }
        Ok(serde_urlencoded::to_string(params)?)
    }
}

pub async fn fetch_audit_events(query: AuditEventsQuery) -> Result<Page<AuditEvent>> {
    fetch_audit_page("audit-events", query).await
}

/// Rows of the audit log page: a bulk action is one group, so a page never splits it.
pub async fn fetch_audit_event_groups(query: AuditEventsQuery) -> Result<Page<AuditEventGroup>> {
    fetch_audit_page("audit-events/groups", query).await
}

async fn fetch_audit_page<T>(path: &str, query: AuditEventsQuery) -> Result<Page<T>>
where
    T: serde::Serialize + for<'de> serde::Deserialize<'de>,
{
    let api_base_url = APP_CONFIG
        .read()
        .as_ref()
        .map(|config| config.api_base_url.clone())
        .ok_or_else(|| anyhow!("Application configuration is not loaded"))?;
    call_api(
        Method::GET,
        &api_base_url,
        &format!("{path}?{}", query.to_query_string()?),
        None::<i32>,
        Some(UI_MODEL.signal()),
    )
    .await
}
