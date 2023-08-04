use chrono::{NaiveDate, TimeZone, Utc};
use graphql_client::Response;
use rstest::*;
use uuid::Uuid;

use universal_inbox::{
    integration_connection::IntegrationProviderKind,
    notification::{
        integrations::linear::LinearNotification, Notification, NotificationMetadata,
        NotificationSourceKind, NotificationStatus,
    },
    task::integrations::todoist::TodoistItem,
};

use universal_inbox_api::{
    configuration::Settings,
    integrations::{linear::notifications_query, oauth2::NangoConnection},
};

use crate::helpers::{
    auth::{authenticated_app, AuthenticatedApp},
    integration_connection::{create_and_mock_integration_connection, nango_linear_connection},
    notification::{
        linear::{
            assert_sync_notifications, mock_linear_notifications_service,
            sync_linear_notifications_response,
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
    sync_linear_notifications_response: Response<notifications_query::ResponseData>,
    todoist_item: Box<TodoistItem>,
    nango_linear_connection: Box<NangoConnection>,
) {
    let app = authenticated_app.await;
    let existing_todoist_task = create_task_from_todoist_item(
        &app.client,
        &app.app_address,
        &todoist_item,
        "Project2".to_string(),
        app.user.id,
    )
    .await;
    let sync_linear_notifications: Vec<LinearNotification> = sync_linear_notifications_response
        .data
        .clone()
        .unwrap()
        .try_into()
        .unwrap();
    let existing_notification: Box<Notification> = match &sync_linear_notifications[2] {
        notif @ LinearNotification::IssueNotification { id, issue, .. } => {
            create_resource(
                &app.client,
                &app.app_address,
                "notifications",
                Box::new(Notification {
                    id: Uuid::new_v4().into(),
                    user_id: app.user.id,
                    title: "title to be updated".to_string(),
                    status: NotificationStatus::Unread,
                    source_id: id.to_string(),
                    source_html_url: Some(issue.url.clone()),
                    metadata: NotificationMetadata::Linear(notif.clone()),
                    updated_at: Utc.with_ymd_and_hms(2014, 11, 6, 0, 0, 0).unwrap(),
                    last_read_at: None,
                    snoozed_until: Some(Utc.with_ymd_and_hms(2064, 1, 1, 0, 0, 0).unwrap()),
                    task_id: Some(existing_todoist_task.task.id),
                }),
            )
            .await
        }
        _ => unreachable!(),
    };
    create_and_mock_integration_connection(
        &app,
        IntegrationProviderKind::Linear,
        &settings,
        nango_linear_connection,
    )
    .await;

    let linear_notifications_mock = mock_linear_notifications_service(
        &app.linear_mock_server,
        &sync_linear_notifications_response,
    );

    let notifications: Vec<Notification> = sync_notifications(
        &app.client,
        &app.app_address,
        Some(NotificationSourceKind::Linear),
        false,
    )
    .await;

    assert_eq!(notifications.len(), sync_linear_notifications.len());
    assert_sync_notifications(&notifications, &sync_linear_notifications, app.user.id);
    linear_notifications_mock.assert();

    let updated_notification: Box<Notification> = get_resource(
        &app.client,
        &app.app_address,
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
        NaiveDate::from_ymd_opt(2023, 7, 31)
            .unwrap()
            .and_hms_milli_opt(6, 1, 27, 112)
            .unwrap()
            .and_local_timezone(Utc)
            .unwrap()
    );
    assert_eq!(
        updated_notification.last_read_at,
        Some(
            NaiveDate::from_ymd_opt(2023, 7, 31)
                .unwrap()
                .and_hms_milli_opt(6, 1, 27, 112)
                .unwrap()
                .and_local_timezone(Utc)
                .unwrap()
        )
    );
    assert_eq!(
        updated_notification.metadata,
        NotificationMetadata::Linear(sync_linear_notifications[2].clone())
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
}

// #[rstest]
// #[tokio::test]
// async fn test_sync_notifications_should_mark_deleted_notification_without_subscription(
//     settings: Settings,
//     #[future] authenticated_app: AuthenticatedApp,
//     // Vec[LinearNotification { source_id: "123", ... }, LinearNotification { source_id: "456", ... } ]
//     sync_linear_notifications: Vec<LinearNotification>,
//     nango_linear_connection: Box<NangoConnection>,
// ) {
//     let app = authenticated_app.await;
//     for linear_notification in sync_linear_notifications.iter() {
//         create_notification_from_linear_notification(
//             &app.client,
//             &app.app_address,
//             linear_notification,
//             app.user.id,
//         )
//         .await;
//     }
//     // to be deleted during sync
//     let existing_notification: Box<Notification> = create_resource(
//         &app.client,
//         &app.app_address,
//         "notifications",
//         Box::new(Notification {
//             id: Uuid::new_v4().into(),
//             user_id: app.user.id,
//             title: "Greetings 3".to_string(),
//             status: NotificationStatus::Unread,
//             source_id: "789".to_string(),
//             source_html_url: linear::get_html_url_from_api_url(
//                 &sync_linear_notifications[1].subject.url,
//             ),
//             metadata: NotificationMetadata::Linear(sync_linear_notifications[1].clone()), // reusing linear notification but not useful
//             updated_at: Utc.with_ymd_and_hms(2014, 11, 6, 0, 0, 0).unwrap(),
//             last_read_at: None,
//             snoozed_until: None,
//             task_id: None,
//         }),
//     )
//     .await;
//     create_and_mock_integration_connection(
//         &app,
//         IntegrationProviderKind::Linear,
//         &settings,
//         nango_linear_connection,
//     )
//     .await;

//     let linear_notifications_mock =
//         mock_linear_notifications_service(&app.linear_mock_server, "1", &sync_linear_notifications);
//     let empty_result = Vec::<LinearNotification>::new();
//     let linear_notifications_mock2 =
//         mock_linear_notifications_service(&app.linear_mock_server, "2", &empty_result);

//     let notifications: Vec<Notification> = sync_notifications(
//         &app.client,
//         &app.app_address,
//         Some(NotificationSourceKind::Linear),
//         false,
//     )
//     .await;

//     assert_eq!(notifications.len(), sync_linear_notifications.len());
//     assert_sync_notifications(&notifications, &sync_linear_notifications, app.user.id);
//     linear_notifications_mock.assert();
//     linear_notifications_mock2.assert();

//     let deleted_notification: Box<Notification> = get_resource(
//         &app.client,
//         &app.app_address,
//         "notifications",
//         existing_notification.id.into(),
//     )
//     .await;
//     assert_eq!(deleted_notification.id, existing_notification.id);
//     assert_eq!(deleted_notification.status, NotificationStatus::Deleted);
// }

// #[rstest]
// #[tokio::test]
// async fn test_sync_all_notifications_asynchronously(
//     settings: Settings,
//     #[future] authenticated_app: AuthenticatedApp,
//     // Vec[LinearNotification { source_id: "123", ... }, LinearNotification { source_id: "456", ... } ]
//     sync_linear_notifications: Vec<LinearNotification>,
//     nango_linear_connection: Box<NangoConnection>,
// ) {
//     let app = authenticated_app.await;
//     let _existing_notification: Box<Notification> = create_resource(
//         &app.client,
//         &app.app_address,
//         "notifications",
//         Box::new(Notification {
//             id: Uuid::new_v4().into(),
//             user_id: app.user.id,
//             title: "Greetings 2".to_string(),
//             status: NotificationStatus::Unread,
//             source_id: sync_linear_notifications[1].id.clone(),
//             source_html_url: linear::get_html_url_from_api_url(
//                 &sync_linear_notifications[1].subject.url,
//             ),
//             metadata: NotificationMetadata::Linear(sync_linear_notifications[1].clone()),
//             updated_at: Utc.with_ymd_and_hms(2014, 11, 6, 0, 0, 0).unwrap(),
//             last_read_at: None,
//             snoozed_until: None,
//             task_id: None,
//         }),
//     )
//     .await;
//     create_and_mock_integration_connection(
//         &app,
//         IntegrationProviderKind::Linear,
//         &settings,
//         nango_linear_connection,
//     )
//     .await;

//     let mut linear_notifications_mock =
//         mock_linear_notifications_service(&app.linear_mock_server, "1", &sync_linear_notifications);
//     let empty_result = Vec::<LinearNotification>::new();
//     let mut linear_notifications_mock2 =
//         mock_linear_notifications_service(&app.linear_mock_server, "2", &empty_result);

//     let unauthenticated_client = reqwest::Client::new();
//     let response = sync_notifications_response(
//         &unauthenticated_client,
//         &app.app_address,
//         Some(NotificationSourceKind::Linear),
//         true, // asynchronously
//     )
//     .await;

//     assert_eq!(response.status(), StatusCode::CREATED);

//     let result = list_notifications(
//         &app.client,
//         &app.app_address,
//         NotificationStatus::Read,
//         false,
//         None,
//     )
//     .await;

//     // The existing notification's status should not have been updated to Read yet
//     assert_eq!(result.len(), 0);

//     let mut i = 0;
//     let synchronized = loop {
//         let result = list_notifications(
//             &app.client,
//             &app.app_address,
//             NotificationStatus::Read,
//             false,
//             None,
//         )
//         .await;

//         if result.len() == 1 {
//             // The existing notification's status has been updated to Read
//             break true;
//         }

//         if i == 10 {
//             // Give up after 10 attempts
//             break false;
//         }

//         sleep(Duration::from_millis(100)).await;
//         i += 1;
//     };

//     assert!(synchronized);
//     linear_notifications_mock.assert();
//     linear_notifications_mock2.assert();

//     linear_notifications_mock.delete();
//     linear_notifications_mock2.delete();
//     // Triggering a new sync should not actually sync again
//     let linear_notifications_mock = app.linear_mock_server.mock(|when, then| {
//         when.any_request();
//         then.status(200);
//     });

//     let unauthenticated_client = reqwest::Client::new();
//     let response = sync_notifications_response(
//         &unauthenticated_client,
//         &app.app_address,
//         Some(NotificationSourceKind::Linear),
//         true, // asynchronously
//     )
//     .await;

//     assert_eq!(response.status(), StatusCode::CREATED);

//     sleep(Duration::from_millis(1000)).await;

//     let result = list_notifications(
//         &app.client,
//         &app.app_address,
//         NotificationStatus::Read,
//         false,
//         None,
//     )
//     .await;

//     // Even after 1s, the existing notification's status should not have been updated
//     // because the sync happen too soon after the previous one
//     assert_eq!(result.len(), 1);
//     linear_notifications_mock.assert_hits(0);
// }

// #[rstest]
// #[tokio::test]
// async fn test_sync_all_notifications_asynchronously_in_error(
//     settings: Settings,
//     #[future] authenticated_app: AuthenticatedApp,
//     nango_linear_connection: Box<NangoConnection>,
// ) {
//     let app = authenticated_app.await;
//     create_and_mock_integration_connection(
//         &app,
//         IntegrationProviderKind::Linear,
//         &settings,
//         nango_linear_connection,
//     )
//     .await;

//     let linear_notifications_mock = app.linear_mock_server.mock(|when, then| {
//         when.any_request();
//         then.status(500);
//     });

//     let unauthenticated_client = reqwest::Client::new();
//     let response = sync_notifications_response(
//         &unauthenticated_client,
//         &app.app_address,
//         Some(NotificationSourceKind::Linear),
//         true, // asynchronously
//     )
//     .await;

//     assert_eq!(response.status(), StatusCode::CREATED);

//     sleep(Duration::from_millis(1000)).await;

//     let result = list_notifications(
//         &app.client,
//         &app.app_address,
//         NotificationStatus::Read,
//         false,
//         None,
//     )
//     .await;

//     // Even after 1s, the existing notification's status should not have been updated
//     // because the sync was in error
//     assert_eq!(result.len(), 0);
//     linear_notifications_mock.assert_hits(1);

//     let integration_connection = get_integration_connection_per_provider(
//         &app,
//         app.user.id,
//         IntegrationProviderKind::Linear,
//         None,
//     )
//     .await
//     .unwrap();
//     assert_eq!(
//         integration_connection
//             .last_sync_failure_message
//             .unwrap()
//             .as_str(),
//         "Failed to fetch notifications from Linear"
//     );
// }
