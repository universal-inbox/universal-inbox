use dioxus::prelude::dioxus_core::use_drop;
use dioxus::prelude::*;
#[cfg(feature = "web")]
use dioxus::web::WebEventExt;
use log::error;
use serde_json::json;
use url::Url;

use universal_inbox::{
    notification::NotificationWithTask,
    task::{TaskId, TaskSummary},
};

#[cfg(feature = "web")]
use crate::services::flyonui::{close_flyonui_modal, forget_flyonui_modal, init_flyonui_modal};
#[cfg(feature = "web")]
use crate::utils::focus_element;
use crate::{
    components::{
        floating_label_inputs::FloatingLabelInputSearchSelect,
        integrations::{icons::NotificationIcon, todoist::icons::Todoist},
    },
    config::get_api_base_url,
    model::UniversalInboxUIModel,
};

#[component]
pub fn TaskLinkModal(
    api_base_url: Url,
    notification_to_link: NotificationWithTask,
    ui_model: Signal<UniversalInboxUIModel>,
    on_task_link: EventHandler<TaskId>,
) -> Element {
    let mut selected_task: Signal<Option<TaskSummary>> = use_signal(|| None);
    #[cfg(feature = "web")]
    let mut mounted_element: Signal<Option<web_sys::Element>> = use_signal(|| None);
    #[cfg(not(feature = "web"))]
    let mut mounted_element: Signal<Option<()>> = use_signal(|| None);

    use_drop(move || {
        #[cfg(feature = "web")]
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
                #[cfg(feature = "web")]
                {
                    let web_element = element.as_web_event();
                    init_flyonui_modal(&web_element);
                    mounted_element.set(Some(web_element));
                }
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
                                #[cfg(feature = "web")]
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

                            FloatingLabelInputSearchSelect::<TaskSummary> {
                                name: "task-search-input".to_string(),
                                label: Some("with".to_string()),
                                autofocus: true,
                                required: true,
                                data_select: json!({
                                    "value": selected_task(),
                                    "apiUrl": format!("{api_base_url}tasks/search"),
                                    "apiSearchQueryKey": "matches",
                                    "apiFieldsMap": {
                                        "id": "source_id",
                                        "val": "source_id",
                                        "title": "title"
                                    }
                                }),
                                on_select: move |task| {
                                    *selected_task.write() = task;
                                    #[cfg(feature = "web")]
                                    spawn({
                                        async move {
                                            if let Err(error) = focus_element("task-modal-link-submit").await {
                                                error!("Error focusing element task-modal-link-submit: {error:?}");
                                            }
                                        }
                                    });
                                },

                                div {
                                    class: "block py-2 px-4 bg-transparent border-0 border-b-2",
                                    Todoist { class: "h-5 w-5" }
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
