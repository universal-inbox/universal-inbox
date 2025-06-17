#![allow(non_snake_case)]
use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::{
        bs_icons::{
            BsArrowRepeat, BsArrowUpRightSquare, BsCalendar2Event, BsPeople, BsPerson,
            BsPersonCheck, BsPersonDash, BsPersonFill, BsPersonX, BsXCircleFill,
        },
        md_communication_icons::MdLocationOn,
    },
    Icon,
};
use rrule::Frequency;

use universal_inbox::{
    notification::NotificationWithTask,
    third_party::integrations::google_calendar::{
        EventAttendee, EventMethod, GoogleCalendarEvent, GoogleCalendarEventAttendeeResponseStatus,
        GoogleCalendarEventStatus,
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

    // Check if the event is recurring
    let _is_recurring = use_memo(move || {
        google_calendar_event().recurrence.is_some()
            || google_calendar_event().recurring_event_id.is_some()
    });

    // Format recurrence rules in a human-readable way
    let recurrence_details = use_memo(move || format_recurrence_details(&google_calendar_event()));

    let sanitized_description = use_memo(move || {
        google_calendar_event()
            .description
            .as_ref()
            .map(|desc| ammonia::clean(desc))
    });

    let accepted_style = if is_accepted() {
        "btn-success checked:bg-success! checked:text-success-content!"
    } else {
        "btn-soft"
    };
    let declined_style = if is_declined() {
        "btn-error checked:bg-error! checked:text-error-content!"
    } else {
        "btn-soft"
    };
    let tentative_style = if is_tentative() {
        "btn-warning checked:bg-warning! checked:text-warning-content!"
    } else {
        "btn-soft"
    };

    rsx! {
        div {
            class: "flex flex-col gap-2 w-full h-full",

            h3 {
                class: "flex items-center gap-2 text-base",

                Icon { class: "flex-none h-5 w-5", icon: BsCalendar2Event }
                a {
                    class: "flex items-center",
                    href: "{link}",
                    target: "_blank",
                    "{notification().title}"
                    Icon { class: "h-5 w-5 min-w-5 text-base-content/50 p-1", icon: BsArrowUpRightSquare }
                }
            }

            if is_cancelled() {
                div {
                    class: "alert alert-error mt-2",
                    div {
                        class: "flex items-center gap-2",
                        Icon { class: "h-6 w-6", icon: BsXCircleFill }
                        span {
                            class: "font-semibold",
                            "This event has been cancelled"
                        }
                    }
                }
            }

            div {
                id: "notification-preview-details",
                class: "flex flex-col gap-2 w-full h-full overflow-y-auto scroll-y-auto",

                if let Some(description) = sanitized_description() {
                    SmallCard {
                        div {
                            class: "prose prose-sm prose-table:text-sm prose-img:max-w-none",
                            dangerous_inner_html: "{description}"
                        }
                    }
                }

                if let Some(creator_label) = creator_label.as_ref() {
                    SmallCard {
                        Icon { class: "text-base-content/50 h-5 w-5", icon: BsPerson }
                        span { "{creator_label}" }
                    }
                }

                SmallCard {
                    Icon { class: "text-base-content/50 h-5 w-5", icon: BsPersonFill }
                    span { "{organizer_label}" }
                }

                if let Some(date_label) = date_label() {
                    SmallCard {
                        Icon { class: "text-base-content/50 h-5 w-5", icon: BsCalendar2Event }
                        span { "{date_label}" }
                    }
                }

                CollapseCardWithIcon {
                    id: "google-calendar-guests",
                    title: "Guests",
                    icon: rsx! { Icon { class: "text-base-content/50 h-5 w-5", icon: BsPeople } },
                    opened: expand_details(),
                    table {
                        class: "table table-auto table-sm w-full",
                        tbody {
                            for attendee in google_calendar_event().attendees {
                                CalendarEventAttendeeRow { attendee }
                            }
                        }
                    }
                }

                if let Some(location) = google_calendar_event().location.as_ref() {
                    SmallCard {
                        Icon { class: "text-base-content/50 h-5 w-5", icon: MdLocationOn }
                        span { "{location}" }
                    }
                }

                // Display recurrence information if present
                if let Some(recurrence_info) = recurrence_details() {
                    div {
                        class: "card bg-base-200 p-3",
                        div {
                            class: "flex items-center gap-2 mb-2",
                            Icon { class: "text-base-content/70 h-5 w-5 min-w-5", icon: BsArrowRepeat }
                            div {
                                class: "space-y-1",
                                for info in recurrence_info.iter() {
                                    div { class: "text-sm", "{info}" }
                                }
                            }
                        }
                    }
                }

                if !is_cancelled() {
                    div {
                        class: "join w-full",
                        input {
                            class: "join-item btn rounded-l-lg grow {accepted_style}",
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
                            class: "join-item btn grow {declined_style}",
                            type: "radio",
                            name: "action",
                            checked: "{is_declined}",
                            onclick: move |_| {
                                notification_service
                                    .send(NotificationCommand::DeclineInvitation(notification().id));
                            },
                            Icon { class: "h-5 w-5", icon: BsPersonX }
                            "No"
                        }
                        input {
                            class: "join-item btn rounded-r-lg grow {tentative_style}",
                            type: "radio",
                            name: "action",
                            checked: "{is_tentative}",
                            onclick: move |_| {
                                notification_service
                                    .send(NotificationCommand::TentativelyAcceptInvitation(notification().id));
                            },
                            Icon { class: "h-5 w-5", icon: BsPersonDash }
                            "Maybe"
                        }
                    }
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

/// Checks if a date appears to be a placeholder/default date rather than meaningful
fn is_placeholder_date(date: &chrono::DateTime<rrule::Tz>) -> bool {
    // Common placeholder dates
    let unix_epoch = chrono::DateTime::parse_from_rfc3339("1970-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&rrule::Tz::UTC);
    let year_1900 = chrono::DateTime::parse_from_rfc3339("1900-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&rrule::Tz::UTC);
    let minimal_date = chrono::DateTime::parse_from_rfc3339("0001-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&rrule::Tz::UTC);

    // Check if it matches common placeholder dates
    if *date == unix_epoch || *date == year_1900 || *date == minimal_date {
        return true;
    }

    // Check if the date is unreasonably old (more than 50 years ago)
    let now = chrono::Utc::now().with_timezone(&rrule::Tz::UTC);
    let fifty_years_ago = now - chrono::Duration::days(365 * 50);
    if *date < fifty_years_ago {
        return true;
    }

    false
}

/// Formats a single RRule into human-readable frequency information
fn format_single_rule(rule: &rrule::RRule) -> String {
    let interval = rule.get_interval();
    let mut frequency_parts = if interval > 1 {
        vec![match rule.get_freq() {
            Frequency::Secondly => format!("every {} seconds", interval),
            Frequency::Minutely => format!("every {} minutes", interval),
            Frequency::Hourly => format!("every {} hours", interval),
            Frequency::Daily => format!("every {} days", interval),
            Frequency::Weekly => format!("every {} weeks", interval),
            Frequency::Monthly => format!("every {} months", interval),
            Frequency::Yearly => format!("every {} years", interval),
        }]
    } else {
        vec![match rule.get_freq() {
            Frequency::Secondly => "every second".to_string(),
            Frequency::Minutely => "every minute".to_string(),
            Frequency::Hourly => "every hour".to_string(),
            Frequency::Daily => "every day".to_string(),
            Frequency::Weekly => "every week".to_string(),
            Frequency::Monthly => "every month".to_string(),
            Frequency::Yearly => "every year".to_string(),
        }]
    };

    // Get days of week if weekly frequency
    if rule.get_freq() == Frequency::Weekly {
        let weekdays = rule.get_by_weekday();
        if !weekdays.is_empty() {
            let day_names: Vec<String> = weekdays
                .iter()
                .map(|day| {
                    let day_str = day.to_string();
                    if day_str.len() >= 2 {
                        format!("{}s", &day_str[0..2])
                    } else {
                        day_str
                    }
                })
                .collect();
            frequency_parts.push(format!("on {}", day_names.join(", ")));
        }
    }

    // Get count or until date for frequency line
    if let Some(count) = rule.get_count() {
        frequency_parts.push(format!("{} times", count));
    } else if let Some(until) = rule.get_until() {
        frequency_parts.push(format!("until {}", until.format("%b %e, %Y")));
    }

    frequency_parts.join(" ")
}

/// Formats recurrence information from a Google Calendar event into human-readable strings
///
/// # Arguments
/// * `event` - The GoogleCalendarEvent containing recurrence information
///
/// # Returns
/// * `Some(Vec<String>)` - Formatted recurrence information strings with:
///   - For single RRule: frequency details, start/end dates, RDATE, exclusions
///   - For multiple RRules: each pattern separately, combined range, RDATE, exclusions
/// * `None` - If the event is not recurring
fn format_recurrence_details(event: &GoogleCalendarEvent) -> Option<Vec<String>> {
    if let Some(recurrence) = &event.recurrence {
        // Get all recurrence rules
        let rules = recurrence.get_rrule();
        if !rules.is_empty() {
            let mut result = Vec::new();

            // Handle multiple rules differently than single rules
            if rules.len() > 1 {
                // Multiple rules: display each one separately
                for (i, rule) in rules.iter().enumerate() {
                    result.push(format!(
                        "Pattern {}: Repeats {}",
                        i + 1,
                        format_single_rule(rule)
                    ));
                }

                // Add combined date range showing overall start and potential end (only if meaningful)
                let dtstart = recurrence.get_dt_start();
                if !is_placeholder_date(dtstart) {
                    let mut combined_range =
                        format!("Combined: from {}", dtstart.format("%b %e, %Y"));

                    // Try to find the latest end date from all rules
                    let mut latest_end: Option<chrono::DateTime<rrule::Tz>> = None;

                    for rule in rules {
                        if let Some(until_date) = rule.get_until() {
                            if latest_end.is_none() || until_date > &latest_end.unwrap() {
                                latest_end = Some(*until_date);
                            }
                        } else if let Some(_count) = rule.get_count() {
                            // For count-based rules, we'd need to calculate occurrences per rule
                            // For simplicity, we'll show combined occurrences from the full RRuleSet
                            let occurrences = recurrence.clone().all(100); // Reasonable limit for display
                            if let Some(last_occurrence) = occurrences.dates.last() {
                                if latest_end.is_none() || *last_occurrence > latest_end.unwrap() {
                                    latest_end = Some(*last_occurrence);
                                }
                            }
                        }
                    }

                    if let Some(end_date) = latest_end {
                        combined_range.push_str(&format!(" to {}", end_date.format("%b %e, %Y")));
                    }

                    result.push(combined_range);
                }
            } else {
                // Single rule: use the existing logic
                let rule = &rules[0];

                result.push(format!("Repeats {}", format_single_rule(rule)));

                // LINE 2: Start and end date of the recurrence from RRULE/RRuleSet (only if meaningful)
                let dtstart = recurrence.get_dt_start();
                if !is_placeholder_date(dtstart) {
                    let mut date_parts = Vec::new();

                    // Start date from the recurrence rule
                    let mut start_end_part = format!("from {}", dtstart.format("%b %e, %Y"));

                    // End date from the recurrence rule
                    if let Some(until_date) = rule.get_until() {
                        // For UNTIL-based rules, show the explicit end date
                        start_end_part.push_str(&format!(" to {}", until_date.format("%b %e, %Y")));
                    } else if let Some(count) = rule.get_count() {
                        // For COUNT-based rules, calculate the final occurrence date
                        let occurrences = recurrence.clone().all(count as u16);
                        if let Some(last_occurrence) = occurrences.dates.last() {
                            start_end_part
                                .push_str(&format!(" to {}", last_occurrence.format("%b %e, %Y")));
                        }
                    }
                    // For unlimited rules, we don't add an end date since they theoretically never end

                    date_parts.push(start_end_part);

                    // Add RDATE (additional recurrence dates)
                    let rdates = recurrence.get_rdate();
                    if !rdates.is_empty() {
                        let formatted_rdates: Vec<String> = rdates
                            .iter()
                            .map(|d| d.format("%b %e, %Y").to_string())
                            .collect();
                        date_parts.push(format!("also on {}", formatted_rdates.join(", ")));
                    }

                    if !date_parts.is_empty() {
                        result.push(date_parts.join(" "));
                    }
                }
            }

            // REMAINING LINES: RDATE and Exclusion dates (applies to both single and multiple rules)

            // Add RDATE (additional recurrence dates) for multiple rule case
            if rules.len() > 1 {
                let rdates = recurrence.get_rdate();
                if !rdates.is_empty() {
                    let formatted_rdates: Vec<String> = rdates
                        .iter()
                        .map(|d| d.format("%b %e, %Y").to_string())
                        .collect();
                    result.push(format!("Additional dates: {}", formatted_rdates.join(", ")));
                }
            }

            // Exclusion dates
            let ex_dates = recurrence.get_exdate();
            if !ex_dates.is_empty() {
                let formatted_dates: Vec<String> = ex_dates
                    .iter()
                    .map(|d| d.format("%b %e, %Y").to_string())
                    .collect();
                if !formatted_dates.is_empty() {
                    result.push(format!("Excluded dates: {}", formatted_dates.join(", ")));
                }
            }

            Some(result)
        } else {
            Some(vec!["Recurring event".to_string()])
        }
    } else if event.recurring_event_id.is_some() {
        // This is an instance of a recurring event
        Some(vec!["Instance of a recurring series".to_string()])
    } else {
        None
    }
}

#[cfg(test)]
mod google_calendar_preview_tests {
    use super::*;
    use chrono::{DateTime, NaiveDate, Utc};
    use rrule::{RRule, RRuleSet};
    use universal_inbox::third_party::integrations::google_calendar::{
        EventCreator, EventDateTime, EventMethod, EventOrganizer, GoogleCalendarEvent,
        GoogleCalendarEventId, GoogleCalendarEventStatus, GoogleCalendarEventType, IcalUID,
    };
    use url::Url;
    use wasm_bindgen_test::*;

    fn create_test_datetime() -> DateTime<rrule::Tz> {
        DateTime::parse_from_rfc3339("2023-12-25T10:00:00Z")
            .unwrap()
            .with_timezone(&rrule::Tz::UTC)
    }

    fn create_test_event() -> GoogleCalendarEvent {
        GoogleCalendarEvent {
            method: EventMethod::Request,
            kind: "calendar#event".to_string(),
            etag: "test-etag".to_string(),
            id: GoogleCalendarEventId::from("test-event-id".to_string()),
            status: GoogleCalendarEventStatus::Confirmed,
            html_link: Url::parse("https://calendar.google.com/event").unwrap(),
            hangout_link: None,
            location: None,
            created: DateTime::parse_from_rfc3339("2023-12-25T09:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            updated: DateTime::parse_from_rfc3339("2023-12-25T09:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            summary: "Test Event".to_string(),
            description: None,
            creator: EventCreator {
                id: None,
                email: Some("creator@example.com".to_string()),
                display_name: Some("Creator".to_string()),
                self_: None,
            },
            organizer: EventOrganizer {
                id: None,
                email: "organizer@example.com".to_string(),
                display_name: Some("Organizer".to_string()),
                self_: None,
            },
            start: EventDateTime {
                date: Some(NaiveDate::from_ymd_opt(2023, 12, 25).unwrap()),
                datetime: Some(
                    DateTime::parse_from_rfc3339("2023-12-25T10:00:00Z")
                        .unwrap()
                        .with_timezone(&Utc),
                ),
                timezone: None,
            },
            end: EventDateTime {
                date: Some(NaiveDate::from_ymd_opt(2023, 12, 25).unwrap()),
                datetime: Some(
                    DateTime::parse_from_rfc3339("2023-12-25T11:00:00Z")
                        .unwrap()
                        .with_timezone(&Utc),
                ),
                timezone: None,
            },
            end_time_unspecified: false,
            icaluid: IcalUID::from("test-ical-uid".to_string()),
            sequence: 1,
            attendees: vec![],
            attachments: vec![],
            attendees_omitted: None,
            source: None,
            conference_data: None,
            guests_can_modify: None,
            reminders: None,
            event_type: GoogleCalendarEventType::Default,
            transparency: None,
            visibility: None,
            recurrence: None,
            recurring_event_id: None,
            original_start_time: None,
        }
    }

    #[wasm_bindgen_test]
    fn test_format_recurrence_details_no_recurrence() {
        let event = create_test_event();
        let result = format_recurrence_details(&event);
        assert_eq!(result, None);
    }

    #[wasm_bindgen_test]
    fn test_format_recurrence_details_recurring_event_instance() {
        let mut event = create_test_event();
        event.recurring_event_id = Some(GoogleCalendarEventId::from(
            "recurring-event-id".to_string(),
        ));
        let result = format_recurrence_details(&event);
        assert_eq!(
            result,
            Some(vec!["Instance of a recurring series".to_string()])
        );
    }

    #[wasm_bindgen_test]
    fn test_format_recurrence_details_daily() {
        let mut event = create_test_event();
        let mut rrule_set = RRuleSet::new(create_test_datetime());
        let rrule = RRule::new(Frequency::Daily)
            .validate(create_test_datetime())
            .unwrap();
        rrule_set = rrule_set.rrule(rrule);
        event.recurrence = Some(rrule_set);

        let result = format_recurrence_details(&event);
        assert_eq!(
            result,
            Some(vec![
                "Repeats every day".to_string(),
                "from Dec 25, 2023".to_string()
            ])
        );
    }

    #[wasm_bindgen_test]
    fn test_format_recurrence_details_weekly_with_interval() {
        let mut event = create_test_event();
        let mut rrule_set = RRuleSet::new(create_test_datetime());
        let rrule = RRule::new(Frequency::Weekly)
            .interval(2)
            .validate(create_test_datetime())
            .unwrap();
        rrule_set = rrule_set.rrule(rrule);
        event.recurrence = Some(rrule_set);

        let result = format_recurrence_details(&event);
        assert_eq!(
            result,
            Some(vec![
                "Repeats every 2 weeks on MOs".to_string(),
                "from Dec 25, 2023".to_string()
            ])
        );
    }

    #[wasm_bindgen_test]
    fn test_format_recurrence_details_with_count() {
        let mut event = create_test_event();
        let mut rrule_set = RRuleSet::new(create_test_datetime());
        let rrule = RRule::new(Frequency::Daily)
            .count(5)
            .validate(create_test_datetime())
            .unwrap();
        rrule_set = rrule_set.rrule(rrule);
        event.recurrence = Some(rrule_set);

        let result = format_recurrence_details(&event);
        assert_eq!(
            result,
            Some(vec![
                "Repeats every day 5 times".to_string(),
                "from Dec 25, 2023 to Dec 29, 2023".to_string()
            ])
        );
    }

    #[wasm_bindgen_test]
    fn test_format_recurrence_details_with_until() {
        let mut event = create_test_event();
        let mut rrule_set = RRuleSet::new(create_test_datetime());
        let until_date = DateTime::parse_from_rfc3339("2024-01-25T10:00:00Z")
            .unwrap()
            .with_timezone(&rrule::Tz::UTC);
        let rrule = RRule::new(Frequency::Weekly)
            .until(until_date)
            .validate(create_test_datetime())
            .unwrap();
        rrule_set = rrule_set.rrule(rrule);
        event.recurrence = Some(rrule_set);

        let result = format_recurrence_details(&event);
        assert_eq!(
            result,
            Some(vec![
                "Repeats every week on MOs until Jan 25, 2024".to_string(),
                "from Dec 25, 2023 to Jan 25, 2024".to_string()
            ])
        );
    }

    #[wasm_bindgen_test]
    fn test_format_recurrence_details_with_rdate() {
        let mut event = create_test_event();
        let mut rrule_set = RRuleSet::new(create_test_datetime());
        let rrule = RRule::new(Frequency::Weekly)
            .validate(create_test_datetime())
            .unwrap();
        rrule_set = rrule_set.rrule(rrule);

        // Add additional recurrence dates (RDATE)
        let rdate1 = DateTime::parse_from_rfc3339("2024-01-15T10:00:00Z")
            .unwrap()
            .with_timezone(&rrule::Tz::UTC);
        let rdate2 = DateTime::parse_from_rfc3339("2024-02-15T10:00:00Z")
            .unwrap()
            .with_timezone(&rrule::Tz::UTC);
        rrule_set = rrule_set.rdate(rdate1).rdate(rdate2);

        event.recurrence = Some(rrule_set);

        let result = format_recurrence_details(&event);
        assert_eq!(
            result,
            Some(vec![
                "Repeats every week on MOs".to_string(),
                "from Dec 25, 2023 also on Jan 15, 2024, Feb 15, 2024".to_string()
            ])
        );
    }

    #[wasm_bindgen_test]
    fn test_format_recurrence_details_multiple_rules() {
        let mut event = create_test_event();
        let mut rrule_set = RRuleSet::new(create_test_datetime());

        // Add two different rules: daily and weekly
        let daily_rule = RRule::new(Frequency::Daily)
            .count(3)
            .validate(create_test_datetime())
            .unwrap();
        let weekly_rule = RRule::new(Frequency::Weekly)
            .count(2)
            .validate(create_test_datetime())
            .unwrap();

        rrule_set = rrule_set.rrule(daily_rule).rrule(weekly_rule);
        event.recurrence = Some(rrule_set);

        let result = format_recurrence_details(&event);

        // Should show both patterns and a combined range
        let expected = vec![
            "Pattern 1: Repeats every day 3 times".to_string(),
            "Pattern 2: Repeats every week on MOs 2 times".to_string(),
            "Combined: from Dec 25, 2023 to Jan  1, 2024".to_string(), // Combined occurrences from both rules
        ];

        assert_eq!(result, Some(expected));
    }

    #[wasm_bindgen_test]
    fn test_format_recurrence_details_multiple_rules_with_until() {
        let mut event = create_test_event();
        let mut rrule_set = RRuleSet::new(create_test_datetime());

        let until_date1 = DateTime::parse_from_rfc3339("2024-01-15T10:00:00Z")
            .unwrap()
            .with_timezone(&rrule::Tz::UTC);
        let until_date2 = DateTime::parse_from_rfc3339("2024-02-15T10:00:00Z")
            .unwrap()
            .with_timezone(&rrule::Tz::UTC);

        // Add two rules with different until dates
        let rule1 = RRule::new(Frequency::Weekly)
            .until(until_date1)
            .validate(create_test_datetime())
            .unwrap();
        let rule2 = RRule::new(Frequency::Daily)
            .until(until_date2)
            .validate(create_test_datetime())
            .unwrap();

        rrule_set = rrule_set.rrule(rule1).rrule(rule2);
        event.recurrence = Some(rrule_set);

        let result = format_recurrence_details(&event);

        // Should show both patterns and combined range with latest end date
        let expected = vec![
            "Pattern 1: Repeats every week on MOs until Jan 15, 2024".to_string(),
            "Pattern 2: Repeats every day until Feb 15, 2024".to_string(),
            "Combined: from Dec 25, 2023 to Feb 15, 2024".to_string(), // Latest end date
        ];

        assert_eq!(result, Some(expected));
    }

    #[wasm_bindgen_test]
    fn test_format_recurrence_details_with_placeholder_date() {
        let mut event = create_test_event();

        // Create RRuleSet with Unix epoch (placeholder date)
        let unix_epoch = DateTime::parse_from_rfc3339("1970-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&rrule::Tz::UTC);
        let mut rrule_set = RRuleSet::new(unix_epoch);
        let rrule = RRule::new(Frequency::Daily)
            .count(5)
            .validate(unix_epoch)
            .unwrap();
        rrule_set = rrule_set.rrule(rrule);
        event.recurrence = Some(rrule_set);

        let result = format_recurrence_details(&event);
        // Should only show frequency, no date range since it's a placeholder
        assert_eq!(result, Some(vec!["Repeats every day 5 times".to_string()]));
    }

    #[wasm_bindgen_test]
    fn test_format_recurrence_details_multiple_rules_with_placeholder_date() {
        let mut event = create_test_event();

        // Create RRuleSet with year 1900 (placeholder date)
        let year_1900 = DateTime::parse_from_rfc3339("1900-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&rrule::Tz::UTC);
        let mut rrule_set = RRuleSet::new(year_1900);

        let daily_rule = RRule::new(Frequency::Daily)
            .count(3)
            .validate(year_1900)
            .unwrap();
        let weekly_rule = RRule::new(Frequency::Weekly)
            .count(2)
            .validate(year_1900)
            .unwrap();

        rrule_set = rrule_set.rrule(daily_rule).rrule(weekly_rule);
        event.recurrence = Some(rrule_set);

        let result = format_recurrence_details(&event);

        // Should show patterns but no combined date range since it's a placeholder
        let expected = vec![
            "Pattern 1: Repeats every day 3 times".to_string(),
            "Pattern 2: Repeats every week on MOs 2 times".to_string(),
        ];

        assert_eq!(result, Some(expected));
    }

    #[wasm_bindgen_test]
    fn test_format_recurrence_details_empty_rules() {
        let mut event = create_test_event();
        let rrule_set = RRuleSet::new(create_test_datetime()); // No rules added
        event.recurrence = Some(rrule_set);

        let result = format_recurrence_details(&event);
        assert_eq!(result, Some(vec!["Recurring event".to_string()]));
    }

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
