#![allow(non_snake_case)]

use dioxus::prelude::dioxus_core::use_drop;
use dioxus::prelude::*;
use dioxus::web::WebEventExt;

use universal_inbox::integration_connection::{
    IntegrationConnection, provider::IntegrationProviderKind,
};

use crate::{
    components::integrations::{icons::TickTick, todoist::icons::Todoist},
    model::LoadState,
    services::{
        flyonui::{forget_flyonui_dropdown_element, init_flyonui_dropdown_element},
        user_preferences_service::USER_PREFERENCES,
    },
};

#[component]
pub fn TaskManagerPicker(
    task_service_integration_connections: Signal<LoadState<Vec<IntegrationConnection>>>,
    selected_task_provider_kind: Signal<Option<IntegrationProviderKind>>,
    on_select: EventHandler<IntegrationProviderKind>,
) -> Element {
    let mut mounted_element: Signal<Option<web_sys::Element>> = use_signal(|| None);

    use_drop(move || {
        if let Some(element) = mounted_element() {
            forget_flyonui_dropdown_element(&element);
        }
    });

    let LoadState::Loaded(connections) = task_service_integration_connections() else {
        return rsx! {
            div { class: "size-5 shrink-0" }
        };
    };

    let current_icon = rsx! {
        ProviderIcon { kind: selected_task_provider_kind() }
    };

    if connections.len() < 2 {
        return rsx! {
            div { class: "size-5 shrink-0", { current_icon } }
        };
    }

    rsx! {
        div {
            class: "dropdown relative inline-flex",
            onmounted: move |element| {
                let web_element = element.as_web_event();
                init_flyonui_dropdown_element(&web_element);
                mounted_element.set(Some(web_element));
            },

            button {
                r#type: "button",
                class: "dropdown-toggle flex items-center gap-1 rounded-sm p-1 hover:bg-base-200 focus:outline-none",
                "aria-haspopup": "menu",
                "aria-expanded": "false",
                "aria-label": "Select task manager",
                tabindex: 0,
                title: "Change task manager",

                div { class: "size-5 shrink-0", { current_icon } }
                span { class: "icon-[tabler--chevron-down] size-3 text-base-content/60" }
            }

            ul {
                class: "dropdown-menu dropdown-open:opacity-100 hidden rounded-box shadow-sm z-80 p-1 min-w-40",
                role: "menu",
                "aria-orientation": "vertical",
                tabindex: 0,

                for connection in connections.iter() {
                    {
                        let kind = connection.provider.kind();
                        rsx! {
                            li { key: "{kind}",
                                button {
                                    r#type: "button",
                                    class: "dropdown-item flex items-center gap-2 w-full",
                                    onclick: move |_| on_select.call(kind),
                                    div { class: "size-5 shrink-0", ProviderIcon { kind: Some(kind) } }
                                    span { "{kind}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

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
