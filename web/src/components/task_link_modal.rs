#![allow(non_snake_case)]

use dioxus::prelude::dioxus_core::use_drop;
use dioxus::prelude::*;
use dioxus::web::WebEventExt;
use gloo_timers::future::TimeoutFuture;
use log::error;
use url::Url;

use universal_inbox::{
    integration_connection::{IntegrationConnection, provider::IntegrationProviderKind},
    notification::NotificationWithTask,
    task::{TaskId, TaskSummary},
};

use crate::{
    components::{
        integrations::icons::NotificationIcon,
        task_manager_picker::{default_task_manager_kind, user_default_task_manager_kind},
        ui::{
            ModalFooter, ModalHeader, ModalSourceRow, SEARCH_DEBOUNCE_MS, TaskAppTileSelect,
            UISearchSelect, UISelectOption,
            button::{Button, ButtonSize, ButtonVariant},
            kbd::Kbd,
            search_tasks,
        },
    },
    config::get_api_base_url,
    model::{LoadState, UniversalInboxUIModel},
    services::flyonui::{close_flyonui_modal, forget_flyonui_modal, init_flyonui_modal},
    utils::focus_element,
};

#[component]
pub fn TaskLinkModal(
    api_base_url: Url,
    notification_to_link: NotificationWithTask,
    task_service_integration_connections: Signal<LoadState<Vec<IntegrationConnection>>>,
    ui_model: Signal<UniversalInboxUIModel>,
    on_task_link: EventHandler<TaskId>,
) -> Element {
    let mut selected_task: Signal<Option<TaskSummary>> = use_signal(|| None);
    let mut selected_task_provider_kind: Signal<Option<IntegrationProviderKind>> =
        use_signal(|| None);
    let mut mounted_element: Signal<Option<web_sys::Element>> = use_signal(|| None);

    // Auto-select the default task service when the connections load.
    let _ = use_memo(move || {
        if selected_task_provider_kind.peek().is_none()
            && let LoadState::Loaded(connections) = task_service_integration_connections()
            && let Some(kind) =
                default_task_manager_kind(&connections, user_default_task_manager_kind())
        {
            *selected_task_provider_kind.write() = Some(kind);
        }
    });

    use_drop(move || {
        if let Some(element) = mounted_element() {
            forget_flyonui_modal(&element);
        }
    });
    let api_base_url = get_api_base_url().unwrap();

    let task_app_options = use_memo(move || match task_service_integration_connections() {
        LoadState::Loaded(connections) => connections
            .iter()
            .map(|c| {
                let kind = c.provider.kind();
                UISelectOption::new(kind, kind.to_string())
            })
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    });

    let invalid = use_memo(move || selected_task.read().is_none());

    let kind_for_icon = notification_to_link.kind;
    let notification_title = notification_to_link.title.clone();

    rsx! {
        div {
            id: "task-linking-modal",
            class: "overlay modal overlay-open:opacity-100 hidden overlay-open:duration-300",
            role: "dialog",
            "aria-modal": "true",
            "aria-labelledby": "link-task-title",
            tabindex: "-1",
            onmounted: move |element| {
                let web_element = element.as_web_event();
                init_flyonui_modal(&web_element);
                mounted_element.set(Some(web_element));
            },

            div {
                class: "modal-dialog overlay-open:opacity-100 overlay-open:duration-300",
                div {
                    class: "modal-content w-[460px] max-w-[calc(100vw-32px)] p-0 border border-ui-border shadow-ui-lg bg-ui-surface",

                    ModalHeader {
                        eyebrow: "From notification".to_string(),
                        title: "Link to a task".to_string(),
                        title_id: "link-task-title".to_string(),
                        overlay_id: "#task-linking-modal".to_string(),
                    }

                    ModalSourceRow {
                        eyebrow: "Notification".to_string(),
                        title: notification_title.clone(),
                        tile: rsx! { NotificationIcon { kind: kind_for_icon } },
                    }

                    form {
                        method: "dialog",
                        onsubmit: move |evt| {
                            evt.prevent_default();
                            if invalid() {
                                return;
                            }
                            if let Some(task) = selected_task() {
                                close_flyonui_modal("#task-linking-modal");
                                on_task_link.call(task.id);
                            }
                        },

                        div { class: "flex flex-col gap-3 px-4 pt-3.5 pb-1",

                            div { class: "grid grid-cols-[44px_1fr] gap-2.5 items-end",

                                div { class: "flex flex-col gap-1.5",
                                    span { class: "text-[11px] font-semibold text-ui-base-muted tracking-[0.01em] inline-flex items-center gap-1", "App" }
                                    TaskAppTileSelect {
                                        value: selected_task_provider_kind,
                                        options: task_app_options(),
                                        on_change: move |kind: Option<IntegrationProviderKind>| {
                                            selected_task_provider_kind.set(kind);
                                            // Drop the previously-selected task — it belonged to another provider.
                                            selected_task.set(None);
                                        },
                                    }
                                }

                                div { class: "flex flex-col gap-1.5",
                                    span { class: "text-[11px] font-semibold text-ui-base-muted tracking-[0.01em] inline-flex items-center gap-1",
                                        "Existing task"
                                        span { class: "text-ui-error-hover ml-0.5", "aria-hidden": "true", "*" }
                                    }
                                    TaskSearchField {
                                        api_base_url: api_base_url.clone(),
                                        selected_task,
                                        selected_task_provider_kind,
                                    }
                                }
                            }
                        }

                        ModalFooter {
                            hint: rsx! {
                                Kbd { label: "Tab".to_string() } span { "to move" }
                                span { class: "opacity-60", "·" }
                                Kbd { label: "Esc".to_string() } span { "cancel" }
                            },
                            Button {
                                variant: ButtonVariant::Ghost,
                                size: ButtonSize::Sm,
                                button_type: "button".to_string(),
                                data_overlay: "#task-linking-modal".to_string(),
                                onclick: move |_| close_flyonui_modal("#task-linking-modal"),
                                "Cancel"
                            }
                            Button {
                                id: "task-modal-link-submit".to_string(),
                                variant: ButtonVariant::Primary,
                                size: ButtonSize::Sm,
                                button_type: "submit".to_string(),
                                disabled: invalid(),
                                "Link task"
                                Kbd { label: "↵".to_string() }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn TaskSearchField(
    api_base_url: ReadSignal<Url>,
    selected_task: Signal<Option<TaskSummary>>,
    selected_task_provider_kind: Signal<Option<IntegrationProviderKind>>,
) -> Element {
    let mut query = use_signal(String::new);
    let mut loading = use_signal(|| false);
    let mut options = use_signal(Vec::<UISelectOption<TaskSummary>>::new);

    let _resource = use_resource(move || async move {
        let q = query();
        let kind = selected_task_provider_kind();
        if q.trim().is_empty() {
            options.set(Vec::new());
            loading.set(false);
            return;
        }
        loading.set(true);
        TimeoutFuture::new(SEARCH_DEBOUNCE_MS).await;
        // Bail if the user kept typing while we were waiting.
        if query() != q {
            return;
        }
        match search_tasks(&api_base_url(), &q, kind).await {
            Ok(results) => options.set(results),
            Err(err) => {
                error!("Failed to search tasks: {err:?}");
                options.set(Vec::new());
            }
        }
        loading.set(false);
    });

    rsx! {
        UISearchSelect::<TaskSummary> {
            value: selected_task,
            options: options(),
            on_change: move |task: Option<TaskSummary>| {
                selected_task.set(task);
                spawn(async move {
                    if let Err(err) = focus_element("task-modal-link-submit").await {
                        error!("Error focusing task-modal-link-submit: {err:?}");
                    }
                });
            },
            on_query: move |q: String| { query.set(q); },
            loading: loading(),
            placeholder: "Search tasks by title…".to_string(),
            search_placeholder: "Type to search tasks…".to_string(),
            empty_hint: "Try a different keyword.".to_string(),
            allow_clear: true,
            name: "task-search-input".to_string(),
            width: "100%".to_string(),
        }
    }
}
