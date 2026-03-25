#![allow(non_snake_case)]
use dioxus::prelude::*;
use gravatar_rs::Generator;
use rrule::Frequency;
use url::Url;

use universal_inbox::{
    notification::NotificationWithTask,
    third_party::integrations::google_calendar::{
        EventMethod, GoogleCalendarEvent, GoogleCalendarEventAttendeeResponseStatus,
        GoogleCalendarEventStatus,
    },
};

use crate::{
    components::{
        UserWithAvatar,
        integrations::google_calendar::utils::{compute_date_label, compute_time_range_label},
        preview_card_header::PreviewCardHeader,
        ui::{
            Card, CardVariant, STATUS_ROW_NAME_CLASS, StatusDot, StatusRow, StatusSection,
            StatusVariant, Tag, TagVariant,
        },
    },
    services::notification_service::NotificationCommand,
};

#[component]
pub fn GoogleCalendarEventPreview(
    notification: ReadSignal<NotificationWithTask>,
    google_calendar_event: ReadSignal<GoogleCalendarEvent>,
    expand_details: ReadSignal<bool>,
) -> Element {
    let notification_service = use_coroutine_handle::<NotificationCommand>();
    let date_label = use_memo(move || compute_date_label(google_calendar_event(), "%A %B %e, %Y"));
    let date_block = use_memo(move || compute_date_block(&google_calendar_event()));
    let organizer_label = use_memo(move || {
        let organizer = google_calendar_event().organizer;
        organizer.display_name.unwrap_or(organizer.email)
    });
    // Dedupe creator vs organizer when they're the same person — render organizer only.
    let creator_label = use_memo(move || {
        let event = google_calendar_event();
        let creator_email = event.creator.email.as_ref();
        let organizer_email = &event.organizer.email;
        if creator_email == Some(organizer_email) {
            None
        } else {
            event.creator.display_name.or(event.creator.email)
        }
    });
    // Aggregate RSVP counts for the guest summary line.
    let rsvp_summary = use_memo(move || {
        let attendees = google_calendar_event().attendees;
        let mut yes = 0;
        let mut no = 0;
        let mut maybe = 0;
        let mut awaiting = 0;
        for a in &attendees {
            match a.response_status {
                GoogleCalendarEventAttendeeResponseStatus::Accepted => yes += 1,
                GoogleCalendarEventAttendeeResponseStatus::Declined => no += 1,
                GoogleCalendarEventAttendeeResponseStatus::Tentative => maybe += 1,
                GoogleCalendarEventAttendeeResponseStatus::NeedsAction => awaiting += 1,
            }
        }
        (yes, no, maybe, awaiting, attendees.len())
    });
    let self_attendee = use_memo(move || google_calendar_event().get_self_attendee());
    let is_cancelled = use_memo(move || {
        let event = google_calendar_event();
        event.status == GoogleCalendarEventStatus::Cancelled || event.method == EventMethod::Cancel
    });
    let is_accepted = use_memo(move || {
        self_attendee().is_some_and(|attendee| {
            attendee.response_status == GoogleCalendarEventAttendeeResponseStatus::Accepted
        })
    });
    let is_declined = use_memo(move || {
        self_attendee().is_some_and(|attendee| {
            attendee.response_status == GoogleCalendarEventAttendeeResponseStatus::Declined
        })
    });
    let is_tentative = use_memo(move || {
        self_attendee().is_some_and(|attendee| {
            attendee.response_status == GoogleCalendarEventAttendeeResponseStatus::Tentative
        })
    });

    // Time range "15:00 – 15:30" + tz abbreviation. None for all-day events.
    let time_range = use_memo(move || compute_time_range_label(&google_calendar_event()));

    // Single-line recurrence summary ("Repeats every week on FRs"). None when not recurring.
    let recurrence_summary = use_memo(move || {
        let event = google_calendar_event();
        if let Some(recurrence) = &event.recurrence {
            let rules = recurrence.get_rrule();
            if let Some(rule) = rules.first() {
                return Some(format!("Repeats {}", format_single_rule(rule)));
            }
            return Some("Recurring event".to_string());
        }
        if event.recurring_event_id.is_some() {
            return Some("Instance of a recurring series".to_string());
        }
        None
    });

    // Gravatar URL + display label for the organizer body row.
    let organizer = use_memo(move || {
        let org = google_calendar_event().organizer;
        let label = org
            .display_name
            .clone()
            .unwrap_or_else(|| org.email.clone());
        let avatar = Generator::default().generate(&org.email);
        (avatar, label)
    });

    let sanitized_description = use_memo(move || {
        google_calendar_event().description.as_ref().map(|desc| {
            ammonia::Builder::default()
                .set_tag_attribute_value("a", "target", "_blank")
                .clean(desc)
                .to_string()
        })
    });

    let dock_yes_class = if is_accepted() {
        "preview-dock-btn yes is-active"
    } else {
        "preview-dock-btn yes"
    };
    let dock_no_class = if is_declined() {
        "preview-dock-btn no is-active"
    } else {
        "preview-dock-btn no"
    };
    let dock_maybe_class = if is_tentative() {
        "preview-dock-btn maybe is-active"
    } else {
        "preview-dock-btn maybe"
    };

    let header_organizer = organizer_label();
    let event_title = notification().title.clone();

    rsx! {
        div {
            class: "flex flex-col w-full h-full",

            PreviewCardHeader {
                brand_icon: rsx! { span { class: "icon-[lucide--calendar] size-4" } },
                title: event_title,
                subline: rsx! {
                    span { "Organized by" }
                    span {
                        style: "color: var(--ui-base-content); font-weight: 500;",
                        "{header_organizer}"
                    }
                    if is_cancelled() {
                        span { class: "sep", "·" }
                        Tag { variant: TagVariant::Error, "Cancelled" }
                    }
                }
            }

            div {
                id: "notification-preview-details",
                class: "flex flex-col gap-2 w-full h-full overflow-y-auto scroll-y-auto p-3",

                // Primary card: date tile + meta column on top, guests + RSVP stacked underneath.
                {
                    let date_block_value = date_block();
                    let (yes, no, maybe, awaiting, total) = rsvp_summary();
                    let dot_variant = if no > 0 {
                        StatusVariant::Error
                    } else if maybe > 0 || awaiting > 0 {
                        StatusVariant::Warning
                    } else {
                        StatusVariant::Success
                    };
                    let plural = if total != 1 { "s" } else { "" };
                    let mut breakdown_parts: Vec<String> = Vec::new();
                    if yes > 0 { breakdown_parts.push(format!("{yes} yes")); }
                    if no > 0 { breakdown_parts.push(format!("{no} no")); }
                    if maybe > 0 { breakdown_parts.push(format!("{maybe} maybe")); }
                    if awaiting > 0 { breakdown_parts.push(format!("{awaiting} awaiting")); }
                    let breakdown = breakdown_parts.join(", ");
                    let summary = if breakdown.is_empty() {
                        format!("{total} guest{plural}")
                    } else {
                        format!("{total} guest{plural} · {breakdown}")
                    };

                    rsx! {
                        Card {
                            variant: CardVariant::Default,

                            if let Some((month, day, weekday, _time)) = date_block_value {
                                div {
                                    style: "display: flex; gap: 14px; align-items: center;",

                                    div {
                                        class: "flex flex-col items-center justify-center border border-ui-border rounded-ui-md overflow-hidden bg-ui-surface min-w-16",
                                        div {
                                            class: "w-full bg-brand-gcal text-white text-[10px] font-bold text-center py-[3px] uppercase tracking-[0.08em]",
                                            "{month}"
                                        }
                                        div {
                                            class: "text-2xl font-bold leading-none pt-1.5 pb-0.5 tracking-tight",
                                            "{day}"
                                        }
                                        div {
                                            class: "text-[10px] text-ui-base-muted uppercase tracking-[0.06em] pb-[5px] font-semibold",
                                            "{weekday}"
                                        }
                                    }
                                    div {
                                        class: "flex flex-col gap-1.5 flex-1 min-w-0",

                                        if let Some((range, tz)) = time_range() {
                                            div {
                                                class: "flex items-baseline gap-1.5 font-bold text-sm tracking-[-0.01em]",
                                                span { "{range}" }
                                                span {
                                                    class: "text-ui-base-muted text-xs font-semibold tracking-[0.04em] uppercase",
                                                    "{tz}"
                                                }
                                            }
                                        } else if let Some(date_label) = date_label() {
                                            div {
                                                class: "flex items-baseline gap-1.5 font-bold text-sm tracking-[-0.01em]",
                                                span { "{date_label}" }
                                            }
                                        }

                                        if let Some(rec) = recurrence_summary() {
                                            div {
                                                class: "flex items-center gap-1.5 text-[12.5px] text-ui-base-muted",
                                                span { class: "icon-[lucide--repeat-2] size-4" }
                                                span { "{rec}" }
                                            }
                                        }

                                        if let Some(location) = google_calendar_event().location.as_ref() {
                                            div {
                                                class: "flex items-center gap-1.5 text-[12.5px] text-ui-base-muted",
                                                span { class: "icon-[lucide--map-pin] size-4" }
                                                span { "{location}" }
                                            }
                                        }

                                        {
                                            let (avatar, label) = organizer();
                                            rsx! {
                                                div {
                                                    class: "flex items-center gap-2 min-w-0",
                                                    img {
                                                        class: "object-cover rounded-full size-[22px] border border-ui-border shrink-0",
                                                        src: "{avatar}",
                                                        alt: "{label}",
                                                    }
                                                    span {
                                                        class: "truncate text-[12.5px] text-ui-base-content",
                                                        "{label}"
                                                    }
                                                }
                                            }
                                        }

                                        if let Some(creator_label) = creator_label.as_ref() {
                                            div {
                                                class: "flex items-center gap-1.5 text-[12.5px] text-ui-base-muted",
                                                span { class: "icon-[lucide--user] size-4" }
                                                span { "Created by {creator_label}" }
                                            }
                                        }
                                    }
                                }
                            }

                            if total > 0 {
                                div {
                                    class: "preview-card-section",

                                    StatusSection {
                                        dot: rsx! { StatusDot { variant: dot_variant } },
                                        label: "Guests".to_string(),
                                        summary,
                                        initially_open: expand_details(),

                                        for attendee in google_calendar_event().attendees {
                                            AttendeeRow { attendee }
                                        }
                                    }
                                }
                            }

                            if !is_cancelled() {
                                div {
                                    class: "preview-rsvp-inline",
                                    button {
                                        r#type: "button",
                                        class: "{dock_yes_class}",
                                        onclick: move |_| {
                                            notification_service
                                                .send(NotificationCommand::AcceptInvitation(notification().id));
                                        },
                                        span { class: "icon-[lucide--user-check] size-4" }
                                        "Yes"
                                    }
                                    button {
                                        r#type: "button",
                                        class: "{dock_no_class}",
                                        onclick: move |_| {
                                            notification_service
                                                .send(NotificationCommand::DeclineInvitation(notification().id));
                                        },
                                        span { class: "icon-[lucide--user-x] size-4" }
                                        "No"
                                    }
                                    button {
                                        r#type: "button",
                                        class: "{dock_maybe_class}",
                                        onclick: move |_| {
                                            notification_service
                                                .send(NotificationCommand::TentativelyAcceptInvitation(notification().id));
                                        },
                                        span { class: "icon-[lucide--user-minus] size-4" }
                                        "Maybe"
                                    }
                                }
                            }
                        }
                    }
                }

                if let Some(description) = sanitized_description() {
                    Card {
                        variant: CardVariant::Default,
                        p {
                            class: "w-full prose prose-sm dark:prose-invert",
                            dangerous_inner_html: "{description}"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn AttendeeRow(
    attendee: ReadSignal<
        universal_inbox::third_party::integrations::google_calendar::EventAttendee,
    >,
) -> Element {
    let attendee_value = attendee();
    let user_name = attendee_value
        .display_name
        .clone()
        .unwrap_or_else(|| attendee_value.email.clone());
    let avatar_url = Url::parse(&Generator::default().generate(&attendee_value.email)).ok();

    let (icon_class, row_variant) = match attendee_value.response_status {
        GoogleCalendarEventAttendeeResponseStatus::Accepted => (
            "icon-[lucide--check-circle-2] size-4 text-success",
            StatusVariant::Default,
        ),
        GoogleCalendarEventAttendeeResponseStatus::Declined => (
            "icon-[lucide--x-circle] size-4 text-error",
            StatusVariant::Error,
        ),
        GoogleCalendarEventAttendeeResponseStatus::Tentative => (
            "icon-[lucide--help-circle] size-4 text-warning",
            StatusVariant::Warning,
        ),
        GoogleCalendarEventAttendeeResponseStatus::NeedsAction => (
            "icon-[lucide--clock] size-4 text-base-content/60",
            StatusVariant::Default,
        ),
    };

    rsx! {
        StatusRow {
            variant: row_variant,
            div {
                class: "{STATUS_ROW_NAME_CLASS}",
                span { class: "{icon_class}" }
                UserWithAvatar {
                    user_name: Some(user_name),
                    avatar_url: Some(avatar_url),
                    display_name: true,
                }
            }
        }
    }
}

/// Compute the (month abbreviation, day-of-month, weekday, time) tuple for the
/// big date block that anchors the preview. Falls back to `None` when the event
/// has no usable start time.
fn compute_date_block(event: &GoogleCalendarEvent) -> Option<(String, String, String, String)> {
    let start_dt = event.start.datetime;
    let start_date = event.start.date;

    if let Some(dt) = start_dt {
        let local = dt.with_timezone(&chrono::Local);
        Some((
            local.format("%b").to_string(),
            local.format("%-d").to_string(),
            local.format("%a").to_string(),
            local.format("%H:%M").to_string(),
        ))
    } else {
        start_date.map(|d| {
            (
                d.format("%b").to_string(),
                d.format("%-d").to_string(),
                d.format("%a").to_string(),
                String::new(),
            )
        })
    }
}

/// Formats a single RRule into human-readable frequency information.
///
/// For the common "weekly on a single day" case, collapses to "every Friday"
/// rather than the verbose "every week on Fridays".
fn format_single_rule(rule: &rrule::RRule) -> String {
    let interval = rule.get_interval();
    let freq = rule.get_freq();
    let weekdays = rule.get_by_weekday();
    let weekday_names: Vec<String> = weekdays.iter().map(weekday_full_name).collect();

    let mut parts: Vec<String> = Vec::new();

    // Collapsed form: "every {Day}" when this is a non-intervalled weekly rule with one weekday.
    if freq == Frequency::Weekly && interval == 1 && weekday_names.len() == 1 {
        parts.push(format!("every {}", weekday_names[0]));
    } else if interval > 1 {
        parts.push(match freq {
            Frequency::Secondly => format!("every {} seconds", interval),
            Frequency::Minutely => format!("every {} minutes", interval),
            Frequency::Hourly => format!("every {} hours", interval),
            Frequency::Daily => format!("every {} days", interval),
            Frequency::Weekly => format!("every {} weeks", interval),
            Frequency::Monthly => format!("every {} months", interval),
            Frequency::Yearly => format!("every {} years", interval),
        });
        if freq == Frequency::Weekly && !weekday_names.is_empty() {
            parts.push(format!("on {}", weekday_names.join(", ")));
        }
    } else {
        parts.push(match freq {
            Frequency::Secondly => "every second".to_string(),
            Frequency::Minutely => "every minute".to_string(),
            Frequency::Hourly => "every hour".to_string(),
            Frequency::Daily => "every day".to_string(),
            Frequency::Weekly => "every week".to_string(),
            Frequency::Monthly => "every month".to_string(),
            Frequency::Yearly => "every year".to_string(),
        });
        if freq == Frequency::Weekly && weekday_names.len() > 1 {
            parts.push(format!("on {}", weekday_names.join(", ")));
        }
    }

    if let Some(count) = rule.get_count() {
        parts.push(format!("{} times", count));
    } else if let Some(until) = rule.get_until() {
        parts.push(format!("until {}", until.format("%b %e, %Y")));
    }

    parts.join(" ")
}

fn weekday_full_name(weekday: &rrule::NWeekday) -> String {
    match weekday.to_string().get(..2).unwrap_or("") {
        "MO" => "Monday",
        "TU" => "Tuesday",
        "WE" => "Wednesday",
        "TH" => "Thursday",
        "FR" => "Friday",
        "SA" => "Saturday",
        "SU" => "Sunday",
        _ => "",
    }
    .to_string()
}

#[cfg(test)]
mod google_calendar_preview_tests {
    use wasm_bindgen_test::*;

    #[wasm_bindgen_test]
    fn test_html_sanitization_in_description() {
        // Test that HTML in Google Calendar event descriptions is properly sanitized
        let malicious_html = r#"<script>alert('xss')</script><p>Safe content</p><img src="javascript:alert('xss')">"#;
        let sanitized = ammonia::clean(malicious_html);

        // Should remove dangerous script tags but keep safe HTML
        assert!(!sanitized.contains("<script>"));
        assert!(!sanitized.contains("javascript:"));
        assert!(sanitized.contains("<p>Safe content</p>"));
    }

    #[wasm_bindgen_test]
    fn test_basic_html_preservation() {
        // Test that basic HTML formatting is preserved
        let basic_html = r#"<p>Meeting notes:</p><ul><li>Item 1</li><li>Item 2</li></ul><br><strong>Important</strong>"#;
        let sanitized = ammonia::clean(basic_html);

        // Should preserve basic formatting tags
        assert!(sanitized.contains("<p>Meeting notes:</p>"));
        assert!(sanitized.contains("<ul>"));
        assert!(sanitized.contains("<li>Item 1</li>"));
        assert!(sanitized.contains("<strong>Important</strong>"));
    }
}
