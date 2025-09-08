use std::{sync::Weak, time::Duration};

use anyhow::{anyhow, Context};
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
            EventAttendee, EventReminder, GoogleCalendarEvent,
            GoogleCalendarEventAttendeeResponseStatus,
        },
        item::{ThirdPartyItem, ThirdPartyItemData},
    },
    user::UserId,
};
use url::Url;
use uuid::Uuid;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

use crate::{
    integrations::oauth2::AccessToken,
    universal_inbox::{
        integration_connection::service::IntegrationConnectionService, UniversalInboxError,
    },
    utils::api::ApiClient,
};

use super::notification::ThirdPartyNotificationSourceService;

const GOOGLE_CALENDAR_BASE_URL: &str = "https://www.googleapis.com/calendar/v3";

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

        let mut auth_header_value: HeaderValue = format!("Bearer {access_token}").parse().unwrap();
        auth_header_value.set_sensitive(true);
        headers.insert("Authorization", auth_header_value);

        ApiClient::build(
            headers,
            [format!(
                "{}/calendars/{{calendar_id}}/events/{{event_id}}",
                self.google_calendar_base_path
            )],
            self.max_retry_duration,
        )
    }

    pub async fn get_event(
        &self,
        calendar_id: &str,
        ical_uid: &str,
        access_token: &AccessToken,
    ) -> Result<GoogleCalendarEvent, UniversalInboxError> {
        let url = format!(
            "{}/calendars/{}/events?iCalUID={}&maxResults=1",
            self.google_calendar_base_url, calendar_id, ical_uid
        );

        let events_list: GoogleCalendarEventsList = self
            .build_google_calendar_client(access_token)?
            .get(&url)
            .await
            .context(format!(
                "Cannot fetch Google Calendar event ical_uid={ical_uid} in calendar {calendar_id}"
            ))?;

        Ok(events_list.items.into_iter().next().ok_or_else(|| {
            anyhow!(
                "Cannot find Google Calendar event ical_uid={ical_uid} in calendar {calendar_id}"
            )
        })?)
    }

    async fn delete_event(
        &self,
        calendar_id: &str,
        event_id: &str,
        access_token: &AccessToken,
    ) -> Result<(), UniversalInboxError> {
        let url = format!(
            "{}/calendars/{}/events/{}",
            self.google_calendar_base_url, calendar_id, event_id
        );
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
                )))
            }
        };

        let url = format!(
            "{}/calendars/primary/events/{}",
            self.google_calendar_base_url, event.id
        );

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

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::*;

    mod notification_conversion {
        use std::{env, fs};

        use super::*;
        use pretty_assertions::assert_eq;

        use universal_inbox::{third_party::item::ThirdPartyItemFromSource, HasHtmlUrl};

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
