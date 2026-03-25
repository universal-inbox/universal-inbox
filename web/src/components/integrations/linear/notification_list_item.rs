#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::{
    notification::{NotificationStatus, NotificationWithTask},
    third_party::integrations::linear::{LinearIssue, LinearNotification, LinearProject},
};

use crate::{
    components::{
        integrations::linear::{
            icons::{Linear, LinearIssueIcon, LinearProjectIcon},
            list_item::LinearIssueListItemSubtitle,
        },
        list::ListItem,
    },
    utils::format_elapsed_time,
};

#[component]
pub fn LinearNotificationListItem(
    notification: ReadSignal<NotificationWithTask>,
    linear_notification: ReadSignal<LinearNotification>,
    is_selected: ReadSignal<bool>,
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
    notification: ReadSignal<NotificationWithTask>,
    notification_type: String,
    linear_issue: ReadSignal<LinearIssue>,
    is_selected: ReadSignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    let notification_updated_at = use_memo(move || format_elapsed_time(notification().updated_at));
    let is_unread = notification().status == NotificationStatus::Unread;

    rsx! {
        ListItem {
            key: "{notification().id}",
            linked_task: notification().task,
            title: "{notification().title}",
            subtitle: rsx! {
                LinearIssueListItemSubtitle { linear_issue }
            },
            time: "{notification_updated_at}",
            icon: rsx! {
                div {
                    class: "w-full h-full flex items-center justify-center rounded-[inherit] bg-[var(--ui-surface)] border border-[var(--ui-border)]",
                    Linear { class: "h-4 w-4" }
                }
            },
            meta_icon: rsx! { LinearIssueIcon { linear_issue, class: "w-full h-full" } },
            is_selected,
            is_unread,
            on_select,
        }
    }
}

#[component]
pub fn LinearProjectNotificationListItem(
    notification: ReadSignal<NotificationWithTask>,
    notification_type: String,
    linear_project: ReadSignal<LinearProject>,
    is_selected: ReadSignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    let notification_updated_at = use_memo(move || format_elapsed_time(notification().updated_at));
    let is_unread = notification().status == NotificationStatus::Unread;
    let project_name = linear_project().name.clone();
    let title = match linear_project().icon {
        Some(icon) => format!("{icon} {}", notification().title),
        None => notification().title.clone(),
    };

    rsx! {
        ListItem {
            key: "{notification().id}",
            linked_task: notification().task,
            title: "{title}",
            subtitle: rsx! { span { class: "ui-nrow-meta-text", "{project_name}" } },
            time: "{notification_updated_at}",
            icon: rsx! {
                div {
                    class: "w-full h-full flex items-center justify-center rounded-[inherit] bg-[var(--ui-surface)] border border-[var(--ui-border)]",
                    Linear { class: "h-4 w-4" }
                }
            },
            meta_icon: rsx! { LinearProjectIcon { linear_project, class: "w-full h-full" } },
            is_selected,
            is_unread,
            on_select,
        }
    }
}
