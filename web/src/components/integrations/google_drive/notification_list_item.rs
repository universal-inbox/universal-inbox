#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::{
    notification::{NotificationStatus, NotificationWithTask},
    third_party::integrations::google_drive::GoogleDriveComment,
};

use crate::{
    components::{integrations::google_drive::icons::GoogleDrive, list::ListItem},
    utils::format_elapsed_time,
};

#[component]
pub fn GoogleDriveCommentListItem(
    notification: ReadSignal<NotificationWithTask>,
    google_drive_comment: ReadSignal<GoogleDriveComment>,
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
            subtitle: rsx! { GoogleDriveCommentSubtitle { google_drive_comment } },
            time: "{notification_updated_at}",
            icon: rsx! {
                GoogleDrive { class: "h-5 w-5" },
            },
            meta_icon: rsx! { span { class: "icon-[lucide--file] w-full h-full" } },
            is_selected,
            is_unread,
            on_select,
        }
    }
}

#[component]
fn GoogleDriveCommentSubtitle(google_drive_comment: ReadSignal<GoogleDriveComment>) -> Element {
    let file_name = &google_drive_comment().file_name;

    rsx! {
        span {
            class: "ui-nrow-meta-text",
            "{file_name}"
        }
    }
}
