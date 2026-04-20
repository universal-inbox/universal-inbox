use dioxus::prelude::dioxus_core::use_drop;
use dioxus::prelude::*;
use dioxus::web::WebEventExt;
use log::error;
use serde_json::json;
use url::Url;

use universal_inbox::{
    integration_connection::{IntegrationConnection, provider::IntegrationProviderKind},
    notification::NotificationWithTask,
    task::{TaskId, TaskSummary},
};

use crate::{
    components::{
        floating_label_inputs::FloatingLabelInputSearchSelect,
        integrations::icons::NotificationIcon,
        task_manager_picker::{
            TaskManagerPicker, default_task_manager_kind, user_default_task_manager_kind,
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

    rsx! {
        div {
            id: "task-linking-modal",
            class: "overlay modal overlay-open:opacity-100 hidden overlay-open:duration-300",
            role: "dialog",
            tabindex: "-1",
            onmounted: move |element| {
                let web_element = element.as_web_event();
                init_flyonui_modal(&web_element);
                mounted_element.set(Some(web_element));
            },

            div {
                class: "modal-dialog overlay-open:opacity-100 overlay-open:duration-300",
                div {
                    class: "modal-content",
                    div {
                        class: "modal-header",
                        h3 { class: "modal-title", "Link notification with task" }
                        button {
                            r#type: "button",
                            class: "btn btn-text btn-circle btn-sm absolute end-3 top-3",
                            "aria-label": "Close",
                            "data-overlay": "#task-linking-modal",
                            span { class: "icon-[tabler--x] size-4" }
                        }
                    }

                    div {
                        class: "modal-body overflow-visible pt-2",

                        form {
                            class: "flex flex-col space-y-4 relative",
                            method: "dialog",
                            onsubmit: move |evt| {
                                evt.prevent_default();
                                close_flyonui_modal("#task-linking-modal");
                                if let Some(task) = selected_task() {
                                    on_task_link.call(task.id);
                                }
                            },

                            div {
                                class: "flex flex-none items-center",
                                div {
                                    class: "block py-2 px-4 bg-transparent",
                                    NotificationIcon { kind: notification_to_link.kind }
                                }
                                div {
                                    id: "notification-to-link",
                                    class: "grow truncate block",
                                    "{notification_to_link.title}"
                                }
                                label {
                                    "for": "notification-to-link",
                                    class: "absolute transform top-2 z-10 left-3 scale-75 -translate-y-6 text-base-content/50",
                                    "Link"
                                }
                            }

                            div {
                                class: "flex items-center gap-2",

                                div {
                                    class: "pl-4",
                                    TaskManagerPicker {
                                        task_service_integration_connections,
                                        selected_task_provider_kind,
                                        on_select: move |kind: IntegrationProviderKind| {
                                            *selected_task_provider_kind.write() = Some(kind);
                                            // Drop the previously-selected task — it belonged to another provider.
                                            *selected_task.write() = None;
                                        },
                                    }
                                }

                                div {
                                    class: "grow",
                                    {
                                        let kind = selected_task_provider_kind();
                                        let api_query = kind
                                            .map(|k| json!({ "provider_kind": k.to_string() }))
                                            .unwrap_or_else(|| json!({}));
                                        let key = kind
                                            .map(|k| k.to_string())
                                            .unwrap_or_else(|| "none".to_string());
                                        rsx! {
                                            FloatingLabelInputSearchSelect::<TaskSummary> {
                                                key: "task-search-{key}",
                                                name: "task-search-input".to_string(),
                                                label: Some("with".to_string()),
                                                autofocus: true,
                                                required: true,
                                                data_select: json!({
                                                    "value": selected_task(),
                                                    "apiUrl": format!("{api_base_url}tasks/search"),
                                                    "apiSearchQueryKey": "matches",
                                                    "apiQuery": api_query,
                                                    "apiFieldsMap": {
                                                        "id": "source_id",
                                                        "val": "source_id",
                                                        "title": "title"
                                                    }
                                                }),
                                                on_select: move |task| {
                                                    *selected_task.write() = task;
                                                    spawn({
                                                        async move {
                                                            if let Err(error) = focus_element("task-modal-link-submit").await {
                                                                error!("Error focusing element task-modal-link-submit: {error:?}");
                                                            }
                                                        }
                                                    });
                                                },
                                            }
                                        }
                                    }
                                }
                            }

                            div {
                                class: "modal-action",
                                button {
                                    id: "task-modal-link-submit",
                                    tabindex: 0,
                                    "type": "submit",
                                    class: "btn btn-primary w-full",
                                    "data-overlay": "#task-linking-modal",
                                    "Link with task"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
