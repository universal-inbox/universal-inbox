use std::str::FromStr;

use chrono::Utc;
use cucumber::{Parameter, given, then, when};
use pretty_assertions::assert_eq;
use serde_json::json;
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{any, body_json, body_string, header, method, path},
};

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::{google_calendar::GoogleCalendarConfig, google_mail::GoogleMailConfig},
    },
    notification::{
        Notification, NotificationStatus, NotificationWithTask,
        service::{InvitationPatch, NotificationPatch},
    },
    third_party::{
        integrations::{
            google_calendar::{
                EventAttendee, GoogleCalendarEvent, GoogleCalendarEventAttendeeResponseStatus,
            },
            google_mail::{GOOGLE_MAIL_INBOX_LABEL, GoogleMailThread},
        },
        item::ThirdPartyItemData,
    },
};

use universal_inbox_api::integrations::oauth2::AccessToken;

use crate::{
    common::settings,
    helpers::{
        integration_connection::{
            OAuthCredentialFixture, create_and_mock_integration_connection,
            google_calendar_oauth_credential, google_mail_oauth_credential,
        },
        notification::google_calendar::{
            create_notification_from_google_calendar_event, google_calendar_event,
            google_calendar_event_reply,
        },
        rest::{get_resource, patch_resource},
    },
    steps::{
        google_mail_invitations::{
            KNOWN_INVITATION_THREAD_ID, have_a_notification_of_the_event, invitation_thread,
            with_my_answer,
        },
        status::UserStatus,
    },
    world::{ApiWorld, Response},
};

#[derive(Clone, Copy, Debug)]
pub struct Answer(GoogleCalendarEventAttendeeResponseStatus);

impl Parameter for Answer {
    const REGEX: &'static str = "accepted|declined|tentative";
    const NAME: &'static str = "answer";
}

impl FromStr for Answer {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "accepted" => Ok(Self(GoogleCalendarEventAttendeeResponseStatus::Accepted)),
            "declined" => Ok(Self(GoogleCalendarEventAttendeeResponseStatus::Declined)),
            "tentative" => Ok(Self(GoogleCalendarEventAttendeeResponseStatus::Tentative)),
            other => Err(format!("Unknown answer {other:?}")),
        }
    }
}

fn notification_named(world: &ApiWorld, name: &str) -> Notification {
    world.notifications.named(name).clone()
}

fn event_of(notification: &Notification) -> GoogleCalendarEvent {
    let ThirdPartyItemData::GoogleCalendarEvent(event) = &notification.source_item.data else {
        panic!("The notification is not the one of a Google Calendar event");
    };
    (**event).clone()
}

fn event_with_my_answer(
    event: &GoogleCalendarEvent,
    answer: GoogleCalendarEventAttendeeResponseStatus,
) -> (Vec<EventAttendee>, GoogleCalendarEvent) {
    let mut attendees = event.attendees.clone();
    let index = attendees
        .iter()
        .position(|attendee| attendee.self_ == Some(true))
        .expect("the event has no self attendee");
    attendees[index] = EventAttendee {
        response_status: answer,
        ..attendees[index].clone()
    };
    let updated = GoogleCalendarEvent {
        attendees: attendees.clone(),
        ..event.clone()
    };
    (attendees, updated)
}

#[given(expr = "I already have the notification {string} of a reply to an invitation")]
async fn i_have_the_notification_of_a_reply(world: &mut ApiWorld, name: String) {
    have_a_notification_of_the_event(world, name, "reply_thread", google_calendar_event_reply())
        .await;
}

#[given(expr = "Google Mail archives, once, the thread of {string}")]
async fn google_mail_archives_the_thread(world: &mut ApiWorld, name: String) {
    let thread_id = notification_named(world, &name)
        .source_item
        .source_item
        .expect("The notification has no Google Mail thread")
        .source_id;
    let synced_label_id = GoogleMailConfig::enabled().synced_label.id;
    let app = world.app.get().await;
    Mock::given(method("POST"))
        .and(path(format!("/users/me/threads/{thread_id}/modify")))
        .and(body_string(
            json!({
                "addLabelIds": Vec::<&str>::new(),
                "removeLabelIds": vec![GOOGLE_MAIL_INBOX_LABEL, synced_label_id.as_str()]
            })
            .to_string(),
        ))
        .and(header(
            "authorization",
            "Bearer google_mail_test_access_token",
        ))
        .respond_with(ResponseTemplate::new(200).insert_header("content-type", "application/json"))
        .expect(1)
        .mount(&app.google_mail_mock_server)
        .await;
}

#[given("Google Calendar is not asked anything")]
async fn google_calendar_is_not_asked_anything(world: &mut ApiWorld) {
    let app = world.app.get().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&app.google_calendar_mock_server)
        .await;
}

#[given(expr = "Google Calendar deletes, once, the event of {string}")]
async fn google_calendar_deletes_the_event(world: &mut ApiWorld, name: String) {
    let event_id = event_of(&notification_named(world, &name)).id;
    let app = world.app.get().await;
    Mock::given(method("DELETE"))
        .and(path(format!("/calendars/primary/events/{event_id}")))
        .and(header(
            "authorization",
            "Bearer google_calendar_test_access_token",
        ))
        .respond_with(ResponseTemplate::new(200).insert_header("content-type", "application/json"))
        .expect(1)
        .mount(&app.google_calendar_mock_server)
        .await;
}

#[given(expr = "Google Calendar records, once, my answer {answer} to the event of {string}")]
async fn google_calendar_records_my_answer(world: &mut ApiWorld, answer: Answer, name: String) {
    let notification = notification_named(world, &name);
    let (attendees, updated) = event_with_my_answer(&event_of(&notification), answer.0);
    let source_id = notification.source_item.source_id;
    let app = world.app.get().await;
    Mock::given(method("PATCH"))
        .and(path(format!("/calendars/primary/events/{source_id}")))
        .and(header(
            "authorization",
            "Bearer google_calendar_test_access_token",
        ))
        .and(body_json(json!({ "attendees": attendees })))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(&updated),
        )
        .expect(1)
        .mount(&app.google_calendar_mock_server)
        .await;
}

async fn patch_status(world: &mut ApiWorld, name: &str, status: NotificationStatus) {
    let id = notification_named(world, name).id;
    let client = world.client();
    let app = world.app.get().await;
    let patched: Box<Notification> = patch_resource(
        &client,
        &app.api_address,
        "notifications",
        id.into(),
        &NotificationPatch {
            status: Some(status),
            ..Default::default()
        },
    )
    .await;
    world.notifications.patched = Some(*patched);
}

#[when(expr = "I set the status of {string} to {status}")]
async fn i_set_the_status(world: &mut ApiWorld, name: String, status: UserStatus) {
    patch_status(world, &name, status.0).await;
}

#[then(
    expr = "the patched notification is {status}, was read just now, and is otherwise as {string} was"
)]
async fn the_patched_notification_is(world: &mut ApiWorld, status: UserStatus, name: String) {
    let before = notification_named(world, &name);
    let patched = world
        .notifications
        .patched
        .as_ref()
        .expect("The notification was not patched");
    assert!(Utc::now() - patched.last_read_at.unwrap() < chrono::Duration::minutes(1));
    assert_eq!(
        *patched,
        Notification {
            status: status.0,
            last_read_at: patched.last_read_at,
            ..before
        }
    );
    crate::steps::notifications::assert_the_patched_notification_is_stored(world).await;
}

#[when(expr = "I answer the invitation of {string} with {answer}")]
async fn i_answer_the_invitation(world: &mut ApiWorld, name: String, answer: Answer) {
    let id = notification_named(world, &name).id;
    let client = world.client();
    let app = world.app.get().await;
    let response = client
        .patch(format!("{}notifications/{id}/invitation", app.api_address))
        .json(&InvitationPatch {
            response_status: answer.0,
        })
        .send()
        .await
        .expect("Failed to execute request");
    world.http.record(Response::read(response).await);
}

#[then(
    expr = "the answered notification is {status}, was read just now, and its event carries my answer {answer}"
)]
async fn the_answered_notification(world: &mut ApiWorld, status: UserStatus, answer: Answer) {
    let patched: Notification =
        serde_json::from_slice(&world.http.last().body).expect("The answer is not a notification");
    assert_eq!(patched.status, status.0);
    assert!(Utc::now() - patched.last_read_at.unwrap() < chrono::Duration::minutes(1));
    let event = event_of(&patched);
    assert_eq!(event.get_self_attendee().unwrap().response_status, answer.0);
    world.notifications.patched = Some(patched);
}

#[then(expr = "{string}, read again, has the event with my answer {answer}")]
async fn the_notification_read_again(world: &mut ApiWorld, name: String, answer: Answer) {
    let before = notification_named(world, &name);
    let (_, updated) = event_with_my_answer(&event_of(&before), answer.0);
    let client = world.client();
    let app = world.app.get().await;
    let read: Box<NotificationWithTask> =
        get_resource(&client, &app.api_address, "notifications", before.id.into()).await;
    assert_eq!(
        read.source_item.data,
        ThirdPartyItemData::GoogleCalendarEvent(Box::new(updated))
    );
}

#[then(expr = "{string}, read again, keeps its status")]
async fn the_notification_keeps_its_status(world: &mut ApiWorld, name: String) {
    let before = notification_named(world, &name);
    let client = world.client();
    let app = world.app.get().await;
    let read: Box<NotificationWithTask> =
        get_resource(&client, &app.api_address, "notifications", before.id.into()).await;
    assert_eq!(read.status, before.status);
}

#[given("Google Mail is not asked anything")]
async fn google_mail_is_not_asked_anything(world: &mut ApiWorld) {
    let app = world.app.get().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&app.google_mail_mock_server)
        .await;
}

#[given(
    expr = "that other user has their own Google Mail and Google Calendar connections, with access tokens of their own, and the notification {string} of the same invitation"
)]
async fn that_other_user_has_the_same_invitation(world: &mut ApiWorld, name: String) {
    let other_user_id = world
        .other_session
        .as_ref()
        .expect("There is no other user")
        .user
        .id;
    let app = world.app.get().await;
    let mail = create_and_mock_integration_connection(
        app,
        other_user_id,
        IntegrationConnectionConfig::GoogleMail(GoogleMailConfig::enabled()),
        &settings(),
        OAuthCredentialFixture {
            access_token: AccessToken("other_user_google_mail_access_token".to_string()),
            ..google_mail_oauth_credential()
        },
        None,
        None,
    )
    .await;
    let calendar = create_and_mock_integration_connection(
        app,
        other_user_id,
        IntegrationConnectionConfig::GoogleCalendar(GoogleCalendarConfig::enabled()),
        &settings(),
        OAuthCredentialFixture {
            access_token: AccessToken("other_user_google_calendar_access_token".to_string()),
            ..google_calendar_oauth_credential()
        },
        None,
        None,
    )
    .await;
    let thread = GoogleMailThread {
        id: KNOWN_INVITATION_THREAD_ID.to_string(),
        ..invitation_thread()
    };
    let notification = create_notification_from_google_calendar_event(
        app,
        &thread,
        &with_my_answer(
            &google_calendar_event(),
            GoogleCalendarEventAttendeeResponseStatus::NeedsAction,
        ),
        other_user_id,
        mail.id,
        calendar.id,
    )
    .await;
    world.notifications.by_name.insert(name, *notification);
}

#[when(expr = "that other user answers the invitation of {string} with {answer}")]
async fn that_other_user_answers(world: &mut ApiWorld, name: String, answer: Answer) {
    let id = notification_named(world, &name).id;
    let client = world
        .other_session
        .as_ref()
        .expect("There is no other user")
        .client
        .clone();
    let app = world.app.get().await;
    let response = client
        .patch(format!("{}notifications/{id}/invitation", app.api_address))
        .json(&InvitationPatch {
            response_status: answer.0,
        })
        .send()
        .await
        .expect("Failed to execute request");
    world.http.record(Response::read(response).await);
}

#[when(expr = "that other user tries to set the status of {string} to {status}")]
async fn that_other_user_tries_to_set_the_status(
    world: &mut ApiWorld,
    name: String,
    status: UserStatus,
) {
    let id = notification_named(world, &name).id;
    let client = world
        .other_session
        .as_ref()
        .expect("There is no other user")
        .client
        .clone();
    let app = world.app.get().await;
    let response = client
        .patch(format!("{}notifications/{id}", app.api_address))
        .json(&NotificationPatch {
            status: Some(status.0),
            ..Default::default()
        })
        .send()
        .await
        .expect("Failed to execute request");
    world.http.record(Response::read(response).await);
}

#[then(expr = "{string}, read again, is exactly as it was")]
async fn the_notification_is_exactly_as_it_was(world: &mut ApiWorld, name: String) {
    let before = notification_named(world, &name);
    let client = world.client();
    let app = world.app.get().await;
    let read: Box<NotificationWithTask> =
        get_resource(&client, &app.api_address, "notifications", before.id.into()).await;
    assert_eq!(Notification::from(*read), before);
}

#[then(
    expr = "the notification {string} of that other user, read by that other user, is exactly as it was"
)]
async fn the_notification_of_the_other_user_is_as_it_was(world: &mut ApiWorld, name: String) {
    let before = notification_named(world, &name);
    let client = world
        .other_session
        .as_ref()
        .expect("There is no other user")
        .client
        .clone();
    let app = world.app.get().await;
    let read: Box<NotificationWithTask> =
        get_resource(&client, &app.api_address, "notifications", before.id.into()).await;
    assert_eq!(Notification::from(*read), before);
}
