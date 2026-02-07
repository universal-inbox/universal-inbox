#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::{
        bs_icons::{
            BsBellSlash, BsBookmarkCheck, BsCalendar2Check, BsClockHistory, BsLightning,
            BsLink45deg, BsTrash,
        },
        md_action_icons::{MdAddTask, MdCheckCircleOutline},
    },
};

use universal_inbox::{
    HasHtmlUrl, Page,
    notification::{NotificationListOrder, NotificationWithTask},
    task::{Task, TaskId, TaskPlanning, TaskPriority},
    third_party::item::ThirdPartyItemData,
};

use crate::{
    components::{
        delete_all_confirmation_modal::DeleteAllConfirmationModal,
        flyonui::tooltip::{Tooltip, TooltipPlacement},
        integrations::{
            api::web_page::notification_list_item::WebPageNotificationListItem,
            github::notification_list_item::GithubNotificationListItem,
            google_calendar::notification_list_item::GoogleCalendarEventListItem,
            google_drive::notification_list_item::GoogleDriveCommentListItem,
            google_mail::notification_list_item::GoogleMailThreadListItem,
            icons::IntegrationProviderIcon,
            linear::notification_list_item::LinearNotificationListItem,
            slack::notification_list_item::{
                SlackReactionNotificationListItem, SlackStarNotificationListItem,
                SlackThreadNotificationListItem,
            },
            todoist::notification_list_item::TodoistNotificationListItem,
        },
        list::{List, ListItemActionButton, ListPaginationButtons},
        task_link_modal::TaskLinkModal,
        task_planning_modal::TaskPlanningModal,
    },
    config::get_api_base_url,
    icons::UILogo,
    model::UI_MODEL,
    services::{
        flyonui::open_flyonui_modal,
        integration_connection_service::TASK_SERVICE_INTEGRATION_CONNECTION,
        notification_service::{
            NotificationCommand, NotificationFilters, NotificationSourceKindFilter,
        },
    },
};

#[derive(Clone, PartialEq)]
pub struct NotificationListContext {
    pub is_task_actions_enabled: bool,
    pub notification_service: Coroutine<NotificationCommand>,
}

#[component]
pub fn NotificationsList(
    notifications: ReadSignal<Page<NotificationWithTask>>,
    notification_filters: Signal<NotificationFilters>,
) -> Element {
    let api_base_url = use_memo(move || get_api_base_url().unwrap());
    let notification_service = use_coroutine_handle::<NotificationCommand>();
    let context = use_memo(move || NotificationListContext {
        is_task_actions_enabled: UI_MODEL.read().is_task_actions_enabled,
        notification_service,
    });
    use_context_provider(move || context);
    let current_notification = UI_MODEL
        .read()
        .selected_notification_index
        .and_then(|index| {
            notifications()
                .content
                .get(index)
                .map(|notification| Signal::new(notification.clone()))
        });
    let current_page = use_signal(|| 1);
    let filters_str = notification_filters()
        .selected()
        .iter()
        .map(|f| f.to_string())
        .collect::<Vec<_>>()
        .join(", ");

    rsx! {
        div {
            id: "notifications-list",
            class: "flex flex-col h-full",
            div {
                class: "flex w-full p-2 gap-2 text-sm text-base-content/50",

                div {
                    class: "flex items-center flex-1 justify-start max-lg:hidden",
                    ListPaginationButtons {
                        current_page,
                        page: notifications,
                        on_select: move |selected_page_token| {
                            notification_filters.write().current_page_token = selected_page_token;
                            notification_service.send(NotificationCommand::Refresh);
                        }
                    }
                }

                div {
                    class: "flex items-center flex-1 justify-center",
                    NotificationSourceKindFilters {
                        notification_source_kind_filters: notification_filters().notification_source_kind_filters,
                        on_select: move |filter| {
                            notification_filters.write().select(filter);
                            notification_service.send(NotificationCommand::Refresh);
                        },
                    }
                }

                div {
                    class: "flex items-center flex-1 justify-end gap-2",
                    if !notifications().content.is_empty() {
                        DeleteAllButton {
                            on_click: move |_| {
                                open_flyonui_modal("#delete-all-confirmation-modal");
                            }
                        }
                    }
                    NotificationListOrdering {
                        notification_list_order: notification_filters().sort_by,
                        on_change: move |new_order| {
                            notification_filters.write().sort_by = new_order;
                            notification_service.send(NotificationCommand::Refresh);
                        }
                    }
                }
            }

            if notifications().content.is_empty() && notification_filters().is_filtered() {
                div {
                    class: "relative w-full h-full flex justify-center items-center",
                    UILogo {
                        class: "opacity-30 dark:opacity-10 w-96 h-96",
                        alt: "No notifications"
                    }
                    div {
                        class: "flex flex-col items-center absolute object-center top-2/3 transform translate-y-1/4",
                        p {
                            class: "text-base-content/50",
                            "There's no new {filters_str} notifications"
                        }
                    }
                }
            } else {
                div {
                    class: "h-full overflow-y-auto scroll-y-auto px-2 snap-y snap-mandatory",
                    List {
                        id: "notifications_list",
                        show_shortcut: UI_MODEL.read().is_help_enabled,

                        tbody {
                            for (i, notification) in notifications().content.into_iter().map(Signal::new).enumerate() {
                                NotificationListItem {
                                    notification,
                                    is_selected: Some(i) == UI_MODEL.read().selected_notification_index,
                                    on_select: move |_| {
                                        UI_MODEL.write().selected_notification_index = Some(i);
                                    },
                                }
                            }
                        }
                    }
                }
            }

            div {
                class: "flex flex-col w-full pb-2 gap-2 text-base text-base-content/50 lg:hidden",

                hr { class: "text-gray-200" }
                div {
                    class: "flex items-center flex-1 justify-center",
                    ListPaginationButtons {
                        current_page,
                        page: notifications,
                        on_select: move |selected_page_token| {
                            notification_filters.write().current_page_token = selected_page_token;
                            notification_service.send(NotificationCommand::Refresh);
                        }
                    }
                }
            }
        }

        if let Some(notification) = current_notification {
                TaskPlanningModal {
                    api_base_url: api_base_url(),
                    notification_to_plan: notification(),
                    task_service_integration_connection: TASK_SERVICE_INTEGRATION_CONNECTION.signal(),
                    ui_model: UI_MODEL.signal(),
                    on_task_planning: move |(params, task_id): (TaskPlanning, TaskId)| {
                        notification_service.send(NotificationCommand::PlanTask(
                            notification(),
                            task_id,
                            params
                        ));
                    },
                    on_task_creation: move |params| {
                        notification_service.send(NotificationCommand::CreateTaskFromNotification(
                            notification(),
                            params
                        ));
                    },
                }
            }

        if let Some(notification) = current_notification {
                TaskLinkModal {
                    api_base_url: api_base_url(),
                    notification_to_link: notification(),
                    ui_model: UI_MODEL.signal(),
                    on_task_link: move |task_id| {
                        notification_service.send(NotificationCommand::LinkNotificationWithTask(
                            notification().id,
                            task_id,
                        ));
                    },
                }
            }

        DeleteAllConfirmationModal {
            on_confirm: move |_| {
                notification_service.send(NotificationCommand::DeleteAll);
            }
        }
    }
}

#[component]
fn NotificationListItem(
    notification: ReadSignal<NotificationWithTask>,
    is_selected: ReadSignal<bool>,
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
        ThirdPartyItemData::GoogleDriveComment(google_drive_comment) => rsx! {
            GoogleDriveCommentListItem {
                notification,
                google_drive_comment: *google_drive_comment,
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
        ThirdPartyItemData::WebPage(web_page) => rsx! {
            WebPageNotificationListItem {
                notification,
                web_page: *web_page,
                is_selected,
                on_select
            }
        },
    }
}

pub fn get_notification_list_item_action_buttons(
    notification: ReadSignal<NotificationWithTask>,
    show_shortcut: bool,
    button_class: Option<String>,
    container_class: Option<String>,
) -> Vec<Element> {
    let context = use_context::<Memo<NotificationListContext>>();

    if !notification().is_built_from_task() {
        let mut buttons = vec![rsx! {
            ListItemActionButton {
                title: "Delete notification",
                shortcut: "d",
                show_shortcut,
                button_class: button_class.clone(),
                container_class: container_class.clone(),
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
                    button_class: button_class.clone(),
                    container_class: container_class.clone(),
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
                button_class: button_class.clone(),
                container_class: container_class.clone(),
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
                button_class: button_class.clone(),
                container_class: container_class.clone(),
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
                    button_class: button_class.clone(),
                    container_class: container_class.clone(),
                    data_overlay: "#task-planning-modal",
                    Icon { class: "w-5 h-5", icon: MdAddTask }
                }
            });

            buttons.push(rsx! {
                ListItemActionButton {
                    title: "Create task with defaults",
                    shortcut: "t",
                    disabled_label: (!context().is_task_actions_enabled)
                        .then_some("No task management service connected".to_string()),
                    show_shortcut,
                    button_class: button_class.clone(),
                    container_class: container_class.clone(),
                    onclick: move |_| {
                        context().notification_service.send(NotificationCommand::CreateTaskWithDetaultsFromNotification(notification()));
                    },
                    Icon { class: "w-5 h-5", icon: BsLightning }
                }
            });

            buttons.push(rsx! {
                ListItemActionButton {
                    title: "Link to task",
                    shortcut: "l",
                    disabled_label: (!context().is_task_actions_enabled)
                        .then_some("No task management service connected".to_string()),
                    show_shortcut,
                    button_class: button_class.clone(),
                    container_class: container_class.clone(),
                    data_overlay: "#task-linking-modal",
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
                    button_class: button_class.clone(),
                    container_class: container_class.clone(),
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
                    button_class: button_class.clone(),
                    container_class: container_class.clone(),
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
                    button_class: button_class.clone(),
                    container_class: container_class.clone(),
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
                    button_class: button_class.clone(),
                    container_class: container_class.clone(),
                    data_overlay: "#task-planning-modal",
                    Icon { class: "w-5 h-5", icon: BsCalendar2Check }
                }
            },
            rsx! {
                ListItemActionButton {
                    title: "Create task with defaults",
                    shortcut: "t",
                    disabled_label: (!context().is_task_actions_enabled)
                        .then_some("No task management service connected".to_string()),
                    show_shortcut,
                    button_class: button_class.clone(),
                    container_class: container_class.clone(),
                    onclick: move |_| {
                        context().notification_service.send(NotificationCommand::CreateTaskWithDetaultsFromNotification(notification()));
                    },
                    Icon { class: "w-5 h-5", icon: BsLightning }
                }
            },
        ]
    }
}

#[component]
pub fn TaskHint(task: ReadSignal<Option<Task>>) -> Element {
    let Some(task) = task() else {
        return rsx! {};
    };
    let html_url = task.get_html_url();
    let (tooltip_style, content_style) = match task {
        Task {
            priority: TaskPriority::P1,
            ..
        } => ("tooltip-red-500", "text-red-500"),
        Task {
            priority: TaskPriority::P2,
            ..
        } => ("tooltip-orange-500", "text-orange-500"),
        Task {
            priority: TaskPriority::P3,
            ..
        } => ("tooltip-yellow-500", "text-yellow-500"),
        Task {
            priority: TaskPriority::P4,
            ..
        } => ("tooltip-gray-500", "text-gray-500"),
    };

    rsx! {
        Tooltip {
            class: "absolute top-0 right-0",
            tooltip_class: "{tooltip_style}",
            text: "Linked to a {task.kind} task",
            placement: TooltipPlacement::Right,

            a {
                class: "{content_style}",
                href: "{html_url}",
                target: "_blank",
                Icon { class: "w-4 h-4", icon: BsBookmarkCheck }
            }
        }
    }
}

#[component]
pub fn NotificationSourceKindFilters(
    notification_source_kind_filters: ReadSignal<Vec<NotificationSourceKindFilter>>,
    on_select: EventHandler<NotificationSourceKindFilter>,
) -> Element {
    rsx! {
        div {
            class: "flex items-center gap-2",
            span { "Filters: " }
            for filter in notification_source_kind_filters() {
                NotificationSourceKindFilterButton { filter, on_select }
            }
        }
    }
}

#[component]
pub fn NotificationSourceKindFilterButton(
    filter: ReadSignal<NotificationSourceKindFilter>,
    on_select: EventHandler<NotificationSourceKindFilter>,
) -> Element {
    let style = use_memo(move || {
        if filter().selected {
            "text-bg-soft-primary btn-active"
        } else {
            "btn-disabled pointer-events-auto!"
        }
    });

    rsx! {
        button {
            class: "btn btn-circle btn-text lg:btn-xs max-lg:btn-lg {style}",
            onclick: move |_| on_select.call(filter()),
            IntegrationProviderIcon { class: "w-4 h-4", provider_kind: filter().kind.into() }
        }
    }
}

#[component]
pub fn NotificationListOrdering(
    notification_list_order: ReadSignal<NotificationListOrder>,
    on_change: EventHandler<NotificationListOrder>,
) -> Element {
    rsx! {
        Tooltip {
            text: "Sort by updated date",
            placement: TooltipPlacement::Right,

            label {
                class: "swap swap-flip",
                input {
                    "type": "checkbox",
                    onclick: move |_| {
                        let new_order = match notification_list_order() {
                            NotificationListOrder::UpdatedAtAsc => NotificationListOrder::UpdatedAtDesc,
                            NotificationListOrder::UpdatedAtDesc => NotificationListOrder::UpdatedAtAsc,
                        };
                        on_change.call(new_order);
                    },
                    checked: "{notification_list_order() == NotificationListOrder::UpdatedAtDesc}",
                }
                span { class: "swap-on icon-[tabler--chevron-down] lg:size-5 max-lg:size-6" }
                span { class: "swap-off icon-[tabler--chevron-up] lg:size-5 max-lg:size-6" }
            }
        }
    }
}

#[component]
pub fn DeleteAllButton(on_click: EventHandler<()>) -> Element {
    rsx! {
        Tooltip {
            text: "Delete all notifications",
            placement: TooltipPlacement::Left,

            button {
                class: "btn btn-circle btn-error lg:btn-xs max-lg:btn-lg",
                onclick: move |_| on_click.call(()),
                Icon { class: "w-4 h-4", icon: BsTrash }
            }
        }
    }
}
