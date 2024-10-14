#![allow(clippy::too_many_arguments)]
use chrono::{TimeZone, Timelike, Utc};
use graphql_client::{Error, Response};
use http::StatusCode;
use rstest::*;
use tokio::time::{sleep, Duration};
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::{github::GithubConfig, todoist::TodoistConfig},
        provider::IntegrationProviderKind,
        IntegrationConnectionStatus,
    },
    notification::{
        integrations::github::{GithubNotification, GithubNotificationSubject},
        Notification, NotificationDetails, NotificationMetadata, NotificationSourceKind,
        NotificationStatus,
    },
    third_party::{
        integrations::todoist::TodoistItem,
        item::{ThirdPartyItem, ThirdPartyItemCreationResult, ThirdPartyItemData},
    },
};

use universal_inbox_api::{
    configuration::Settings,
    integrations::{
        github::graphql::{discussions_search_query, pull_request_query},
        oauth2::NangoConnection,
        todoist::TodoistSyncResponse,
    },
    repository::integration_connection::{
        MAX_SYNC_FAILURES_BEFORE_DISCONNECT, TOO_MANY_SYNC_FAILURES_ERROR_MESSAGE,
    },
};

use crate::helpers::{
    auth::{authenticated_app, AuthenticatedApp},
    integration_connection::{
        create_and_mock_integration_connection, create_integration_connection,
        get_integration_connection_per_provider, nango_github_connection, nango_todoist_connection,
    },
    notification::{
        github::{
            assert_sync_notifications, create_notification_from_github_notification,
            github_discussion_123_response, github_notification, github_pull_request_123_response,
            mock_github_discussions_search_query, mock_github_notifications_service,
            mock_github_pull_request_query, sync_github_notifications,
        },
        list_notifications, sync_notifications, sync_notifications_response,
    },
    rest::{create_resource, get_resource},
    settings,
    task::todoist::{
        mock_todoist_sync_resources_service, sync_todoist_projects_response, todoist_item,
    },
    tested_app_with_local_auth,
    user::create_user_and_login,
    TestedApp,
};

#[rstest]
#[tokio::test]
async fn test_sync_notifications_should_add_new_notification_and_update_existing_one(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    // Vec[GithubNotification { source_id: "123", ... }, GithubNotification { source_id: "456", ... } ]
    sync_github_notifications: Vec<GithubNotification>,
    github_pull_request_123_response: Response<pull_request_query::ResponseData>,
    todoist_item: Box<TodoistItem>,
    sync_todoist_projects_response: TodoistSyncResponse,
    nango_github_connection: Box<NangoConnection>,
    nango_todoist_connection: Box<NangoConnection>,
) {
    let app = authenticated_app.await;
    let integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings,
        nango_todoist_connection,
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
            data: ThirdPartyItemData::TodoistItem(TodoistItem {
                project_id: "2222".to_string(), // ie. "Project2"
                added_at: Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
                ..*todoist_item.clone()
            }),
            integration_connection_id: integration_connection.id,
        }),
    )
    .await;
    let existing_todoist_task = creation.task.as_ref().unwrap();
    let existing_notification: Box<Notification> = create_resource(
        &app.client,
        &app.app.api_address,
        "notifications",
        Box::new(Notification {
            id: Uuid::new_v4().into(),
            user_id: app.user.id,
            title: "Greetings 2".to_string(),
            status: NotificationStatus::Unread,
            source_id: sync_github_notifications[1].id.clone(),
            metadata: NotificationMetadata::Github(Box::new(sync_github_notifications[1].clone())),
            updated_at: Utc.with_ymd_and_hms(2014, 11, 6, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: Some(Utc.with_ymd_and_hms(2064, 1, 1, 0, 0, 0).unwrap()),
            details: None,
            task_id: Some(existing_todoist_task.id),
        }),
    )
    .await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        &settings,
        nango_github_connection,
        None,
    )
    .await;
    let expected_notification_123_details: NotificationDetails = github_pull_request_123_response
        .data
        .clone()
        .unwrap()
        .try_into()
        .unwrap();

    let github_notifications_mock = mock_github_notifications_service(
        &app.app.github_mock_server,
        "1",
        &sync_github_notifications,
    );
    let empty_result = Vec::<GithubNotification>::new();
    let github_notifications_mock2 =
        mock_github_notifications_service(&app.app.github_mock_server, "2", &empty_result);

    let github_pull_request_123_query_mock = mock_github_pull_request_query(
        &app.app.github_mock_server,
        "octokit".to_string(),
        "octokit.rb".to_string(),
        123,
        &github_pull_request_123_response,
    );

    let notifications: Vec<Notification> = sync_notifications(
        &app.client,
        &app.app.api_address,
        Some(NotificationSourceKind::Github),
        false,
    )
    .await;

    assert_eq!(notifications.len(), sync_github_notifications.len());
    github_pull_request_123_query_mock.assert();
    assert_sync_notifications(
        &notifications,
        &sync_github_notifications,
        app.user.id,
        Some(expected_notification_123_details),
    );
    github_notifications_mock.assert();
    github_notifications_mock2.assert();

    let updated_notification: Box<Notification> = get_resource(
        &app.client,
        &app.app.api_address,
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
        NotificationMetadata::Github(Box::new(sync_github_notifications[1].clone()))
    );
    // `snoozed_until` and `task_id` should not be reset
    assert_eq!(
        updated_notification.snoozed_until,
        Some(Utc.with_ymd_and_hms(2064, 1, 1, 0, 0, 0).unwrap())
    );
    assert_eq!(updated_notification.task_id, Some(existing_todoist_task.id));

    let integration_connection = get_integration_connection_per_provider(
        &app,
        app.user.id,
        IntegrationProviderKind::Github,
        None,
        None,
    )
    .await
    .unwrap();
    assert!(integration_connection
        .last_notifications_sync_started_at
        .is_some());
    assert!(integration_connection
        .last_notifications_sync_completed_at
        .is_some());
    assert!(integration_connection
        .last_notifications_sync_failure_message
        .is_none());
    assert_eq!(integration_connection.notifications_sync_failures, 0);
    assert_eq!(
        integration_connection.status,
        IntegrationConnectionStatus::Validated
    );
    assert!(integration_connection.failure_message.is_none(),);
}

#[rstest]
#[tokio::test]
async fn test_sync_notifications_should_mark_deleted_notification_without_subscription(
    settings: Settings,
    #[future] tested_app_with_local_auth: TestedApp,
    // Vec[GithubNotification { source_id: "123", ... }, GithubNotification { source_id: "456", ... } ]
    sync_github_notifications: Vec<GithubNotification>,
    nango_github_connection: Box<NangoConnection>,
) {
    let app = tested_app_with_local_auth.await;

    let (other_client, other_user) = create_user_and_login(
        &app,
        "Jane",
        "Doe",
        "jane@doe.net".parse().unwrap(),
        "password",
    )
    .await;

    let other_user_existing_notification: Box<Notification> = create_resource(
        &other_client,
        &app.api_address,
        "notifications",
        Box::new(Notification {
            id: Uuid::new_v4().into(),
            user_id: other_user.id,
            title: "Greetings other".to_string(),
            status: NotificationStatus::Unread,
            source_id: "789".to_string(), // Same for both users as they may share the same source of notification
            metadata: NotificationMetadata::Github(Box::new(sync_github_notifications[1].clone())), // reusing github notification but not useful
            updated_at: Utc.with_ymd_and_hms(2014, 11, 6, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
            details: None,
            task_id: None,
        }),
    )
    .await;

    let (client, user) = create_user_and_login(
        &app,
        "John",
        "Doe",
        "john@doe.net".parse().unwrap(),
        "password",
    )
    .await;

    for github_notification in sync_github_notifications.iter() {
        create_notification_from_github_notification(
            &client,
            &app.api_address,
            github_notification,
            user.id,
        )
        .await;
    }

    // to be deleted during sync
    let existing_notification: Box<Notification> = create_resource(
        &client,
        &app.api_address,
        "notifications",
        Box::new(Notification {
            id: Uuid::new_v4().into(),
            user_id: user.id,
            title: "Greetings 3".to_string(),
            status: NotificationStatus::Unread,
            source_id: "789".to_string(),
            metadata: NotificationMetadata::Github(Box::new(sync_github_notifications[1].clone())), // reusing github notification but not useful
            updated_at: Utc.with_ymd_and_hms(2014, 11, 6, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
            details: None,
            task_id: None,
        }),
    )
    .await;
    create_and_mock_integration_connection(
        &app,
        user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        &settings,
        nango_github_connection,
        None,
    )
    .await;

    let github_notifications_mock =
        mock_github_notifications_service(&app.github_mock_server, "1", &sync_github_notifications);
    let empty_result = Vec::<GithubNotification>::new();
    let github_notifications_mock2 =
        mock_github_notifications_service(&app.github_mock_server, "2", &empty_result);

    let notifications: Vec<Notification> = sync_notifications(
        &client,
        &app.api_address,
        Some(NotificationSourceKind::Github),
        false,
    )
    .await;

    assert_eq!(notifications.len(), sync_github_notifications.len());
    assert_sync_notifications(&notifications, &sync_github_notifications, user.id, None);
    github_notifications_mock.assert();
    github_notifications_mock2.assert();

    let deleted_notification: Box<Notification> = get_resource(
        &client,
        &app.api_address,
        "notifications",
        existing_notification.id.into(),
    )
    .await;
    assert_eq!(deleted_notification.id, existing_notification.id);
    assert_eq!(deleted_notification.status, NotificationStatus::Deleted);

    let refreshed_other_user_existing_notification: Box<Notification> = get_resource(
        &other_client,
        &app.api_address,
        "notifications",
        other_user_existing_notification.id.into(),
    )
    .await;
    // Make sure other users notifications are not touched
    assert_eq!(
        refreshed_other_user_existing_notification.status,
        NotificationStatus::Unread
    );
}

#[rstest]
#[case::trigger_sync_when_listing_notifications(true)]
#[case::trigger_sync_with_sync_endpoint(false)]
#[tokio::test]
async fn test_sync_all_notifications_asynchronously(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    // Vec[GithubNotification { source_id: "123", ... }, GithubNotification { source_id: "456", ... } ]
    sync_github_notifications: Vec<GithubNotification>,
    github_pull_request_123_response: Response<pull_request_query::ResponseData>,
    nango_github_connection: Box<NangoConnection>,
    #[case] trigger_sync_when_listing_notifications: bool,
) {
    let app = authenticated_app.await;
    let _existing_notification: Box<Notification> = create_resource(
        &app.client,
        &app.app.api_address,
        "notifications",
        Box::new(Notification {
            id: Uuid::new_v4().into(),
            user_id: app.user.id,
            title: "Greetings 2".to_string(),
            status: NotificationStatus::Unread,
            source_id: sync_github_notifications[1].id.clone(),
            metadata: NotificationMetadata::Github(Box::new(sync_github_notifications[1].clone())),
            updated_at: Utc.with_ymd_and_hms(2014, 11, 6, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
            details: None,
            task_id: None,
        }),
    )
    .await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        &settings,
        nango_github_connection,
        None,
    )
    .await;

    let mut github_notifications_mock = mock_github_notifications_service(
        &app.app.github_mock_server,
        "1",
        &sync_github_notifications,
    );
    let empty_result = Vec::<GithubNotification>::new();
    let mut github_notifications_mock2 =
        mock_github_notifications_service(&app.app.github_mock_server, "2", &empty_result);
    let mut github_pull_request_123_query_mock = mock_github_pull_request_query(
        &app.app.github_mock_server,
        "octokit".to_string(),
        "octokit.rb".to_string(),
        123,
        &github_pull_request_123_response,
    );

    if trigger_sync_when_listing_notifications {
        let result = list_notifications(
            &app.client,
            &app.app.api_address,
            vec![NotificationStatus::Read],
            false,
            None,
            None,
            true,
        )
        .await;

        // The existing notification's status should not have been updated to Read yet
        assert_eq!(result.len(), 0);
    } else {
        let unauthenticated_client = reqwest::Client::new();
        let response = sync_notifications_response(
            &unauthenticated_client,
            &app.app.api_address,
            Some(NotificationSourceKind::Github),
            true, // asynchronously
        )
        .await;

        assert_eq!(response.status(), StatusCode::CREATED);
    }

    let mut i = 0;
    let synchronized = loop {
        let result = list_notifications(
            &app.client,
            &app.app.api_address,
            vec![NotificationStatus::Read],
            false,
            None,
            None,
            trigger_sync_when_listing_notifications,
        )
        .await;

        if result.len() == 1 {
            // The existing notification's status has been updated to Read
            break true;
        }

        if i == 20 {
            // Give up after 20 attempts
            break false;
        }

        sleep(Duration::from_millis(100)).await;
        i += 1;
    };

    assert!(synchronized);
    github_notifications_mock.assert();
    github_notifications_mock2.assert();
    github_pull_request_123_query_mock.assert();

    github_notifications_mock.delete();
    github_notifications_mock2.delete();
    github_pull_request_123_query_mock.delete();

    // Triggering a new sync should not actually sync again
    let github_notifications_mock = app.app.github_mock_server.mock(|when, then| {
        when.any_request();
        then.status(200);
    });

    let unauthenticated_client = reqwest::Client::new();
    let response = sync_notifications_response(
        &unauthenticated_client,
        &app.app.api_address,
        Some(NotificationSourceKind::Github),
        true, // asynchronously
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);

    sleep(Duration::from_millis(1000)).await;

    let result = list_notifications(
        &app.client,
        &app.app.api_address,
        vec![NotificationStatus::Read],
        false,
        None,
        None,
        false,
    )
    .await;

    // Even after 1s, the existing notification's status should not have been updated
    // because the sync happen too soon after the previous one
    assert_eq!(result.len(), 1);
    github_notifications_mock.assert_hits(0);
}

#[rstest]
#[tokio::test]
async fn test_sync_all_notifications_with_no_validated_integration_connections(
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    create_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        IntegrationConnectionStatus::Created,
        None,
        None,
        None,
    )
    .await;

    let github_notifications_mock = app.app.github_mock_server.mock(|when, then| {
        when.any_request();
        then.status(200);
    });

    let response = sync_notifications_response(
        &app.client,
        &app.app.api_address,
        Some(NotificationSourceKind::Github),
        false, // synchronously
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    github_notifications_mock.assert_hits(0);
}

#[rstest]
#[tokio::test]
async fn test_sync_all_notifications_with_synchronization_disabled(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    nango_github_connection: Box<NangoConnection>,
) {
    let app = authenticated_app.await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Github(GithubConfig::disabled()),
        &settings,
        nango_github_connection,
        None,
    )
    .await;

    let github_notifications_mock = app.app.github_mock_server.mock(|when, then| {
        when.any_request();
        then.status(200);
    });

    let response = sync_notifications_response(
        &app.client,
        &app.app.api_address,
        Some(NotificationSourceKind::Github),
        false, // synchronously
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
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
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        &settings,
        nango_github_connection,
        // Starting with max sync failures minus 1, it should mark the connection as failing with a new failure
        Some(MAX_SYNC_FAILURES_BEFORE_DISCONNECT - 1),
    )
    .await;

    let github_notifications_mock = app.app.github_mock_server.mock(|when, then| {
        when.any_request();
        then.status(500);
    });

    let unauthenticated_client = reqwest::Client::new();
    let response = sync_notifications_response(
        &unauthenticated_client,
        &app.app.api_address,
        Some(NotificationSourceKind::Github),
        true, // asynchronously
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);

    sleep(Duration::from_millis(1000)).await;

    let result = list_notifications(
        &app.client,
        &app.app.api_address,
        vec![NotificationStatus::Read],
        false,
        None,
        None,
        false,
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
        None,
    )
    .await
    .unwrap();
    assert!(integration_connection
        .last_notifications_sync_started_at
        .is_some());
    assert!(integration_connection
        .last_notifications_sync_completed_at
        .is_some());
    assert_eq!(
        integration_connection
            .last_notifications_sync_failure_message
            .unwrap()
            .as_str(),
        "Failed to fetch notifications from Github"
    );
    assert_eq!(
        integration_connection.notifications_sync_failures,
        MAX_SYNC_FAILURES_BEFORE_DISCONNECT
    );
    assert_eq!(
        integration_connection.status,
        IntegrationConnectionStatus::Failing
    );
    assert_eq!(
        integration_connection.failure_message,
        Some(TOO_MANY_SYNC_FAILURES_ERROR_MESSAGE.to_string())
    );
}

#[rstest]
#[tokio::test]
async fn test_sync_discussion_notification_with_details(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    mut github_notification: Box<GithubNotification>,
    github_discussion_123_response: Response<discussions_search_query::ResponseData>,
    nango_github_connection: Box<NangoConnection>,
) {
    github_notification.subject = GithubNotificationSubject {
        title: "test discussion".to_string(),
        url: None,
        latest_comment_url: None,
        r#type: "Discussion".to_string(),
    };

    let app = authenticated_app.await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        &settings,
        nango_github_connection,
        None,
    )
    .await;

    let github_notifications_response = vec![*github_notification];
    let github_notifications_mock = mock_github_notifications_service(
        &app.app.github_mock_server,
        "1",
        &github_notifications_response,
    );

    let github_discussions_search_query_mock = mock_github_discussions_search_query(
        &app.app.github_mock_server,
        "repo:octocat/Hello-World \"test discussion\"",
        &github_discussion_123_response,
    );

    let notifications: Vec<Notification> = sync_notifications(
        &app.client,
        &app.app.api_address,
        Some(NotificationSourceKind::Github),
        false,
    )
    .await;

    assert_eq!(notifications.len(), 1);
    github_discussions_search_query_mock.assert();
    github_notifications_mock.assert();

    let notifications = list_notifications(
        &app.client,
        &app.app.api_address,
        vec![NotificationStatus::Unread],
        false,
        None,
        None,
        false,
    )
    .await;

    assert_eq!(notifications.len(), 1);
    match &notifications[0].details {
        Some(NotificationDetails::GithubDiscussion(details)) => {
            assert_eq!(details.title, "test discussion");
        }
        _ => unreachable!("Expected a GithubDiscussion notification"),
    }
}

#[rstest]
#[tokio::test]
async fn test_sync_discussion_notification_with_error(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    mut github_notification: Box<GithubNotification>,
    nango_github_connection: Box<NangoConnection>,
) {
    github_notification.subject = GithubNotificationSubject {
        title: "test discussion".to_string(),
        url: None,
        latest_comment_url: None,
        r#type: "Discussion".to_string(),
    };

    let app = authenticated_app.await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        &settings,
        nango_github_connection,
        None,
    )
    .await;

    let github_notifications_response = vec![*github_notification];
    let github_notifications_mock = mock_github_notifications_service(
        &app.app.github_mock_server,
        "1",
        &github_notifications_response,
    );

    let error_response = Response {
        data: None,
        errors: Some(vec![Error {
            message: "Something went wrong".to_string(),
            locations: None,
            path: None,
            extensions: None,
        }]),
        extensions: None,
    };
    let github_discussions_search_query_mock = mock_github_discussions_search_query(
        &app.app.github_mock_server,
        "repo:octocat/Hello-World \"test discussion\"",
        &error_response,
    );

    let response = sync_notifications_response(
        &app.client,
        &app.app.api_address,
        Some(NotificationSourceKind::Github),
        false,
    )
    .await;

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    github_discussions_search_query_mock.assert();
    github_notifications_mock.assert();

    let notifications = list_notifications(
        &app.client,
        &app.app.api_address,
        vec![NotificationStatus::Unread],
        false,
        None,
        None,
        false,
    )
    .await;

    assert_eq!(notifications.len(), 1);
    assert!(notifications[0].details.is_none());

    let integration_connection = get_integration_connection_per_provider(
        &app,
        app.user.id,
        IntegrationProviderKind::Github,
        None,
        None,
    )
    .await
    .unwrap();
    assert!(integration_connection
        .last_notifications_sync_started_at
        .is_some());
    assert!(integration_connection
        .last_notifications_sync_completed_at
        .is_some());
    assert_eq!(
        integration_connection
            .last_notifications_sync_failure_message
            .unwrap()
            .as_str(),
        "Failed to fetch notification details from Github"
    );
    assert_eq!(integration_connection.notifications_sync_failures, 1);
    assert_eq!(
        integration_connection.status,
        IntegrationConnectionStatus::Validated
    );
}
