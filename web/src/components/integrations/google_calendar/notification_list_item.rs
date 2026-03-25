#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::{
    notification::{NotificationStatus, NotificationWithTask},
    third_party::integrations::google_calendar::GoogleCalendarEvent,
};

use crate::{
    components::{
        integrations::google_calendar::{icons::GoogleCalendar, utils::compute_date_label},
        list::ListItem,
    },
    utils::format_elapsed_time,
};

#[component]
pub fn GoogleCalendarEventListItem(
    notification: ReadSignal<NotificationWithTask>,
    google_calendar_event: ReadSignal<GoogleCalendarEvent>,
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
            subtitle: rsx! { GoogleCalendarEventSubtitle { google_calendar_event } },
            time: "{notification_updated_at}",
            icon: rsx! {
                div {
                    class: "w-full h-full flex items-center justify-center rounded-[inherit] bg-[var(--ui-surface)] border border-[var(--ui-border)]",
                    GoogleCalendar { class: "h-4 w-4" }
                }
            },
            meta_icon: rsx! { span { class: "icon-[lucide--calendar] w-full h-full" } },
            is_selected,
            is_unread,
            provider: Some("google_calendar"),
            on_select,
        }
    }
}

#[component]
fn GoogleCalendarEventSubtitle(google_calendar_event: ReadSignal<GoogleCalendarEvent>) -> Element {
    let date_label = use_memo(move || compute_date_label(google_calendar_event(), "%a %b %e, %Y"));

    rsx! {
        if let Some(date_label) = date_label() {
            span { class: "ui-nrow-meta-text", "{date_label}" }
        }
    }
}
