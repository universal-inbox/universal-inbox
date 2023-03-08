use gloo_timers::future::TimeoutFuture;
use log::error;
use std::collections::HashMap;

use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsX, Icon};

use universal_inbox::{
    notification::{NotificationMetadata, NotificationWithTask},
    task::{TaskId, TaskSummary},
};

use crate::{
    components::floating_label_inputs::floating_label_input_search_select,
    components::icons::{github, todoist},
    services::api::call_api,
    utils::focus_element,
};

#[inline_props]
pub fn task_association_modal<'a>(
    cx: Scope,
    notification_to_associate: NotificationWithTask,
    on_close: EventHandler<'a, ()>,
    on_task_association: EventHandler<'a, TaskId>,
) -> Element {
    let notification_icon = match notification_to_associate.metadata {
        NotificationMetadata::Github(_) => cx.render(rsx!(self::github { class: "h-5 w-5" })),
        NotificationMetadata::Todoist => cx.render(rsx!(self::todoist { class: "h-5 w-5" })),
    };
    let selected_task: &UseState<Option<TaskSummary>> = use_state(cx, || None);
    let search_expression = use_state(cx, || "".to_string());
    let search_results: &UseState<Vec<TaskSummary>> = use_state(cx, Vec::new);

    use_future(cx, &search_expression.clone(), |search_expression| {
        to_owned![search_results];
        async move {
            if search_expression.len() > 2 {
                TimeoutFuture::new(500).await;
                search_results.set(search_tasks(&search_expression.current()).await);
            }
        }
    });

    cx.render(rsx!(
        dialog {
            id: "task-association-modal",
            tabindex: "-1",
            class: "modal modal-open backdrop-blur-sm fixed top-0 left-0 w-full h-full",
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
                        "Associate notification with task"
                    }

                    form {
                        class: "flex flex-col space-y-4 relative",
                        method: "dialog",
                        onsubmit: |_| {
                            if let Some(task) = &*selected_task.current() {
                                on_task_association.call(task.id);
                            }
                        },

                        div {
                            class: "flex flex-none items-center gap-2 py-2",
                            div { class: "h-5 w-5 flex-none px-4", notification_icon }
                            div {
                                id: "notification-to-associate",
                                class: "grow truncate block",
                                "{notification_to_associate.title}"
                            }
                            label {
                                "for": "notification-to-associate",
                                class: "absolute transform top-2 z-10 origin-[0] scale-75 -translate-y-6",
                                "Associate"
                            }
                        }

                        floating_label_input_search_select {
                            name: "task-search-input".to_string(),
                            label: "with".to_string(),
                            value: selected_task.clone(),
                            search_expression: search_expression.clone(),
                            search_results: search_results.clone(),
                            autofocus: true,
                            on_select: |_task| {
                                cx.spawn({
                                    async move {
                                        if let Err(error) = focus_element("task-modal-association-submit").await {
                                            error!("Error focusing element task-modal-association-submit: {error:?}");
                                        }
                                    }
                                });
                            },
                        }

                        div {
                            class: "modal-action",
                            button {
                                id: "task-modal-association-submit",
                                tabindex: 0,
                                "type": "submit",
                                class: "btn btn-primary w-full",
                                "Associate with task"
                            }
                        }
                    }
                }
            }
        }
    ))
}

async fn search_tasks(search: &str) -> Vec<TaskSummary> {
    call_api(
        "GET",
        &format!("/tasks/search?matches={}", search),
        HashMap::new(),
    )
    .await
    .unwrap()
}
