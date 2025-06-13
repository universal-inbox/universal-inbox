use anyhow::anyhow;
use chrono::{DateTime, NaiveDate, Timelike, Utc};
use rrule::{RRuleSet, Tz};
use serde::{Deserialize, Deserializer, Serialize};
use serde_with::serde_as;
use std::str::FromStr;
use typed_id::TypedId;
use url::Url;
use uuid::Uuid;

use crate::{
    integration_connection::IntegrationConnectionId,
    third_party::item::{ThirdPartyItem, ThirdPartyItemData, ThirdPartyItemFromSource},
    user::UserId,
    HasHtmlUrl,
};

pub const DEFAULT_GOOGLE_CALENDAR_HTML_URL: &str = "https://calendar.google.com";

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct GoogleCalendarEvent {
    #[serde(default)]
    pub method: EventMethod,
    pub kind: String,
    pub etag: String,
    pub id: GoogleCalendarEventId,
    pub status: GoogleCalendarEventStatus,
    #[serde(rename = "htmlLink")]
    pub html_link: Url,
    #[serde(
        default,
        rename = "hangoutLink",
        skip_serializing_if = "Option::is_none"
    )]
    pub hangout_link: Option<Url>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
    pub summary: String,
    pub description: Option<String>,
    pub creator: EventCreator,
    pub organizer: EventOrganizer,
    pub start: EventDateTime,
    pub end: EventDateTime,
    #[serde(default, rename = "endTimeUnspecified")]
    pub end_time_unspecified: bool,
    #[serde(rename = "iCalUID")]
    pub icaluid: IcalUID,
    pub sequence: i64,
    pub attendees: Vec<EventAttendee>,
    #[serde(default)]
    pub attachments: Vec<EventAttachment>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "attendeesOmitted"
    )]
    pub attendees_omitted: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<EventSource>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "conferenceData"
    )]
    pub conference_data: Option<ConferenceData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guests_can_modify: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reminders: Option<EventReminders>,
    #[serde(rename = "eventType")]
    pub event_type: GoogleCalendarEventType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transparency: Option<EventTransparency>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<EventVisibility>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_recurrence",
        serialize_with = "serialize_recurrence"
    )]
    pub recurrence: Option<RRuleSet>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "recurringEventId"
    )]
    pub recurring_event_id: Option<GoogleCalendarEventId>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "originalStartTime"
    )]
    pub original_start_time: Option<EventDateTime>,
}

pub type GoogleCalendarEventId = TypedId<String, GoogleCalendarEvent>;
pub type IcalUID = TypedId<String, GoogleCalendarEvent>;

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy)]
pub enum GoogleCalendarEventType {
    #[serde(rename = "birthday")]
    Birthday,
    #[serde(rename = "default")]
    Default,
    #[serde(rename = "outOfOffice")]
    OutOfOffice,
    #[serde(rename = "focusTime")]
    FocusTime,
    #[serde(rename = "fromGmail")]
    FromGmail,
    #[serde(rename = "workingLocation")]
    WorkingLocation,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy)]
pub enum GoogleCalendarEventStatus {
    #[serde(rename = "confirmed")]
    Confirmed,
    #[serde(rename = "tentative")]
    Tentative,
    #[serde(rename = "cancelled")]
    Cancelled,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy)]
pub enum EventTransparency {
    #[serde(rename = "opaque")]
    Opaque,
    #[serde(rename = "transparent")]
    Transparent,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy)]
pub enum EventVisibility {
    #[serde(rename = "default")]
    Default,
    #[serde(rename = "public")]
    Public,
    #[serde(rename = "private")]
    Private,
    #[serde(rename = "confidential")]
    Confidential,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct EventSource {
    pub url: Url,
    pub title: String,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct EventAttachment {
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "fileId")]
    pub file_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "fileUrl")]
    pub file_url: Option<Url>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "iconLink")]
    pub icon_link: Option<Url>,
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    pub title: String,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct EventCreator {
    #[serde(
        default,
        rename = "displayName",
        skip_serializing_if = "Option::is_none"
    )]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, rename = "self")]
    pub self_: Option<bool>,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct EventOrganizer {
    #[serde(
        default,
        rename = "displayName",
        skip_serializing_if = "Option::is_none"
    )]
    pub display_name: Option<String>,
    pub email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, rename = "self")]
    pub self_: Option<bool>,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct EventAttendee {
    #[serde(
        default,
        rename = "additionalGuests",
        skip_serializing_if = "Option::is_none"
    )]
    pub additional_guests: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(
        default,
        rename = "displayName",
        skip_serializing_if = "Option::is_none"
    )]
    pub display_name: Option<String>,
    pub email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optional: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub organizer: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<bool>,
    #[serde(rename = "responseStatus")]
    pub response_status: GoogleCalendarEventAttendeeResponseStatus,
    #[serde(default, rename = "self")]
    pub self_: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy)]
pub enum GoogleCalendarEventAttendeeResponseStatus {
    #[serde(rename = "accepted")]
    Accepted,
    #[serde(rename = "tentative")]
    Tentative,
    #[serde(rename = "declined")]
    Declined,
    #[serde(rename = "needsAction")]
    NeedsAction,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct EventDateTime {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<NaiveDate>,
    #[serde(default, rename = "dateTime", skip_serializing_if = "Option::is_none")]
    pub datetime: Option<DateTime<Utc>>,
    #[serde(default, rename = "timeZone", skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct ConferenceData {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "conferenceId"
    )]
    pub conference_id: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "conferenceSolution"
    )]
    pub conference_solution: Option<ConferenceSolution>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "createRequest"
    )]
    pub create_request: Option<CreateConferenceRequest>,
    #[serde(default, rename = "entryPoints")]
    pub entry_points: Vec<EntryPoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct ConferenceSolution {
    #[serde(rename = "iconUri")]
    pub icon_uri: Url,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<ConferenceSolutionKey>,
    pub name: String,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct ConferenceSolutionKey {
    #[serde(rename = "type")]
    pub type_: ConferenceSolutionKeyType,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy)]
pub enum ConferenceSolutionKeyType {
    #[serde(rename = "eventHangout")]
    EventHangout,
    #[serde(rename = "eventNamedHangout")]
    EventNamedHangout,
    #[serde(rename = "hangoutsMeet")]
    HangoutsMeet,
    #[serde(rename = "addOn")]
    AddOn,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct CreateConferenceRequest {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "conferenceSolutionKey"
    )]
    pub conference_solution_key: Option<ConferenceSolutionKey>,
    #[serde(default, rename = "requestId")]
    pub request_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<ConferenceRequestStatus>,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct ConferenceRequestStatus {
    #[serde(rename = "statusCode")]
    pub status_code: ConferenceRequestStatusCode,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy)]
pub enum ConferenceRequestStatusCode {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "success")]
    Success,
    #[serde(rename = "failure")]
    Failure,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct EntryPoint {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "accessCode"
    )]
    pub access_code: Option<String>,
    #[serde(rename = "entryPointType")]
    pub entry_point_type: EntryPointType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "meetingCode"
    )]
    pub meeting_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passcode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin: Option<String>,
    pub uri: Url,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy)]
pub enum EntryPointType {
    #[serde(rename = "video")]
    Video,
    #[serde(rename = "phone")]
    Phone,
    #[serde(rename = "sip")]
    Sip,
    #[serde(rename = "more")]
    More,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct EventReminders {
    #[serde(default)]
    pub overrides: Vec<EventReminder>,
    #[serde(default, rename = "useDefault")]
    pub use_default: bool,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct EventReminder {
    pub method: EventReminderMethod,
    pub minutes: i64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy)]
pub enum EventReminderMethod {
    #[serde(rename = "email")]
    Email,
    #[serde(rename = "popup")]
    Popup,
}

impl GoogleCalendarEvent {
    pub fn get_self_attendee(&self) -> Option<EventAttendee> {
        self.attendees
            .iter()
            .find(|attendee| attendee.self_ == Some(true))
            .cloned()
    }
}

impl HasHtmlUrl for GoogleCalendarEvent {
    fn get_html_url(&self) -> Url {
        self.html_link.clone()
    }
}

impl TryFrom<ThirdPartyItem> for GoogleCalendarEvent {
    type Error = anyhow::Error;

    fn try_from(item: ThirdPartyItem) -> Result<Self, Self::Error> {
        match item.data {
            ThirdPartyItemData::GoogleCalendarEvent(event) => Ok(*event),
            _ => Err(anyhow!(
                "Unable to convert ThirdPartyItem {} to GoogleCalendarEvent",
                item.id
            )),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy, Default)]
pub enum EventMethod {
    #[serde(rename = "PUBLISH")]
    Publish,
    #[default]
    #[serde(rename = "REQUEST")]
    Request,
    #[serde(rename = "REFRESH")]
    Refresh,
    #[serde(rename = "CANCEL")]
    Cancel,
    #[serde(rename = "ADD")]
    Add,
    #[serde(rename = "REPLY")]
    Reply,
    #[serde(rename = "COUNTER")]
    Counter,
    #[serde(rename = "DECLINECOUNTER")]
    DeclineCounter,
}

impl FromStr for EventMethod {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "PUBLISH" => Ok(EventMethod::Publish),
            "REQUEST" => Ok(EventMethod::Request),
            "REFRESH" => Ok(EventMethod::Refresh),
            "CANCEL" => Ok(EventMethod::Cancel),
            "ADD" => Ok(EventMethod::Add),
            "REPLY" => Ok(EventMethod::Reply),
            "COUNTER" => Ok(EventMethod::Counter),
            "DECLINECOUNTER" => Ok(EventMethod::DeclineCounter),
            _ => Err(anyhow!("Unknown iCal METHOD value: {}", s)),
        }
    }
}

fn parse_rrule_set_from_ical_properties(properties: &[String]) -> Result<RRuleSet, anyhow::Error> {
    // Check if DTSTART is already provided in the properties
    let has_dtstart = properties.iter().any(|prop| prop.starts_with("DTSTART:"));

    let mut ical_string = String::new();

    // Only add default DTSTART if not already present
    if !has_dtstart {
        let default_start = DateTime::from_timestamp(0, 0)
            .unwrap()
            .with_timezone(&Tz::UTC);
        ical_string = format!("DTSTART:{}\n", default_start.format("%Y%m%dT%H%M%SZ"));
    }

    ical_string.push_str(&properties.join("\n"));

    ical_string
        .parse()
        .map_err(|e| anyhow!("Failed to parse iCal recurrence properties: {}", e))
}

fn deserialize_recurrence<'de, D>(deserializer: D) -> Result<Option<RRuleSet>, D::Error>
where
    D: Deserializer<'de>,
{
    let recurrence_strings: Option<Vec<String>> = Option::deserialize(deserializer)?;

    match recurrence_strings {
        Some(strings) if !strings.is_empty() => parse_rrule_set_from_ical_properties(&strings)
            .map(Some)
            .map_err(serde::de::Error::custom),
        _ => Ok(None),
    }
}

fn serialize_recurrence<S>(rrule_set: &Option<RRuleSet>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match rrule_set {
        Some(rrule_set) => {
            // Convert RRuleSet back to the original Vec<String> format
            let mut recurrence_strings = Vec::new();

            // Add RRULEs
            for rrule in rrule_set.get_rrule() {
                recurrence_strings.push(format!("RRULE:{}", rrule));
            }

            // Add RDATEs
            for rdate in rrule_set.get_rdate() {
                recurrence_strings.push(format!("RDATE:{}", rdate.format("%Y%m%dT%H%M%SZ")));
            }

            // Add EXRULEs
            for exrule in rrule_set.get_exrule() {
                recurrence_strings.push(format!("EXRULE:{}", exrule));
            }

            // Add EXDATEs
            for exdate in rrule_set.get_exdate() {
                recurrence_strings.push(format!("EXDATE:{}", exdate.format("%Y%m%dT%H%M%SZ")));
            }

            Some(recurrence_strings).serialize(serializer)
        }
        None => None::<Vec<String>>.serialize(serializer),
    }
}

impl ThirdPartyItemFromSource for GoogleCalendarEvent {
    fn into_third_party_item(
        self,
        user_id: UserId,
        integration_connection_id: IntegrationConnectionId,
    ) -> ThirdPartyItem {
        ThirdPartyItem {
            id: Uuid::new_v4().into(),
            source_id: self.id.to_string(),
            data: ThirdPartyItemData::GoogleCalendarEvent(Box::new(self.clone())),
            created_at: Utc::now().with_nanosecond(0).unwrap(),
            updated_at: Utc::now().with_nanosecond(0).unwrap(),
            user_id,
            integration_connection_id,
            source_item: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod event_method {
        use super::*;

        #[test]
        fn test_event_method_from_str() {
            assert_eq!(
                "REQUEST".parse::<EventMethod>().unwrap(),
                EventMethod::Request
            );
            assert_eq!(
                "CANCEL".parse::<EventMethod>().unwrap(),
                EventMethod::Cancel
            );
            assert_eq!("REPLY".parse::<EventMethod>().unwrap(), EventMethod::Reply);
            assert_eq!(
                "PUBLISH".parse::<EventMethod>().unwrap(),
                EventMethod::Publish
            );
            assert_eq!(
                "REFRESH".parse::<EventMethod>().unwrap(),
                EventMethod::Refresh
            );
            assert_eq!("ADD".parse::<EventMethod>().unwrap(), EventMethod::Add);
            assert_eq!(
                "COUNTER".parse::<EventMethod>().unwrap(),
                EventMethod::Counter
            );
            assert_eq!(
                "DECLINECOUNTER".parse::<EventMethod>().unwrap(),
                EventMethod::DeclineCounter
            );
        }

        #[test]
        fn test_event_method_from_str_invalid() {
            assert!("INVALID".parse::<EventMethod>().is_err());
        }

        #[test]
        fn test_event_method_default() {
            assert_eq!(EventMethod::default(), EventMethod::Request);
        }
    }

    mod rrule_parsing {
        use super::*;
        use chrono::TimeZone;
        use rrule::Frequency;

        #[test]
        fn test_parse_simple_rrule() {
            let properties = vec!["RRULE:FREQ=WEEKLY;BYDAY=MO,WE,FR".to_string()];

            let rrule_set = parse_rrule_set_from_ical_properties(&properties).unwrap();

            assert!(!rrule_set.get_rrule().is_empty());

            let rrule = &rrule_set.get_rrule()[0];
            assert_eq!(rrule.get_freq(), Frequency::Weekly);

            // Verify the DTSTART is set to the default (epoch) by checking the timestamp
            let dtstart = rrule_set.get_dt_start();
            assert_eq!(dtstart.timestamp(), 0);
        }

        #[test]
        fn test_parse_complex_recurrence() {
            let properties = vec![
                "RRULE:FREQ=WEEKLY;INTERVAL=2;COUNT=10;BYDAY=MO,WE,FR".to_string(),
                "RDATE:20240201T100000Z,20240203T100000Z".to_string(),
                "EXDATE:20240215T100000Z".to_string(),
            ];

            let rrule_set = parse_rrule_set_from_ical_properties(&properties).unwrap();

            assert!(!rrule_set.get_rrule().is_empty());
            assert_eq!(rrule_set.get_rdate().len(), 2);
            assert_eq!(rrule_set.get_exdate().len(), 1);

            // Verify the DTSTART is set to the default (epoch) by checking the timestamp
            let dtstart = rrule_set.get_dt_start();
            assert_eq!(dtstart.timestamp(), 0);

            // Verify RDATE values by checking timestamps
            let rdates = rrule_set.get_rdate();
            let expected_rdate1_ts = Utc
                .with_ymd_and_hms(2024, 2, 1, 10, 0, 0)
                .unwrap()
                .timestamp();
            let expected_rdate2_ts = Utc
                .with_ymd_and_hms(2024, 2, 3, 10, 0, 0)
                .unwrap()
                .timestamp();
            assert!(rdates.iter().any(|d| d.timestamp() == expected_rdate1_ts));
            assert!(rdates.iter().any(|d| d.timestamp() == expected_rdate2_ts));

            // Verify EXDATE values by checking timestamps
            let exdates = rrule_set.get_exdate();
            let expected_exdate_ts = Utc
                .with_ymd_and_hms(2024, 2, 15, 10, 0, 0)
                .unwrap()
                .timestamp();
            assert!(exdates.iter().any(|d| d.timestamp() == expected_exdate_ts));
        }

        #[test]
        fn test_parse_with_custom_dtstart() {
            // Test that when Google Calendar provides recurrence properties,
            // they work with our default DTSTART
            let properties = vec!["RRULE:FREQ=DAILY;COUNT=3".to_string()];

            let rrule_set = parse_rrule_set_from_ical_properties(&properties).unwrap();

            // Verify DTSTART is our default epoch time by checking the timestamp
            let dtstart = rrule_set.get_dt_start();
            assert_eq!(dtstart.timestamp(), 0);

            // Verify the rule was parsed correctly
            let rrule = &rrule_set.get_rrule()[0];
            assert_eq!(rrule.get_freq(), Frequency::Daily);
            assert_eq!(rrule.get_count(), Some(3));
        }

        #[test]
        fn test_parse_with_explicit_dtstart_in_properties() {
            // Test with a DTSTART provided in the properties (Google Calendar might include this)
            let properties = vec![
                "DTSTART:20240301T090000Z".to_string(),
                "RRULE:FREQ=WEEKLY;BYDAY=TU,TH".to_string(),
                "RDATE:20240315T090000Z".to_string(),
            ];

            let rrule_set = parse_rrule_set_from_ical_properties(&properties).unwrap();

            assert!(!rrule_set.get_rrule().is_empty());
            assert_eq!(rrule_set.get_rdate().len(), 1);

            // Verify the DTSTART from the properties is used (not our default)
            let dtstart = rrule_set.get_dt_start();
            let expected_dtstart_ts = Utc
                .with_ymd_and_hms(2024, 3, 1, 9, 0, 0)
                .unwrap()
                .timestamp();
            assert_eq!(dtstart.timestamp(), expected_dtstart_ts);

            // Verify the rule was parsed correctly
            let rrule = &rrule_set.get_rrule()[0];
            assert_eq!(rrule.get_freq(), Frequency::Weekly);

            // Verify RDATE value
            let rdates = rrule_set.get_rdate();
            let expected_rdate_ts = Utc
                .with_ymd_and_hms(2024, 3, 15, 9, 0, 0)
                .unwrap()
                .timestamp();
            assert!(rdates.iter().any(|d| d.timestamp() == expected_rdate_ts));
        }

        #[test]
        fn test_parse_google_calendar_style_recurrence_with_dtstart() {
            // Test realistic Google Calendar API recurrence response that includes DTSTART
            // Use UTC format to avoid timezone complexity in this test
            let properties = vec![
                "DTSTART:20240401T140000Z".to_string(),
                "RRULE:FREQ=WEEKLY;INTERVAL=2;BYDAY=MO;COUNT=5".to_string(),
                "EXDATE:20240415T140000Z".to_string(),
            ];

            let rrule_set = parse_rrule_set_from_ical_properties(&properties).unwrap();

            assert!(!rrule_set.get_rrule().is_empty());
            assert_eq!(rrule_set.get_exdate().len(), 1);

            // Verify the DTSTART from properties is parsed correctly using timestamp comparison
            let dtstart = rrule_set.get_dt_start();
            let expected_dtstart_ts = Utc
                .with_ymd_and_hms(2024, 4, 1, 14, 0, 0)
                .unwrap()
                .timestamp();
            assert_eq!(dtstart.timestamp(), expected_dtstart_ts);

            // Verify the rule was parsed correctly
            let rrule = &rrule_set.get_rrule()[0];
            assert_eq!(rrule.get_freq(), Frequency::Weekly);
            assert_eq!(rrule.get_interval(), 2);
            assert_eq!(rrule.get_count(), Some(5));

            // Verify EXDATE value using timestamp comparison
            let exdates = rrule_set.get_exdate();
            let expected_exdate_ts = Utc
                .with_ymd_and_hms(2024, 4, 15, 14, 0, 0)
                .unwrap()
                .timestamp();
            assert!(exdates.iter().any(|d| d.timestamp() == expected_exdate_ts));
        }

        #[test]
        fn test_parse_invalid_rrule() {
            let properties = vec!["RRULE:INVALID_RULE".to_string()];

            let result = parse_rrule_set_from_ical_properties(&properties);

            assert!(result.is_err());
        }
    }

    mod google_calendar_event_deserialization {
        use super::*;
        use serde_json;

        #[test]
        fn test_deserialize_event_with_recurrence() {
            let json = r#"{
                "method": "REQUEST",
                "kind": "calendar#event",
                "etag": "test-etag",
                "id": "test-id",
                "status": "confirmed",
                "htmlLink": "https://calendar.google.com/event?eid=test",
                "created": "2024-01-01T00:00:00Z",
                "updated": "2024-01-01T00:00:00Z",
                "summary": "Test Recurring Event",
                "creator": {
                    "email": "test@example.com"
                },
                "organizer": {
                    "email": "test@example.com"
                },
                "start": {
                    "date": "2024-01-01"
                },
                "end": {
                    "date": "2024-01-01"
                },
                "iCalUID": "test-uid",
                "sequence": 0,
                "attendees": [],
                "eventType": "default",
                "recurrence": [
                    "RRULE:FREQ=WEEKLY;BYDAY=MO,WE,FR",
                    "RDATE:20240201T100000Z,20240203T100000Z",
                    "EXDATE:20240215T100000Z"
                ]
            }"#;

            let event: GoogleCalendarEvent = serde_json::from_str(json).unwrap();

            assert!(event.recurrence.is_some());
            let rrule_set = event.recurrence.unwrap();
            assert!(!rrule_set.get_rrule().is_empty());
            assert_eq!(rrule_set.get_rdate().len(), 2);
            assert_eq!(rrule_set.get_exdate().len(), 1);
        }

        #[test]
        fn test_deserialize_event_without_recurrence() {
            let json = r#"{
                "method": "REQUEST",
                "kind": "calendar#event",
                "etag": "test-etag",
                "id": "test-id",
                "status": "confirmed",
                "htmlLink": "https://calendar.google.com/event?eid=test",
                "created": "2024-01-01T00:00:00Z",
                "updated": "2024-01-01T00:00:00Z",
                "summary": "Test Single Event",
                "creator": {
                    "email": "test@example.com"
                },
                "organizer": {
                    "email": "test@example.com"
                },
                "start": {
                    "date": "2024-01-01"
                },
                "end": {
                    "date": "2024-01-01"
                },
                "iCalUID": "test-uid",
                "sequence": 0,
                "attendees": [],
                "eventType": "default"
            }"#;

            let event: GoogleCalendarEvent = serde_json::from_str(json).unwrap();

            assert!(event.recurrence.is_none());
        }

        #[test]
        fn test_deserialize_event_with_empty_recurrence() {
            let json = r#"{
                "method": "REQUEST",
                "kind": "calendar#event",
                "etag": "test-etag",
                "id": "test-id",
                "status": "confirmed",
                "htmlLink": "https://calendar.google.com/event?eid=test",
                "created": "2024-01-01T00:00:00Z",
                "updated": "2024-01-01T00:00:00Z",
                "summary": "Test Event",
                "creator": {
                    "email": "test@example.com"
                },
                "organizer": {
                    "email": "test@example.com"
                },
                "start": {
                    "date": "2024-01-01"
                },
                "end": {
                    "date": "2024-01-01"
                },
                "iCalUID": "test-uid",
                "sequence": 0,
                "attendees": [],
                "eventType": "default",
                "recurrence": []
            }"#;

            let event: GoogleCalendarEvent = serde_json::from_str(json).unwrap();

            assert!(event.recurrence.is_none());
        }

        #[test]
        fn test_deserialize_real_google_calendar_api_response() {
            // Test with a more realistic Google Calendar API response structure
            let json = r#"{
                "kind": "calendar#event",
                "etag": "\"3471714048456000\"",
                "id": "eventid1",
                "status": "confirmed",
                "htmlLink": "https://www.google.com/calendar/event?eid=test",
                "created": "2024-12-30T22:32:57.000Z",
                "updated": "2025-01-02T22:30:24.198Z",
                "summary": "Weekly team meeting",
                "creator": {
                    "email": "david@universal-inbox.com"
                },
                "organizer": {
                    "email": "david@universal-inbox.com"
                },
                "start": {
                    "dateTime": "2025-01-03T15:00:00+01:00",
                    "timeZone": "Europe/Paris"
                },
                "end": {
                    "dateTime": "2025-01-03T15:30:00+01:00",
                    "timeZone": "Europe/Paris"
                },
                "iCalUID": "event_icaluid1",
                "sequence": 1,
                "attendees": [
                    {
                        "email": "user2@example.com",
                        "self": true,
                        "responseStatus": "needsAction"
                    }
                ],
                "recurrence": [
                    "RRULE:FREQ=WEEKLY;BYDAY=MO"
                ],
                "eventType": "default"
            }"#;

            let event: GoogleCalendarEvent = serde_json::from_str(json).unwrap();

            assert_eq!(event.summary, "Weekly team meeting");
            assert!(event.recurrence.is_some());

            let rrule_set = event.recurrence.unwrap();
            assert!(!rrule_set.get_rrule().is_empty());

            // Check that the RRULE was parsed correctly
            let rrule = &rrule_set.get_rrule()[0];
            assert_eq!(rrule.get_freq(), rrule::Frequency::Weekly);
        }

        #[test]
        fn test_simplified_api_usage() {
            // Demonstrate the simplified API usage
            let json = r#"{
                "kind": "calendar#event",
                "etag": "test-etag",
                "id": "test-id",
                "status": "confirmed",
                "htmlLink": "https://calendar.google.com/event?eid=test",
                "created": "2024-01-01T00:00:00Z",
                "updated": "2024-01-01T00:00:00Z",
                "summary": "Daily standup",
                "creator": {
                    "email": "test@example.com"
                },
                "organizer": {
                    "email": "test@example.com"
                },
                "start": {
                    "dateTime": "2024-01-01T09:00:00Z"
                },
                "end": {
                    "dateTime": "2024-01-01T09:30:00Z"
                },
                "iCalUID": "test-uid",
                "sequence": 0,
                "attendees": [],
                "eventType": "default",
                "recurrence": [
                    "RRULE:FREQ=DAILY;BYDAY=MO,TU,WE,TH,FR"
                ]
            }"#;

            let event: GoogleCalendarEvent = serde_json::from_str(json).unwrap();

            // Simple, direct access to RRuleSet
            if let Some(rrule_set) = &event.recurrence {
                // Get recurrence rules
                let rules = rrule_set.get_rrule();
                assert!(!rules.is_empty());
                assert_eq!(rules[0].get_freq(), rrule::Frequency::Daily);

                // Get additional recurrence dates
                let rdates = rrule_set.get_rdate();
                assert!(rdates.is_empty()); // No RDATE in this example

                // Get exception dates
                let exdates = rrule_set.get_exdate();
                assert!(exdates.is_empty()); // No EXDATE in this example

                // Could generate occurrences if needed:
                // let occurrences = rrule_set.all(100); // Get first 100 occurrences
            }
        }
    }
}
