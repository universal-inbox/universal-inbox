use universal_inbox::third_party::integrations::google_calendar::GoogleCalendarEvent;

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

    if let Some(start_time) = google_calendar_event
        .start
        .datetime
        .map(|datetime| datetime.naive_local().time())
    {
        if let Some(end_time) = google_calendar_event
            .end
            .datetime
            .map(|datetime| datetime.naive_local().time())
        {
            if start_date == end_date {
                return Some(format!(
                    "{} {} - {}",
                    start_date.format(date_format),
                    start_time.format("%H:%M"),
                    end_time.format("%H:%M")
                ));
            }
            return Some(format!(
                "{} {} - {} {}",
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
