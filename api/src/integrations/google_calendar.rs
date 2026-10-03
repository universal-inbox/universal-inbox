use std::{sync::Weak, time::Duration};

use anyhow::{Context, anyhow};
use async_trait::async_trait;
use chrono::{DateTime, Timelike, Utc};
use http::{HeaderMap, HeaderValue};

use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;
use universal_inbox::{
    integration_connection::provider::{IntegrationProviderKind, IntegrationProviderSource},
    notification::{Notification, NotificationSource, NotificationSourceKind, NotificationStatus},
    third_party::{
        integrations::google_calendar::{
            EventAttendee, EventDateTime, EventMethod, EventReminder, GoogleCalendarEvent,
            GoogleCalendarEventAttendeeResponseStatus, RecurrenceId,
        },
        item::{ThirdPartyItem, ThirdPartyItemData},
    },
    user::UserId,
};
use url::Url;
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

use crate::{
    integrations::oauth2::AccessToken,
    universal_inbox::{
        UniversalInboxError, integration_connection::service::IntegrationConnectionService,
    },
    utils::api::ApiClient,
};

use super::notification::ThirdPartyNotificationSourceService;

const GOOGLE_CALENDAR_BASE_URL: &str = "https://www.googleapis.com/calendar/v3";
const MAX_EVENT_LOOKUP_PAGES: usize = 5;

#[derive(Clone)]
pub struct GoogleCalendarService {
    google_calendar_base_url: String,
    google_calendar_base_path: String,
    integration_connection_service: Weak<RwLock<IntegrationConnectionService>>,
    max_retry_duration: Duration,
}

impl GoogleCalendarService {
    pub fn new(
        google_calendar_base_url: Option<String>,
        integration_connection_service: Weak<RwLock<IntegrationConnectionService>>,
        max_retry_duration: Duration,
    ) -> Result<GoogleCalendarService, UniversalInboxError> {
        let google_calendar_base_url =
            google_calendar_base_url.unwrap_or_else(|| GOOGLE_CALENDAR_BASE_URL.to_string());
        let google_calendar_base_path = Url::parse(&google_calendar_base_url)
            .context("Failed to parse Google Calendar base URL")?
            .path()
            .to_string();
        Ok(GoogleCalendarService {
            google_calendar_base_url,
            google_calendar_base_path: if &google_calendar_base_path == "/" {
                "".to_string()
            } else {
                google_calendar_base_path
            },
            integration_connection_service,
            max_retry_duration,
        })
    }

    pub async fn mock_all(mock_server: &MockServer) {
        Mock::given(method("GET"))
            .and(path("/calendars/[^/]*/events[^/]*"))
            .respond_with(ResponseTemplate::new(404))
            .mount(mock_server)
            .await;
    }

    fn build_google_calendar_client(
        &self,
        access_token: &AccessToken,
    ) -> Result<ApiClient, UniversalInboxError> {
        let mut headers = HeaderMap::new();

        let mut auth_header_value: HeaderValue =
            format!("Bearer {}", access_token.as_str()).parse().unwrap();
        auth_header_value.set_sensitive(true);
        headers.insert("Authorization", auth_header_value);

        ApiClient::build(
            headers,
            otel_known_paths(&self.google_calendar_base_path),
            self.max_retry_duration,
        )
    }

    /// Find the Google Calendar event an emailed invitation is about.
    ///
    /// A recurring event's iCalUID matches its master event and all its modified or cancelled
    /// occurrences, and a page may come back empty with a `nextPageToken`: all pages are
    /// fetched (deleted events included, to see cancellations) before picking the right one.
    pub async fn get_event(
        &self,
        calendar_id: &str,
        ical_uid: &str,
        recurrence_id: Option<&RecurrenceId>,
        method: EventMethod,
        access_token: &AccessToken,
    ) -> Result<GoogleCalendarEvent, UniversalInboxError> {
        let client = self.build_google_calendar_client(access_token)?;
        let mut items = vec![];
        let mut page_token: Option<String> = None;
        for _ in 0..MAX_EVENT_LOOKUP_PAGES {
            // `ical_uid` comes from an emailed .ics attachment: encode it so `&`
            // or `#` cannot add parameters or drop the filters.
            let mut query = vec![("iCalUID", ical_uid), ("showDeleted", "true")];
            if let Some(page_token) = page_token.as_deref() {
                query.push(("pageToken", page_token));
            }
            let url = build_endpoint(
                &self.google_calendar_base_url,
                &["calendars", calendar_id, "events"],
                &query,
            )?;

            let events_list: RawGoogleCalendarEventsList = client.get(&url).await.context(
                format!(
                    "Cannot fetch Google Calendar event ical_uid={ical_uid} in calendar {calendar_id}"
                ),
            )?;
            items.extend(events_list.items);
            page_token = events_list.next_page_token;
            if page_token.is_none() {
                break;
            }
        }

        let mut event = select_invitation_event(items, recurrence_id, method).with_context(|| {
            format!(
                "Cannot find Google Calendar event ical_uid={ical_uid} in calendar {calendar_id}"
            )
        })?;
        event.method = method;
        Ok(event)
    }

    async fn delete_event(
        &self,
        calendar_id: &str,
        event_id: &str,
        access_token: &AccessToken,
    ) -> Result<(), UniversalInboxError> {
        let url = build_endpoint(
            &self.google_calendar_base_url,
            &["calendars", calendar_id, "events", event_id],
            &[],
        )?;
        self.build_google_calendar_client(access_token)?
            .delete_no_response(&url)
            .await
            .context(format!(
                "Cannot delete Google Calendar event {event_id} in calendar {calendar_id}"
            ))?;

        Ok(())
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            third_party_item_id = source_item.id.to_string(),
            response_status = serde_json::to_string(&response_status).unwrap(),
            user.id = user_id.to_string(),
        ),
        err
    )]
    pub async fn answer_invitation(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        source_item: &ThirdPartyItem,
        response_status: GoogleCalendarEventAttendeeResponseStatus,
        user_id: UserId,
    ) -> Result<GoogleCalendarEvent, UniversalInboxError> {
        let (access_token, _) = self
            .integration_connection_service
            .upgrade()
            .context(
                "Unable to access integration_connection_service from google_calendar_service",
            )?
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::GoogleCalendar, user_id)
            .await?
            .ok_or_else(|| {
                anyhow!("Cannot answer Google Calendar invitation without an access token")
            })?;

        let event = match &source_item.data {
            ThirdPartyItemData::GoogleCalendarEvent(event) => event,
            _ => {
                return Err(UniversalInboxError::Unexpected(anyhow!(
                    "Cannot answer invitation for non-Google Calendar event: {:?}",
                    source_item.data
                )));
            }
        };

        let url = build_endpoint(
            &self.google_calendar_base_url,
            &["calendars", "primary", "events", &event.id.to_string()],
            &[],
        )?;

        // Find the self attendee to update
        let self_attendee = event.get_self_attendee().ok_or_else(|| {
            UniversalInboxError::Unexpected(anyhow!("Cannot find self attendee in event"))
        })?;

        // Build updated attendee with new response status
        let updated_attendee = EventAttendee {
            response_status,
            ..self_attendee
        };

        // Update attendees list with new response status
        let mut attendees = event.attendees.clone();
        if let Some(idx) = attendees.iter().position(|a| a.self_ == Some(true)) {
            attendees[idx] = updated_attendee;
        }

        // Build patch payload
        let patch_body = serde_json::json!({
            "attendees": attendees
        });

        let updated_event: GoogleCalendarEvent = self
            .build_google_calendar_client(&access_token)?
            .patch(&url, Some(&patch_body))
            .await
            .context(format!(
                "Cannot answer Google Calendar event {} invitation",
                event.id
            ))?;

        Ok(updated_event)
    }
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct GoogleCalendarEventsList {
    pub kind: String,
    pub etag: String,
    pub summary: String,
    pub description: String,
    pub updated: DateTime<Utc>,
    #[serde(rename = "timeZone")]
    pub timezone: String,
    #[serde(rename = "accessRole")]
    pub access_role: GoogleCalendarAccessRole,
    #[serde(default, rename = "defaultReminders")]
    pub default_reminders: Vec<EventReminder>,
    #[serde(default, rename = "nextSyncToken")]
    pub next_sync_token: Option<String>,
    #[serde(default, rename = "nextPageToken")]
    pub next_page_token: Option<String>,
    pub items: Vec<GoogleCalendarEvent>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub enum GoogleCalendarAccessRole {
    #[serde(rename = "none")]
    None,
    #[serde(rename = "freeBusyReader")]
    FreeBusyReader,
    #[serde(rename = "reader")]
    Reader,
    #[serde(rename = "writer")]
    Writer,
    #[serde(rename = "owner")]
    Owner,
}

#[async_trait]
impl ThirdPartyNotificationSourceService<GoogleCalendarEvent> for GoogleCalendarService {
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            source_id = source_third_party_item.source_id,
            third_party_item_id = source_third_party_item.id.to_string(),
            user.id = user_id.to_string(),
        ),
        err
    )]
    async fn third_party_item_into_notification(
        &self,
        source: &GoogleCalendarEvent,
        source_third_party_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<Box<Notification>, UniversalInboxError> {
        let user_response_status = source.attendees.iter().find_map(|attendee| {
            attendee
                .self_
                .unwrap_or_default()
                .then_some(attendee.response_status)
        });
        let status = match user_response_status.as_ref() {
            // A cancellation is news even for an event you answered. Whether it should rather
            // replace a pending invitation is decided by the notification service.
            _ if source.is_cancelled() => NotificationStatus::Unread,
            Some(GoogleCalendarEventAttendeeResponseStatus::Accepted) => NotificationStatus::Read,
            Some(GoogleCalendarEventAttendeeResponseStatus::Declined) => NotificationStatus::Read,
            Some(GoogleCalendarEventAttendeeResponseStatus::Tentative) => {
                NotificationStatus::Unread
            }
            Some(GoogleCalendarEventAttendeeResponseStatus::NeedsAction) => {
                NotificationStatus::Unread
            }
            _ => NotificationStatus::Unread,
        };

        Ok(Box::new(Notification {
            id: Uuid::new_v4().into(),
            title: source.summary.clone(),
            status,
            created_at: Utc::now().with_nanosecond(0).unwrap(),
            updated_at: Utc::now().with_nanosecond(0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
            user_id,
            kind: NotificationSourceKind::GoogleCalendar,
            source_item: source_third_party_item.clone(),
            task_id: None,
        }))
    }

    /// Nothing is done when deleting a Google Calendar event notification
    async fn delete_notification_from_source(
        &self,
        _executor: &mut Transaction<'_, Postgres>,
        _source_item: &ThirdPartyItem,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        Ok(())
    }

    /// Deleting the Google Calendar event when unsubscribing from the notification
    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(third_party_item_id = source_item.id.to_string(), user.id = user_id.to_string()),
        err
    )]
    async fn unsubscribe_notification_from_source(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        source_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let (access_token, _) = self
            .integration_connection_service
            .upgrade()
            .context(
                "Unable to access integration_connection_service from google_calendar_service",
            )?
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::GoogleCalendar, user_id)
            .await?
            .ok_or_else(|| {
                anyhow!(
                    "Cannot unsubscribe from GoogleCalendar notifications without an access token"
                )
            })?;

        self.delete_event("primary", &source_item.source_id, &access_token)
            .await
    }

    async fn snooze_notification_from_source(
        &self,
        _executor: &mut Transaction<'_, Postgres>,
        _source_item: &ThirdPartyItem,
        _snoozed_until_at: DateTime<Utc>,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        // Google Calendar events cannot be snoozed from the API => no-op
        Ok(())
    }
}

impl IntegrationProviderSource for GoogleCalendarService {
    fn get_integration_provider_kind(&self) -> IntegrationProviderKind {
        IntegrationProviderKind::GoogleCalendar
    }
}

impl NotificationSource for GoogleCalendarService {
    fn get_notification_source_kind(&self) -> NotificationSourceKind {
        NotificationSourceKind::GoogleCalendar
    }

    fn is_supporting_snoozed_notifications(&self) -> bool {
        false
    }
}

/// Route templates used to name outbound request spans. Every path the client
/// calls must match one of them, otherwise the span is named `<METHOD> UNKNOWN`.
fn otel_known_paths(base_path: &str) -> Vec<String> {
    [
        "/calendars/{calendar_id}/events",
        "/calendars/{calendar_id}/events/{event_id}",
    ]
    .iter()
    .map(|path| format!("{base_path}{path}"))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest_tracing::OtelPathNames;
    use rstest::*;

    #[rstest]
    #[case(
        "/calendar/v3/calendars/primary/events",
        "/calendar/v3/calendars/{calendar_id}/events"
    )]
    #[case(
        "/calendar/v3/calendars/primary/events/e1",
        "/calendar/v3/calendars/{calendar_id}/events/{event_id}"
    )]
    fn test_otel_known_paths_match_called_endpoints(
        #[case] path: &str,
        #[case] expected_template: &str,
    ) {
        let path_names = OtelPathNames::known_paths(otel_known_paths("/calendar/v3")).unwrap();
        assert_eq!(path_names.find(path), Some(expected_template));
    }

    mod notification_conversion {
        use std::{env, fs};

        use super::*;
        use pretty_assertions::assert_eq;

        use universal_inbox::{HasHtmlUrl, third_party::item::ThirdPartyItemFromSource};

        #[fixture]
        fn google_calendar_service() -> GoogleCalendarService {
            GoogleCalendarService::new(
                Some("https://calendar.googleapis.com/calendar/v3".to_string()),
                Weak::new(),
                Duration::from_secs(5),
            )
            .unwrap()
        }

        fn fixture_path(fixture_file_name: &str) -> String {
            format!(
                "{}/tests/api/fixtures/{fixture_file_name}",
                env::var("CARGO_MANIFEST_DIR").unwrap()
            )
        }

        #[fixture]
        fn google_calendar_event() -> GoogleCalendarEvent {
            let input_str = fs::read_to_string(fixture_path("google_calendar_event.json")).unwrap();
            serde_json::from_str(&input_str).unwrap()
        }

        #[rstest]
        #[tokio::test]
        async fn test_google_calendar_event_into_notification(
            google_calendar_service: GoogleCalendarService,
            google_calendar_event: GoogleCalendarEvent,
        ) {
            let user_id = Uuid::new_v4().into();
            let google_calendar_event_tpi = google_calendar_event
                .clone()
                .into_third_party_item(user_id, Uuid::new_v4().into());

            let google_calendar_notification = google_calendar_service
                .third_party_item_into_notification(
                    &google_calendar_event,
                    &google_calendar_event_tpi,
                    user_id,
                )
                .await
                .unwrap();

            assert_eq!(
                google_calendar_notification.title,
                "Weekly meeting".to_string()
            );
            assert_eq!(
                google_calendar_notification.source_item.source_id,
                "eventid1".to_string()
            );
            assert_eq!(
                google_calendar_notification.get_html_url(),
                "https://www.google.com/calendar/event?eid=test"
                    .parse::<Url>()
                    .unwrap()
            );
            // Self response status is "needsAction"
            assert_eq!(
                google_calendar_notification.status,
                NotificationStatus::Unread
            );
        }
    }
}

/// URL of a Google Calendar API endpoint below `base_url`, with every path
/// segment percent-encoded and the query built by the `url` crate, so ids
/// taken from third-party content cannot retarget the request.
/// Events list page whose items are kept raw: a cancelled occurrence only carries its `id`,
/// `status`, `recurringEventId` and `originalStartTime` and cannot be parsed on its own
#[derive(Deserialize, Debug)]
struct RawGoogleCalendarEventsList {
    #[serde(default, rename = "nextPageToken")]
    next_page_token: Option<String>,
    #[serde(default)]
    items: Vec<serde_json::Value>,
}

/// Pick the event an invitation is about among the events sharing its iCalUID:
/// - with a `RECURRENCE-ID`, the occurrence starting at that time (completed with its master
///   event's fields when Google only returns a partial cancelled occurrence), or the master
///   event for a `METHOD:REPLY` about an unmodified occurrence,
/// - otherwise a standalone or master event, preferring a cancelled one for a `METHOD:CANCEL`
///   invitation and an active one otherwise (an iCalUID can be reused after a deletion), then
///   the most recently updated one.
fn select_invitation_event(
    items: Vec<serde_json::Value>,
    recurrence_id: Option<&RecurrenceId>,
    method: EventMethod,
) -> Result<GoogleCalendarEvent, UniversalInboxError> {
    let field = |item: &serde_json::Value, name: &str| item.get(name).cloned();
    let is_occurrence = |item: &serde_json::Value| field(item, "recurringEventId").is_some();

    let occurrence = recurrence_id.and_then(|recurrence_id| {
        items.iter().find(|item| {
            is_occurrence(item)
                && field(item, "originalStartTime")
                    .and_then(|value| serde_json::from_value::<EventDateTime>(value).ok())
                    .is_some_and(|original_start_time| recurrence_id.matches(&original_start_time))
        })
    });
    // An attendee can answer a single occurrence that was never modified: its reply
    // (carrying the occurrence start) is attached to the master event
    let can_use_master_event = recurrence_id.is_none() || method == EventMethod::Reply;
    let prefer_cancelled = method == EventMethod::Cancel;
    let selected = occurrence
        .or_else(|| {
            items
                .iter()
                .filter(|item| can_use_master_event && !is_occurrence(item))
                .max_by_key(|item| {
                    let is_cancelled = field(item, "status")
                        .is_some_and(|status| status == serde_json::json!("cancelled"));
                    let updated = field(item, "updated")
                        .and_then(|value| serde_json::from_value::<DateTime<Utc>>(value).ok());
                    (is_cancelled == prefer_cancelled, updated)
                })
        })
        .ok_or_else(|| anyhow!("No matching event among {} events", items.len()))?;

    let master = field(selected, "recurringEventId").and_then(|master_id| {
        items
            .iter()
            .find(|item| field(item, "id") == Some(master_id.clone()))
    });
    let event_value = match (master, selected) {
        (Some(serde_json::Value::Object(master)), serde_json::Value::Object(occurrence)) => {
            let mut merged = master.clone();
            merged.remove("recurrence");
            if !occurrence.contains_key("start") {
                // A partial cancelled occurrence: it starts at its original start time and
                // lasts as long as its master event
                shift_master_times_to_occurrence(&mut merged, occurrence.get("originalStartTime"));
            }
            merged.extend(occurrence.clone());
            serde_json::Value::Object(merged)
        }
        _ => selected.clone(),
    };

    Ok(serde_json::from_value(event_value).context("Failed to parse Google Calendar event")?)
}

fn shift_master_times_to_occurrence(
    master: &mut serde_json::Map<String, serde_json::Value>,
    original_start_time: Option<&serde_json::Value>,
) {
    let parse = |value: Option<&serde_json::Value>| {
        value.and_then(|value| serde_json::from_value::<EventDateTime>(value.clone()).ok())
    };
    let (Some(start), Some(end), Some(original_start_time)) = (
        parse(master.get("start")),
        parse(master.get("end")),
        parse(original_start_time),
    ) else {
        return;
    };

    let occurrence_end = match (start.datetime, end.datetime, original_start_time.datetime) {
        (Some(start_datetime), Some(end_datetime), Some(occurrence_start)) => EventDateTime {
            datetime: Some(occurrence_start + (end_datetime - start_datetime)),
            ..end
        },
        _ => match (start.date, end.date, original_start_time.date) {
            (Some(start_date), Some(end_date), Some(occurrence_start)) => EventDateTime {
                date: Some(occurrence_start + (end_date - start_date)),
                ..end
            },
            _ => return,
        },
    };
    if let (Ok(start), Ok(end)) = (
        serde_json::to_value(original_start_time),
        serde_json::to_value(occurrence_end),
    ) {
        master.insert("start".to_string(), start);
        master.insert("end".to_string(), end);
    }
}

fn build_endpoint(
    base_url: &str,
    segments: &[&str],
    query: &[(&str, &str)],
) -> Result<String, UniversalInboxError> {
    let mut url = Url::parse(base_url).context("Cannot parse Google Calendar base URL")?;
    url.path_segments_mut()
        .map_err(|_| anyhow!("Google Calendar base URL cannot be a base"))?
        .pop_if_empty()
        .extend(segments);
    if !query.is_empty() {
        url.query_pairs_mut().extend_pairs(query);
    }
    Ok(url.to_string())
}

#[cfg(test)]
mod endpoint_tests {
    use super::*;

    #[test]
    fn test_build_endpoint_encodes_ical_uid() {
        assert_eq!(
            build_endpoint(
                "https://www.googleapis.com/calendar/v3",
                &["calendars", "primary", "events"],
                &[("iCalUID", "uid-1@example.com"), ("maxResults", "1")],
            )
            .unwrap(),
            "https://www.googleapis.com/calendar/v3/calendars/primary/events?iCalUID=uid-1%40example.com&maxResults=1"
        );
        assert_eq!(
            build_endpoint(
                "https://www.googleapis.com/calendar/v3",
                &["calendars", "primary", "events"],
                &[("iCalUID", "x&showDeleted=true#frag"), ("maxResults", "1")],
            )
            .unwrap(),
            "https://www.googleapis.com/calendar/v3/calendars/primary/events?iCalUID=x%26showDeleted%3Dtrue%23frag&maxResults=1"
        );
    }
}

#[cfg(test)]
mod select_invitation_event_tests {
    use chrono::TimeZone;
    use pretty_assertions::assert_eq;
    use serde_json::json;
    use universal_inbox::third_party::integrations::google_calendar::GoogleCalendarEventStatus;

    use super::*;

    fn master_event() -> serde_json::Value {
        // A weekly recurring event
        serde_json::from_str(include_str!(
            "../../tests/api/fixtures/google_calendar_event.json"
        ))
        .unwrap()
    }

    fn modified_occurrence() -> serde_json::Value {
        let mut occurrence = master_event();
        occurrence["id"] = json!("eventid1_20240510T131500Z");
        occurrence["summary"] = json!("Weekly meeting (moved)");
        occurrence["recurringEventId"] = json!("eventid1");
        occurrence["originalStartTime"] = json!({ "dateTime": "2024-05-10T13:15:00Z" });
        occurrence["start"] = json!({ "dateTime": "2024-05-10T14:00:00Z" });
        occurrence["end"] = json!({ "dateTime": "2024-05-10T14:15:00Z" });
        occurrence.as_object_mut().unwrap().remove("recurrence");
        occurrence
    }

    // Google only returns these fields for a cancelled occurrence of a recurring event
    fn partial_cancelled_occurrence() -> serde_json::Value {
        json!({
            "kind": "calendar#event",
            "etag": "\"3\"",
            "id": "eventid1_20240517T131500Z",
            "status": "cancelled",
            "recurringEventId": "eventid1",
            "originalStartTime": { "dateTime": "2024-05-17T13:15:00Z" }
        })
    }

    fn recurrence_id(value: &str) -> RecurrenceId {
        RecurrenceId::DateTime(
            chrono::NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%SZ")
                .unwrap()
                .and_utc(),
        )
    }

    #[test]
    fn test_select_the_master_event_among_its_occurrences() {
        let event = select_invitation_event(
            vec![
                partial_cancelled_occurrence(),
                modified_occurrence(),
                master_event(),
            ],
            None,
            EventMethod::Request,
        )
        .unwrap();

        assert_eq!(event.id.to_string(), "eventid1");
    }

    #[test]
    fn test_select_the_active_event_when_its_ical_uid_was_reused_after_a_cancellation() {
        let mut cancelled_event = master_event();
        cancelled_event["id"] = json!("eventid0");
        cancelled_event["status"] = json!("cancelled");
        cancelled_event["updated"] = json!("2030-01-01T00:00:00Z");
        let items = vec![cancelled_event, master_event()];

        let event = select_invitation_event(items.clone(), None, EventMethod::Request).unwrap();
        assert_eq!(event.id.to_string(), "eventid1");

        let event = select_invitation_event(items, None, EventMethod::Cancel).unwrap();
        assert_eq!(event.id.to_string(), "eventid0");
    }

    #[test]
    fn test_select_the_modified_occurrence_of_a_recurrence_id() {
        let event = select_invitation_event(
            vec![master_event(), modified_occurrence()],
            Some(&recurrence_id("20240510T131500Z")),
            EventMethod::Request,
        )
        .unwrap();

        assert_eq!(event.id.to_string(), "eventid1_20240510T131500Z");
        assert_eq!(event.summary, "Weekly meeting (moved)");
    }

    #[test]
    fn test_complete_a_partial_cancelled_occurrence_with_its_master_event() {
        let master: GoogleCalendarEvent = serde_json::from_value(master_event()).unwrap();

        let event = select_invitation_event(
            vec![master_event(), partial_cancelled_occurrence()],
            Some(&recurrence_id("20240517T131500Z")),
            EventMethod::Cancel,
        )
        .unwrap();

        assert_eq!(event.id.to_string(), "eventid1_20240517T131500Z");
        assert_eq!(event.status, GoogleCalendarEventStatus::Cancelled);
        assert_eq!(event.summary, master.summary);
        assert!(event.recurrence.is_none());
        let occurrence_start = Utc.with_ymd_and_hms(2024, 5, 17, 13, 15, 0).unwrap();
        assert_eq!(event.start.datetime, Some(occurrence_start));
        assert_eq!(
            event.end.datetime,
            Some(
                occurrence_start + (master.end.datetime.unwrap() - master.start.datetime.unwrap())
            )
        );
    }

    #[test]
    fn test_select_the_master_event_for_a_reply_to_an_unmodified_occurrence() {
        let event = select_invitation_event(
            vec![master_event()],
            Some(&recurrence_id("20240524T131500Z")),
            EventMethod::Reply,
        )
        .unwrap();

        assert_eq!(event.id.to_string(), "eventid1");
    }

    #[test]
    fn test_fail_when_no_event_matches() {
        assert!(select_invitation_event(vec![], None, EventMethod::Request).is_err());
        assert!(
            select_invitation_event(
                vec![master_event()],
                Some(&recurrence_id("20240524T131500Z")),
                EventMethod::Request,
            )
            .is_err()
        );
    }
}
