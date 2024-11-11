use log::error;

use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsX, Icon};

use gloo_timers::future::TimeoutFuture;
use reqwest::Method;
use url::Url;

use universal_inbox::{
    notification::NotificationWithTask,
    task::{TaskId, TaskSummary},
};

use crate::{
    components::floating_label_inputs::FloatingLabelInputSearchSelect,
    components::integrations::{icons::NotificationIcon, todoist::icons::Todoist},
    model::UniversalInboxUIModel,
    services::api::call_api,
    utils::focus_element,
};

use super::floating_label_inputs::Searchable;

#[component]
pub fn TaskLinkModal(
    api_base_url: ReadOnlySignal<Url>,
    notification_to_link: ReadOnlySignal<NotificationWithTask>,
    ui_model: Signal<UniversalInboxUIModel>,
    on_close: EventHandler<()>,
    on_task_link: EventHandler<TaskId>,
) -> Element {
    let selected_task: Signal<Option<TaskSummary>> = use_signal(|| None);
    let search_expression = use_signal(|| "".to_string());
    let mut search_results: Signal<Vec<TaskSummary>> = use_signal(Vec::new);

    let _ = use_resource(move || async move {
        if search_expression().len() > 2 {
            TimeoutFuture::new(500).await;
            *search_results.write() =
                search_tasks(&api_base_url(), &search_expression(), ui_model).await;
        }
    });

    rsx! {
        dialog {
            id: "task-link-modal",
            tabindex: "-1",
            class: "modal modal-open text-base-content backdrop-blur-sm fixed top-0 left-0 w-full h-full",
            open: true,

            div {
                class: "modal-box relative w-full overflow-visible",

                button {
                    "type": "button",
                    class: "btn btn-sm btn-ghost absolute right-2 top-5",
                    onclick: move |_| on_close.call(()),
                    tabindex: -1,

                    span { class: "sr-only", "Close" }
                    Icon { class: "w-5 h-5", icon: BsX }
                }

                div {
                    h3 {
                        class: "mb-4 text-xl font-medium",
                        "Link notification with task"
                    }

                    form {
                        class: "flex flex-col space-y-4 relative",
                        method: "dialog",
                        onsubmit: move |_| {
                            if let Some(task) = selected_task() {
                                on_task_link.call(task.id);
                            }
                        },

                        div {
                            class: "flex flex-none items-center",
                            div {
                                class: "block py-2 px-4 bg-transparent",
                                NotificationIcon { class: "h-5 w-5", kind: notification_to_link().kind }
                            }
                            div {
                                id: "notification-to-link",
                                class: "grow truncate block",
                                "{notification_to_link().title}"
                            }
                            label {
                                "for": "notification-to-link",
                                class: "absolute transform top-2 z-10 origin-[0] scale-75 -translate-y-6",
                                "Link"
                            }
                        }

                        FloatingLabelInputSearchSelect {
                            name: "task-search-input".to_string(),
                            label: Some("with".to_string()),
                            value: selected_task,
                            search_expression: search_expression,
                            search_results: search_results,
                            autofocus: true,
                            on_select: |_task| {
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
                                "Link with task"
                            }
                        }
                    }
                }
            }
        }
    }
}

async fn search_tasks(
    api_base_url: &Url,
    search: &str,
    ui_model: Signal<UniversalInboxUIModel>,
) -> Vec<TaskSummary> {
    let search_result = call_api(
        Method::GET,
        api_base_url,
        &format!("tasks/search?matches={search}"),
        None::<i32>,
        Some(ui_model),
    )
    .await;

    match search_result {
        Ok(tasks) => tasks,
        Err(error) => {
            error!("Error searching tasks: {error:?}");
            Vec::new()
        }
    }
}

impl Searchable for TaskSummary {
    fn get_title(&self) -> String {
        self.title.clone()
    }

    fn get_id(&self) -> String {
        self.id.to_string()
    }
}
