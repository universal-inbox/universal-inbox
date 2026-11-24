use std::str::FromStr;

use cucumber::{Parameter, given, then, when};
use email_address::EmailAddress;
use pretty_assertions::assert_eq;

use universal_inbox::{
    integration_connection::{
        IntegrationConnection,
        config::IntegrationConnectionConfig,
        integrations::google_mail::{GoogleMailConfig, GoogleMailContext},
        provider::{IntegrationConnectionContext, IntegrationProvider},
    },
    notification::{
        NotificationSourceKind, NotificationStatus, NotificationWithTask,
        service::NotificationPatch,
    },
    pii::Pii,
    third_party::{
        integrations::google_mail::{
            GOOGLE_MAIL_INBOX_LABEL, GOOGLE_MAIL_SENT_LABEL, GOOGLE_MAIL_UNREAD_LABEL,
            GoogleMailMessageHeader, GoogleMailThread,
        },
        item::ThirdPartyItemData,
    },
};
use universal_inbox_api::integrations::google_mail::{
    GoogleMailThreadList, GoogleMailThreadMinimal,
};

use crate::{
    common::settings,
    helpers::{
        integration_connection::{
            OAuthCredentialFixture, create_and_mock_integration_connection, google_account_id,
            google_mail_oauth_credential,
        },
        notification::{
            google_calendar::mock_google_calendar_list_events_page_service,
            google_mail::{
                assert_sync_notifications, create_notification_from_google_mail_thread,
                google_mail_labels_list, google_mail_thread_get_123, google_mail_thread_get_456,
                google_mail_user_profile, mock_google_mail_get_attachment_service,
                mock_google_mail_get_user_profile_service, mock_google_mail_labels_list_service,
                mock_google_mail_thread_get_service, mock_google_mail_thread_modify_service,
                mock_google_mail_threads_list_service, raw_google_mail_thread_get_123,
                raw_google_mail_thread_get_456,
            },
            sync_notifications,
        },
        rest::get_resource,
    },
    state::Pages,
    steps::{
        integration_connections::stored_connection, kept_triage::read_notification,
        notifications::update, status::UserStatus,
    },
    world::ApiWorld,
};

const THREAD_456: &str = "456";

fn user_email_address() -> Pii<EmailAddress> {
    Pii::<EmailAddress>::from_str(&google_mail_user_profile().email_address).unwrap()
}

fn thread_456(world: &mut ApiWorld) -> &mut GoogleMailThread {
    world
        .google_mail_sync
        .threads
        .iter_mut()
        .find(|thread| thread.id == THREAD_456)
        .expect("Google Mail does not report the thread 456")
}

pub async fn mount_google_mail(world: &mut ApiWorld) {
    let sync = &world.google_mail_sync;
    let threads = sync.threads.clone();
    let paged = sync.pages == Pages::Two;
    let attachment = sync.attachment.as_ref().map(|attachment| {
        (
            attachment.message_id,
            attachment.attachment_id,
            attachment.body.clone(),
        )
    });
    let calendar = sync
        .calendar
        .as_ref()
        .map(|calendar| (calendar.ical_uid, calendar.pages.clone()));
    let page_size = settings()
        .integrations
        .get("google_mail")
        .unwrap()
        .page_size
        .unwrap();
    let synced_label = GoogleMailConfig::enabled().synced_label.id;
    let app = world.app.get().await;
    let server = &app.google_mail_mock_server;

    mock_google_mail_get_user_profile_service(server, &google_mail_user_profile()).await;
    mock_google_mail_labels_list_service(server, &google_mail_labels_list()).await;
    let minimal: Vec<_> = threads
        .iter()
        .map(|thread| GoogleMailThreadMinimal {
            id: thread.id.clone(),
            snippet: thread.messages[0].snippet.clone(),
            history_id: thread.history_id.clone(),
        })
        .collect();
    mock_google_mail_threads_list_service(
        server,
        None,
        page_size,
        Some(vec![synced_label.clone()]),
        &GoogleMailThreadList {
            result_size_estimate: minimal.len(),
            threads: (!minimal.is_empty()).then_some(minimal),
            next_page_token: paged.then(|| "next_token".to_string()),
        },
    )
    .await;
    if paged {
        mock_google_mail_threads_list_service(
            server,
            Some("next_token"),
            page_size,
            Some(vec![synced_label.clone()]),
            &GoogleMailThreadList {
                threads: None,
                result_size_estimate: 1,
                next_page_token: None,
            },
        )
        .await;
    }
    for thread in &threads {
        mock_google_mail_thread_get_service(server, &thread.id, &thread.clone().into()).await;
        mock_google_mail_thread_modify_service(
            server,
            &thread.id,
            vec![],
            vec![GOOGLE_MAIL_INBOX_LABEL, &synced_label],
        )
        .await;
    }
    if let Some((message_id, attachment_id, body)) = attachment {
        mock_google_mail_get_attachment_service(server, message_id, attachment_id, &body).await;
    }
    if let Some((ical_uid, pages)) = calendar {
        for (page_token, body) in pages {
            mock_google_calendar_list_events_page_service(
                &app.google_calendar_mock_server,
                ical_uid,
                page_token,
                &body,
            )
            .await;
        }
    }
}

pub async fn sync_google_mail(world: &mut ApiWorld) {
    mount_google_mail(world).await;
    let client = world.client();
    let app = world.app.get().await;
    let synced = sync_notifications(
        &client,
        &app.api_address,
        Some(NotificationSourceKind::GoogleMail),
        false,
    )
    .await;
    world.google_mail_sync.synced = synced;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddressedTo {
    Me,
    Somebody,
}

impl Parameter for AddressedTo {
    const REGEX: &'static str = "me|somebody else";
    const NAME: &'static str = "addressee";
}

impl FromStr for AddressedTo {
    type Err = String;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "me" => Ok(Self::Me),
            "somebody else" => Ok(Self::Somebody),
            other => Err(format!("Unknown addressee {other:?}")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reading {
    Unread,
    Read,
}

impl Parameter for Reading {
    const REGEX: &'static str = "unread|read";
    const NAME: &'static str = "reading";
}

impl FromStr for Reading {
    type Err = String;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "unread" => Ok(Self::Unread),
            "read" => Ok(Self::Read),
            other => Err(format!("Unknown reading {other:?}")),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Sender {
    Me,
    Stranger,
    StrangerUsingMyAddress,
}

impl Parameter for Sender {
    const REGEX: &'static str = "me|a stranger|a stranger who put my address in the display name";
    const NAME: &'static str = "sender";
}

impl FromStr for Sender {
    type Err = String;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "me" => Ok(Self::Me),
            "a stranger" => Ok(Self::Stranger),
            "a stranger who put my address in the display name" => Ok(Self::StrangerUsingMyAddress),
            other => Err(format!("Unknown sender {other:?}")),
        }
    }
}

#[given("I am connected to Google Mail")]
async fn i_am_connected_to_google_mail(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let connection = create_and_mock_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::GoogleMail(GoogleMailConfig::enabled()),
        &settings(),
        google_mail_oauth_credential(),
        None,
        None,
    )
    .await;
    world.integrations.google_mail_connection = Some(connection);
}

async fn connect_with_a_cached_address(
    world: &mut ApiWorld,
    address: String,
    google_account: Option<String>,
) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let connection = create_and_mock_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::GoogleMail(GoogleMailConfig::enabled()),
        &settings(),
        OAuthCredentialFixture {
            provider_user_id: google_account.as_deref().map(google_account_id),
            ..google_mail_oauth_credential()
        },
        None,
        Some(IntegrationConnectionContext::GoogleMail(
            GoogleMailContext {
                user_email_address: Pii::<EmailAddress>::from_str(&address).unwrap(),
                labels: vec![],
            },
        )),
    )
    .await;
    world.integrations.google_mail_connection = Some(connection);
}

#[given(
    expr = "I am connected to Google Mail, which cached the address {string} and is pinned to the Google account of {string}"
)]
async fn i_am_connected_with_a_pinned_account(
    world: &mut ApiWorld,
    address: String,
    google_account: String,
) {
    connect_with_a_cached_address(world, address, Some(google_account)).await;
}

#[given(
    expr = "I am connected to Google Mail, which cached the address {string} and is not pinned to a Google account yet"
)]
async fn i_am_connected_without_a_pinned_account(world: &mut ApiWorld, address: String) {
    connect_with_a_cached_address(world, address, None).await;
}

#[given("Google Mail reports the threads 123 and 456")]
fn google_mail_reports_the_threads(world: &mut ApiWorld) {
    let profile = google_mail_user_profile();
    world.google_mail_sync.threads = vec![
        google_mail_thread_get_123(raw_google_mail_thread_get_123(), profile.clone()),
        google_mail_thread_get_456(raw_google_mail_thread_get_456(), profile),
    ];
}

#[given("Google Mail reports the thread 456")]
fn google_mail_reports_the_thread_456(world: &mut ApiWorld) {
    world.google_mail_sync.threads = vec![google_mail_thread_get_456(
        raw_google_mail_thread_get_456(),
        google_mail_user_profile(),
    )];
}

#[given("Google Mail reports no thread")]
fn google_mail_reports_no_thread(world: &mut ApiWorld) {
    world.google_mail_sync.threads = vec![];
}

#[given("Google Mail lists its threads over two pages")]
fn google_mail_lists_its_threads_over_two_pages(world: &mut ApiWorld) {
    world.google_mail_sync.pages = Pages::Two;
}

#[given(expr = "I already have the notification {string} of the Google Mail thread 456")]
async fn i_already_have_the_notification_of_thread_456(world: &mut ApiWorld, name: String) {
    let user_id = world.user().id;
    let connection_id = world
        .integrations
        .google_mail_connection
        .as_ref()
        .expect("The account is not connected to Google Mail")
        .id;
    let thread = thread_456(world).clone();
    let app = world.app.get().await;
    let notification =
        create_notification_from_google_mail_thread(app, &thread, user_id, connection_id).await;
    world.notifications.by_name.insert(name, *notification);
}

#[given(
    expr = "I unsubscribed from the Google Mail thread 456, {string}, before its second message arrived"
)]
async fn i_unsubscribed_before_the_second_message(world: &mut ApiWorld, name: String) {
    thread_456(world).messages[0].label_ids = None;
    let mut known = thread_456(world).clone();
    known.messages.truncate(1);
    let user_id = world.user().id;
    let connection_id = world
        .integrations
        .google_mail_connection
        .as_ref()
        .expect("The account is not connected to Google Mail")
        .id;
    let app = world.app.get().await;
    let notification =
        create_notification_from_google_mail_thread(app, &known, user_id, connection_id).await;
    world
        .notifications
        .by_name
        .insert(name.clone(), *notification);
    update(
        world,
        &name,
        NotificationPatch {
            status: Some(NotificationStatus::Unsubscribed),
            ..Default::default()
        },
    )
    .await;
}

#[given(expr = "that second message is addressed to {addressee} and is {reading}")]
fn the_second_message_is(world: &mut ApiWorld, addressee: AddressedTo, reading: Reading) {
    let synced_label = GoogleMailConfig::enabled().synced_label.id;
    let address = user_email_address();
    let message = &mut thread_456(world).messages[1];
    let mut labels = vec![
        "TEST_LABEL".to_string(),
        GOOGLE_MAIL_INBOX_LABEL.to_string(),
        synced_label,
    ];
    if reading == Reading::Unread {
        labels.push(GOOGLE_MAIL_UNREAD_LABEL.to_string());
    }
    message.label_ids = Some(labels);
    message.payload.headers = vec![GoogleMailMessageHeader {
        name: "To".to_string(),
        value: if addressee == AddressedTo::Me {
            format!(
                "other@example.com, You <{}>, test@example.com",
                address.expose()
            )
        } else {
            "other@example.com".to_string()
        },
    }];
}

#[given(expr = "the last message of that thread is unread and was sent by {sender}")]
fn the_last_message_is_unread_and(world: &mut ApiWorld, sender: Sender) {
    let synced_label = GoogleMailConfig::enabled().synced_label.id;
    let address = user_email_address();
    let mut labels = vec![
        GOOGLE_MAIL_INBOX_LABEL.to_string(),
        synced_label,
        GOOGLE_MAIL_UNREAD_LABEL.to_string(),
    ];
    if matches!(sender, Sender::Me) {
        labels.push(GOOGLE_MAIL_SENT_LABEL.to_string());
    }
    let from = match sender {
        Sender::Me => format!("User Name <{}>", address.expose()),
        Sender::Stranger => "External Sender <external@example.com>".to_string(),
        Sender::StrangerUsingMyAddress => {
            format!("{} <external@example.com>", address.expose())
        }
    };
    let message = &mut thread_456(world).messages[1];
    message.label_ids = Some(labels);
    message.payload.headers = ["Date", "Subject", "From", "To"]
        .into_iter()
        .zip([
            "Wed, 13 Sep 2023 22:27:16 +0200".to_string(),
            "Re: test 456".to_string(),
            from,
            "other@example.com".to_string(),
        ])
        .map(|(name, value)| GoogleMailMessageHeader {
            name: name.to_string(),
            value,
        })
        .collect();
}

#[when("I sync my Google Mail notifications")]
async fn i_sync_my_google_mail_notifications(world: &mut ApiWorld) {
    sync_google_mail(world).await;
}

#[then("my notifications are the threads Google Mail reported")]
fn my_notifications_are_the_reported_threads(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let synced = &world.google_mail_sync.synced;
    assert_eq!(synced.len(), 2);
    let threads = &world.google_mail_sync.threads;
    assert_sync_notifications(synced, &threads[0], &threads[1], user_id);
}

#[then("my Google Mail connection learned my address and the labels Google Mail reported")]
async fn my_connection_learned_the_address_and_labels(world: &mut ApiWorld) {
    let id = world
        .integrations
        .google_mail_connection
        .as_ref()
        .unwrap()
        .id;
    let connection = stored_connection(world, id).await;
    assert_eq!(
        connection.provider,
        IntegrationProvider::GoogleMail {
            context: Some(GoogleMailContext {
                user_email_address: user_email_address(),
                labels: google_mail_labels_list()
                    .labels
                    .unwrap_or_default()
                    .into_iter()
                    .map(Into::into)
                    .collect(),
            }),
            config: GoogleMailConfig::enabled()
        }
    );
}

#[then(expr = "{string} ends up {status}")]
async fn a_notification_ends_up(world: &mut ApiWorld, name: String, status: UserStatus) {
    let original = world.notifications.named(&name).clone();
    let notification = read_notification(world, &name).await;
    assert_eq!(notification.id, original.id);
    assert_eq!(notification.kind, NotificationSourceKind::GoogleMail);
    assert_eq!(notification.status, status.0);
}

#[then(expr = "I get one notification of the Google Mail thread 456, that ends up {status}")]
async fn i_get_one_notification_that_ends_up(world: &mut ApiWorld, status: UserStatus) {
    assert_eq!(world.google_mail_sync.synced.len(), 1);
    let id = world.google_mail_sync.synced[0].id;
    let connection_id = world
        .integrations
        .google_mail_connection
        .as_ref()
        .unwrap()
        .id;
    let client = world.client();
    let app = world.app.get().await;
    let notification: Box<NotificationWithTask> =
        get_resource(&client, &app.api_address, "notifications", id.into()).await;
    assert_eq!(notification.kind, NotificationSourceKind::GoogleMail);
    assert_eq!(notification.status, status.0);
    assert_eq!(
        notification.source_item.integration_connection_id,
        connection_id
    );
}

#[then(expr = "the stored thread of {string} has two messages, the first one without any label")]
async fn the_stored_thread_has_two_messages(world: &mut ApiWorld, name: String) {
    let thread = stored_thread(world, &name).await;
    assert_eq!(thread.messages.len(), 2);
    assert_eq!(thread.messages[0].label_ids, None);
}

#[then(expr = "the second message of {string} keeps the labels {string}")]
async fn the_second_message_keeps_the_labels(world: &mut ApiWorld, name: String, labels: String) {
    let synced_label = GoogleMailConfig::enabled().synced_label.id;
    let expected: Vec<String> = labels
        .split(", ")
        .map(|label| match label {
            "INBOX" => GOOGLE_MAIL_INBOX_LABEL.to_string(),
            "UNREAD" => GOOGLE_MAIL_UNREAD_LABEL.to_string(),
            "the synced label" => synced_label.clone(),
            other => other.to_string(),
        })
        .collect();
    let thread = stored_thread(world, &name).await;
    assert_eq!(thread.messages[1].label_ids, Some(expected));
}

async fn stored_thread(world: &mut ApiWorld, name: &str) -> GoogleMailThread {
    let notification = read_notification(world, name).await;
    let ThirdPartyItemData::GoogleMailThread(thread) = notification.source_item.data else {
        panic!("Unexpected source item data");
    };
    *thread
}

async fn requests_to(world: &mut ApiWorld, is_wanted: impl Fn(&str) -> bool) -> usize {
    let app = world.app.get().await;
    app.google_mail_mock_server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|request| is_wanted(request.url.path()))
        .count()
}

#[then(expr = "the number of archive requests Google Mail received is {int}")]
async fn google_mail_received_archive_requests(world: &mut ApiWorld, expected: usize) {
    assert_eq!(
        requests_to(world, |path| path.ends_with("/modify")).await,
        expected
    );
}

#[then(expr = "the number of profile requests Google Mail received is {int}")]
async fn google_mail_received_profile_requests(world: &mut ApiWorld, expected: usize) {
    assert_eq!(
        requests_to(world, |path| path == "/users/me/profile").await,
        expected
    );
}

#[then(expr = "my Google Mail connection has the address {string}")]
async fn my_connection_has_the_address(world: &mut ApiWorld, address: String) {
    let id = world
        .integrations
        .google_mail_connection
        .as_ref()
        .unwrap()
        .id;
    let connection: IntegrationConnection = stored_connection(world, id).await;
    let IntegrationProvider::GoogleMail {
        context: Some(GoogleMailContext {
            user_email_address, ..
        }),
        ..
    } = connection.provider
    else {
        panic!("The Google Mail connection must have a context");
    };
    assert_eq!(
        user_email_address,
        Pii::<EmailAddress>::from_str(&address).unwrap()
    );
}

#[then("my sync returned no notification")]
fn my_sync_returned_no_notification(world: &mut ApiWorld) {
    assert!(world.google_mail_sync.synced.is_empty());
}
