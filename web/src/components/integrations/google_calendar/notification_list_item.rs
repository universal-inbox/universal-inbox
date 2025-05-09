#![allow(non_snake_case)]

use chrono::{DateTime, Local};
use dioxus::prelude::*;

use dioxus_free_icons::{
    icons::bs_icons::{
        BsCalendar2Event, BsCheckCircleFill, BsPersonCheck, BsPersonDash, BsPersonX,
        BsQuestionCircleFill, BsXCircleFill,
    },
    Icon,
};
use universal_inbox::{
    notification::NotificationWithTask,
    third_party::integrations::google_calendar::{GoogleCalendarEvent, GoogleCalendarEventStatus},
    HasHtmlUrl,
};

use crate::{
    components::{
        integrations::google_calendar::{icons::GoogleCalendar, utils::compute_date_label},
        list::{ListContext, ListItem, ListItemActionButton},
        notifications_list::{
            get_notification_list_item_action_buttons, NotificationListContext, TaskHint,
        },
    },
    services::notification_service::NotificationCommand,
};

#[component]
pub fn GoogleCalendarEventListItem(
    notification: ReadOnlySignal<NotificationWithTask>,
    google_calendar_event: ReadOnlySignal<GoogleCalendarEvent>,
    is_selected: ReadOnlySignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    let notification_updated_at = use_memo(move || {
        Into::<DateTime<Local>>::into(notification().updated_at)
            .format("%Y-%m-%d %H:%M")
            .to_string()
    });
    let list_context = use_context::<Memo<ListContext>>();
    let status_icon = match google_calendar_event().status {
        GoogleCalendarEventStatus::Confirmed => {
            rsx! { Icon { class: "h-5 w-5 text-success", icon: BsCheckCircleFill } }
        }
        GoogleCalendarEventStatus::Tentative => {
            rsx! { Icon { class: "h-5 w-5 text-warning", icon: BsQuestionCircleFill } }
        }
        GoogleCalendarEventStatus::Cancelled => {
            rsx! { Icon { class: "h-5 w-5 text-error", icon: BsXCircleFill } }
        }
    };
    let link = notification().get_html_url();

    rsx! {
        ListItem {
            key: "{notification().id}",
            title: "{notification().title}",
            link,
            subtitle: rsx! { GoogleCalendarEventSubtitle { google_calendar_event } },
            icon: rsx! {
                GoogleCalendar { class: "h-8 w-8" },
                TaskHint { task: notification().task }
            },
            subicon: rsx! { Icon { class: "h-5 w-5", icon: BsCalendar2Event } },
            action_buttons: get_google_calendar_notification_list_item_action_buttons(
                notification,
                list_context().show_shortcut
            ),
            is_selected,
            on_select,

            { status_icon }
            span { class: "text-base-content/50 whitespace-nowrap text-xs font-mono", "{notification_updated_at}" }
        }
    }
}

#[component]
fn GoogleCalendarEventSubtitle(
    google_calendar_event: ReadOnlySignal<GoogleCalendarEvent>,
) -> Element {
    let organizer = google_calendar_event().organizer.email;
    let date_label = use_memo(move || compute_date_label(google_calendar_event(), "%a %b %e, %Y"));

    rsx! {
        div {
            class: "flex gap-2 text-xs text-base-content/50",

            span { class: "text-xs text-base-content/50 break-all", "{organizer}" }
            if let Some(date_label) = date_label() {
                span { class: "text-xs text-base-content/50", "{date_label}" }
            }
        }
    }
}

fn get_google_calendar_notification_list_item_action_buttons(
    notification: ReadOnlySignal<NotificationWithTask>,
    show_shortcut: bool,
) -> Vec<Element> {
    let context = use_context::<Memo<NotificationListContext>>();
    vec![
        rsx! {
            ListItemActionButton {
                title: "Accept",
                shortcut: "y",
                show_shortcut,
                onclick: move |_| {
                    context()
                        .notification_service
                        .send(NotificationCommand::AcceptInvitation(notification().id));
                },
                Icon { class: "w-5 h-5", icon: BsPersonCheck }
            }
        },
        rsx! {
            ListItemActionButton {
                title: "Decline",
                shortcut: "n",
                show_shortcut,
                onclick: move |_| {
                    context()
                        .notification_service
                        .send(NotificationCommand::DeclineInvitation(notification().id));
                },
                Icon { class: "w-5 h-5", icon: BsPersonX }
            }
        },
        rsx! {
            ListItemActionButton {
                title: "Maybe",
                shortcut: "m",
                show_shortcut,
                onclick: move |_| {
                    context()
                        .notification_service
                        .send(NotificationCommand::TentativelyAcceptInvitation(notification().id));
                },
                Icon { class: "w-5 h-5", icon: BsPersonDash }
            }
        },
    ]
    .into_iter()
    .chain(get_notification_list_item_action_buttons(
        notification,
        show_shortcut,
        None,
        None,
    ))
    .collect::<Vec<Element>>()
}
