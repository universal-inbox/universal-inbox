use log::error;

use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsX, Icon};
use fermi::UseAtomRef;
use gloo_timers::future::TimeoutFuture;
use reqwest::Method;
use url::Url;

use universal_inbox::{
    notification::{NotificationMetadata, NotificationWithTask},
    task::{TaskId, TaskSummary},
};

use crate::{
    components::floating_label_inputs::FloatingLabelInputSearchSelect,
    components::{
        icons::{GoogleMail, Linear, Todoist},
        integrations::github::icons::Github,
    },
    model::UniversalInboxUIModel,
    services::api::call_api,
    utils::focus_element,
};

use super::floating_label_inputs::Searchable;

#[inline_props]
pub fn TaskLinkModal<'a>(
    cx: Scope,
    api_base_url: Url,
    notification_to_link: NotificationWithTask,
    ui_model_ref: UseAtomRef<UniversalInboxUIModel>,
    on_close: EventHandler<'a, ()>,
    on_task_link: EventHandler<'a, TaskId>,
) -> Element {
    // tag: New notification integration
    let notification_icon = match notification_to_link.metadata {
        NotificationMetadata::Github(_) => render! { Github { class: "h-5 w-5" } },
        NotificationMetadata::Linear(_) => render! { Linear { class: "h-5 w-5" } },
        NotificationMetadata::GoogleMail(_) => render! { GoogleMail { class: "h-5 w-5" } },
        NotificationMetadata::Todoist => render! { Todoist { class: "h-5 w-5" } },
    };
    let selected_task: &UseState<Option<TaskSummary>> = use_state(cx, || None);
    let search_expression = use_state(cx, || "".to_string());
    let search_results: &UseState<Vec<TaskSummary>> = use_state(cx, Vec::new);

    use_future(cx, &search_expression.clone(), |search_expression| {
        to_owned![search_results];
        to_owned![api_base_url];
        to_owned![ui_model_ref];

        async move {
            if search_expression.len() > 2 {
                TimeoutFuture::new(500).await;
                search_results.set(
                    search_tasks(&api_base_url, &search_expression.current(), ui_model_ref).await,
                );
            }
        }
    });

    render! {
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
                        onsubmit: |_| {
                            if let Some(task) = &*selected_task.current() {
                                on_task_link.call(task.id);
                            }
                        },

                        div {
                            class: "flex flex-none items-center",
                            div { class: "block py-2 px-4 bg-transparent", notification_icon }
                            div {
                                id: "notification-to-link",
                                class: "grow truncate block",
                                "{notification_to_link.title}"
                            }
                            label {
                                "for": "notification-to-link",
                                class: "absolute transform top-2 z-10 origin-[0] scale-75 -translate-y-6",
                                "Link"
                            }
                        }

                        FloatingLabelInputSearchSelect {
                            name: "task-search-input".to_string(),
                            label: "with".to_string(),
                            value: selected_task.clone(),
                            search_expression: search_expression.clone(),
                            search_results: search_results.clone(),
                            autofocus: true,
                            on_select: |_task| {
                                cx.spawn({
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
    ui_model_ref: UseAtomRef<UniversalInboxUIModel>,
) -> Vec<TaskSummary> {
    let search_result = call_api(
        Method::GET,
        api_base_url,
        &format!("tasks/search?matches={search}"),
        None::<i32>,
        Some(ui_model_ref),
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
