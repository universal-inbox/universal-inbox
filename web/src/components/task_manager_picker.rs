#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::integration_connection::{
    IntegrationConnection, provider::IntegrationProviderKind,
};

use crate::{
    components::integrations::{icons::TickTick, todoist::icons::Todoist},
    services::user_preferences_service::USER_PREFERENCES,
};

#[component]
pub fn ProviderIcon(kind: Option<IntegrationProviderKind>) -> Element {
    match kind {
        Some(IntegrationProviderKind::Todoist) => rsx! { Todoist {} },
        Some(IntegrationProviderKind::TickTick) => rsx! { TickTick {} },
        _ => rsx! {},
    }
}

/// Best default task manager for a picker: the user's configured preference
/// when they actually have that service connected; otherwise the first
/// connected task service; otherwise `None`.
pub fn default_task_manager_kind(
    connections: &[IntegrationConnection],
    user_default: Option<IntegrationProviderKind>,
) -> Option<IntegrationProviderKind> {
    user_default
        .filter(|kind| connections.iter().any(|c| c.provider.kind() == *kind))
        .or_else(|| connections.first().map(|c| c.provider.kind()))
}

/// Read the user's default task manager preference from the global signal.
pub fn user_default_task_manager_kind() -> Option<IntegrationProviderKind> {
    USER_PREFERENCES
        .read()
        .as_ref()
        .and_then(|p| p.default_task_manager_provider_kind)
}

/// Resolve the task manager provider for project queries: explicit config
/// choice, then the user's preference, then Todoist as a safe default.
pub fn resolve_task_manager_kind(
    explicit: Option<IntegrationProviderKind>,
) -> IntegrationProviderKind {
    explicit
        .or_else(user_default_task_manager_kind)
        .unwrap_or(IntegrationProviderKind::Todoist)
}
