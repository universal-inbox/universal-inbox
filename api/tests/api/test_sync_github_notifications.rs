use actix_http::StatusCode;
use chrono::{TimeZone, Utc};
use rstest::*;
use tokio::time::{sleep, Duration};
use uuid::Uuid;

use universal_inbox::{
    integration_connection::IntegrationProviderKind,
    notification::{
        integrations::github::GithubNotification, Notification, NotificationMetadata,
        NotificationSourceKind, NotificationStatus,
    },
    task::integrations::todoist::TodoistItem,
};

use universal_inbox_api::{
    configuration::Settings,
    integrations::{github, oauth2::NangoConnection},
};

use crate::helpers::{
    auth::{authenticated_app, AuthenticatedApp},
    integration_connection::{
        create_and_mock_integration_connection, get_integration_connection_per_provider,
        nango_github_connection,
    },
    notification::{
        github::{
            assert_sync_notifications, create_notification_from_github_notification,
            mock_github_notifications_service, sync_github_notifications,
        },
        list_notifications, sync_notifications, sync_notifications_response,
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
    // Vec[GithubNotification { source_id: "123", ... }, GithubNotification { source_id: "456", ... } ]
    sync_github_notifications: Vec<GithubNotification>,
    todoist_item: Box<TodoistItem>,
    nango_github_connection: Box<NangoConnection>,
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
    let existing_notification: Box<Notification> = create_resource(
        &app.client,
        &app.app_address,
        "notifications",
        Box::new(Notification {
            id: Uuid::new_v4().into(),
            user_id: app.user.id,
            title: "Greetings 2".to_string(),
            status: NotificationStatus::Unread,
            source_id: sync_github_notifications[1].id.clone(),
            source_html_url: github::get_html_url_from_api_url(
                &sync_github_notifications[1].subject.url,
            ),
            metadata: NotificationMetadata::Github(sync_github_notifications[1].clone()),
            updated_at: Utc.with_ymd_and_hms(2014, 11, 6, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: Some(Utc.with_ymd_and_hms(2064, 1, 1, 0, 0, 0).unwrap()),
            task_id: Some(existing_todoist_task.task.id),
        }),
    )
    .await;
    create_and_mock_integration_connection(
        &app,
        IntegrationProviderKind::Github,
        &settings,
        nango_github_connection,
    )
    .await;

    let github_notifications_mock =
        mock_github_notifications_service(&app.github_mock_server, "1", &sync_github_notifications);
    let empty_result = Vec::<GithubNotification>::new();
    let github_notifications_mock2 =
        mock_github_notifications_service(&app.github_mock_server, "2", &empty_result);

    let notifications: Vec<Notification> = sync_notifications(
        &app.client,
        &app.app_address,
        Some(NotificationSourceKind::Github),
        false,
    )
    .await;

    assert_eq!(notifications.len(), sync_github_notifications.len());
    assert_sync_notifications(&notifications, &sync_github_notifications, app.user.id);
    github_notifications_mock.assert();
    github_notifications_mock2.assert();

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
        Utc.with_ymd_and_hms(2014, 11, 7, 23, 1, 45).unwrap()
    );
    assert_eq!(
        updated_notification.last_read_at,
        Some(Utc.with_ymd_and_hms(2014, 11, 7, 23, 2, 45).unwrap())
    );
    assert_eq!(
        updated_notification.metadata,
        NotificationMetadata::Github(sync_github_notifications[1].clone())
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

#[rstest]
#[tokio::test]
async fn test_sync_notifications_should_mark_deleted_notification_without_subscription(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    // Vec[GithubNotification { source_id: "123", ... }, GithubNotification { source_id: "456", ... } ]
    sync_github_notifications: Vec<GithubNotification>,
    nango_github_connection: Box<NangoConnection>,
) {
    let app = authenticated_app.await;
    for github_notification in sync_github_notifications.iter() {
        create_notification_from_github_notification(
            &app.client,
            &app.app_address,
            github_notification,
            app.user.id,
        )
        .await;
    }
    // to be deleted during sync
    let existing_notification: Box<Notification> = create_resource(
        &app.client,
        &app.app_address,
        "notifications",
        Box::new(Notification {
            id: Uuid::new_v4().into(),
            user_id: app.user.id,
            title: "Greetings 3".to_string(),
            status: NotificationStatus::Unread,
            source_id: "789".to_string(),
            source_html_url: github::get_html_url_from_api_url(
                &sync_github_notifications[1].subject.url,
            ),
            metadata: NotificationMetadata::Github(sync_github_notifications[1].clone()), // reusing github notification but not useful
            updated_at: Utc.with_ymd_and_hms(2014, 11, 6, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
            task_id: None,
        }),
    )
    .await;
    create_and_mock_integration_connection(
        &app,
        IntegrationProviderKind::Github,
        &settings,
        nango_github_connection,
    )
    .await;

    let github_notifications_mock =
        mock_github_notifications_service(&app.github_mock_server, "1", &sync_github_notifications);
    let empty_result = Vec::<GithubNotification>::new();
    let github_notifications_mock2 =
        mock_github_notifications_service(&app.github_mock_server, "2", &empty_result);

    let notifications: Vec<Notification> = sync_notifications(
        &app.client,
        &app.app_address,
        Some(NotificationSourceKind::Github),
        false,
    )
    .await;

    assert_eq!(notifications.len(), sync_github_notifications.len());
    assert_sync_notifications(&notifications, &sync_github_notifications, app.user.id);
    github_notifications_mock.assert();
    github_notifications_mock2.assert();

    let deleted_notification: Box<Notification> = get_resource(
        &app.client,
        &app.app_address,
        "notifications",
        existing_notification.id.into(),
    )
    .await;
    assert_eq!(deleted_notification.id, existing_notification.id);
    assert_eq!(deleted_notification.status, NotificationStatus::Deleted);
}

#[rstest]
#[tokio::test]
async fn test_sync_all_notifications_asynchronously(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    // Vec[GithubNotification { source_id: "123", ... }, GithubNotification { source_id: "456", ... } ]
    sync_github_notifications: Vec<GithubNotification>,
    nango_github_connection: Box<NangoConnection>,
) {
    let app = authenticated_app.await;
    let _existing_notification: Box<Notification> = create_resource(
        &app.client,
        &app.app_address,
        "notifications",
        Box::new(Notification {
            id: Uuid::new_v4().into(),
            user_id: app.user.id,
            title: "Greetings 2".to_string(),
            status: NotificationStatus::Unread,
            source_id: sync_github_notifications[1].id.clone(),
            source_html_url: github::get_html_url_from_api_url(
                &sync_github_notifications[1].subject.url,
            ),
            metadata: NotificationMetadata::Github(sync_github_notifications[1].clone()),
            updated_at: Utc.with_ymd_and_hms(2014, 11, 6, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
            task_id: None,
        }),
    )
    .await;
    create_and_mock_integration_connection(
        &app,
        IntegrationProviderKind::Github,
        &settings,
        nango_github_connection,
    )
    .await;

    let mut github_notifications_mock =
        mock_github_notifications_service(&app.github_mock_server, "1", &sync_github_notifications);
    let empty_result = Vec::<GithubNotification>::new();
    let mut github_notifications_mock2 =
        mock_github_notifications_service(&app.github_mock_server, "2", &empty_result);

    let unauthenticated_client = reqwest::Client::new();
    let response = sync_notifications_response(
        &unauthenticated_client,
        &app.app_address,
        Some(NotificationSourceKind::Github),
        true, // asynchronously
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);

    let result = list_notifications(
        &app.client,
        &app.app_address,
        NotificationStatus::Read,
        false,
        None,
    )
    .await;

    // The existing notification's status should not have been updated to Read yet
    assert_eq!(result.len(), 0);

    let mut i = 0;
    let synchronized = loop {
        let result = list_notifications(
            &app.client,
            &app.app_address,
            NotificationStatus::Read,
            false,
            None,
        )
        .await;

        if result.len() == 1 {
            // The existing notification's status has been updated to Read
            break true;
        }

        if i == 10 {
            // Give up after 10 attempts
            break false;
        }

        sleep(Duration::from_millis(100)).await;
        i += 1;
    };

    assert!(synchronized);
    github_notifications_mock.assert();
    github_notifications_mock2.assert();

    github_notifications_mock.delete();
    github_notifications_mock2.delete();
    // Triggering a new sync should not actually sync again
    let github_notifications_mock = app.github_mock_server.mock(|when, then| {
        when.any_request();
        then.status(200);
    });

    let unauthenticated_client = reqwest::Client::new();
    let response = sync_notifications_response(
        &unauthenticated_client,
        &app.app_address,
        Some(NotificationSourceKind::Github),
        true, // asynchronously
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);

    sleep(Duration::from_millis(1000)).await;

    let result = list_notifications(
        &app.client,
        &app.app_address,
        NotificationStatus::Read,
        false,
        None,
    )
    .await;

    // Even after 1s, the existing notification's status should not have been updated
    // because the sync happen too soon after the previous one
    assert_eq!(result.len(), 1);
    github_notifications_mock.assert_hits(0);
}

#[rstest]
#[tokio::test]
async fn test_sync_all_notifications_asynchronously_in_error(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    nango_github_connection: Box<NangoConnection>,
) {
    let app = authenticated_app.await;
    create_and_mock_integration_connection(
        &app,
        IntegrationProviderKind::Github,
        &settings,
        nango_github_connection,
    )
    .await;

    let github_notifications_mock = app.github_mock_server.mock(|when, then| {
        when.any_request();
        then.status(500);
    });

    let unauthenticated_client = reqwest::Client::new();
    let response = sync_notifications_response(
        &unauthenticated_client,
        &app.app_address,
        Some(NotificationSourceKind::Github),
        true, // asynchronously
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);

    sleep(Duration::from_millis(1000)).await;

    let result = list_notifications(
        &app.client,
        &app.app_address,
        NotificationStatus::Read,
        false,
        None,
    )
    .await;

    // Even after 1s, the existing notification's status should not have been updated
    // because the sync was in error
    assert_eq!(result.len(), 0);
    github_notifications_mock.assert_hits(1);

    let integration_connection = get_integration_connection_per_provider(
        &app,
        app.user.id,
        IntegrationProviderKind::Github,
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        integration_connection
            .last_sync_failure_message
            .unwrap()
            .as_str(),
        "Failed to fetch notifications from Github"
    );
}
