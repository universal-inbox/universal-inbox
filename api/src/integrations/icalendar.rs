use anyhow::anyhow;
use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use ical::{parser::ical::component::IcalEvent, property::Property};
use universal_inbox::pii::Pii;

use universal_inbox::third_party::integrations::google_calendar::{
    EventDateTime, EventReply, GoogleCalendarEventAttendeeResponseStatus, RecurrenceId,
};

/// The occurrence of a recurring event a VEVENT is about, from its `RECURRENCE-ID`.
/// Returns `None` when the VEVENT has none, and an error when it cannot be parsed.
pub fn parse_recurrence_id(vcal_event: &IcalEvent) -> Result<Option<RecurrenceId>, anyhow::Error> {
    let Some(property) = find_property(vcal_event, "RECURRENCE-ID") else {
        return Ok(None);
    };
    parse_date_time_property(property)
        .as_ref()
        .and_then(RecurrenceId::from_event_date_time)
        .map(Some)
        .ok_or_else(|| anyhow!("Invalid RECURRENCE-ID `{:?}`", property.value))
}

/// Extract the attendee answer from the VEVENT of an iCalendar `METHOD:REPLY` message.
/// Returns `None` when the event has no attendee with a usable email.
pub fn parse_event_reply(vcal_event: &IcalEvent) -> Option<EventReply> {
    let attendee = find_property(vcal_event, "ATTENDEE")?;
    let attendee_email = attendee
        .value
        .as_deref()?
        .trim()
        .trim_start_matches("mailto:")
        .trim_start_matches("MAILTO:")
        .to_string();
    if attendee_email.is_empty() {
        return None;
    }

    let response_status = match find_param(attendee, "PARTSTAT")
        .unwrap_or_default()
        .to_uppercase()
        .as_str()
    {
        "ACCEPTED" => GoogleCalendarEventAttendeeResponseStatus::Accepted,
        "DECLINED" => GoogleCalendarEventAttendeeResponseStatus::Declined,
        "TENTATIVE" => GoogleCalendarEventAttendeeResponseStatus::Tentative,
        _ => GoogleCalendarEventAttendeeResponseStatus::NeedsAction,
    };

    let comment = find_param(attendee, "X-RESPONSE-COMMENT")
        .or_else(|| find_property(vcal_event, "COMMENT").and_then(|p| p.value.clone()))
        .filter(|comment| !comment.trim().is_empty());

    let occurrence_start = find_property(vcal_event, "RECURRENCE-ID")
        .or_else(|| find_property(vcal_event, "DTSTART"))
        .and_then(parse_date_time_property);

    Some(EventReply {
        attendee_email: Pii::new(attendee_email),
        attendee_display_name: find_param(attendee, "CN").map(Pii::new),
        response_status,
        comment,
        occurrence_start,
    })
}

fn find_property<'a>(vcal_event: &'a IcalEvent, name: &str) -> Option<&'a Property> {
    vcal_event
        .properties
        .iter()
        .find(|property| property.name.eq_ignore_ascii_case(name))
}

fn find_param(property: &Property, name: &str) -> Option<String> {
    property
        .params
        .as_ref()?
        .iter()
        .find(|(param_name, _)| param_name.eq_ignore_ascii_case(name))
        .and_then(|(_, values)| values.first())
        .map(|value| value.trim_matches('"').to_string())
}

/// Parse a `DTSTART` / `RECURRENCE-ID` property: a UTC (`...Z`), `TZID=` or floating
/// date-time, or a `VALUE=DATE` date.
fn parse_date_time_property(property: &Property) -> Option<EventDateTime> {
    let value = property.value.as_deref()?.trim();

    let is_date = find_param(property, "VALUE").is_some_and(|v| v.eq_ignore_ascii_case("DATE"))
        || !value.contains('T');
    if is_date {
        return NaiveDate::parse_from_str(value, "%Y%m%d")
            .ok()
            .map(|date| EventDateTime {
                date: Some(date),
                datetime: None,
                timezone: None,
            });
    }

    if let Some(utc_value) = value.strip_suffix('Z') {
        let naive = NaiveDateTime::parse_from_str(utc_value, "%Y%m%dT%H%M%S").ok()?;
        return Some(EventDateTime {
            date: None,
            datetime: Some(Utc.from_utc_datetime(&naive)),
            timezone: None,
        });
    }

    let naive = NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%S").ok()?;
    let tzid = find_param(property, "TZID");
    let datetime: DateTime<Utc> = match tzid.as_deref().and_then(|tz| tz.parse::<Tz>().ok()) {
        Some(tz) => tz
            .from_local_datetime(&naive)
            .earliest()?
            .with_timezone(&Utc),
        // Floating time or unknown timezone: best effort, read it as UTC
        None => Utc.from_utc_datetime(&naive),
    };

    Some(EventDateTime {
        date: None,
        datetime: Some(datetime),
        timezone: tzid,
    })
}

#[cfg(test)]
mod tests {
    use std::io::BufReader;

    use ical::IcalParser;
    use pretty_assertions::assert_eq;

    use super::*;

    fn parse_vevent(vevent: &str) -> IcalEvent {
        let raw = format!(
            "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nMETHOD:REPLY\r\nBEGIN:VEVENT\r\n{vevent}END:VEVENT\r\nEND:VCALENDAR\r\n"
        );
        IcalParser::new(BufReader::new(raw.as_bytes()))
            .next()
            .unwrap()
            .unwrap()
            .events
            .remove(0)
    }

    #[test]
    fn test_parse_accepted_reply_with_tzid_start() {
        let event = parse_vevent(
            "UID:event_icaluid2\r\n\
             DTSTART;TZID=Europe/Paris:20240506T151500\r\n\
             ATTENDEE;CUTYPE=INDIVIDUAL;ROLE=REQ-PARTICIPANT;PARTSTAT=ACCEPTED;RSVP=TRUE;CN=Jane Doe;X-NUM-GUESTS=0:mailto:user2@example.com\r\n",
        );

        assert_eq!(
            parse_event_reply(&event),
            Some(EventReply {
                attendee_email: Pii::new("user2@example.com".to_string()),
                attendee_display_name: Some(Pii::new("Jane Doe".to_string())),
                response_status: GoogleCalendarEventAttendeeResponseStatus::Accepted,
                comment: None,
                occurrence_start: Some(EventDateTime {
                    date: None,
                    datetime: Some(Utc.with_ymd_and_hms(2024, 5, 6, 13, 15, 0).unwrap()),
                    timezone: Some("Europe/Paris".to_string()),
                }),
            })
        );
    }

    #[test]
    fn test_parse_declined_reply_for_a_recurring_event_occurrence() {
        let event = parse_vevent(
            "UID:event_icaluid2\r\n\
             RECURRENCE-ID:20251030T090000Z\r\n\
             DTSTART;TZID=Europe/Paris:20240506T151500\r\n\
             ATTENDEE;PARTSTAT=DECLINED;CN=\"Doe, Jane\";X-RESPONSE-COMMENT=\"On holidays\":mailto:user2@example.com\r\n",
        );

        let reply = parse_event_reply(&event).unwrap();

        assert_eq!(
            reply.response_status,
            GoogleCalendarEventAttendeeResponseStatus::Declined
        );
        assert_eq!(
            reply.attendee_display_name,
            Some(Pii::new("Doe, Jane".to_string()))
        );
        assert_eq!(reply.comment, Some("On holidays".to_string()));
        assert_eq!(
            reply.occurrence_start,
            Some(EventDateTime {
                date: None,
                datetime: Some(Utc.with_ymd_and_hms(2025, 10, 30, 9, 0, 0).unwrap()),
                timezone: None,
            })
        );
    }

    #[test]
    fn test_parse_tentative_reply_for_an_all_day_event() {
        let event = parse_vevent(
            "UID:event_icaluid2\r\n\
             DTSTART;VALUE=DATE:20251030\r\n\
             ATTENDEE;PARTSTAT=TENTATIVE:mailto:user2@example.com\r\n",
        );

        let reply = parse_event_reply(&event).unwrap();

        assert_eq!(
            reply.response_status,
            GoogleCalendarEventAttendeeResponseStatus::Tentative
        );
        assert_eq!(reply.attendee_display_name, None);
        assert_eq!(
            reply.occurrence_start,
            Some(EventDateTime {
                date: Some(NaiveDate::from_ymd_opt(2025, 10, 30).unwrap()),
                datetime: None,
                timezone: None,
            })
        );
    }

    #[test]
    fn test_parse_reply_without_attendee() {
        let event = parse_vevent("UID:event_icaluid2\r\nDTSTART:20251030T090000Z\r\n");

        assert_eq!(parse_event_reply(&event), None);
    }

    mod recurrence_id {
        use pretty_assertions::assert_eq;

        use super::*;

        fn recurrence_id(property: &str) -> Result<Option<RecurrenceId>, anyhow::Error> {
            parse_recurrence_id(&parse_vevent(&format!("UID:event_icaluid2\r\n{property}")))
        }

        fn at(hour: u32) -> Option<RecurrenceId> {
            Some(RecurrenceId::DateTime(
                Utc.with_ymd_and_hms(2026, 6, 18, hour, 15, 0).unwrap(),
            ))
        }

        #[test]
        fn test_parse_utc_recurrence_id() {
            assert_eq!(
                recurrence_id("RECURRENCE-ID:20260618T091500Z\r\n").unwrap(),
                at(9)
            );
        }

        #[test]
        fn test_parse_recurrence_id_with_tzid() {
            assert_eq!(
                recurrence_id("RECURRENCE-ID;TZID=Europe/Paris:20260618T111500\r\n").unwrap(),
                at(9)
            );
        }

        #[test]
        fn test_parse_floating_recurrence_id_or_unknown_tzid_as_utc() {
            assert_eq!(
                recurrence_id("RECURRENCE-ID:20260618T091500\r\n").unwrap(),
                at(9)
            );
            assert_eq!(
                recurrence_id("RECURRENCE-ID;TZID=Mars/Olympus:20260618T091500\r\n").unwrap(),
                at(9)
            );
        }

        #[test]
        fn test_parse_all_day_recurrence_id() {
            let expected = Some(RecurrenceId::Date(
                NaiveDate::from_ymd_opt(2026, 6, 18).unwrap(),
            ));
            assert_eq!(
                recurrence_id("RECURRENCE-ID:20260618\r\n").unwrap(),
                expected
            );
            assert_eq!(
                recurrence_id("RECURRENCE-ID;VALUE=DATE:20260618\r\n").unwrap(),
                expected
            );
        }

        #[test]
        fn test_parse_missing_recurrence_id() {
            assert_eq!(recurrence_id("").unwrap(), None);
        }

        #[test]
        fn test_parse_invalid_recurrence_id() {
            assert!(recurrence_id("RECURRENCE-ID:not-a-date\r\n").is_err());
        }
    }
}
