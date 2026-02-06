#![allow(clippy::too_many_arguments)]

use std::str::FromStr;

use chrono::{Datelike, Duration, TimeZone, Timelike, Utc};
use email_address::EmailAddress;
use pretty_assertions::assert_eq;
use rstest::*;
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::{
            google_drive::{GoogleDriveConfig, GoogleDriveContext},
            todoist::TodoistConfig,
        },
        provider::IntegrationProvider,
        IntegrationConnection,
    },
    notification::{
        service::NotificationPatch, Notification, NotificationSourceKind, NotificationStatus,
    },
    third_party::{
        integrations::{google_drive::GoogleDriveComment, todoist::TodoistItem},
        item::{ThirdPartyItem, ThirdPartyItemCreationResult, ThirdPartyItemData},
    },
};

use universal_inbox_api::{
    configuration::Settings,
    integrations::{
        google_drive::{
            GoogleDriveAboutResponse, GoogleDriveCommentList, GoogleDriveFileList,
            GoogleDriveUserInfo, RawGoogleDriveComment, RawGoogleDriveCommentReply,
        },
        oauth2::NangoConnection,
        todoist::TodoistSyncResponse,
    },
};

use crate::helpers::{
    auth::{authenticated_app, AuthenticatedApp},
    integration_connection::{
        create_and_mock_integration_connection, get_integration_connection,
        nango_google_drive_connection, nango_todoist_connection,
    },
    notification::{
        google_drive::{
            assert_sync_notifications, create_notification_from_google_drive_comment,
            google_drive_comment_123, google_drive_comment_456, google_drive_comments_list,
            google_drive_files_list, mock_google_drive_comments_list_service,
            mock_google_drive_files_list_service, mock_google_drive_get_user_info_service,
        },
        sync_notifications, update_notification,
    },
    rest::{create_resource, get_resource},
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
    google_drive_comment_123: GoogleDriveComment,
    google_drive_comment_456: GoogleDriveComment,
    google_drive_comments_list: GoogleDriveCommentList,
    google_drive_files_list: GoogleDriveFileList,
    todoist_item: Box<TodoistItem>,
    sync_todoist_projects_response: TodoistSyncResponse,
    nango_google_drive_connection: Box<NangoConnection>,
    nango_todoist_connection: Box<NangoConnection>,
) {
    let app = authenticated_app.await;
    let user_email_address = EmailAddress::from_str("jane.doe@example.com").unwrap();

    let todoist_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings,
        nango_todoist_connection,
        None,
        None,
    )
    .await;
    mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
        "projects",
        &sync_todoist_projects_response,
        None,
    );

    let creation: Box<ThirdPartyItemCreationResult> = create_resource(
        &app.client,
        &app.app.api_address,
        "third_party/task/items",
        Box::new(ThirdPartyItem {
            id: Uuid::new_v4().into(),
            source_id: todoist_item.id.clone(),
            created_at: Utc::now().with_nanosecond(0).unwrap(),
            updated_at: Utc::now().with_nanosecond(0).unwrap(),
            user_id: app.user.id,
            data: ThirdPartyItemData::TodoistItem(Box::new(TodoistItem {
                project_id: "2222".to_string(), // ie. "Project2"
                added_at: Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
                ..*todoist_item.clone()
            })),
            integration_connection_id: todoist_integration_connection.id,
            source_item: None,
        }),
    )
    .await;
    let existing_todoist_task = creation.task.as_ref().unwrap();

    let google_drive_config = GoogleDriveConfig::enabled();
    let google_drive_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::GoogleDrive(google_drive_config.clone()),
        &settings,
        nango_google_drive_connection,
        None,
        None,
    )
    .await;
    let existing_notification = create_notification_from_google_drive_comment(
        &app.app,
        &google_drive_comment_456,
        app.user.id,
        google_drive_integration_connection.id,
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

    let google_drive_about_response = GoogleDriveAboutResponse {
        user: GoogleDriveUserInfo {
            email_address: user_email_address.to_string(),
            display_name: "Jane Doe".to_string(),
        },
    };
    let google_drive_get_user_info_mock = mock_google_drive_get_user_info_service(
        &app.app.google_drive_mock_server,
        &google_drive_about_response,
    );
    let google_drive_files_list_mock = mock_google_drive_files_list_service(
        &app.app.google_drive_mock_server,
        None,
        settings
            .integrations
            .get("google_drive")
            .unwrap()
            .page_size
            .unwrap(),
        google_drive_integration_connection.created_at,
        &google_drive_files_list,
    );
    let empty_files_result = GoogleDriveFileList {
        files: None,
        incomplete_search: None,
        next_page_token: None,
    };
    let google_drive_files_list_mock2 = mock_google_drive_files_list_service(
        &app.app.google_drive_mock_server,
        Some("next_token"),
        settings
            .integrations
            .get("google_drive")
            .unwrap()
            .page_size
            .unwrap(),
        google_drive_integration_connection.created_at,
        &empty_files_result,
    );

    let modified_time = Utc::now() + Duration::minutes(5); // ensure it's after the existing notification time
    let google_drive_comment_456 = GoogleDriveComment {
        modified_time,
        ..google_drive_comment_456
    };
    let google_drive_comments_list = GoogleDriveCommentList {
        comments: Some(vec![
            google_drive_comments_list.comments.as_ref().unwrap()[0].clone(),
            RawGoogleDriveComment {
                modified_time,
                ..google_drive_comments_list.comments.as_ref().unwrap()[1].clone()
            },
        ]),
        ..google_drive_comments_list
    };
    let google_drive_comments_list_mock = mock_google_drive_comments_list_service(
        &app.app.google_drive_mock_server,
        None,
        settings
            .integrations
            .get("google_drive")
            .unwrap()
            .page_size
            .unwrap(),
        &google_drive_files_list.files.as_ref().unwrap()[0].id,
        &google_drive_comments_list,
    );
    let empty_result = GoogleDriveCommentList {
        comments: None,
        next_page_token: None,
    };
    let google_drive_comments_list_mock2 = mock_google_drive_comments_list_service(
        &app.app.google_drive_mock_server,
        Some("next_token"),
        settings
            .integrations
            .get("google_drive")
            .unwrap()
            .page_size
            .unwrap(),
        &google_drive_files_list.files.as_ref().unwrap()[0].id,
        &empty_result,
    );

    let google_drive_comments_list_mock3 = mock_google_drive_comments_list_service(
        &app.app.google_drive_mock_server,
        None,
        settings
            .integrations
            .get("google_drive")
            .unwrap()
            .page_size
            .unwrap(),
        &google_drive_files_list.files.as_ref().unwrap()[1].id,
        &empty_result,
    );

    let notifications: Vec<Notification> = sync_notifications(
        &app.client,
        &app.app.api_address,
        Some(NotificationSourceKind::GoogleDrive),
        false,
    )
    .await;

    assert_eq!(notifications.len(), 2);
    assert_sync_notifications(
        &notifications,
        &google_drive_comment_123,
        &google_drive_comment_456,
        app.user.id,
        user_email_address.as_ref(),
        "Jane Doe",
    );
    google_drive_get_user_info_mock.assert_hits(1);
    google_drive_files_list_mock.assert();
    google_drive_files_list_mock2.assert();
    google_drive_comments_list_mock.assert();
    google_drive_comments_list_mock2.assert();
    google_drive_comments_list_mock3.assert();

    let updated_notification: Box<Notification> = get_resource(
        &app.client,
        &app.app.api_address,
        "notifications",
        existing_notification.id.into(),
    )
    .await;
    assert_eq!(updated_notification.id, existing_notification.id);
    assert_eq!(
        updated_notification.kind,
        NotificationSourceKind::GoogleDrive
    );
    assert_eq!(
        updated_notification.source_item.source_id,
        existing_notification.source_item.source_id
    );
    // The last reply on this comment is from the user (jane.doe@example.com),
    // so the notification should be marked as Deleted (user already responded)
    assert_eq!(updated_notification.status, NotificationStatus::Deleted);
    assert_eq!(updated_notification.last_read_at, None);
    assert_eq!(
        updated_notification.source_item.data,
        ThirdPartyItemData::GoogleDriveComment(Box::new(GoogleDriveComment {
            user_email_address: Some(user_email_address.to_string()),
            user_display_name: Some("Jane Doe".to_string()),
            ..google_drive_comment_456
        }))
    );
    // `snoozed_until` and `task_id` should not be reset
    assert_eq!(
        updated_notification.snoozed_until,
        Some(Utc.with_ymd_and_hms(2064, 1, 1, 0, 0, 0).unwrap())
    );
    assert_eq!(updated_notification.task_id, Some(existing_todoist_task.id));

    let updated_integration_connection =
        get_integration_connection(&app, google_drive_integration_connection.id)
            .await
            .unwrap();
    assert_eq!(
        updated_integration_connection,
        IntegrationConnection {
            provider: IntegrationProvider::GoogleDrive {
                context: Some(GoogleDriveContext {
                    user_email_address,
                    user_display_name: "Jane Doe".to_string()
                }),
                config: GoogleDriveConfig::enabled()
            },
            ..updated_integration_connection.clone()
        }
    );
}

#[rstest]
#[case(false, true, NotificationStatus::Unsubscribed)]
#[case(false, false, NotificationStatus::Unsubscribed)]
#[case(true, true, NotificationStatus::Unread)]
#[tokio::test]
async fn test_sync_notifications_of_unsubscribed_notification_with_new_messages(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    google_drive_comment_123: GoogleDriveComment,
    mut google_drive_files_list: GoogleDriveFileList,
    mut google_drive_comments_list: GoogleDriveCommentList,
    nango_google_drive_connection: Box<NangoConnection>,
    #[case] has_new_message_addressed_directly: bool,
    #[case] has_new_unread_message: bool,
    #[case] expected_notification_status_after_sync: NotificationStatus,
) {
    // When a Universal Inbox notification is marked as unsubscribed from a Google Drive comment with new
    // messages and none of these new messages are directly addressed to the user (ie. the user's email
    // is not in the message), the status of the notification remains unchanged.
    let app = authenticated_app.await;
    let google_drive_config = GoogleDriveConfig::enabled();
    let user_email_address = EmailAddress::from_str("jane.doe@example.com").unwrap();

    let content = if has_new_message_addressed_directly {
        format!("Hello @{user_email_address}")
    } else {
        "Hello".to_string()
    };

    let comment = google_drive_comments_list.comments.as_ref().unwrap()[0].clone();
    let reply = comment.replies.as_ref().unwrap()[0].clone();
    google_drive_comments_list = GoogleDriveCommentList {
        comments: Some(vec![RawGoogleDriveComment {
            replies: Some(vec![RawGoogleDriveCommentReply {
                content,
                // Set reply modification time in the future to be sure it is newer than the notification
                // modification time
                modified_time: if has_new_unread_message {
                    Utc::now().with_year(Utc::now().year() + 1).unwrap()
                } else {
                    Utc::now().with_year(Utc::now().year() - 1).unwrap()
                },
                ..reply
            }]),
            ..comment
        }]),
        ..google_drive_comments_list
    };

    let google_drive_about_response = GoogleDriveAboutResponse {
        user: GoogleDriveUserInfo {
            email_address: user_email_address.to_string(),
            display_name: "Jane Doe".to_string(),
        },
    };
    let google_drive_get_user_info_mock = mock_google_drive_get_user_info_service(
        &app.app.google_drive_mock_server,
        &google_drive_about_response,
    );

    let google_drive_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::GoogleDrive(google_drive_config.clone()),
        &settings,
        nango_google_drive_connection,
        None,
        None,
    )
    .await;
    let existing_notification = create_notification_from_google_drive_comment(
        &app.app,
        &google_drive_comment_123,
        app.user.id,
        google_drive_integration_connection.id,
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

    google_drive_files_list.next_page_token = None;
    google_drive_files_list.files = Some(vec![
        google_drive_files_list.files.as_ref().unwrap()[0].clone()
    ]);
    let google_drive_files_list_mock = mock_google_drive_files_list_service(
        &app.app.google_drive_mock_server,
        None,
        settings
            .integrations
            .get("google_drive")
            .unwrap()
            .page_size
            .unwrap(),
        google_drive_integration_connection.created_at,
        &google_drive_files_list,
    );

    google_drive_comments_list.next_page_token = None;
    let google_drive_comments_list_mock = mock_google_drive_comments_list_service(
        &app.app.google_drive_mock_server,
        None,
        settings
            .integrations
            .get("google_drive")
            .unwrap()
            .page_size
            .unwrap(),
        &google_drive_files_list.files.as_ref().unwrap()[0].id,
        &google_drive_comments_list,
    );

    let notifications: Vec<Notification> = sync_notifications(
        &app.client,
        &app.app.api_address,
        Some(NotificationSourceKind::GoogleDrive),
        false,
    )
    .await;

    assert_eq!(
        notifications.len(),
        if expected_notification_status_after_sync == NotificationStatus::Unsubscribed {
            0
        } else {
            1
        }
    );
    google_drive_get_user_info_mock.assert_hits(1);
    google_drive_files_list_mock.assert();
    google_drive_comments_list_mock.assert();

    let updated_notification: Box<Notification> = get_resource(
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
}
