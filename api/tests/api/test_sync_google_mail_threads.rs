#![allow(clippy::too_many_arguments)]

use chrono::{TimeZone, Utc};
use pretty_assertions::assert_eq;
use rstest::*;
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::google_mail::{GoogleMailConfig, GoogleMailContext},
        provider::IntegrationProvider,
        IntegrationConnection,
    },
    notification::{
        integrations::google_mail::{
            EmailAddress, GoogleMailMessageHeader, GoogleMailThread, GOOGLE_MAIL_INBOX_LABEL,
            GOOGLE_MAIL_UNREAD_LABEL,
        },
        Notification, NotificationMetadata, NotificationSourceKind, NotificationStatus,
    },
    task::integrations::todoist::TodoistItem,
};

use universal_inbox_api::{
    configuration::Settings,
    integrations::{
        google_mail::{
            GoogleMailLabelList, GoogleMailThreadList, GoogleMailThreadMinimal,
            GoogleMailUserProfile,
        },
        oauth2::NangoConnection,
    },
};

use crate::helpers::{
    auth::{authenticated_app, AuthenticatedApp},
    integration_connection::{
        create_and_mock_integration_connection, get_integration_connection,
        nango_google_mail_connection,
    },
    notification::{
        google_mail::{
            assert_sync_notifications, google_mail_labels_list, google_mail_thread_get_123,
            google_mail_thread_get_456, google_mail_user_profile,
            mock_google_mail_get_user_profile_service, mock_google_mail_labels_list_service,
            mock_google_mail_thread_get_service, mock_google_mail_thread_modify_service,
            mock_google_mail_threads_list_service,
        },
        sync_notifications,
    },
    rest::{create_resource, get_resource},
    settings,
    task::todoist::{create_task_from_todoist_item, todoist_item},
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
    nango_google_mail_connection: Box<NangoConnection>,
) {
    let app = authenticated_app.await;
    let user_email_address = EmailAddress(google_mail_user_profile.email_address.clone());
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
    let existing_todoist_task = create_task_from_todoist_item(
        &app.client,
        &app.api_address,
        &todoist_item,
        "Project2".to_string(),
        app.user.id,
    )
    .await;
    let existing_notification: Box<Notification> = create_resource(
        &app.client,
        &app.api_address,
        "notifications",
        Box::new(Notification {
            id: Uuid::new_v4().into(),
            user_id: app.user.id,
            title: "test subject 456".to_string(),
            status: NotificationStatus::Unread,
            source_id: google_mail_thread_get_456.id.clone(),
            source_html_url: Some(google_mail_thread_get_456.get_html_url_from_metadata()),
            metadata: NotificationMetadata::GoogleMail(Box::new(
                google_mail_thread_get_456.clone(),
            )),
            updated_at: Utc.with_ymd_and_hms(2023, 9, 13, 20, 19, 32).unwrap(),
            last_read_at: None,
            snoozed_until: Some(Utc.with_ymd_and_hms(2064, 1, 1, 0, 0, 0).unwrap()),
            details: None,
            task_id: Some(existing_todoist_task.task.id),
        }),
    )
    .await;
    let google_mail_config = GoogleMailConfig::enabled();
    let integration_connection = create_and_mock_integration_connection(
        &app,
        &settings.integrations.oauth2.nango_secret_key,
        IntegrationConnectionConfig::GoogleMail(google_mail_config.clone()),
        &settings,
        nango_google_mail_connection,
    )
    .await;

    let google_mail_get_user_profile_mock = mock_google_mail_get_user_profile_service(
        &app.google_mail_mock_server,
        &google_mail_user_profile,
    );
    let google_mail_labels_list_mock = mock_google_mail_labels_list_service(
        &app.google_mail_mock_server,
        &google_mail_labels_list,
    );
    let google_mail_threads_list_mock = mock_google_mail_threads_list_service(
        &app.google_mail_mock_server,
        None,
        settings.integrations.google_mail.page_size,
        Some(vec![google_mail_config.synced_label.id.clone()]),
        &google_mail_threads_list,
    );
    let empty_result = GoogleMailThreadList {
        threads: None,
        result_size_estimate: 1,
        next_page_token: None,
    };
    let google_mail_threads_list_mock2 = mock_google_mail_threads_list_service(
        &app.google_mail_mock_server,
        Some("next_token"),
        settings.integrations.google_mail.page_size,
        Some(vec![google_mail_config.synced_label.id.clone()]),
        &empty_result,
    );
    let raw_google_mail_thread_get_123 = google_mail_thread_get_123.clone().into();
    let google_mail_thread_get_123_mock = mock_google_mail_thread_get_service(
        &app.google_mail_mock_server,
        "123",
        &raw_google_mail_thread_get_123,
    );
    let raw_google_mail_thread_get_456 = google_mail_thread_get_456.clone().into();
    let google_mail_thread_get_456_mock = mock_google_mail_thread_get_service(
        &app.google_mail_mock_server,
        "456",
        &raw_google_mail_thread_get_456,
    );

    let notifications: Vec<Notification> = sync_notifications(
        &app.client,
        &app.api_address,
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
    google_mail_get_user_profile_mock.assert_hits(1);
    google_mail_labels_list_mock.assert_hits(1);
    google_mail_threads_list_mock.assert();
    google_mail_threads_list_mock2.assert();
    google_mail_thread_get_123_mock.assert();
    google_mail_thread_get_456_mock.assert();

    let updated_notification: Box<Notification> = get_resource(
        &app.client,
        &app.api_address,
        "notifications",
        existing_notification.id.into(),
    )
    .await;
    assert_eq!(updated_notification.id, existing_notification.id);
    assert_eq!(
        updated_notification.source_id,
        existing_notification.source_id
    );
    assert_eq!(updated_notification.status, NotificationStatus::Read);
    assert_eq!(
        updated_notification.updated_at,
        Utc.with_ymd_and_hms(2023, 9, 13, 20, 27, 16).unwrap()
    );
    assert_eq!(
        updated_notification.last_read_at,
        Some(Utc.with_ymd_and_hms(2023, 9, 13, 20, 27, 16).unwrap())
    );
    assert_eq!(
        updated_notification.metadata,
        NotificationMetadata::GoogleMail(Box::new(google_mail_thread_get_456.clone()))
    );
    // `snoozed_until` and `task_id` should not be reset
    assert_eq!(
        updated_notification.snoozed_until,
        Some(Utc.with_ymd_and_hms(2064, 1, 1, 0, 0, 0).unwrap())
    );
    assert_eq!(
        updated_notification.task_id,
        Some(existing_todoist_task.task.id)
    );

    let updated_integration_connection =
        get_integration_connection(&app, integration_connection.id)
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
    nango_google_mail_connection: Box<NangoConnection>,
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
    let user_email_address = EmailAddress(google_mail_user_profile.email_address.clone());

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
            synced_label_id.clone(),
            GOOGLE_MAIL_INBOX_LABEL.to_string(),
        ]
    });

    google_mail_thread_get_456.messages[1].payload.headers = vec![GoogleMailMessageHeader {
        name: "To".to_string(),
        value: if has_new_message_addressed_directly {
            format!("other@example.com, You <{user_email_address}>, test@example.com")
        } else {
            "other@example.com".to_string()
        },
    }];

    let existing_notification: Box<Notification> = create_resource(
        &app.client,
        &app.api_address,
        "notifications",
        Box::new(Notification {
            id: Uuid::new_v4().into(),
            user_id: app.user.id,
            title: "test subject 456".to_string(),
            status: NotificationStatus::Unsubscribed,
            source_id: google_mail_thread_get_456.id.clone(),
            source_html_url: Some(google_mail_thread_get_456.get_html_url_from_metadata()),
            metadata: NotificationMetadata::GoogleMail(Box::new(
                google_mail_thread_get_456.clone(),
            )),
            updated_at: Utc.with_ymd_and_hms(2023, 9, 13, 20, 19, 32).unwrap(),
            last_read_at: None,
            snoozed_until: None,
            details: None,
            task_id: None,
        }),
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

    create_and_mock_integration_connection(
        &app,
        &settings.integrations.oauth2.nango_secret_key,
        IntegrationConnectionConfig::GoogleMail(google_mail_config.clone()),
        &settings,
        nango_google_mail_connection,
    )
    .await;

    let google_mail_get_user_profile_mock = mock_google_mail_get_user_profile_service(
        &app.google_mail_mock_server,
        &google_mail_user_profile,
    );
    let google_mail_labels_list_mock = mock_google_mail_labels_list_service(
        &app.google_mail_mock_server,
        &google_mail_labels_list,
    );
    let google_mail_threads_list_mock = mock_google_mail_threads_list_service(
        &app.google_mail_mock_server,
        None,
        settings.integrations.google_mail.page_size,
        Some(vec![synced_label_id.clone()]),
        &google_mail_threads_list,
    );
    let raw_google_mail_thread_get_456 = google_mail_thread_get_456.clone().into();
    let google_mail_thread_get_456_mock = mock_google_mail_thread_get_service(
        &app.google_mail_mock_server,
        "456",
        &raw_google_mail_thread_get_456,
    );
    let google_mail_thread_modify_mock =
        (expected_notification_status_after_sync == NotificationStatus::Unsubscribed).then(|| {
            mock_google_mail_thread_modify_service(
                &app.google_mail_mock_server,
                &google_mail_thread_get_456.id,
                vec![],
                vec![GOOGLE_MAIL_INBOX_LABEL, &synced_label_id],
            )
        });

    let notifications: Vec<Notification> = sync_notifications(
        &app.client,
        &app.api_address,
        Some(NotificationSourceKind::GoogleMail),
        false,
    )
    .await;

    assert_eq!(notifications.len(), 1);
    google_mail_get_user_profile_mock.assert_hits(1);
    google_mail_labels_list_mock.assert_hits(1);
    google_mail_threads_list_mock.assert();
    google_mail_thread_get_456_mock.assert();
    if let Some(google_mail_thread_modify_mock) = google_mail_thread_modify_mock {
        google_mail_thread_modify_mock.assert();
    }

    let updated_notification: Box<Notification> = get_resource(
        &app.client,
        &app.api_address,
        "notifications",
        existing_notification.id.into(),
    )
    .await;
    assert_eq!(updated_notification.id, existing_notification.id);
    assert_eq!(
        updated_notification.status,
        expected_notification_status_after_sync
    );
    match updated_notification.metadata {
        NotificationMetadata::GoogleMail(thread) => {
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
        _ => unreachable!("Unexpected metadata {:?}", updated_notification.metadata),
    };
}
