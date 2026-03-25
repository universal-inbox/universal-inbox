//! Async option loaders for [`UISearchSelect`](super::select::UISearchSelect)
//! consumers.
//!
//! Each helper hits a search endpoint and maps the response into
//! [`UISelectOption`] so the call site stays focused on wiring the
//! `use_resource` + debounce + signal updates rather than HTTP plumbing.

use anyhow::Result;
use reqwest::Method;
use serde::Serialize;
use url::Url;

use universal_inbox::{
    integration_connection::{
        IntegrationConnectionId, integrations::slack::SlackEmojiSuggestion,
        provider::IntegrationProviderKind,
    },
    task::{ProjectSummary, TaskSummary},
};

use super::select::UISelectOption;
use crate::services::api::call_api;

/// Default debounce window between a keystroke and the actual API call.
pub const SEARCH_DEBOUNCE_MS: u32 = 200;

#[derive(Serialize)]
struct SearchParams<'a> {
    matches: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_kind: Option<IntegrationProviderKind>,
}

fn build_search_path(endpoint: &str, query: &str, kind: Option<IntegrationProviderKind>) -> String {
    let qs = serde_urlencoded::to_string(SearchParams {
        matches: query,
        provider_kind: kind,
    })
    .unwrap_or_default();
    format!("{endpoint}?{qs}")
}

/// Search tasks via `GET /tasks/search?matches=…&provider_kind=…`.
/// Returns options keyed on `TaskSummary` with the task title as the label.
pub async fn search_tasks(
    api_base_url: &Url,
    query: &str,
    provider_kind: Option<IntegrationProviderKind>,
) -> Result<Vec<UISelectOption<TaskSummary>>> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let path = build_search_path("tasks/search", query, provider_kind);
    let tasks: Vec<TaskSummary> =
        call_api::<Vec<TaskSummary>, ()>(Method::GET, api_base_url, &path, None, None).await?;
    Ok(tasks
        .into_iter()
        .map(|t| {
            let label = t.title.clone();
            let search_text = format!("{} {}", t.title, t.project);
            UISelectOption::new(t, label).with_search_text(search_text)
        })
        .collect())
}

/// Search Slack emoji via `GET /integration-connections/{id}/slack/emojis/search?matches=…`.
pub async fn search_slack_emojis(
    api_base_url: &Url,
    connection_id: IntegrationConnectionId,
    query: &str,
) -> Result<Vec<UISelectOption<SlackEmojiSuggestion>>> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let qs = serde_urlencoded::to_string([("matches", query)]).unwrap_or_default();
    let path = format!("integration-connections/{connection_id}/slack/emojis/search?{qs}");
    let suggestions: Vec<SlackEmojiSuggestion> =
        call_api::<Vec<SlackEmojiSuggestion>, ()>(Method::GET, api_base_url, &path, None, None)
            .await?;
    Ok(suggestions
        .into_iter()
        .map(|s| {
            let label = format!(":{}:", s.name);
            let search_text = format!("{} {}", s.name, s.display_name);
            UISelectOption::new(s, label).with_search_text(search_text)
        })
        .collect())
}

/// Search projects via `GET /tasks/projects/search?matches=…&provider_kind=…`.
pub async fn search_projects(
    api_base_url: &Url,
    query: &str,
    provider_kind: Option<IntegrationProviderKind>,
) -> Result<Vec<UISelectOption<ProjectSummary>>> {
    let path = build_search_path("tasks/projects/search", query, provider_kind);
    let projects: Vec<ProjectSummary> =
        call_api::<Vec<ProjectSummary>, ()>(Method::GET, api_base_url, &path, None, None).await?;
    Ok(projects
        .into_iter()
        .map(|p| {
            let label = p.name.clone();
            UISelectOption::new(p, label)
        })
        .collect())
}
