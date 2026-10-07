#![allow(clippy::too_many_arguments)]

use std::str::FromStr;

use chrono::{TimeDelta, TimeZone, Utc};
use email_address::EmailAddress;
use pretty_assertions::assert_eq;
use rrule::Frequency;
use rstest::*;
use serde_json::json;
use universal_inbox::pii::Pii;
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{method, path},
};

use universal_inbox::{
    integration_connection::{
        IntegrationConnection,
        config::IntegrationConnectionConfig,
        integrations::{
            google_calendar::GoogleCalendarConfig,
            google_mail::{GoogleMailConfig, GoogleMailContext},
            todoist::TodoistConfig,
        },
        provider::{IntegrationConnectionContext, IntegrationProvider},
    },
    notification::{
        Notification, NotificationSourceKind, NotificationStatus, NotificationWithTask,
        service::NotificationPatch,
    },
    third_party::{
        integrations::{
            google_calendar::{
                EventAttendee, EventMethod, GoogleCalendarEvent,
                GoogleCalendarEventAttendeeResponseStatus, GoogleCalendarEventStatus,
            },
            google_mail::{
                GOOGLE_MAIL_INBOX_LABEL, GOOGLE_MAIL_UNREAD_LABEL, GoogleMailMessageBody,
                GoogleMailMessageHeader, GoogleMailThread,
            },
            todoist::TodoistItem,
        },
        item::ThirdPartyItemData,
    },
};

use universal_inbox_api::{
    configuration::Settings,
    integrations::{
        google_calendar::GoogleCalendarEventsList,
        google_mail::{
            GoogleMailLabelList, GoogleMailThreadList, GoogleMailThreadMinimal,
            GoogleMailUserProfile,
        },
        todoist::TodoistSyncResponse,
    },
};

use crate::helpers::integration_connection::OAuthCredentialFixture;
use crate::helpers::third_party::create_task_third_party_item;
use crate::helpers::{
    auth::{AuthenticatedApp, authenticated_app},
    integration_connection::{
        create_and_mock_integration_connection, get_integration_connection,
        google_calendar_oauth_credential, google_mail_oauth_credential, todoist_oauth_credential,
    },
    load_json_fixture_file,
    notification::{
        google_calendar::{
            create_notification_from_google_calendar_event, google_calendar_event,
            google_calendar_event_reply, google_calendar_events_list,
            google_calendar_events_list_reply, mock_google_calendar_list_events_page_service,
            mock_google_calendar_list_events_service,
        },
        google_mail::{
            assert_sync_notifications, create_notification_from_google_mail_thread,
            google_mail_invitation_attachment, google_mail_invitation_reply_attachment,
            google_mail_labels_list, google_mail_thread_get_123, google_mail_thread_get_456,
            google_mail_thread_with_invitation, google_mail_thread_with_invitation_reply,
            google_mail_user_profile, mock_google_mail_get_attachment_service,
            mock_google_mail_get_user_profile_service, mock_google_mail_labels_list_service,
            mock_google_mail_thread_get_service, mock_google_mail_thread_modify_service,
            mock_google_mail_threads_list_service,
        },
        sync_notifications, update_notification,
    },
    rest::get_resource,
    settings,
    task::todoist::{
        mock_todoist_sync_resources_service, sync_todoist_projects_response, todoist_item,
    },
};

#[rstest]
#[tokio::test]
async fn test_sync_notifications_should_add_new_notification_and_update_existing_one(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    google_mail_thread_get_123: GoogleMailThread,
    google_mail_thread_get_456: GoogleMailThread,
    google_mail_user_profile: GoogleMailUserProfile,
    google_mail_labels_list: GoogleMailLabelList,
    todoist_item: Box<TodoistItem>,
    sync_todoist_projects_response: TodoistSyncResponse,
    google_mail_oauth_credential: OAuthCredentialFixture,
    todoist_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    let user_email_address =
        Pii::<EmailAddress>::from_str(&google_mail_user_profile.email_address).unwrap();
    let google_mail_threads_list = GoogleMailThreadList {
        threads: Some(vec![
            GoogleMailThreadMinimal {
                id: google_mail_thread_get_123.id.clone(),
                snippet: google_mail_thread_get_123.messages[0].snippet.clone(),
                history_id: google_mail_thread_get_123.history_id.clone(),
            },
            GoogleMailThreadMinimal {
                id: google_mail_thread_get_456.id.clone(),
                snippet: google_mail_thread_get_456.messages[0].snippet.clone(),
                history_id: google_mail_thread_get_456.history_id.clone(),
            },
        ]),
        result_size_estimate: 1,
        next_page_token: Some("next_token".to_string()),
    };

    let _todoist_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings,
        todoist_oauth_credential,
        None,
        None,
    )
    .await;
    mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
        "projects",
        &sync_todoist_projects_response,
        None,
    )
    .await;

    let creation = create_task_third_party_item(
        &app.app,
        ThirdPartyItemData::TodoistItem(Box::new(TodoistItem {
            project_id: "2222".to_string(), // ie. "Project2"
            added_at: Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
            ..*todoist_item.clone()
        })),
        app.user.id,
    )
    .await;
    let existing_todoist_task = creation.task.as_ref().unwrap();

    let google_mail_config = GoogleMailConfig::enabled();
    let google_mail_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::GoogleMail(google_mail_config.clone()),
        &settings,
        google_mail_oauth_credential,
        None,
        None,
    )
    .await;
    let existing_notification = create_notification_from_google_mail_thread(
        &app.app,
        &google_mail_thread_get_456,
        app.user.id,
        google_mail_integration_connection.id,
    )
    .await;
    let existing_notification = update_notification(
        &app,
        existing_notification.id,
        &NotificationPatch {
            task_id: Some(existing_todoist_task.id),
            snoozed_until: Some(Utc.with_ymd_and_hms(2064, 1, 1, 0, 0, 0).unwrap()),
            ..NotificationPatch::default()
        },
        app.user.id,
    )
    .await;

    let _google_mail_get_user_profile_mock = mock_google_mail_get_user_profile_service(
        &app.app.google_mail_mock_server,
        &google_mail_user_profile,
    )
    .await;
    let _google_mail_labels_list_mock = mock_google_mail_labels_list_service(
        &app.app.google_mail_mock_server,
        &google_mail_labels_list,
    )
    .await;
    let _google_mail_threads_list_mock = mock_google_mail_threads_list_service(
        &app.app.google_mail_mock_server,
        None,
        settings
            .integrations
            .get("google_mail")
            .unwrap()
            .page_size
            .unwrap(),
        Some(vec![google_mail_config.synced_label.id.clone()]),
        &google_mail_threads_list,
    )
    .await;
    let empty_result = GoogleMailThreadList {
        threads: None,
        result_size_estimate: 1,
        next_page_token: None,
    };
    let _google_mail_threads_list_mock2 = mock_google_mail_threads_list_service(
        &app.app.google_mail_mock_server,
        Some("next_token"),
        settings
            .integrations
            .get("google_mail")
            .unwrap()
            .page_size
            .unwrap(),
        Some(vec![google_mail_config.synced_label.id.clone()]),
        &empty_result,
    )
    .await;
    let raw_google_mail_thread_get_123 = google_mail_thread_get_123.clone().into();
    let _google_mail_thread_get_123_mock = mock_google_mail_thread_get_service(
        &app.app.google_mail_mock_server,
        "123",
        &raw_google_mail_thread_get_123,
    )
    .await;
    let raw_google_mail_thread_get_456 = google_mail_thread_get_456.clone().into();
    let _google_mail_thread_get_456_mock = mock_google_mail_thread_get_service(
        &app.app.google_mail_mock_server,
        "456",
        &raw_google_mail_thread_get_456,
    )
    .await;

    let notifications: Vec<Notification> = sync_notifications(
        &app.client,
        &app.app.api_address,
        Some(NotificationSourceKind::GoogleMail),
        false,
    )
    .await;

    assert_eq!(notifications.len(), 2);
    assert_sync_notifications(
        &notifications,
        &google_mail_thread_get_123,
        &google_mail_thread_get_456,
        app.user.id,
    );

    let updated_notification: Box<NotificationWithTask> = get_resource(
        &app.client,
        &app.app.api_address,
        "notifications",
        existing_notification.id.into(),
    )
    .await;
    assert_eq!(updated_notification.id, existing_notification.id);
    assert_eq!(
        updated_notification.kind,
        NotificationSourceKind::GoogleMail
    );
    assert_eq!(
        updated_notification.source_item.source_id,
        existing_notification.source_item.source_id
    );
    assert_eq!(updated_notification.status, NotificationStatus::Read);
    assert_eq!(
        updated_notification.last_read_at,
        Some(Utc.with_ymd_and_hms(2023, 9, 13, 20, 27, 16).unwrap())
    );
    assert_eq!(
        updated_notification.source_item.data,
        ThirdPartyItemData::GoogleMailThread(Box::new(google_mail_thread_get_456))
    );
    // `snoozed_until` and `task_id` should not be reset
    assert_eq!(
        updated_notification.snoozed_until,
        Some(Utc.with_ymd_and_hms(2064, 1, 1, 0, 0, 0).unwrap())
    );
    assert_eq!(
        updated_notification.task.as_ref().map(|t| t.id),
        Some(existing_todoist_task.id)
    );

    let updated_integration_connection =
        get_integration_connection(&app, google_mail_integration_connection.id)
            .await
            .unwrap();
    assert_eq!(
        updated_integration_connection,
        IntegrationConnection {
            provider: IntegrationProvider::GoogleMail {
                context: Some(GoogleMailContext {
                    user_email_address,
                    labels: google_mail_labels_list
                        .labels
                        .unwrap_or_default()
                        .into_iter()
                        .map(|label| label.into())
                        .collect(),
                }),
                config: GoogleMailConfig::enabled()
            },
            ..updated_integration_connection.clone()
        }
    );
}

#[rstest]
#[case(false, true, NotificationStatus::Unsubscribed)]
#[case(false, false, NotificationStatus::Unsubscribed)]
#[case(true, true, NotificationStatus::Unread)]
#[case(true, false, NotificationStatus::Unsubscribed)]
#[tokio::test]
async fn test_sync_notifications_of_unsubscribed_notification_with_new_messages(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    mut google_mail_thread_get_456: GoogleMailThread,
    google_mail_user_profile: GoogleMailUserProfile,
    google_mail_labels_list: GoogleMailLabelList,
    google_mail_oauth_credential: OAuthCredentialFixture,
    #[case] has_new_message_addressed_directly: bool,
    #[case] has_new_unread_message: bool,
    #[case] expected_notification_status_after_sync: NotificationStatus,
) {
    // When a Universal Inbox notification is marked as unsubscribed from a Google Mail thread with new
    // messages and none of these new messages are directly addressed to the user (ie. the user's email
    // is not in the `To` header), the status of the notification remains unchanged.
    // In that case all new messages are automatically archived (ie. the `INBOX` label and the label to
    // sync are removed)
    //
    // If there is at least one new message with the user's email address in the `To` header, the
    // notification's status will be updated either to:
    // - `Unread` if one of the new messages have the UNREAD label
    // - `Unsubscribed` otherwise
    let app = authenticated_app.await;
    let google_mail_config = GoogleMailConfig::enabled();
    let synced_label_id = google_mail_config.synced_label.id.clone();
    let user_email_address =
        Pii::<EmailAddress>::from_str(&google_mail_user_profile.email_address).unwrap();

    // First message is already known by Universal Inbox and marked as unsubscribed
    google_mail_thread_get_456.messages[0].label_ids = None; // Read & archived
    // Second message is new (ie. in INBOX)
    google_mail_thread_get_456.messages[1].label_ids = Some(if has_new_unread_message {
        vec![
            "TEST_LABEL".to_string(),
            GOOGLE_MAIL_INBOX_LABEL.to_string(),
            synced_label_id.clone(),
            GOOGLE_MAIL_UNREAD_LABEL.to_string(),
        ]
    } else {
        vec![
            "TEST_LABEL".to_string(),
            GOOGLE_MAIL_INBOX_LABEL.to_string(),
            synced_label_id.clone(),
        ]
    });

    google_mail_thread_get_456.messages[1].payload.headers = vec![GoogleMailMessageHeader {
        name: "To".to_string(),
        value: if has_new_message_addressed_directly {
            format!(
                "other@example.com, You <{}>, test@example.com",
                user_email_address.expose()
            )
        } else {
            "other@example.com".to_string()
        },
    }];

    let google_mail_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::GoogleMail(google_mail_config.clone()),
        &settings,
        google_mail_oauth_credential,
        None,
        None,
    )
    .await;
    let existing_notification = create_notification_from_google_mail_thread(
        &app.app,
        &google_mail_thread_get_456,
        app.user.id,
        google_mail_integration_connection.id,
    )
    .await;
    update_notification(
        &app,
        existing_notification.id,
        &NotificationPatch {
            status: Some(NotificationStatus::Unsubscribed),
            ..NotificationPatch::default()
        },
        app.user.id,
    )
    .await;

    let google_mail_threads_list = GoogleMailThreadList {
        threads: Some(vec![GoogleMailThreadMinimal {
            id: google_mail_thread_get_456.id.clone(),
            snippet: google_mail_thread_get_456.messages[0].snippet.clone(),
            history_id: google_mail_thread_get_456.history_id.clone(),
        }]),
        result_size_estimate: 1,
        next_page_token: None,
    };

    let _google_mail_get_user_profile_mock = mock_google_mail_get_user_profile_service(
        &app.app.google_mail_mock_server,
        &google_mail_user_profile,
    )
    .await;
    let _google_mail_labels_list_mock = mock_google_mail_labels_list_service(
        &app.app.google_mail_mock_server,
        &google_mail_labels_list,
    )
    .await;
    let _google_mail_threads_list_mock = mock_google_mail_threads_list_service(
        &app.app.google_mail_mock_server,
        None,
        settings
            .integrations
            .get("google_mail")
            .unwrap()
            .page_size
            .unwrap(),
        Some(vec![synced_label_id.clone()]),
        &google_mail_threads_list,
    )
    .await;
    let raw_google_mail_thread_get_456 = google_mail_thread_get_456.clone().into();
    let _google_mail_thread_get_456_mock = mock_google_mail_thread_get_service(
        &app.app.google_mail_mock_server,
        "456",
        &raw_google_mail_thread_get_456,
    )
    .await;
    if expected_notification_status_after_sync == NotificationStatus::Unsubscribed {
        mock_google_mail_thread_modify_service(
            &app.app.google_mail_mock_server,
            &google_mail_thread_get_456.id,
            vec![],
            vec![GOOGLE_MAIL_INBOX_LABEL, &synced_label_id],
        )
        .await;
    }

    let notifications: Vec<Notification> = sync_notifications(
        &app.client,
        &app.app.api_address,
        Some(NotificationSourceKind::GoogleMail),
        false,
    )
    .await;

    assert_eq!(notifications.len(), 1);

    let updated_notification: Box<NotificationWithTask> = get_resource(
        &app.client,
        &app.app.api_address,
        "notifications",
        existing_notification.id.into(),
    )
    .await;
    assert_eq!(updated_notification.id, existing_notification.id);
    assert_eq!(
        updated_notification.status,
        expected_notification_status_after_sync
    );
    let ThirdPartyItemData::GoogleMailThread(thread) = &updated_notification.source_item.data
    else {
        unreachable!(
            "Unexpected source item data {:?}",
            updated_notification.source_item.data
        );
    };
    assert_eq!(thread.messages.len(), 2);
    assert_eq!(thread.messages[0].label_ids, None);

    let mut expected_labels = vec!["TEST_LABEL".to_string()];
    if has_new_unread_message {
        if has_new_message_addressed_directly {
            expected_labels.push(GOOGLE_MAIL_INBOX_LABEL.to_string());
            expected_labels.push(synced_label_id.clone());
        }
        expected_labels.push(GOOGLE_MAIL_UNREAD_LABEL.to_string());
    }

    assert_eq!(thread.messages[1].label_ids, Some(expected_labels));
}

#[rstest]
#[tokio::test]
async fn test_sync_notifications_should_create_a_new_google_calendar_notification_from_a_google_mail_invitation(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    google_mail_thread_with_invitation: GoogleMailThread,
    google_mail_user_profile: GoogleMailUserProfile,
    google_mail_labels_list: GoogleMailLabelList,
    google_mail_invitation_attachment: GoogleMailMessageBody,
    _google_calendar_event: GoogleCalendarEvent,
    google_calendar_events_list: GoogleCalendarEventsList,
    google_mail_oauth_credential: OAuthCredentialFixture,
    google_calendar_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    let google_mail_threads_list = GoogleMailThreadList {
        threads: Some(vec![GoogleMailThreadMinimal {
            id: google_mail_thread_with_invitation.id.clone(),
            snippet: google_mail_thread_with_invitation.messages[0]
                .snippet
                .clone(),
            history_id: google_mail_thread_with_invitation.history_id.clone(),
        }]),
        result_size_estimate: 1,
        next_page_token: Some("next_token".to_string()),
    };

    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::GoogleCalendar(GoogleCalendarConfig::enabled()),
        &settings,
        google_calendar_oauth_credential,
        None,
        None,
    )
    .await;

    let google_mail_config = GoogleMailConfig::enabled();
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::GoogleMail(google_mail_config.clone()),
        &settings,
        google_mail_oauth_credential,
        None,
        None,
    )
    .await;

    let _google_mail_get_user_profile_mock = mock_google_mail_get_user_profile_service(
        &app.app.google_mail_mock_server,
        &google_mail_user_profile,
    )
    .await;
    let _google_mail_labels_list_mock = mock_google_mail_labels_list_service(
        &app.app.google_mail_mock_server,
        &google_mail_labels_list,
    )
    .await;
    let _google_mail_threads_list_mock = mock_google_mail_threads_list_service(
        &app.app.google_mail_mock_server,
        None,
        settings
            .integrations
            .get("google_mail")
            .unwrap()
            .page_size
            .unwrap(),
        Some(vec![google_mail_config.synced_label.id.clone()]),
        &google_mail_threads_list,
    )
    .await;
    let empty_result = GoogleMailThreadList {
        threads: None,
        result_size_estimate: 1,
        next_page_token: None,
    };
    mock_google_mail_threads_list_service(
        &app.app.google_mail_mock_server,
        Some("next_token"),
        settings
            .integrations
            .get("google_mail")
            .unwrap()
            .page_size
            .unwrap(),
        Some(vec![google_mail_config.synced_label.id.clone()]),
        &empty_result,
    )
    .await;

    let raw_google_mail_thread_with_invitation = google_mail_thread_with_invitation.clone().into();
    let _google_mail_thread_with_invitation_mock = mock_google_mail_thread_get_service(
        &app.app.google_mail_mock_server,
        "789",
        &raw_google_mail_thread_with_invitation,
    )
    .await;
    let _google_mail_get_attachment_mock = mock_google_mail_get_attachment_service(
        &app.app.google_mail_mock_server,
        "789",
        "attachmentid1", // Found in google_mail_thread_with_invitation
        &google_mail_invitation_attachment,
    )
    .await;
    let _google_calendar_list_events_mock = mock_google_calendar_list_events_service(
        &app.app.google_calendar_mock_server,
        "event_icaluid1", // Found in the ical attachment in google_mail_invitation_attachment
        &google_calendar_events_list,
    )
    .await;

    let notifications: Vec<Notification> = sync_notifications(
        &app.client,
        &app.app.api_address,
        Some(NotificationSourceKind::GoogleMail),
        false,
    )
    .await;

    assert_eq!(notifications.len(), 1);
    assert_eq!(
        notifications[0].kind,
        NotificationSourceKind::GoogleCalendar
    );

    let new_notification: Box<NotificationWithTask> = get_resource(
        &app.client,
        &app.app.api_address,
        "notifications",
        notifications[0].id.into(),
    )
    .await;
    assert_eq!(new_notification.id, notifications[0].id);
    assert_eq!(
        new_notification.kind,
        NotificationSourceKind::GoogleCalendar
    );
    assert_eq!(new_notification.status, NotificationStatus::Unread);
    assert!(new_notification.last_read_at.is_none());

    let gcal_third_party_item = &new_notification.source_item;
    assert_eq!(new_notification.source_item.source_id, "eventid1");
    // Verify that the recurrence field is properly parsed from the fixture
    let ThirdPartyItemData::GoogleCalendarEvent(ref event) = gcal_third_party_item.data else {
        panic!("Expected GoogleCalendarEvent");
    };

    // Verify basic event properties
    assert_eq!(event.id.to_string(), "eventid1");
    assert_eq!(event.summary, "Weekly meeting");

    // Check if recurrence is present and parsed correctly
    let Some(ref rrule_set) = event.recurrence else {
        unreachable!("No recurrence found in event");
    };
    assert!(
        !rrule_set.get_rrule().is_empty(),
        "Expected at least one RRULE if recurrence is present"
    );

    // Verify the specific recurrence rule from our fixture: FREQ=WEEKLY;BYDAY=FR
    let rrule = &rrule_set.get_rrule()[0];
    assert_eq!(rrule.get_freq(), Frequency::Weekly);
    // Note: Testing exact by-day parsing would require more complex assertions

    let gmail_third_party_item = gcal_third_party_item.source_item.as_ref().unwrap();
    assert_eq!(
        gmail_third_party_item.data,
        ThirdPartyItemData::GoogleMailThread(Box::new(google_mail_thread_with_invitation))
    );
}

#[rstest]
#[tokio::test]
async fn test_sync_notifications_should_update_a_google_calendar_notification_from_a_google_mail_invitation_update(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    google_mail_thread_with_invitation: GoogleMailThread,
    google_mail_user_profile: GoogleMailUserProfile,
    google_mail_labels_list: GoogleMailLabelList,
    google_mail_invitation_attachment: GoogleMailMessageBody,
    google_calendar_event: GoogleCalendarEvent,
    google_calendar_events_list: GoogleCalendarEventsList,
    google_mail_oauth_credential: OAuthCredentialFixture,
    google_calendar_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    let google_mail_threads_list = GoogleMailThreadList {
        threads: Some(vec![GoogleMailThreadMinimal {
            id: google_mail_thread_with_invitation.id.clone(),
            snippet: google_mail_thread_with_invitation.messages[0]
                .snippet
                .clone(),
            history_id: google_mail_thread_with_invitation.history_id.clone(),
        }]),
        result_size_estimate: 1,
        next_page_token: Some("next_token".to_string()),
    };

    let google_calendar_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::GoogleCalendar(GoogleCalendarConfig::enabled()),
        &settings,
        google_calendar_oauth_credential,
        None,
        None,
    )
    .await;

    let google_mail_config = GoogleMailConfig::enabled();
    let google_mail_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::GoogleMail(google_mail_config.clone()),
        &settings,
        google_mail_oauth_credential,
        None,
        None,
    )
    .await;

    let first_google_mail_thread_with_invitation = GoogleMailThread {
        id: "anything".to_string(),
        ..google_mail_thread_with_invitation.clone()
    };
    let existing_notification = create_notification_from_google_calendar_event(
        &app.app,
        &first_google_mail_thread_with_invitation,
        &google_calendar_event,
        app.user.id,
        google_mail_integration_connection.id,
        google_calendar_integration_connection.id,
    )
    .await;
    let existing_gcal_third_party_item = &existing_notification.source_item;
    let existing_gmail_third_party_item =
        existing_gcal_third_party_item.source_item.as_ref().unwrap();

    let _google_mail_get_user_profile_mock = mock_google_mail_get_user_profile_service(
        &app.app.google_mail_mock_server,
        &google_mail_user_profile,
    )
    .await;
    let _google_mail_labels_list_mock = mock_google_mail_labels_list_service(
        &app.app.google_mail_mock_server,
        &google_mail_labels_list,
    )
    .await;
    let _google_mail_threads_list_mock = mock_google_mail_threads_list_service(
        &app.app.google_mail_mock_server,
        None,
        settings
            .integrations
            .get("google_mail")
            .unwrap()
            .page_size
            .unwrap(),
        Some(vec![google_mail_config.synced_label.id.clone()]),
        &google_mail_threads_list,
    )
    .await;
    let empty_result = GoogleMailThreadList {
        threads: None,
        result_size_estimate: 1,
        next_page_token: None,
    };
    mock_google_mail_threads_list_service(
        &app.app.google_mail_mock_server,
        Some("next_token"),
        settings
            .integrations
            .get("google_mail")
            .unwrap()
            .page_size
            .unwrap(),
        Some(vec![google_mail_config.synced_label.id.clone()]),
        &empty_result,
    )
    .await;

    let raw_google_mail_thread_with_invitation = google_mail_thread_with_invitation.clone().into();
    let _google_mail_thread_with_invitation_mock = mock_google_mail_thread_get_service(
        &app.app.google_mail_mock_server,
        "789",
        &raw_google_mail_thread_with_invitation,
    )
    .await;
    let _google_mail_get_attachment_mock = mock_google_mail_get_attachment_service(
        &app.app.google_mail_mock_server,
        "789",
        "attachmentid1", // Found in google_mail_thread_with_invitation
        &google_mail_invitation_attachment,
    )
    .await;
    let _google_calendar_list_events_mock = mock_google_calendar_list_events_service(
        &app.app.google_calendar_mock_server,
        "event_icaluid1", // Found in the ical attachment in google_mail_invitation_attachment
        &google_calendar_events_list,
    )
    .await;

    let notifications: Vec<Notification> = sync_notifications(
        &app.client,
        &app.app.api_address,
        Some(NotificationSourceKind::GoogleMail),
        false,
    )
    .await;

    assert_eq!(notifications.len(), 1);
    assert_eq!(
        notifications[0].kind,
        NotificationSourceKind::GoogleCalendar
    );
    assert_eq!(notifications[0].id, existing_notification.id);

    let new_notification: Box<NotificationWithTask> = get_resource(
        &app.client,
        &app.app.api_address,
        "notifications",
        notifications[0].id.into(),
    )
    .await;
    assert_eq!(new_notification.id, existing_notification.id);
    assert_eq!(new_notification.status, NotificationStatus::Unread);

    let gcal_third_party_item = &new_notification.source_item;
    // The source Google Calendar event should be the same as the existing one
    assert_eq!(gcal_third_party_item.id, existing_gcal_third_party_item.id);
    assert_eq!(new_notification.source_item.source_id, "eventid1");

    // Verify that the recurrence field is properly parsed in the updated notification
    let ThirdPartyItemData::GoogleCalendarEvent(ref event) = gcal_third_party_item.data else {
        panic!("Expected GoogleCalendarEvent");
    };

    // Verify basic event properties
    assert_eq!(event.id.to_string(), "eventid1");
    assert_eq!(event.summary, "Weekly meeting");

    // Check if recurrence is present and parsed correctly
    if let Some(ref rrule_set) = event.recurrence {
        assert!(
            !rrule_set.get_rrule().is_empty(),
            "Expected at least one RRULE if recurrence is present"
        );

        // Verify the specific recurrence rule from our fixture: FREQ=WEEKLY;BYDAY=FR
        let rrule = &rrule_set.get_rrule()[0];
        assert_eq!(rrule.get_freq(), Frequency::Weekly);
    }

    let gmail_third_party_item = gcal_third_party_item.source_item.as_ref().unwrap();
    // The source Google mail thread should be a new one, not the existing one
    assert_ne!(
        gmail_third_party_item.id,
        existing_gmail_third_party_item.id
    );
    assert_eq!(
        gmail_third_party_item.data,
        ThirdPartyItemData::GoogleMailThread(Box::new(google_mail_thread_with_invitation))
    );
}

#[rstest]
#[tokio::test]
async fn test_sync_notifications_should_create_a_new_google_calendar_notification_from_a_google_mail_invitation_reply(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    google_mail_thread_with_invitation_reply: GoogleMailThread,
    google_mail_user_profile: GoogleMailUserProfile,
    google_mail_labels_list: GoogleMailLabelList,
    google_mail_invitation_reply_attachment: GoogleMailMessageBody,
    google_calendar_event_reply: GoogleCalendarEvent,
    google_calendar_events_list_reply: GoogleCalendarEventsList,
    google_mail_oauth_credential: OAuthCredentialFixture,
    google_calendar_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    let google_mail_threads_list = GoogleMailThreadList {
        threads: Some(vec![GoogleMailThreadMinimal {
            id: google_mail_thread_with_invitation_reply.id.clone(),
            snippet: google_mail_thread_with_invitation_reply.messages[0]
                .snippet
                .clone(),
            history_id: google_mail_thread_with_invitation_reply.history_id.clone(),
        }]),
        result_size_estimate: 1,
        next_page_token: Some("next_token".to_string()),
    };

    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::GoogleCalendar(GoogleCalendarConfig::enabled()),
        &settings,
        google_calendar_oauth_credential,
        None,
        None,
    )
    .await;

    let google_mail_config = GoogleMailConfig::enabled();
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::GoogleMail(google_mail_config.clone()),
        &settings,
        google_mail_oauth_credential,
        None,
        None,
    )
    .await;

    let _google_mail_get_user_profile_mock = mock_google_mail_get_user_profile_service(
        &app.app.google_mail_mock_server,
        &google_mail_user_profile,
    )
    .await;
    let _google_mail_labels_list_mock = mock_google_mail_labels_list_service(
        &app.app.google_mail_mock_server,
        &google_mail_labels_list,
    )
    .await;
    let _google_mail_threads_list_mock = mock_google_mail_threads_list_service(
        &app.app.google_mail_mock_server,
        None,
        settings
            .integrations
            .get("google_mail")
            .unwrap()
            .page_size
            .unwrap(),
        Some(vec![google_mail_config.synced_label.id.clone()]),
        &google_mail_threads_list,
    )
    .await;
    let empty_result = GoogleMailThreadList {
        threads: None,
        result_size_estimate: 1,
        next_page_token: None,
    };
    mock_google_mail_threads_list_service(
        &app.app.google_mail_mock_server,
        Some("next_token"),
        settings
            .integrations
            .get("google_mail")
            .unwrap()
            .page_size
            .unwrap(),
        Some(vec![google_mail_config.synced_label.id.clone()]),
        &empty_result,
    )
    .await;

    let raw_google_mail_thread_with_invitation_reply =
        google_mail_thread_with_invitation_reply.clone().into();
    let _google_mail_thread_with_invitation_reply_mock = mock_google_mail_thread_get_service(
        &app.app.google_mail_mock_server,
        "890",
        &raw_google_mail_thread_with_invitation_reply,
    )
    .await;
    let _google_mail_get_attachment_mock = mock_google_mail_get_attachment_service(
        &app.app.google_mail_mock_server,
        "890",
        "attachmentid2", // Found in google_mail_thread_with_invitation_reply
        &google_mail_invitation_reply_attachment,
    )
    .await;
    let _google_calendar_list_events_mock = mock_google_calendar_list_events_service(
        &app.app.google_calendar_mock_server,
        "event_icaluid2", // Found in the ical attachment in google_mail_invitation_reply_attachment
        &google_calendar_events_list_reply,
    )
    .await;

    let notifications: Vec<Notification> = sync_notifications(
        &app.client,
        &app.app.api_address,
        Some(NotificationSourceKind::GoogleMail),
        false,
    )
    .await;

    assert_eq!(notifications.len(), 1);
    assert_eq!(
        notifications[0].kind,
        NotificationSourceKind::GoogleCalendar
    );

    let new_notification: Box<NotificationWithTask> = get_resource(
        &app.client,
        &app.app.api_address,
        "notifications",
        notifications[0].id.into(),
    )
    .await;
    assert_eq!(new_notification.id, notifications[0].id);
    assert_eq!(
        new_notification.kind,
        NotificationSourceKind::GoogleCalendar
    );
    assert_eq!(new_notification.status, NotificationStatus::Unread);
    assert!(new_notification.last_read_at.is_none());

    let gcal_third_party_item = &new_notification.source_item;
    assert_eq!(new_notification.source_item.source_id, "eventid2");
    assert_eq!(
        gcal_third_party_item.data,
        ThirdPartyItemData::GoogleCalendarEvent(Box::new(google_calendar_event_reply))
    );

    // Verify the REPLY method is correctly extracted and stored
    let ThirdPartyItemData::GoogleCalendarEvent(gcal_event) = &gcal_third_party_item.data else {
        unreachable!("Expected GoogleCalendarEvent");
    };
    assert_eq!(
        gcal_event.method,
        universal_inbox::third_party::integrations::google_calendar::EventMethod::Reply
    );

    let gmail_third_party_item = gcal_third_party_item.source_item.as_ref().unwrap();
    assert_eq!(
        gmail_third_party_item.data,
        ThirdPartyItemData::GoogleMailThread(Box::new(google_mail_thread_with_invitation_reply))
    );
}

#[rstest]
#[case(true, NotificationStatus::Deleted)]
#[case(false, NotificationStatus::Unread)]
#[tokio::test]
async fn test_sync_notifications_should_mark_notification_as_deleted_when_user_replied(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    mut google_mail_thread_get_456: GoogleMailThread,
    google_mail_user_profile: GoogleMailUserProfile,
    google_mail_labels_list: GoogleMailLabelList,
    google_mail_oauth_credential: OAuthCredentialFixture,
    #[case] last_message_from_user: bool,
    #[case] expected_notification_status: NotificationStatus,
) {
    // When a GoogleMailThread has new unread messages and the last message is from the user,
    // the notification should be marked as Deleted (user already responded).
    // Otherwise, it should be marked as Unread.
    let app = authenticated_app.await;
    let google_mail_config = GoogleMailConfig::enabled();
    let synced_label_id = google_mail_config.synced_label.id.clone();
    let user_email_address =
        Pii::<EmailAddress>::from_str(&google_mail_user_profile.email_address).unwrap();

    // Set thread as unread (required for testing the user-replied logic)
    google_mail_thread_get_456.messages[1].label_ids = Some(vec![
        GOOGLE_MAIL_INBOX_LABEL.to_string(),
        synced_label_id.clone(),
        GOOGLE_MAIL_UNREAD_LABEL.to_string(),
    ]);

    // Set the "From" header of the last message based on test case
    google_mail_thread_get_456.messages[1].payload.headers = vec![
        GoogleMailMessageHeader {
            name: "Date".to_string(),
            value: "Wed, 13 Sep 2023 22:27:16 +0200".to_string(),
        },
        GoogleMailMessageHeader {
            name: "Subject".to_string(),
            value: "Re: test 456".to_string(),
        },
        GoogleMailMessageHeader {
            name: "From".to_string(),
            value: if last_message_from_user {
                format!("User Name <{}>", user_email_address.expose())
            } else {
                "External Sender <external@example.com>".to_string()
            },
        },
        GoogleMailMessageHeader {
            name: "To".to_string(),
            value: "other@example.com".to_string(),
        },
    ];

    let google_mail_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::GoogleMail(google_mail_config.clone()),
        &settings,
        google_mail_oauth_credential,
        None,
        None,
    )
    .await;

    let google_mail_threads_list = GoogleMailThreadList {
        threads: Some(vec![GoogleMailThreadMinimal {
            id: google_mail_thread_get_456.id.clone(),
            snippet: google_mail_thread_get_456.messages[0].snippet.clone(),
            history_id: google_mail_thread_get_456.history_id.clone(),
        }]),
        result_size_estimate: 1,
        next_page_token: None,
    };

    let _google_mail_get_user_profile_mock = mock_google_mail_get_user_profile_service(
        &app.app.google_mail_mock_server,
        &google_mail_user_profile,
    )
    .await;
    let _google_mail_labels_list_mock = mock_google_mail_labels_list_service(
        &app.app.google_mail_mock_server,
        &google_mail_labels_list,
    )
    .await;
    let _google_mail_threads_list_mock = mock_google_mail_threads_list_service(
        &app.app.google_mail_mock_server,
        None,
        settings
            .integrations
            .get("google_mail")
            .unwrap()
            .page_size
            .unwrap(),
        Some(vec![synced_label_id.clone()]),
        &google_mail_threads_list,
    )
    .await;
    let raw_google_mail_thread_get_456 = google_mail_thread_get_456.clone().into();
    let _google_mail_thread_get_456_mock = mock_google_mail_thread_get_service(
        &app.app.google_mail_mock_server,
        "456",
        &raw_google_mail_thread_get_456,
    )
    .await;

    let notifications: Vec<Notification> = sync_notifications(
        &app.client,
        &app.app.api_address,
        Some(NotificationSourceKind::GoogleMail),
        false,
    )
    .await;

    assert_eq!(notifications.len(), 1);

    let synced_notification: Box<NotificationWithTask> = get_resource(
        &app.client,
        &app.app.api_address,
        "notifications",
        notifications[0].id.into(),
    )
    .await;
    assert_eq!(synced_notification.kind, NotificationSourceKind::GoogleMail);
    assert_eq!(
        synced_notification.status, expected_notification_status,
        "Expected notification status to be {:?} when last_message_from_user={}, but got {:?}",
        expected_notification_status, last_message_from_user, synced_notification.status
    );
    assert_eq!(
        synced_notification.source_item.integration_connection_id,
        google_mail_integration_connection.id
    );
}

/// The cached user email address follows the Google account the connection
/// is pinned to: it is read again from Gmail once the connection has been
/// reconnected with another account, and reused otherwise.
#[rstest]
#[case::reconnected_with_another_account(Some("user@example.com"), 1)]
#[case::same_account(Some("previous@example.com"), 0)]
#[case::not_pinned_yet(None, 0)]
#[tokio::test]
async fn test_sync_notifications_should_refresh_user_email_address_of_pinned_account(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    google_mail_user_profile: GoogleMailUserProfile,
    google_mail_labels_list: GoogleMailLabelList,
    google_mail_oauth_credential: OAuthCredentialFixture,
    #[case] provider_user_id: Option<&str>,
    #[case] expected_user_profile_calls: u64,
) {
    let app = authenticated_app.await;
    let previous_email_address = Pii::<EmailAddress>::from_str("Previous@example.com").unwrap();
    let google_mail_config = GoogleMailConfig::enabled();
    let google_mail_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::GoogleMail(google_mail_config.clone()),
        &settings,
        OAuthCredentialFixture {
            provider_user_id: provider_user_id.map(|id| id.to_string()),
            ..google_mail_oauth_credential
        },
        None,
        Some(IntegrationConnectionContext::GoogleMail(
            GoogleMailContext {
                user_email_address: previous_email_address.clone(),
                labels: vec![],
            },
        )),
    )
    .await;

    Mock::given(method("GET"))
        .and(path("/users/me/profile"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&google_mail_user_profile))
        .expect(expected_user_profile_calls)
        .mount(&app.app.google_mail_mock_server)
        .await;
    mock_google_mail_labels_list_service(
        &app.app.google_mail_mock_server,
        &google_mail_labels_list,
    )
    .await;
    mock_google_mail_threads_list_service(
        &app.app.google_mail_mock_server,
        None,
        settings
            .integrations
            .get("google_mail")
            .unwrap()
            .page_size
            .unwrap(),
        Some(vec![google_mail_config.synced_label.id.clone()]),
        &GoogleMailThreadList {
            threads: None,
            result_size_estimate: 0,
            next_page_token: None,
        },
    )
    .await;

    let notifications: Vec<Notification> = sync_notifications(
        &app.client,
        &app.app.api_address,
        Some(NotificationSourceKind::GoogleMail),
        false,
    )
    .await;
    assert!(notifications.is_empty());

    let updated_integration_connection =
        get_integration_connection(&app, google_mail_integration_connection.id)
            .await
            .unwrap();
    let expected_email_address = if expected_user_profile_calls > 0 {
        Pii::<EmailAddress>::from_str(&google_mail_user_profile.email_address).unwrap()
    } else {
        previous_email_address
    };
    let IntegrationProvider::GoogleMail {
        context: Some(GoogleMailContext {
            user_email_address, ..
        }),
        ..
    } = updated_integration_connection.provider
    else {
        panic!("Google Mail integration connection must have a context");
    };
    assert_eq!(user_email_address, expected_email_address);
}

/// Mock a Google Mail sync returning `google_mail_thread_with_invitation` with the given
/// calendar attachment
async fn mock_google_mail_invitation_sync(
    app: &AuthenticatedApp,
    settings: &Settings,
    google_mail_config: &GoogleMailConfig,
    google_mail_thread_with_invitation: &GoogleMailThread,
    google_mail_user_profile: &GoogleMailUserProfile,
    google_mail_labels_list: &GoogleMailLabelList,
    invitation_attachment: &GoogleMailMessageBody,
) {
    let page_size = settings
        .integrations
        .get("google_mail")
        .unwrap()
        .page_size
        .unwrap();
    mock_google_mail_get_user_profile_service(
        &app.app.google_mail_mock_server,
        google_mail_user_profile,
    )
    .await;
    mock_google_mail_labels_list_service(&app.app.google_mail_mock_server, google_mail_labels_list)
        .await;
    mock_google_mail_threads_list_service(
        &app.app.google_mail_mock_server,
        None,
        page_size,
        Some(vec![google_mail_config.synced_label.id.clone()]),
        &GoogleMailThreadList {
            threads: Some(vec![GoogleMailThreadMinimal {
                id: google_mail_thread_with_invitation.id.clone(),
                snippet: google_mail_thread_with_invitation.messages[0]
                    .snippet
                    .clone(),
                history_id: google_mail_thread_with_invitation.history_id.clone(),
            }]),
            result_size_estimate: 1,
            next_page_token: Some("next_token".to_string()),
        },
    )
    .await;
    mock_google_mail_threads_list_service(
        &app.app.google_mail_mock_server,
        Some("next_token"),
        page_size,
        Some(vec![google_mail_config.synced_label.id.clone()]),
        &GoogleMailThreadList {
            threads: None,
            result_size_estimate: 1,
            next_page_token: None,
        },
    )
    .await;
    mock_google_mail_thread_get_service(
        &app.app.google_mail_mock_server,
        "789",
        &google_mail_thread_with_invitation.clone().into(),
    )
    .await;
    mock_google_mail_get_attachment_service(
        &app.app.google_mail_mock_server,
        "789",
        "attachmentid1", // Found in google_mail_thread_with_invitation
        invitation_attachment,
    )
    .await;
}

async fn get_synced_google_calendar_notification(app: &AuthenticatedApp) -> NotificationWithTask {
    let notifications: Vec<Notification> = sync_notifications(
        &app.client,
        &app.app.api_address,
        Some(NotificationSourceKind::GoogleMail),
        false,
    )
    .await;

    assert_eq!(notifications.len(), 1);
    assert_eq!(
        notifications[0].kind,
        NotificationSourceKind::GoogleCalendar
    );

    let notification: Box<NotificationWithTask> = get_resource(
        &app.client,
        &app.app.api_address,
        "notifications",
        notifications[0].id.into(),
    )
    .await;
    *notification
}

fn google_calendar_event_from_notification(
    notification: &NotificationWithTask,
) -> &GoogleCalendarEvent {
    let ThirdPartyItemData::GoogleCalendarEvent(ref event) = notification.source_item.data else {
        panic!("Expected GoogleCalendarEvent");
    };
    event
}

#[rstest]
#[tokio::test]
async fn test_sync_notifications_should_find_the_google_calendar_event_of_an_invitation_on_a_later_page(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    google_mail_thread_with_invitation: GoogleMailThread,
    google_mail_user_profile: GoogleMailUserProfile,
    google_mail_labels_list: GoogleMailLabelList,
    google_mail_invitation_attachment: GoogleMailMessageBody,
    google_calendar_events_list: GoogleCalendarEventsList,
    google_mail_oauth_credential: OAuthCredentialFixture,
    google_calendar_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::GoogleCalendar(GoogleCalendarConfig::enabled()),
        &settings,
        google_calendar_oauth_credential,
        None,
        None,
    )
    .await;
    let google_mail_config = GoogleMailConfig::enabled();
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::GoogleMail(google_mail_config.clone()),
        &settings,
        google_mail_oauth_credential,
        None,
        None,
    )
    .await;
    mock_google_mail_invitation_sync(
        &app,
        &settings,
        &google_mail_config,
        &google_mail_thread_with_invitation,
        &google_mail_user_profile,
        &google_mail_labels_list,
        &google_mail_invitation_attachment,
    )
    .await;
    // Google Calendar may return an empty page with a next page token
    mock_google_calendar_list_events_page_service(
        &app.app.google_calendar_mock_server,
        "event_icaluid1",
        None,
        &json!({ "items": [], "nextPageToken": "page2" }),
    )
    .await;
    mock_google_calendar_list_events_page_service(
        &app.app.google_calendar_mock_server,
        "event_icaluid1",
        Some("page2"),
        &google_calendar_events_list,
    )
    .await;

    let notification = get_synced_google_calendar_notification(&app).await;

    assert_eq!(notification.status, NotificationStatus::Unread);
    assert_eq!(notification.source_item.source_id, "eventid1");
}

#[rstest]
#[tokio::test]
async fn test_sync_notifications_should_create_a_google_calendar_notification_for_the_updated_occurrence_of_a_recurring_event(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    google_mail_thread_with_invitation: GoogleMailThread,
    google_mail_user_profile: GoogleMailUserProfile,
    google_mail_labels_list: GoogleMailLabelList,
    google_calendar_event: GoogleCalendarEvent,
    google_mail_oauth_credential: OAuthCredentialFixture,
    google_calendar_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::GoogleCalendar(GoogleCalendarConfig::enabled()),
        &settings,
        google_calendar_oauth_credential,
        None,
        None,
    )
    .await;
    let google_mail_config = GoogleMailConfig::enabled();
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::GoogleMail(google_mail_config.clone()),
        &settings,
        google_mail_oauth_credential,
        None,
        None,
    )
    .await;
    mock_google_mail_invitation_sync(
        &app,
        &settings,
        &google_mail_config,
        &google_mail_thread_with_invitation,
        &google_mail_user_profile,
        &google_mail_labels_list,
        // RECURRENCE-ID;TZID=Europe/Paris:20240510T151500, moved to 16:00
        &load_json_fixture_file("google_mail_invitation_occurrence_update_attachment.json"),
    )
    .await;
    // The iCalUID of a recurring event matches its master event and its modified occurrences
    let mut occurrence = serde_json::to_value(&google_calendar_event).unwrap();
    occurrence["id"] = json!("eventid1_20240510T131500Z");
    occurrence["summary"] = json!("Weekly meeting (moved)");
    occurrence["recurringEventId"] = json!("eventid1");
    occurrence["originalStartTime"] =
        json!({ "dateTime": "2024-05-10T13:15:00Z", "timeZone": "Europe/Paris" });
    occurrence["start"] = json!({ "dateTime": "2024-05-10T14:00:00Z", "timeZone": "Europe/Paris" });
    occurrence["end"] = json!({ "dateTime": "2024-05-10T14:15:00Z", "timeZone": "Europe/Paris" });
    occurrence.as_object_mut().unwrap().remove("recurrence");
    mock_google_calendar_list_events_page_service(
        &app.app.google_calendar_mock_server,
        "event_icaluid1",
        None,
        &json!({ "items": [google_calendar_event, occurrence] }),
    )
    .await;

    let notification = get_synced_google_calendar_notification(&app).await;

    assert_eq!(notification.status, NotificationStatus::Unread);
    assert_eq!(
        notification.source_item.source_id,
        "eventid1_20240510T131500Z"
    );
    let event = google_calendar_event_from_notification(&notification);
    assert_eq!(event.summary, "Weekly meeting (moved)");
    assert!(event.recurrence.is_none());
}

#[rstest]
#[case::cancel_a_pending_invitation(
    Some(GoogleCalendarEventAttendeeResponseStatus::NeedsAction),
    NotificationStatus::Deleted
)]
#[case::cancel_an_accepted_invitation(
    Some(GoogleCalendarEventAttendeeResponseStatus::Accepted),
    NotificationStatus::Unread
)]
#[case::cancel_an_unknown_invitation(None, NotificationStatus::Unread)]
#[tokio::test]
async fn test_sync_notifications_should_handle_a_cancelled_google_calendar_event_from_a_google_mail_invitation(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    google_mail_thread_with_invitation: GoogleMailThread,
    google_mail_user_profile: GoogleMailUserProfile,
    google_mail_labels_list: GoogleMailLabelList,
    google_calendar_event: GoogleCalendarEvent,
    google_mail_oauth_credential: OAuthCredentialFixture,
    google_calendar_oauth_credential: OAuthCredentialFixture,
    #[case] existing_invitation_response: Option<GoogleCalendarEventAttendeeResponseStatus>,
    #[case] expected_status: NotificationStatus,
) {
    let app = authenticated_app.await;
    let google_calendar_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::GoogleCalendar(GoogleCalendarConfig::enabled()),
        &settings,
        google_calendar_oauth_credential,
        None,
        None,
    )
    .await;
    let google_mail_config = GoogleMailConfig::enabled();
    let google_mail_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::GoogleMail(google_mail_config.clone()),
        &settings,
        google_mail_oauth_credential,
        None,
        None,
    )
    .await;

    let self_response_status = existing_invitation_response
        .unwrap_or(GoogleCalendarEventAttendeeResponseStatus::NeedsAction);
    let with_self_response = |event: &GoogleCalendarEvent| GoogleCalendarEvent {
        attendees: event
            .attendees
            .iter()
            .map(|attendee| EventAttendee {
                response_status: if attendee.self_ == Some(true) {
                    self_response_status
                } else {
                    attendee.response_status
                },
                ..attendee.clone()
            })
            .collect(),
        ..event.clone()
    };
    let existing_notification = if existing_invitation_response.is_some() {
        Some(
            create_notification_from_google_calendar_event(
                &app.app,
                &GoogleMailThread {
                    id: "invitation_thread".to_string(),
                    ..google_mail_thread_with_invitation.clone()
                },
                &with_self_response(&google_calendar_event),
                app.user.id,
                google_mail_integration_connection.id,
                google_calendar_integration_connection.id,
            )
            .await,
        )
    } else {
        None
    };

    mock_google_mail_invitation_sync(
        &app,
        &settings,
        &google_mail_config,
        &google_mail_thread_with_invitation,
        &google_mail_user_profile,
        &google_mail_labels_list,
        // METHOD:CANCEL
        &load_json_fixture_file("google_mail_invitation_cancel_attachment.json"),
    )
    .await;
    let cancelled_event = GoogleCalendarEvent {
        status: GoogleCalendarEventStatus::Cancelled,
        // Cancelled after the existing invitation was synced
        updated: Utc::now() + TimeDelta::hours(1),
        ..with_self_response(&google_calendar_event)
    };
    mock_google_calendar_list_events_page_service(
        &app.app.google_calendar_mock_server,
        "event_icaluid1",
        None,
        &json!({ "items": [cancelled_event] }),
    )
    .await;

    let notification = get_synced_google_calendar_notification(&app).await;

    if let Some(existing_notification) = existing_notification {
        assert_eq!(notification.id, existing_notification.id);
    }
    assert_eq!(notification.status, expected_status);
    assert_eq!(notification.source_item.source_id, "eventid1");
    let event = google_calendar_event_from_notification(&notification);
    assert_eq!(event.method, EventMethod::Cancel);
    assert!(event.is_cancelled());
}
