#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::{
        bs_icons::{
            BsArrowUpRightSquare, BsCalendar2Event, BsPeople, BsPerson, BsPersonCheck,
            BsPersonDash, BsPersonFill, BsPersonX,
        },
        md_communication_icons::MdLocationOn,
    },
    Icon,
};

use universal_inbox::{
    notification::NotificationWithTask,
    third_party::integrations::google_calendar::{
        EventAttendee, GoogleCalendarEvent, GoogleCalendarEventAttendeeResponseStatus,
    },
    HasHtmlUrl,
};

use crate::{
    components::{
        integrations::google_calendar::utils::compute_date_label, CollapseCardWithIcon, SmallCard,
    },
    services::notification_service::NotificationCommand,
};

#[component]
pub fn GoogleCalendarEventPreview(
    notification: ReadOnlySignal<NotificationWithTask>,
    google_calendar_event: ReadOnlySignal<GoogleCalendarEvent>,
    expand_details: ReadOnlySignal<bool>,
) -> Element {
    let notification_service = use_coroutine_handle::<NotificationCommand>();
    let link = notification().get_html_url();
    let date_label = use_memo(move || compute_date_label(google_calendar_event(), "%A %B %e, %Y"));
    let organizer_label = use_memo(move || {
        let organizer = google_calendar_event().organizer;
        organizer.display_name.unwrap_or(organizer.email)
    });
    let creator_label = use_memo(move || {
        let creator = google_calendar_event().creator;
        creator.display_name.or(creator.email)
    });
    let self_attendee = use_memo(move || google_calendar_event().get_self_attendee());
    let is_accepted = use_memo(move || {
        self_attendee().map_or(false, |attendee| {
            attendee.response_status == GoogleCalendarEventAttendeeResponseStatus::Accepted
        })
    });
    let is_declined = use_memo(move || {
        self_attendee().map_or(false, |attendee| {
            attendee.response_status == GoogleCalendarEventAttendeeResponseStatus::Declined
        })
    });
    let is_tentative = use_memo(move || {
        self_attendee().map_or(false, |attendee| {
            attendee.response_status == GoogleCalendarEventAttendeeResponseStatus::Tentative
        })
    });
    let accepted_style = if is_accepted() { "bg-success" } else { "" };
    let declined_style = if is_declined() { "bg-error" } else { "" };
    let tentative_style = if is_tentative() { "bg-warning" } else { "" };

    rsx! {
        div {
            class: "flex flex-col gap-2 w-full",

            h2 {
                class: "flex items-center gap-2 text-lg",

                Icon { class: "flex-none h-5 w-5", icon: BsCalendar2Event }
                a {
                    href: "{link}",
                    target: "_blank",
                    "{notification().title}"
                }
                a {
                    class: "flex-none",
                    href: "{link}",
                    target: "_blank",
                    Icon { class: "h-5 w-5 text-gray-400 p-1", icon: BsArrowUpRightSquare }
                }
            }

            if let Some(description) = google_calendar_event().description.as_ref() {
                SmallCard { span { "{description}" } }
            }

            if let Some(creator_label) = creator_label.as_ref() {
                SmallCard {
                    Icon { class: "h-5 w-5", icon: BsPerson }
                    span { "{creator_label}" }
                }
            }

            SmallCard {
                Icon { class: "h-5 w-5", icon: BsPersonFill }
                span { "{organizer_label}" }
            }

            if let Some(date_label) = date_label() {
                SmallCard {
                    Icon { class: "h-5 w-5", icon: BsCalendar2Event }
                    span { "{date_label}" }
                }
            }

            CollapseCardWithIcon {
                title: "Guests",
                icon: rsx! { Icon { class: "h-5 w-5", icon: BsPeople } },
                opened: expand_details(),
                table {
                    class: "table table-auto table-xs w-full",
                    tbody {
                        for attendee in google_calendar_event().attendees {
                            CalendarEventAttendeeRow { attendee }
                        }
                    }
                }
            }

            if let Some(location) = google_calendar_event().location.as_ref() {
                SmallCard {
                    Icon { class: "h-5 w-5", icon: MdLocationOn }
                    span { "{location}" }
                }
            }

            div {
                class: "join w-full",
                input {
                    class: "join-item btn btn-sm rounded-l-lg grow {accepted_style}",
                    type: "radio",
                    name: "action",
                    checked: "{is_accepted}",
                    onclick: move |_| {
                        notification_service
                            .send(NotificationCommand::AcceptInvitation(notification().id));
                    },
                    Icon { class: "h-5 w-5", icon: BsPersonCheck }
                    "Yes"
                }
                input {
                    class: "join-item btn btn-sm grow {declined_style}",
                    type: "radio",
                    name: "action",
                    checked: "{is_declined}",
                    onclick: move |_| {
                        notification_service
                            .send(NotificationCommand::AcceptInvitation(notification().id));
                    },
                    Icon { class: "h-5 w-5", icon: BsPersonX }
                    "No"
                }
                input {
                    class: "join-item btn btn-sm rounded-r-lg grow {tentative_style}",
                    type: "radio",
                    name: "action",
                    checked: "{is_tentative}",
                    onclick: move |_| {
                        notification_service
                            .send(NotificationCommand::AcceptInvitation(notification().id));
                    },
                    Icon { class: "h-5 w-5", icon: BsPersonDash }
                    "Maybe"
                }
            }
        }
    }
}

#[component]
fn CalendarEventAttendeeRow(attendee: ReadOnlySignal<EventAttendee>) -> Element {
    let display_name = attendee().display_name.unwrap_or_else(|| attendee().email);
    let response_status_icon = match attendee().response_status {
        GoogleCalendarEventAttendeeResponseStatus::Accepted => {
            rsx! { Icon { class: "h-5 w-5 text-success", icon: BsPersonCheck } }
        }
        GoogleCalendarEventAttendeeResponseStatus::Declined => {
            rsx! { Icon { class: "h-5 w-5 text-error", icon: BsPersonX } }
        }
        GoogleCalendarEventAttendeeResponseStatus::NeedsAction => {
            rsx! { Icon { class: "h-5 w-5", icon: BsPerson } }
        }
        GoogleCalendarEventAttendeeResponseStatus::Tentative => {
            rsx! { Icon { class: "h-5 w-5 text-warning", icon: BsPersonDash } }
        }
    };

    rsx! {
        tr {
            td { class: "flex justify-center", { response_status_icon } }
            td { "{display_name}" }
        }
    }
}