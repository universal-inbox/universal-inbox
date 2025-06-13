use anyhow::anyhow;
use chrono::{DateTime, NaiveDate, Timelike, Utc};
use serde::{Deserialize, Serialize};
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
}
