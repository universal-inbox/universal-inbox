#![allow(non_snake_case)]

use dioxus::prelude::*;

use dioxus_free_icons::{
    icons::{
        bs_icons::{
            BsBellSlash, BsBookmarkCheck, BsCalendar2Check, BsClockHistory, BsLink45deg, BsTrash,
        },
        md_action_icons::{MdAddTask, MdCheckCircleOutline},
    },
    Icon,
};

use universal_inbox::{
    notification::NotificationWithTask,
    task::{Task, TaskId, TaskPlanning, TaskPriority},
    third_party::item::ThirdPartyItemData,
    HasHtmlUrl,
};

use crate::{
    components::{
        integrations::{
            github::notification_list_item::GithubNotificationListItem,
            google_calendar::notification_list_item::GoogleCalendarEventListItem,
            google_mail::notification_list_item::GoogleMailThreadListItem,
            linear::notification_list_item::LinearNotificationListItem,
            slack::notification_list_item::{
                SlackReactionNotificationListItem, SlackStarNotificationListItem,
                SlackThreadNotificationListItem,
            },
            todoist::notification_list_item::TodoistNotificationListItem,
        },
        list::{List, ListItemActionButton},
        task_link_modal::TaskLinkModal,
        task_planning_modal::TaskPlanningModal,
    },
    config::get_api_base_url,
    model::UI_MODEL,
    services::{
        integration_connection_service::TASK_SERVICE_INTEGRATION_CONNECTION,
        notification_service::NotificationCommand,
    },
};

#[derive(Clone, PartialEq)]
pub struct NotificationListContext {
    pub is_task_actions_enabled: bool,
    pub notification_service: Coroutine<NotificationCommand>,
}

#[component]
pub fn NotificationsList(notifications: ReadOnlySignal<Vec<NotificationWithTask>>) -> Element {
    let api_base_url = use_memo(move || get_api_base_url().unwrap());
    let notification_service = use_coroutine_handle::<NotificationCommand>();
    let context = use_memo(move || NotificationListContext {
        is_task_actions_enabled: UI_MODEL.read().is_task_actions_enabled,
        notification_service,
    });
    use_context_provider(move || context);

    rsx! {
        List {
            id: "notifications_list",
            show_shortcut: UI_MODEL.read().is_help_enabled,

            tbody {
                for (i, notification) in notifications().into_iter().map(Signal::new).enumerate() {
                    NotificationListItem {
                        notification,
                        is_selected: i == UI_MODEL.read().selected_notification_index,
                        on_select: move |_| {
                            UI_MODEL.write().selected_notification_index = i;
                        },
                    }
                }
            }
        }

        if UI_MODEL.read().task_planning_modal_opened {
            if let Some(notification) = notifications()
                .get(UI_MODEL.read().selected_notification_index)
                .map(|notification| Signal::new(notification.clone())) {
                TaskPlanningModal {
                    notification_to_plan: notification,
                    task_service_integration_connection: TASK_SERVICE_INTEGRATION_CONNECTION.signal(),
                    ui_model: UI_MODEL.signal(),
                    on_close: move |_| { UI_MODEL.write().task_planning_modal_opened = false; },
                    on_task_planning: move |(params, task_id): (TaskPlanning, TaskId)| {
                        UI_MODEL.write().task_planning_modal_opened = false;
                        notification_service.send(NotificationCommand::PlanTask(
                            notification(),
                            task_id,
                            params
                        ));
                    },
                    on_task_creation: move |params| {
                        UI_MODEL.write().task_planning_modal_opened = false;
                        notification_service.send(NotificationCommand::CreateTaskFromNotification(
                            notification(),
                            params
                        ));
                    },
                }
            }
        }

        if UI_MODEL.read().task_link_modal_opened {
            if let Some(notification) = notifications()
                .get(UI_MODEL.read().selected_notification_index)
                .map(|notification| Signal::new(notification.clone())) {
                TaskLinkModal {
                    api_base_url,
                    notification_to_link: notification,
                    ui_model: UI_MODEL.signal(),
                    on_close: move |_| { UI_MODEL.write().task_link_modal_opened = false; },
                    on_task_link: move |task_id| {
                        UI_MODEL.write().task_link_modal_opened = false;
                        notification_service.send(NotificationCommand::LinkNotificationWithTask(
                            notification().id,
                            task_id,
                        ));
                    },
                }
            }
        }
    }
}

#[component]
fn NotificationListItem(
    notification: ReadOnlySignal<NotificationWithTask>,
    is_selected: ReadOnlySignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    match notification().source_item.data {
        ThirdPartyItemData::GithubNotification(github_notification) => rsx! {
            GithubNotificationListItem {
                notification,
                github_notification: *github_notification,
                is_selected,
                on_select
            }
        },
        ThirdPartyItemData::LinearNotification(linear_notification) => rsx! {
            LinearNotificationListItem {
                notification,
                linear_notification: *linear_notification,
                is_selected,
                on_select,
            }
        },
        ThirdPartyItemData::GoogleCalendarEvent(google_calendar_event) => rsx! {
            GoogleCalendarEventListItem {
                notification,
                google_calendar_event: *google_calendar_event,
                is_selected,
                on_select,
            }
        },
        ThirdPartyItemData::GoogleMailThread(google_mail_thread) => rsx! {
            GoogleMailThreadListItem {
                notification,
                google_mail_thread: *google_mail_thread,
                is_selected,
                on_select,
            }
        },
        ThirdPartyItemData::SlackStar(_) => rsx! {
            SlackStarNotificationListItem { notification, is_selected, on_select },
        },
        ThirdPartyItemData::SlackReaction(slack_reaction) => rsx! {
            SlackReactionNotificationListItem {
                notification,
                slack_reaction: *slack_reaction,
                is_selected,
                on_select,
            },
        },
        ThirdPartyItemData::SlackThread(_) => rsx! {
            SlackThreadNotificationListItem { notification, is_selected, on_select },
        },
        ThirdPartyItemData::TodoistItem(todoist_item) => rsx! {
            TodoistNotificationListItem {
                notification,
                todoist_item: *todoist_item,
                is_selected,
                on_select,
            }
        },
        ThirdPartyItemData::LinearIssue(_) => rsx! {},
    }
}

pub fn get_notification_list_item_action_buttons(
    notification: ReadOnlySignal<NotificationWithTask>,
    show_shortcut: bool,
) -> Vec<Element> {
    let context = use_context::<Memo<NotificationListContext>>();

    if !notification().is_built_from_task() {
        let mut buttons = vec![rsx! {
            ListItemActionButton {
                title: "Delete notification",
                shortcut: "d",
                show_shortcut,
                onclick: move |_| {
                    context().notification_service
                        .send(NotificationCommand::DeleteFromNotification(notification()));
                },
                Icon { class: "w-5 h-5", icon: BsTrash }
            }
        }];

        if notification().task.is_some() {
            buttons.push(rsx! {
                ListItemActionButton {
                    title: "Complete task",
                    shortcut: "c",
                    disabled_label: (!context().is_task_actions_enabled)
                        .then_some("No task management service connected".to_string()),
                    show_shortcut,
                    onclick: move |_| {
                        context().notification_service
                            .send(NotificationCommand::CompleteTaskFromNotification(notification()));
                    },
                    Icon { class: "w-5 h-5", icon: MdCheckCircleOutline }
                }
            });
        }

        buttons.push(rsx! {
            ListItemActionButton {
                title: "Unsubscribe from the notification",
                shortcut: "u",
                show_shortcut,
                onclick: move |_| {
                    context().notification_service.send(NotificationCommand::Unsubscribe(notification().id));
                },
                Icon { class: "w-5 h-5", icon: BsBellSlash }
            }
        });

        buttons.push(rsx! {
            ListItemActionButton {
                title: "Snooze notification",
                shortcut: "s",
                show_shortcut,
                onclick: move |_| {
                    context().notification_service.send(NotificationCommand::Snooze(notification().id));
                },
                Icon { class: "w-5 h-5", icon: BsClockHistory }
            }
        });

        if notification().task.is_none() {
            buttons.push(rsx! {
                ListItemActionButton {
                    title: "Create task",
                    shortcut: "p",
                    disabled_label: (!context().is_task_actions_enabled)
                        .then_some("No task management service connected".to_string()),
                    show_shortcut,
                    onclick: move |_| {
                        UI_MODEL.write().task_planning_modal_opened = true;
                    },
                    Icon { class: "w-5 h-5", icon: MdAddTask }
                }
            });

            buttons.push(rsx! {
                ListItemActionButton {
                    title: "Link to task",
                    shortcut: "l",
                    disabled_label: (!context().is_task_actions_enabled)
                        .then_some("No task management service connected".to_string()),
                    show_shortcut,
                    onclick: move |_| {
                        UI_MODEL.write().task_link_modal_opened = true;
                    },
                    Icon { class: "w-5 h-5", icon: BsLink45deg }
                }
            });
        }

        buttons
    } else {
        vec![
            rsx! {
                ListItemActionButton {
                    title: "Delete task",
                    shortcut: "d",
                    disabled_label: (!context().is_task_actions_enabled)
                        .then_some("No task management service connected".to_string()),
                    show_shortcut,
                    onclick: move |_| {
                        context().notification_service
                            .send(NotificationCommand::DeleteFromNotification(notification()));
                    },
                    Icon { class: "w-5 h-5", icon: BsTrash }
                }
            },
            rsx! {
                ListItemActionButton {
                    title: "Complete task",
                    shortcut: "c",
                    disabled_label: (!context().is_task_actions_enabled)
                        .then_some("No task management service connected".to_string()),
                    show_shortcut,
                    onclick: move |_| {
                        context().notification_service
                            .send(NotificationCommand::CompleteTaskFromNotification(notification()));
                    },
                    Icon { class: "w-5 h-5", icon: MdCheckCircleOutline }
                }
            },
            rsx! {
                ListItemActionButton {
                    title: "Snooze notification",
                    shortcut: "s",
                    show_shortcut,
                    onclick: move |_| {
                        context().notification_service.send(NotificationCommand::Snooze(notification().id));
                    },
                    Icon { class: "w-5 h-5", icon: BsClockHistory }
                }
            },
            rsx! {
                ListItemActionButton {
                    title: "Plan task",
                    shortcut: "p",
                    disabled_label: (!context().is_task_actions_enabled)
                        .then_some("No task management service connected".to_string()),
                    show_shortcut,
                    onclick: move |_| {
                        UI_MODEL.write().task_planning_modal_opened = true;
                    },
                    Icon { class: "w-5 h-5", icon: BsCalendar2Check }
                }
            },
        ]
    }
}

#[component]
pub fn TaskHint(task: ReadOnlySignal<Option<Task>>) -> Element {
    let Some(task) = task() else {
        return rsx! {};
    };
    let html_url = task.get_html_url();
    let style = match task {
        Task {
            priority: TaskPriority::P1,
            ..
        } => "text-red-500",
        Task {
            priority: TaskPriority::P2,
            ..
        } => "text-orange-500",
        Task {
            priority: TaskPriority::P3,
            ..
        } => "text-yellow-500",
        Task {
            priority: TaskPriority::P4,
            ..
        } => "text-gray-500",
    };

    rsx! {
        div {
            class: "absolute top-0 right-0 tooltip tooltip-right text-xs {style}",
            "data-tip": "Linked to a {task.kind} task",

            a {
                href: "{html_url}",
                target: "_blank",
                Icon { class: "w-4 h-4", icon: BsBookmarkCheck }
            }
        }
    }
}
