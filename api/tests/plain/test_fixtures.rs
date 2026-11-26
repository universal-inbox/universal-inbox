use rrule::Frequency;
use rstest::*;

use crate::helpers::notification::google_calendar::google_calendar_event;

#[rstest]
fn test_google_calendar_event_fixture_has_recurrence() {
    let event = google_calendar_event();

    let Some(rrule_set) = event.recurrence else {
        unreachable!("Expected recurrence to be present in fixture");
    };

    let Some(rrule) = rrule_set.get_rrule().first() else {
        unreachable!("Expected at least one RRULE");
    };

    assert_eq!(rrule.get_freq(), Frequency::Weekly);
}
