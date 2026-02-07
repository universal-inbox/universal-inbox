use rstest::*;
use serde_json::json;
use wiremock::matchers::{body_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use universal_inbox::{
    integration_connection::IntegrationConnectionId,
    notification::Notification,
    third_party::{
        integrations::{
            google_calendar::{EventAttendee, GoogleCalendarEvent},
            google_mail::GoogleMailThread,
        },
        item::{ThirdPartyItem, ThirdPartyItemData},
    },
    user::UserId,
};

use universal_inbox_api::{
    integrations::google_calendar::GoogleCalendarEventsList,
    repository::third_party::ThirdPartyItemRepository,
};

use crate::helpers::{TestedApp, load_json_fixture_file};

pub async fn create_notification_from_google_calendar_event(
    app: &TestedApp,
    google_mail_thread: &GoogleMailThread,
    google_calendar_event: &GoogleCalendarEvent,
    user_id: UserId,
    google_mail_integration_connection_id: IntegrationConnectionId,
    google_calendar_integration_connection_id: IntegrationConnectionId,
) -> Box<Notification> {
    let google_calendar_service = app
        .notification_service
        .read()
        .await
        .google_calendar_service
        .clone();

    let mut transaction = app.repository.begin().await.unwrap();

    let gmail_third_party_item = ThirdPartyItem::new(
        google_mail_thread.id.to_string(),
        ThirdPartyItemData::GoogleMailThread(Box::new(google_mail_thread.clone())),
        user_id,
        google_mail_integration_connection_id,
    );
    let gmail_third_party_item = app
        .repository
        .create_or_update_third_party_item(&mut transaction, Box::new(gmail_third_party_item))
        .await
        .unwrap()
        .value();

    let mut gcal_third_party_item = ThirdPartyItem::new(
        google_calendar_event.id.to_string(),
        ThirdPartyItemData::GoogleCalendarEvent(Box::new(google_calendar_event.clone())),
        user_id,
        google_calendar_integration_connection_id,
    );
    gcal_third_party_item.source_item = Some(gmail_third_party_item);
    let gcal_third_party_item = app
        .repository
        .create_or_update_third_party_item(&mut transaction, Box::new(gcal_third_party_item))
        .await
        .unwrap()
        .value();

    let notification = app
        .notification_service
        .read()
        .await
        .create_notification_from_third_party_item(
            &mut transaction,
            *gcal_third_party_item,
            google_calendar_service,
            user_id,
        )
        .await
        .unwrap()
        .unwrap();

    transaction.commit().await.unwrap();

    Box::new(notification)
}

pub async fn mock_google_calendar_list_events_service(
    google_calendar_mock_server: &MockServer,
    event_id: &str,
    result: &GoogleCalendarEventsList,
) {
    Mock::given(method("GET"))
        .and(path("/calendars/primary/events"))
        .and(header(
            "authorization",
            "Bearer google_calendar_test_access_token",
        ))
        .and(query_param("iCalUID", event_id))
        .and(query_param("maxResults", "1"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(result),
        )
        .mount(google_calendar_mock_server)
        .await;
}

pub async fn mock_google_calendar_event_delete_service(
    google_calendar_mock_server: &MockServer,
    event_id: &str,
) {
    Mock::given(method("DELETE"))
        .and(path(format!("/calendars/primary/events/{event_id}")))
        .and(header(
            "authorization",
            "Bearer google_calendar_test_access_token",
        ))
        .respond_with(ResponseTemplate::new(200).insert_header("content-type", "application/json"))
        .mount(google_calendar_mock_server)
        .await;
}

pub async fn mock_google_calendar_event_answer_service(
    google_calendar_mock_server: &MockServer,
    event_id: &str,
    attendees: Vec<EventAttendee>,
    result: &GoogleCalendarEvent,
) {
    Mock::given(method("PATCH"))
        .and(path(format!("/calendars/primary/events/{event_id}")))
        .and(header(
            "authorization",
            "Bearer google_calendar_test_access_token",
        ))
        .and(body_json(json!({
            "attendees": attendees
        })))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(result),
        )
        .mount(google_calendar_mock_server)
        .await;
}

#[fixture]
pub fn google_calendar_events_list() -> GoogleCalendarEventsList {
    load_json_fixture_file("google_calendar_events_list.json")
}

#[fixture]
pub fn google_calendar_events_list_reply() -> GoogleCalendarEventsList {
    load_json_fixture_file("google_calendar_events_list_reply.json")
}

#[fixture]
pub fn google_calendar_event() -> GoogleCalendarEvent {
    load_json_fixture_file("google_calendar_event.json")
}

#[fixture]
pub fn google_calendar_event_reply() -> GoogleCalendarEvent {
    load_json_fixture_file("google_calendar_event_reply.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rrule::Frequency;

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
}
