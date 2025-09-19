#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::bs_icons::{BsChatSquareText, BsCheckCircle},
    Icon,
};
use url::Url;

use universal_inbox::{
    notification::NotificationWithTask,
    third_party::integrations::google_drive::GoogleDriveComment, HasHtmlUrl,
};

use crate::{
    components::{
        integrations::google_drive::icons::{GoogleDrive, GoogleDriveFile},
        list::{ListContext, ListItem},
        notifications_list::{get_notification_list_item_action_buttons, TaskHint},
        UserWithAvatar,
    },
    utils::format_elapsed_time,
};

#[component]
pub fn GoogleDriveCommentListItem(
    notification: ReadOnlySignal<NotificationWithTask>,
    google_drive_comment: ReadOnlySignal<GoogleDriveComment>,
    is_selected: ReadOnlySignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    let notification_updated_at = use_memo(move || format_elapsed_time(notification().updated_at));
    let list_context = use_context::<Memo<ListContext>>();
    let is_resolved = google_drive_comment().resolved;
    let icon_style = if is_resolved {
        "text-green-500"
    } else {
        "text-blue-500"
    };

    let link = notification().get_html_url();
    let avatar_url = google_drive_comment()
        .author
        .photo_link
        .and_then(|link| link.parse::<Url>().ok());

    rsx! {
        ListItem {
            key: "{notification().id}",
            title: "{notification().title}",
            link,
            subtitle: rsx! { GoogleDriveCommentSubtitle { google_drive_comment } },
            icon: rsx! {
                GoogleDrive { class: "h-5 w-5" },
                TaskHint { task: notification().task }
            },
            subicon: rsx! {
                GoogleDriveFile {
                    class: "h-5 w-5 min-w-5 {icon_style}",
                    mime_type: google_drive_comment().file_mime_type
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

            if !google_drive_comment().replies.is_empty() {
                Icon {
                    class: "mx-0.5 h-4 w-4 text-base-content/70",
                    icon: BsChatSquareText
                }
                span {
                    class: "text-xs text-base-content/70",
                    "{google_drive_comment().replies.len()}"
                }
            }

            UserWithAvatar {
                user_name: google_drive_comment().author.display_name,
                avatar_url,
                display_name: false
            }
            if is_resolved {
                Icon { class: "mx-0.5 h-4 w-4 text-green-500", icon: BsCheckCircle }
            }

            span { class: "text-base-content/50 whitespace-nowrap text-xs font-mono", "{notification_updated_at}" }
        }
    }
}

#[component]
fn GoogleDriveCommentSubtitle(google_drive_comment: ReadOnlySignal<GoogleDriveComment>) -> Element {
    let author_name = &google_drive_comment().author.display_name;
    let file_name = &google_drive_comment().file_name;

    rsx! {
        div {
            class: "flex gap-2 text-xs text-base-content/50",

            span { "{author_name}" }
            span { "•" }
            span { "{file_name}" }
        }
    }
}
