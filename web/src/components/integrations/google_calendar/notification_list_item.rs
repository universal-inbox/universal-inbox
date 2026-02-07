#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::bs_icons::{
        BsArrowRepeat, BsCalendar2Event, BsCheckCircleFill, BsPersonCheck, BsPersonDash, BsPersonX,
        BsQuestionCircleFill, BsXCircleFill,
    },
};

use universal_inbox::{
    HasHtmlUrl,
    notification::NotificationWithTask,
    third_party::integrations::google_calendar::{
        EventMethod, GoogleCalendarEvent, GoogleCalendarEventStatus,
    },
};

use crate::{
    components::{
        integrations::google_calendar::{icons::GoogleCalendar, utils::compute_date_label},
        list::{ListContext, ListItem, ListItemActionButton},
        notifications_list::{
            NotificationListContext, TaskHint, get_notification_list_item_action_buttons,
        },
    },
    services::notification_service::NotificationCommand,
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
    let list_context = use_context::<Memo<ListContext>>();
    let status_icon = match (
        google_calendar_event().status,
        google_calendar_event().method,
    ) {
        // Cancelled events - either by status or method
        (GoogleCalendarEventStatus::Cancelled, _) | (_, EventMethod::Cancel) => {
            rsx! { Icon { class: "h-5 w-5 text-error", icon: BsXCircleFill } }
        }
        // Confirmed events
        (GoogleCalendarEventStatus::Confirmed, _) => {
            rsx! { Icon { class: "h-5 w-5 text-success", icon: BsCheckCircleFill } }
        }
        // Tentative events
        (GoogleCalendarEventStatus::Tentative, _) => {
            rsx! { Icon { class: "h-5 w-5 text-warning", icon: BsQuestionCircleFill } }
        }
    };

    // Check if the event is recurring
    let is_recurring = google_calendar_event().recurrence.is_some()
        || google_calendar_event().recurring_event_id.is_some();
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
            if is_recurring {
                Icon { class: "h-5 w-5 text-base-content/70", icon: BsArrowRepeat }
            }
            span { class: "text-base-content/50 whitespace-nowrap text-xs font-mono", "{notification_updated_at}" }
        }
    }
}

#[component]
fn GoogleCalendarEventSubtitle(google_calendar_event: ReadSignal<GoogleCalendarEvent>) -> Element {
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
    notification: ReadSignal<NotificationWithTask>,
    show_shortcut: bool,
) -> Vec<Element> {
    let context = use_context::<Memo<NotificationListContext>>();

    // Get the Google Calendar event from the notification
    let google_calendar_event_data = use_memo(move || {
        if let universal_inbox::third_party::item::ThirdPartyItemData::GoogleCalendarEvent(event) =
            &notification().source_item.data
        {
            Some(event.as_ref().clone())
        } else {
            None
        }
    });

    let is_cancelled = use_memo(move || {
        google_calendar_event_data().is_some_and(|event| {
            event.status == GoogleCalendarEventStatus::Cancelled
                || event.method == EventMethod::Cancel
        })
    });

    if is_cancelled() {
        return vec![];
    }

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
