use universal_inbox::third_party::integrations::google_calendar::GoogleCalendarEvent;

use chrono::TimeZone;
use chrono_tz::{OffsetName, Tz};

pub fn compute_date_label(
    google_calendar_event: GoogleCalendarEvent,
    date_format: &str,
) -> Option<String> {
    let start_date = google_calendar_event.start.date.or_else(|| {
        google_calendar_event
            .start
            .datetime
            .map(|datetime| datetime.date_naive())
    })?;
    let end_date = google_calendar_event.end.date.or_else(|| {
        google_calendar_event
            .end
            .datetime
            .map(|datetime| datetime.date_naive())
    })?;

    // Extract timezone information
    let timezone = google_calendar_event
        .start
        .timezone
        .as_ref()
        .or(google_calendar_event.end.timezone.as_ref());

    if let Some(start_datetime) = google_calendar_event.start.datetime {
        if let Some(end_datetime) = google_calendar_event.end.datetime {
            // Format times in the specified timezone if available
            if let Some(tz_str) = timezone {
                // Try to parse the timezone
                if let Ok(tz) = tz_str.parse::<Tz>() {
                    let start_time = tz.from_utc_datetime(&start_datetime.naive_utc());
                    let end_time = tz.from_utc_datetime(&end_datetime.naive_utc());
                    let tz_name = start_time
                        .offset()
                        .abbreviation()
                        .unwrap_or_else(|| tz.name());

                    if start_date == end_date {
                        return Some(format!(
                            "{} {} - {} ({})",
                            start_date.format(date_format),
                            start_time.format("%H:%M"),
                            end_time.format("%H:%M"),
                            tz_name
                        ));
                    }
                    return Some(format!(
                        "{} {} - {} {} ({})",
                        start_date.format(date_format),
                        start_time.format("%H:%M"),
                        end_date.format(date_format),
                        end_time.format("%H:%M"),
                        tz_name
                    ));
                }
            }

            // Fallback to UTC if timezone parsing fails or is not available
            let start_time = start_datetime.time();
            let end_time = end_datetime.time();

            if start_date == end_date {
                return Some(format!(
                    "{} {} - {} (UTC)",
                    start_date.format(date_format),
                    start_time.format("%H:%M"),
                    end_time.format("%H:%M")
                ));
            }
            return Some(format!(
                "{} {} - {} {} (UTC)",
                start_date.format(date_format),
                start_time.format("%H:%M"),
                end_date.format(date_format),
                end_time.format("%H:%M")
            ));
        }
    }

    if start_date == end_date {
        return Some(format!("{}", start_date.format(date_format)));
    }
    Some(format!(
        "{} - {}",
        start_date.format(date_format),
        end_date.format(date_format)
    ))
}
