use httpmock::{
    Method::{DELETE, GET},
    Mock, MockServer,
};
use rstest::*;

use serde_json::json;
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

use crate::helpers::{load_json_fixture_file, TestedApp};

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

pub fn mock_google_calendar_list_events_service<'a>(
    google_calendar_mock_server: &'a MockServer,
    event_id: &'a str,
    result: &'a GoogleCalendarEventsList,
) -> Mock<'a> {
    google_calendar_mock_server.mock(|when, then| {
        when.method(GET)
            .path("/calendars/primary/events")
            .header("authorization", "Bearer google_calendar_test_access_token")
            .query_param("iCalUID", event_id)
            .query_param("maxResults", "1");
        then.status(200)
            .header("content-type", "application/json")
            .json_body_obj(result);
    })
}

pub fn mock_google_calendar_event_delete_service<'a>(
    google_calendar_mock_server: &'a MockServer,
    event_id: &'a str,
) -> Mock<'a> {
    google_calendar_mock_server.mock(|when, then| {
        when.method(DELETE)
            .path(format!("/calendars/primary/events/{event_id}"))
            .header("authorization", "Bearer google_calendar_test_access_token");
        then.status(200).header("content-type", "application/json");
    })
}

pub fn mock_google_calendar_event_answer_service<'a>(
    google_calendar_mock_server: &'a MockServer,
    event_id: &'a str,
    attendees: Vec<EventAttendee>,
    result: &'a GoogleCalendarEvent,
) -> Mock<'a> {
    google_calendar_mock_server.mock(|when, then| {
        when.method("PATCH")
            .path(format!("/calendars/primary/events/{event_id}"))
            .header("authorization", "Bearer google_calendar_test_access_token")
            .json_body(json!({
                "attendees": attendees
            }));
        then.status(200)
            .header("content-type", "application/json")
            .json_body_obj(result);
    })
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
