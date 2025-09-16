#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::{
    notification::NotificationWithTask,
    third_party::integrations::linear::{LinearIssue, LinearNotification, LinearProject},
    HasHtmlUrl,
};

use crate::{
    components::{
        integrations::linear::{
            get_notification_type_label,
            icons::{Linear, LinearIssueIcon, LinearProjectIcon},
            list_item::LinearIssueListItemSubtitle,
        },
        list::{ListContext, ListItem},
        notifications_list::{get_notification_list_item_action_buttons, TaskHint},
        Tag, TagDisplay, UserWithAvatar,
    },
    utils::format_elapsed_time,
};

#[component]
pub fn LinearNotificationListItem(
    notification: ReadOnlySignal<NotificationWithTask>,
    linear_notification: ReadOnlySignal<LinearNotification>,
    is_selected: ReadOnlySignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    match linear_notification() {
        LinearNotification::IssueNotification { issue, r#type, .. } => rsx! {
            LinearIssueNotificationListItem {
                notification,
                notification_type: r#type.clone(),
                linear_issue: issue,
                is_selected,
                on_select,
            }
        },
        LinearNotification::ProjectNotification {
            project, r#type, ..
        } => rsx! {
            LinearProjectNotificationListItem {
                notification,
                notification_type: r#type.clone(),
                linear_project: project,
                is_selected,
                on_select,
            }
        },
    }
}

#[component]
pub fn LinearIssueNotificationListItem(
    notification: ReadOnlySignal<NotificationWithTask>,
    notification_type: String,
    linear_issue: ReadOnlySignal<LinearIssue>,
    is_selected: ReadOnlySignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    let notification_updated_at = use_memo(move || format_elapsed_time(notification().updated_at));
    let list_context = use_context::<Memo<ListContext>>();
    let link = notification().get_html_url();

    rsx! {
        ListItem {
            key: "{notification().id}",
            title: "{notification().title}",
            subtitle: rsx! { LinearIssueListItemSubtitle { linear_issue }},
            link,
            icon: rsx! {
                Linear { class: "h-5 w-5" },
                TaskHint { task: notification().task }
            },
            subicon: rsx! { LinearIssueIcon { class: "h-5 w-5 min-w-5", linear_issue } },
            action_buttons: get_notification_list_item_action_buttons(
                notification,
                list_context().show_shortcut,
                None,
                None
            ),
            is_selected,
            on_select,

            div {
                class: "flex flex-wrap items-center gap-1",
                TagDisplay {
                    tag: Into::<Tag>::into(get_notification_type_label(&notification_type))
                }
            }

            if let Some(assignee) = linear_issue().assignee {
                UserWithAvatar { avatar_url: assignee.avatar_url.clone(), user_name: assignee.name.clone() }
            } else {
                UserWithAvatar {}
            }

            span { class: "text-base-content/50 whitespace-nowrap text-xs font-mono", "{notification_updated_at}" }
        }
    }
}

#[component]
pub fn LinearProjectNotificationListItem(
    notification: ReadOnlySignal<NotificationWithTask>,
    notification_type: String,
    linear_project: ReadOnlySignal<LinearProject>,
    is_selected: ReadOnlySignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    let notification_updated_at = use_memo(move || format_elapsed_time(notification().updated_at));
    let list_context = use_context::<Memo<ListContext>>();
    let link = notification().get_html_url();

    rsx! {
        ListItem {
            key: "{notification().id}",
            title: "{notification().title}",
            subtitle: rsx! { },
            link,
            icon: rsx! {
                Linear { class: "h-5 w-5" },
                TaskHint { task: notification().task }
            },
            subicon: rsx! {
                LinearProjectIcon {
                    class: "h-5 w-5 min-w-5",
                    linear_project: linear_project
                }
            },
            action_buttons: get_notification_list_item_action_buttons(
                notification,
                list_context().show_shortcut,
                None,
                None
            ),
            is_selected,
            on_select,

            div {
                class: "flex flex-wrap items-center gap-1",
                TagDisplay {
                    tag: Into::<Tag>::into(get_notification_type_label(&notification_type))
                }
            }

            if let Some(lead) = linear_project().lead {
                UserWithAvatar { avatar_url: lead.avatar_url.clone(), user_name: lead.name.clone() }
            }

            span { class: "text-base-content/50 whitespace-nowrap text-xs font-mono", "{notification_updated_at}" }
        }
    }
}
