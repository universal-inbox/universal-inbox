use chrono::{TimeDelta, Utc};
use cucumber::{given, then};
use pretty_assertions::assert_eq;
use rrule::Frequency;
use serde_json::json;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::google_calendar::GoogleCalendarConfig,
    },
    notification::{NotificationSourceKind, NotificationStatus, NotificationWithTask},
    third_party::{
        integrations::{
            google_calendar::{
                EventAttendee, EventMethod, GoogleCalendarEvent,
                GoogleCalendarEventAttendeeResponseStatus, GoogleCalendarEventStatus,
            },
            google_mail::GoogleMailThread,
        },
        item::ThirdPartyItemData,
    },
};

use crate::{
    common::settings,
    helpers::{
        integration_connection::{
            create_and_mock_integration_connection, google_calendar_oauth_credential,
        },
        load_json_fixture_file,
        notification::{
            google_calendar::{
                create_notification_from_google_calendar_event, google_calendar_event,
                google_calendar_event_reply, google_calendar_events_list,
                google_calendar_events_list_reply,
            },
            google_mail::{
                google_mail_invitation_attachment, google_mail_invitation_reply_attachment,
                google_mail_thread_with_invitation, google_mail_thread_with_invitation_reply,
                google_mail_user_profile, raw_google_mail_thread_with_invitation,
                raw_google_mail_thread_with_invitation_reply,
            },
        },
        rest::get_resource,
    },
    state::{Attachment, CalendarPages},
    steps::status::UserStatus,
    world::ApiWorld,
};

const INVITATION_ICAL_UID: &str = "event_icaluid1";
const REPLY_ICAL_UID: &str = "event_icaluid2";
const EARLIER_THREAD_ID: &str = "anything";
const KNOWN_INVITATION_THREAD_ID: &str = "invitation_thread";

fn invitation_thread() -> GoogleMailThread {
    google_mail_thread_with_invitation(
        raw_google_mail_thread_with_invitation(),
        google_mail_user_profile(),
    )
}

fn report_the_invitation(
    world: &mut ApiWorld,
    body: universal_inbox::third_party::integrations::google_mail::GoogleMailMessageBody,
) {
    world.google_mail_sync.threads = vec![invitation_thread()];
    world.google_mail_sync.attachment = Some(Attachment {
        message_id: "789",
        attachment_id: "attachmentid1",
        body,
    });
    world.google_mail_sync.calendar = Some(CalendarPages {
        ical_uid: INVITATION_ICAL_UID,
        pages: vec![],
    });
}

fn calendar_pages(world: &mut ApiWorld) -> &mut CalendarPages {
    world
        .google_mail_sync
        .calendar
        .as_mut()
        .expect("Google Mail reports no invitation in this scenario")
}

pub(super) fn with_my_answer(
    event: &GoogleCalendarEvent,
    answer: GoogleCalendarEventAttendeeResponseStatus,
) -> GoogleCalendarEvent {
    GoogleCalendarEvent {
        attendees: event
            .attendees
            .iter()
            .map(|attendee| EventAttendee {
                response_status: if attendee.self_ == Some(true) {
                    answer
                } else {
                    attendee.response_status
                },
                ..attendee.clone()
            })
            .collect(),
        ..event.clone()
    }
}

#[given("I am connected to Google Calendar")]
async fn i_am_connected_to_google_calendar(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let connection = create_and_mock_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::GoogleCalendar(GoogleCalendarConfig::enabled()),
        &settings(),
        google_calendar_oauth_credential(),
        None,
        None,
    )
    .await;
    world.integrations.google_calendar_connection = Some(connection);
}

#[given("Google Mail reports a thread with an invitation")]
fn google_mail_reports_an_invitation(world: &mut ApiWorld) {
    report_the_invitation(world, google_mail_invitation_attachment());
}

#[given(
    "Google Mail reports a thread with an invitation to a moved occurrence of a recurring event"
)]
fn google_mail_reports_a_moved_occurrence(world: &mut ApiWorld) {
    report_the_invitation(
        world,
        load_json_fixture_file("google_mail_invitation_occurrence_update_attachment.json"),
    );
}

#[given("Google Mail reports a thread with the cancellation of an invitation")]
fn google_mail_reports_a_cancellation(world: &mut ApiWorld) {
    report_the_invitation(
        world,
        load_json_fixture_file("google_mail_invitation_cancel_attachment.json"),
    );
}

#[given("Google Mail reports a thread with a reply to an invitation")]
fn google_mail_reports_a_reply(world: &mut ApiWorld) {
    world.google_mail_sync.threads = vec![google_mail_thread_with_invitation_reply(
        raw_google_mail_thread_with_invitation_reply(),
        google_mail_user_profile(),
    )];
    world.google_mail_sync.attachment = Some(Attachment {
        message_id: "890",
        attachment_id: "attachmentid2",
        body: google_mail_invitation_reply_attachment(),
    });
    world.google_mail_sync.calendar = Some(CalendarPages {
        ical_uid: REPLY_ICAL_UID,
        pages: vec![],
    });
}

#[given("Google Calendar knows the event of that invitation")]
fn google_calendar_knows_the_event(world: &mut ApiWorld) {
    let events = serde_json::to_value(google_calendar_events_list()).unwrap();
    calendar_pages(world).pages.push((None, events));
}

#[given("Google Calendar knows the event of that reply")]
fn google_calendar_knows_the_event_of_the_reply(world: &mut ApiWorld) {
    let events = serde_json::to_value(google_calendar_events_list_reply()).unwrap();
    calendar_pages(world).pages.push((None, events));
}

#[given(
    "Google Calendar answers with an empty page and a next page token, and knows the event on the second page"
)]
fn google_calendar_knows_the_event_on_a_later_page(world: &mut ApiWorld) {
    let events = serde_json::to_value(google_calendar_events_list()).unwrap();
    let pages = &mut calendar_pages(world).pages;
    pages.push((None, json!({ "items": [], "nextPageToken": "page2" })));
    pages.push((Some("page2"), events));
}

#[given("Google Calendar knows the recurring event and its moved occurrence")]
fn google_calendar_knows_the_moved_occurrence(world: &mut ApiWorld) {
    let event = google_calendar_event();
    let mut occurrence = serde_json::to_value(&event).unwrap();
    occurrence["id"] = json!("eventid1_20240510T131500Z");
    occurrence["summary"] = json!("Weekly meeting (moved)");
    occurrence["recurringEventId"] = json!("eventid1");
    occurrence["originalStartTime"] =
        json!({ "dateTime": "2024-05-10T13:15:00Z", "timeZone": "Europe/Paris" });
    occurrence["start"] = json!({ "dateTime": "2024-05-10T14:00:00Z", "timeZone": "Europe/Paris" });
    occurrence["end"] = json!({ "dateTime": "2024-05-10T14:15:00Z", "timeZone": "Europe/Paris" });
    occurrence.as_object_mut().unwrap().remove("recurrence");
    calendar_pages(world)
        .pages
        .push((None, json!({ "items": [event, occurrence] })));
}

#[given("Google Calendar reports that event as cancelled")]
fn google_calendar_reports_the_event_cancelled(world: &mut ApiWorld) {
    let answer = world
        .google_mail_sync
        .answer
        .unwrap_or(GoogleCalendarEventAttendeeResponseStatus::NeedsAction);
    let cancelled = GoogleCalendarEvent {
        status: GoogleCalendarEventStatus::Cancelled,
        updated: Utc::now() + TimeDelta::hours(1),
        ..with_my_answer(&google_calendar_event(), answer)
    };
    calendar_pages(world)
        .pages
        .push((None, json!({ "items": [cancelled] })));
}

pub(super) async fn have_a_notification_of_the_event(
    world: &mut ApiWorld,
    name: String,
    thread_id: &str,
    event: GoogleCalendarEvent,
) {
    let user_id = world.user().id;
    let mail_connection_id = world
        .integrations
        .google_mail_connection
        .as_ref()
        .unwrap()
        .id;
    let calendar_connection_id = world
        .integrations
        .google_calendar_connection
        .as_ref()
        .unwrap()
        .id;
    let thread = GoogleMailThread {
        id: thread_id.to_string(),
        ..invitation_thread()
    };
    let app = world.app.get().await;
    let notification = create_notification_from_google_calendar_event(
        app,
        &thread,
        &event,
        user_id,
        mail_connection_id,
        calendar_connection_id,
    )
    .await;
    world.notifications.by_name.insert(name, *notification);
}

#[given(
    expr = "I already have the notification {string} of that event, from an earlier Google Mail thread"
)]
async fn i_already_have_the_notification_of_the_event(world: &mut ApiWorld, name: String) {
    have_a_notification_of_the_event(world, name, EARLIER_THREAD_ID, google_calendar_event()).await;
}

#[given(
    expr = "I already have the notification {string} of that invitation, which I have not answered"
)]
async fn i_have_not_answered_the_invitation(world: &mut ApiWorld, name: String) {
    let answer = GoogleCalendarEventAttendeeResponseStatus::NeedsAction;
    world.google_mail_sync.answer = Some(answer);
    have_a_notification_of_the_event(
        world,
        name,
        KNOWN_INVITATION_THREAD_ID,
        with_my_answer(&google_calendar_event(), answer),
    )
    .await;
}

#[given(expr = "I already have the notification {string} of that invitation, which I accepted")]
async fn i_accepted_the_invitation(world: &mut ApiWorld, name: String) {
    let answer = GoogleCalendarEventAttendeeResponseStatus::Accepted;
    world.google_mail_sync.answer = Some(answer);
    have_a_notification_of_the_event(
        world,
        name,
        KNOWN_INVITATION_THREAD_ID,
        with_my_answer(&google_calendar_event(), answer),
    )
    .await;
}

#[given("I never received that invitation")]
fn i_never_received_that_invitation(world: &mut ApiWorld) {
    assert!(world.notifications.by_name.is_empty());
}

async fn synced_calendar_notification(world: &mut ApiWorld) -> NotificationWithTask {
    assert_eq!(world.google_mail_sync.synced.len(), 1);
    let synced = world.google_mail_sync.synced[0].clone();
    assert_eq!(synced.kind, NotificationSourceKind::GoogleCalendar);
    let client = world.client();
    let app = world.app.get().await;
    let notification: Box<NotificationWithTask> =
        get_resource(&client, &app.api_address, "notifications", synced.id.into()).await;
    assert_eq!(notification.id, synced.id);
    *notification
}

fn event_of(notification: &NotificationWithTask) -> &GoogleCalendarEvent {
    let ThirdPartyItemData::GoogleCalendarEvent(event) = &notification.source_item.data else {
        panic!("Expected a Google Calendar event");
    };
    event
}

#[then("I get one Google Calendar notification, unread and never read")]
async fn i_get_one_unread_calendar_notification(world: &mut ApiWorld) {
    let notification = synced_calendar_notification(world).await;
    assert_eq!(notification.status, NotificationStatus::Unread);
    assert!(notification.last_read_at.is_none());
}

#[then(expr = "I get one Google Calendar notification of the event {string}, unread")]
async fn i_get_one_calendar_notification_of_the_event(world: &mut ApiWorld, source_id: String) {
    let notification = synced_calendar_notification(world).await;
    assert_eq!(notification.status, NotificationStatus::Unread);
    assert_eq!(notification.source_item.source_id, source_id);
}

#[then(expr = "I get one Google Calendar notification, {status}")]
async fn i_get_one_calendar_notification_with_the_status(world: &mut ApiWorld, status: UserStatus) {
    let notification = synced_calendar_notification(world).await;
    assert_eq!(notification.status, status.0);
}

#[then(expr = "it is the event {string}, {string}")]
async fn it_is_the_event(world: &mut ApiWorld, source_id: String, summary: String) {
    let notification = synced_calendar_notification(world).await;
    assert_eq!(notification.source_item.source_id, source_id);
    let event = event_of(&notification);
    assert_eq!(event.id.to_string(), source_id);
    assert_eq!(event.summary, summary);
}

#[then("it repeats every week")]
async fn it_repeats_every_week(world: &mut ApiWorld) {
    let notification = synced_calendar_notification(world).await;
    let rrule_set = event_of(&notification)
        .recurrence
        .as_ref()
        .expect("The event does not repeat");
    assert_eq!(rrule_set.get_rrule()[0].get_freq(), Frequency::Weekly);
}

#[then("it does not repeat")]
async fn it_does_not_repeat(world: &mut ApiWorld) {
    let notification = synced_calendar_notification(world).await;
    assert!(event_of(&notification).recurrence.is_none());
}

#[then("it comes from the Google Mail thread Google Mail reported")]
async fn it_comes_from_the_reported_thread(world: &mut ApiWorld) {
    let notification = synced_calendar_notification(world).await;
    let mail_item = notification.source_item.source_item.as_ref().unwrap();
    assert_eq!(
        mail_item.data,
        ThirdPartyItemData::GoogleMailThread(Box::new(world.google_mail_sync.threads[0].clone()))
    );
}

#[then("it is the event of the reply as Google Calendar reports it, with the method reply")]
async fn it_is_the_event_of_the_reply(world: &mut ApiWorld) {
    let notification = synced_calendar_notification(world).await;
    assert_eq!(notification.source_item.source_id, "eventid2");
    assert_eq!(
        notification.source_item.data,
        ThirdPartyItemData::GoogleCalendarEvent(Box::new(google_calendar_event_reply()))
    );
    assert_eq!(event_of(&notification).method, EventMethod::Reply);
}

#[then(
    expr = "it is the notification {string} of the same Google Calendar event, now fed by the new Google Mail thread"
)]
async fn it_is_the_known_notification_with_the_new_thread(world: &mut ApiWorld, name: String) {
    let known = world.notifications.named(&name).clone();
    let notification = synced_calendar_notification(world).await;
    assert_eq!(notification.id, known.id);
    assert_eq!(notification.source_item.id, known.source_item.id);
    assert_eq!(notification.source_item.source_id, "eventid1");
    let known_mail_item = known.source_item.source_item.as_ref().unwrap();
    let mail_item = notification.source_item.source_item.as_ref().unwrap();
    assert_ne!(mail_item.id, known_mail_item.id);
    assert_eq!(
        mail_item.data,
        ThirdPartyItemData::GoogleMailThread(Box::new(world.google_mail_sync.threads[0].clone()))
    );
}

#[then("it is the cancelled event \"eventid1\"")]
async fn it_is_the_cancelled_event(world: &mut ApiWorld) {
    let notification = synced_calendar_notification(world).await;
    assert_eq!(notification.source_item.source_id, "eventid1");
    let event = event_of(&notification);
    assert_eq!(event.method, EventMethod::Cancel);
    assert!(event.is_cancelled());
}

#[then(expr = "it is {string}, which I already had")]
async fn it_is_the_notification_i_already_had(world: &mut ApiWorld, name: String) {
    let known = world.notifications.named(&name).id;
    let notification = synced_calendar_notification(world).await;
    assert_eq!(notification.id, known);
}
