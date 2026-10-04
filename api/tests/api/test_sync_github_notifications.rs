#![allow(clippy::too_many_arguments)]
use chrono::{TimeDelta, TimeZone, Utc};
use graphql_client::{Error, Response};
use http::StatusCode;
use rstest::*;
use tokio::time::{Duration, sleep};

use universal_inbox::{
    integration_connection::{
        IntegrationConnectionStatus,
        config::IntegrationConnectionConfig,
        integrations::{github::GithubConfig, todoist::TodoistConfig},
        provider::IntegrationProviderKind,
    },
    notification::{
        Notification, NotificationSourceKind, NotificationStatus, NotificationWithTask,
        service::NotificationPatch,
    },
    third_party::{
        integrations::{
            github::{GithubNotification, GithubNotificationItem, GithubNotificationSubject},
            todoist::TodoistItem,
        },
        item::ThirdPartyItemData,
    },
};

use universal_inbox_api::{
    configuration::Settings,
    integrations::{
        github::graphql::{discussion_query, pull_request_query},
        todoist::TodoistSyncResponse,
    },
    repository::integration_connection::TOO_MANY_SYNC_FAILURES_ERROR_MESSAGE,
};

use crate::helpers::integration_connection::OAuthCredentialFixture;
use crate::helpers::third_party::create_task_third_party_item;
use crate::helpers::{
    TestedApp,
    auth::{AuthenticatedApp, authenticated_app},
    integration_connection::{
        create_and_mock_integration_connection,
        create_and_mock_integration_connection_with_backoff, create_integration_connection,
        get_integration_connection_per_provider, github_oauth_credential, todoist_oauth_credential,
    },
    notification::{
        github::{
            assert_sync_notifications, create_notification_from_github_notification,
            github_discussion_123_comments_response, github_discussion_123_response,
            github_discussion_comment_1_replies_page_2_response, github_notification,
            github_pull_request_123_no_commits_response, github_pull_request_123_response,
            mock_github_discussion_comment_replies_query, mock_github_discussion_comments_query,
            mock_github_discussion_query, mock_github_notifications_service,
            mock_github_pull_request_query, sync_github_notifications,
        },
        list_notifications, sync_notifications, sync_notifications_response, update_notification,
    },
    rest::get_resource,
    settings,
    task::todoist::{
        mock_todoist_sync_resources_service, sync_todoist_projects_response, todoist_item,
    },
    tested_app_with_local_auth,
    user::create_user_and_login,
};
use wiremock::{Mock, ResponseTemplate};

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
    github_oauth_credential: OAuthCredentialFixture,
    todoist_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    let _integration_connection = create_and_mock_integration_connection(
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

    let github_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        &settings,
        github_oauth_credential,
        None,
        None,
    )
    .await;

    let existing_notification = create_notification_from_github_notification(
        &app.app,
        &sync_github_notifications[1],
        app.user.id,
        github_integration_connection.id,
    )
    .await;
    update_notification(
        &app,
        existing_notification.id,
        &NotificationPatch {
            snoozed_until: Some(Utc.with_ymd_and_hms(2064, 1, 1, 0, 0, 0).unwrap()),
            task_id: Some(existing_todoist_task.id),
            ..NotificationPatch::default()
        },
        app.user.id,
    )
    .await;

    let _github_notifications_mock = mock_github_notifications_service(
        &app.app.github_mock_server,
        "1",
        &sync_github_notifications,
    )
    .await;
    let empty_result = Vec::<GithubNotification>::new();
    let _github_notifications_mock2 =
        mock_github_notifications_service(&app.app.github_mock_server, "2", &empty_result).await;

    let _github_pull_request_123_query_mock = mock_github_pull_request_query(
        &app.app.github_mock_server,
        "octokit".to_string(),
        "octokit.rb".to_string(),
        123,
        &github_pull_request_123_response,
    )
    .await;

    let notifications: Vec<Notification> = sync_notifications(
        &app.client,
        &app.app.api_address,
        Some(NotificationSourceKind::Github),
        false,
    )
    .await;

    assert_eq!(notifications.len(), sync_github_notifications.len());
    assert_sync_notifications(
        &notifications,
        &sync_github_notifications,
        app.user.id,
        Some(GithubNotificationItem::GithubPullRequest(
            github_pull_request_123_response
                .data
                .unwrap()
                .try_into()
                .unwrap(),
        )),
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
        updated_notification.source_item.source_id,
        existing_notification.source_item.source_id
    );
    assert_eq!(updated_notification.status, NotificationStatus::Read);
    assert_eq!(
        updated_notification.last_read_at,
        Some(Utc.with_ymd_and_hms(2014, 11, 7, 23, 2, 45).unwrap())
    );
    assert_eq!(updated_notification.kind, NotificationSourceKind::Github);
    // `snoozed_until` and `task_id` should not be reset
    assert_eq!(
        updated_notification.snoozed_until,
        Some(Utc.with_ymd_and_hms(2064, 1, 1, 0, 0, 0).unwrap())
    );
    assert_eq!(
        updated_notification.task.as_ref().map(|t| t.id),
        Some(existing_todoist_task.id)
    );

    let integration_connection = get_integration_connection_per_provider(
        &app,
        app.user.id,
        IntegrationProviderKind::Github,
        None,
        None,
    )
    .await
    .unwrap();
    assert!(
        integration_connection
            .last_notifications_sync_started_at
            .is_some()
    );
    assert!(
        integration_connection
            .last_notifications_sync_completed_at
            .is_some()
    );
    assert!(
        integration_connection
            .last_notifications_sync_failed_at
            .is_none()
    );
    assert!(
        integration_connection
            .last_notifications_sync_failure_message
            .is_none()
    );
    assert_eq!(integration_connection.notifications_sync_failures, 0);
    assert_eq!(
        integration_connection.status,
        IntegrationConnectionStatus::Validated
    );
    assert!(integration_connection.failure_message.is_none(),);
}

/// Github does not move its `last_read_at` when a thread is marked as done: a deleted
/// notification brought back by new activity must keep the delete time as its read marker,
/// unless Github reports a later read.
#[rstest]
#[case::github_marker_is_older(false)]
#[case::github_marker_is_newer(true)]
#[tokio::test]
async fn test_sync_notifications_should_keep_latest_last_read_at_of_deleted_notification(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    // Vec[GithubNotification { source_id: "123", ... }, GithubNotification { source_id: "456", ... } ]
    mut sync_github_notifications: Vec<GithubNotification>,
    github_pull_request_123_response: Response<pull_request_query::ResponseData>,
    github_oauth_credential: OAuthCredentialFixture,
    #[case] github_marker_is_newer: bool,
) {
    let app = authenticated_app.await;
    let github_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        &settings,
        github_oauth_credential,
        None,
        None,
    )
    .await;

    let existing_notification = create_notification_from_github_notification(
        &app.app,
        &sync_github_notifications[1],
        app.user.id,
        github_integration_connection.id,
    )
    .await;
    let deleted_notification = update_notification(
        &app,
        existing_notification.id,
        &NotificationPatch {
            status: Some(NotificationStatus::Deleted),
            ..NotificationPatch::default()
        },
        app.user.id,
    )
    .await;
    let deleted_at = deleted_notification
        .last_read_at
        .expect("deleting a notification must set its last_read_at");
    assert!(deleted_at > existing_notification.last_read_at.unwrap());

    // New activity brings the thread back as unread
    let newer_github_last_read_at = deleted_at + TimeDelta::hours(1);
    sync_github_notifications[1].unread = true;
    sync_github_notifications[1].updated_at = deleted_at + TimeDelta::hours(2);
    if github_marker_is_newer {
        sync_github_notifications[1].last_read_at = Some(newer_github_last_read_at);
    }

    mock_github_notifications_service(&app.app.github_mock_server, "1", &sync_github_notifications)
        .await;
    let empty_result = Vec::<GithubNotification>::new();
    mock_github_notifications_service(&app.app.github_mock_server, "2", &empty_result).await;
    mock_github_pull_request_query(
        &app.app.github_mock_server,
        "octokit".to_string(),
        "octokit.rb".to_string(),
        123,
        &github_pull_request_123_response,
    )
    .await;

    sync_notifications(
        &app.client,
        &app.app.api_address,
        Some(NotificationSourceKind::Github),
        false,
    )
    .await;

    let synced_notification: Box<NotificationWithTask> = get_resource(
        &app.client,
        &app.app.api_address,
        "notifications",
        existing_notification.id.into(),
    )
    .await;
    assert_eq!(synced_notification.status, NotificationStatus::Unread);
    assert_eq!(
        synced_notification.last_read_at,
        Some(if github_marker_is_newer {
            newer_github_last_read_at
        } else {
            deleted_at
        })
    );
}

#[rstest]
#[tokio::test]
async fn test_sync_notifications_should_handle_pull_request_without_commits(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    // Vec[GithubNotification { source_id: "123", ... }, GithubNotification { source_id: "456", ... } ]
    sync_github_notifications: Vec<GithubNotification>,
    github_pull_request_123_no_commits_response: Response<pull_request_query::ResponseData>,
    github_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;

    let _github_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        &settings,
        github_oauth_credential,
        None,
        None,
    )
    .await;

    let _github_notifications_mock = mock_github_notifications_service(
        &app.app.github_mock_server,
        "1",
        &sync_github_notifications,
    )
    .await;
    let empty_result = Vec::<GithubNotification>::new();
    let _github_notifications_mock2 =
        mock_github_notifications_service(&app.app.github_mock_server, "2", &empty_result).await;

    // The pull request associated with notification "123" has no commit at all
    // (e.g. an empty/administrative PR): syncing should still succeed and simply
    // store `latest_commit: None` instead of failing the whole sync.
    let _github_pull_request_123_query_mock = mock_github_pull_request_query(
        &app.app.github_mock_server,
        "octokit".to_string(),
        "octokit.rb".to_string(),
        123,
        &github_pull_request_123_no_commits_response,
    )
    .await;

    let notifications: Vec<Notification> = sync_notifications(
        &app.client,
        &app.app.api_address,
        Some(NotificationSourceKind::Github),
        false,
    )
    .await;

    assert_eq!(notifications.len(), sync_github_notifications.len());
    assert_sync_notifications(
        &notifications,
        &sync_github_notifications,
        app.user.id,
        Some(GithubNotificationItem::GithubPullRequest(
            github_pull_request_123_no_commits_response
                .data
                .unwrap()
                .try_into()
                .unwrap(),
        )),
    );

    let integration_connection = get_integration_connection_per_provider(
        &app,
        app.user.id,
        IntegrationProviderKind::Github,
        None,
        None,
    )
    .await
    .unwrap();
    assert!(
        integration_connection
            .last_notifications_sync_completed_at
            .is_some()
    );
    assert!(
        integration_connection
            .last_notifications_sync_failed_at
            .is_none()
    );
    assert_eq!(integration_connection.notifications_sync_failures, 0);
    assert!(integration_connection.failure_message.is_none());
}

/// Stored third party data can stop matching the current types (eg. after a serde or
/// dependency upgrade). Syncing the same item again must overwrite the broken data with the
/// fresh upstream one instead of failing the sync.
#[rstest]
#[tokio::test]
async fn test_sync_notifications_should_heal_undecodable_existing_third_party_item(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    // Vec[GithubNotification { source_id: "123", ... }, GithubNotification { source_id: "456", ... } ]
    sync_github_notifications: Vec<GithubNotification>,
    github_pull_request_123_response: Response<pull_request_query::ResponseData>,
    github_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;

    let github_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        &settings,
        github_oauth_credential,
        None,
        None,
    )
    .await;

    let existing_notification = create_notification_from_github_notification(
        &app.app,
        &sync_github_notifications[1],
        app.user.id,
        github_integration_connection.id,
    )
    .await;
    sqlx::query(
        r#"UPDATE third_party_item SET data = '{"type": "GithubNotification", "content": {"unexpected": true}}'::jsonb WHERE id = $1"#,
    )
    .bind(existing_notification.source_item.id.0)
    .execute(&*app.app.repository.pool)
    .await
    .expect("Failed to corrupt the third party item data");

    let _github_notifications_mock = mock_github_notifications_service(
        &app.app.github_mock_server,
        "1",
        &sync_github_notifications,
    )
    .await;
    let empty_result = Vec::<GithubNotification>::new();
    let _github_notifications_mock2 =
        mock_github_notifications_service(&app.app.github_mock_server, "2", &empty_result).await;

    let _github_pull_request_123_query_mock = mock_github_pull_request_query(
        &app.app.github_mock_server,
        "octokit".to_string(),
        "octokit.rb".to_string(),
        123,
        &github_pull_request_123_response,
    )
    .await;

    let notifications: Vec<Notification> = sync_notifications(
        &app.client,
        &app.app.api_address,
        Some(NotificationSourceKind::Github),
        false,
    )
    .await;

    assert_eq!(notifications.len(), sync_github_notifications.len());

    let healed_notification: Box<NotificationWithTask> = get_resource(
        &app.client,
        &app.app.api_address,
        "notifications",
        existing_notification.id.into(),
    )
    .await;
    // The broken row is updated in place, not duplicated
    assert_eq!(
        healed_notification.source_item.id,
        existing_notification.source_item.id
    );
    assert_eq!(
        healed_notification.source_item.data,
        ThirdPartyItemData::GithubNotification(Box::new(sync_github_notifications[1].clone()))
    );
    // Downstream notification update happened as for a regular update
    assert_eq!(healed_notification.status, NotificationStatus::Read);
    assert_eq!(
        healed_notification.last_read_at,
        Some(Utc.with_ymd_and_hms(2014, 11, 7, 23, 2, 45).unwrap())
    );
}

#[rstest]
#[tokio::test]
async fn test_sync_notifications_should_mark_deleted_notification_without_subscription(
    settings: Settings,
    #[future] tested_app_with_local_auth: TestedApp,
    // Vec[GithubNotification { source_id: "123", ... }, GithubNotification { source_id: "456", ... } ]
    sync_github_notifications: Vec<GithubNotification>,
    github_pull_request_123_response: Response<pull_request_query::ResponseData>,
    github_oauth_credential: OAuthCredentialFixture,
) {
    let app = tested_app_with_local_auth.await;

    let (other_client, other_user) = create_user_and_login(
        &app,
        "jane@doe.net".parse().unwrap(),
        "Very-harD-pasSword-5",
    )
    .await;

    let other_github_integration_connection = create_and_mock_integration_connection(
        &app,
        other_user.id,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        &settings,
        github_oauth_credential.clone(),
        None,
        None,
    )
    .await;

    let mut other_existing_github_notification = sync_github_notifications[1].clone();
    other_existing_github_notification.id = "789".to_string();
    other_existing_github_notification.unread = true;
    let other_user_existing_notification = create_notification_from_github_notification(
        &app,
        &other_existing_github_notification,
        other_user.id,
        other_github_integration_connection.id,
    )
    .await;

    let (client, user) = create_user_and_login(
        &app,
        "john@doe.net".parse().unwrap(),
        "Very-harD-pasSword-5",
    )
    .await;

    let github_integration_connection = create_and_mock_integration_connection(
        &app,
        user.id,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        &settings,
        github_oauth_credential,
        None,
        None,
    )
    .await;

    for github_notification in sync_github_notifications.iter() {
        create_notification_from_github_notification(
            &app,
            github_notification,
            user.id,
            github_integration_connection.id,
        )
        .await;
    }

    // to be deleted during sync
    let mut existing_github_notification = sync_github_notifications[1].clone();
    existing_github_notification.id = "789".to_string();
    let existing_notification = create_notification_from_github_notification(
        &app,
        &existing_github_notification,
        user.id,
        github_integration_connection.id,
    )
    .await;

    let _github_notifications_mock =
        mock_github_notifications_service(&app.github_mock_server, "1", &sync_github_notifications)
            .await;
    let empty_result = Vec::<GithubNotification>::new();
    let _github_notifications_mock2 =
        mock_github_notifications_service(&app.github_mock_server, "2", &empty_result).await;

    // Sync of Github notification 123 will trigger a query of the associated pull request
    // sync_github_notifications[1] won't trigger any query
    let _github_pull_request_123_query_mock = mock_github_pull_request_query(
        &app.github_mock_server,
        "octokit".to_string(),
        "octokit.rb".to_string(),
        123,
        &github_pull_request_123_response,
    )
    .await;

    let notifications: Vec<Notification> = sync_notifications(
        &client,
        &app.api_address,
        Some(NotificationSourceKind::Github),
        false,
    )
    .await;

    assert_eq!(notifications.len(), sync_github_notifications.len());
    assert_sync_notifications(
        &notifications,
        &sync_github_notifications,
        user.id,
        Some(GithubNotificationItem::GithubPullRequest(
            github_pull_request_123_response
                .data
                .unwrap()
                .try_into()
                .unwrap(),
        )),
    );

    let deleted_notification: Box<NotificationWithTask> = get_resource(
        &client,
        &app.api_address,
        "notifications",
        existing_notification.id.into(),
    )
    .await;
    assert_eq!(deleted_notification.id, existing_notification.id);
    assert_eq!(deleted_notification.status, NotificationStatus::Deleted);

    let refreshed_other_user_existing_notification: Box<NotificationWithTask> = get_resource(
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
    github_oauth_credential: OAuthCredentialFixture,
    #[case] trigger_sync_when_listing_notifications: bool,
) {
    let app = authenticated_app.await;
    let github_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        &settings,
        github_oauth_credential,
        None,
        None,
    )
    .await;
    let existing_notification = create_notification_from_github_notification(
        &app.app,
        &sync_github_notifications[1],
        app.user.id,
        github_integration_connection.id,
    )
    .await;
    update_notification(
        &app,
        existing_notification.id,
        &NotificationPatch {
            status: Some(NotificationStatus::Unread),
            ..NotificationPatch::default()
        },
        app.user.id,
    )
    .await;

    let _github_notifications_mock = mock_github_notifications_service(
        &app.app.github_mock_server,
        "1",
        &sync_github_notifications,
    )
    .await;
    let empty_result = Vec::<GithubNotification>::new();
    let _github_notifications_mock2 =
        mock_github_notifications_service(&app.app.github_mock_server, "2", &empty_result).await;
    let _github_pull_request_123_query_mock = mock_github_pull_request_query(
        &app.app.github_mock_server,
        "octokit".to_string(),
        "octokit.rb".to_string(),
        123,
        &github_pull_request_123_response,
    )
    .await;

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
        let response = sync_notifications_response(
            &app.client,
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

    // Triggering a new sync should not actually sync again
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.app.github_mock_server)
        .await;
    let response = sync_notifications_response(
        &app.client,
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
        None,
        None,
    )
    .await;

    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.app.github_mock_server)
        .await;

    let response = sync_notifications_response(
        &app.client,
        &app.app.api_address,
        Some(NotificationSourceKind::Github),
        false, // synchronously
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
}

#[rstest]
#[tokio::test]
async fn test_sync_all_notifications_with_synchronization_disabled(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    github_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Github(GithubConfig::disabled()),
        &settings,
        github_oauth_credential,
        None,
        None,
    )
    .await;

    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.app.github_mock_server)
        .await;

    let response = sync_notifications_response(
        &app.client,
        &app.app.api_address,
        Some(NotificationSourceKind::Github),
        false, // synchronously
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
}

#[rstest]
#[tokio::test]
async fn test_sync_all_notifications_asynchronously_in_error(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    github_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    // Set first_notifications_sync_failed_at beyond the failure window (dev config = 1h)
    // so the next failure will trigger the Failing status
    create_and_mock_integration_connection_with_backoff(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        &settings,
        github_oauth_credential,
        Some(5),
        Some(Utc::now() - TimeDelta::hours(2)),
        None,
    )
    .await;

    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(400))
        .mount(&app.app.github_mock_server)
        .await;
    let response = sync_notifications_response(
        &app.client,
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

    let integration_connection = get_integration_connection_per_provider(
        &app,
        app.user.id,
        IntegrationProviderKind::Github,
        None,
        None,
    )
    .await
    .unwrap();
    assert!(
        integration_connection
            .last_notifications_sync_started_at
            .is_some()
    );
    assert!(
        integration_connection
            .last_notifications_sync_completed_at
            .is_none()
    );
    assert!(
        integration_connection
            .last_notifications_sync_failed_at
            .is_some()
    );
    assert_eq!(
        integration_connection
            .last_notifications_sync_failure_message
            .unwrap()
            .as_str(),
        "Failed to fetch notifications from Github"
    );
    assert_eq!(integration_connection.notifications_sync_failures, 6);
    assert!(
        integration_connection
            .first_notifications_sync_failed_at
            .is_some()
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
    github_discussion_123_response: Response<discussion_query::ResponseData>,
    github_oauth_credential: OAuthCredentialFixture,
) {
    github_notification.subject = GithubNotificationSubject {
        title: "test discussion".to_string(),
        url: Some(
            "https://api.github.com/repos/octokit/octokit.rb/discussions/123"
                .parse()
                .unwrap(),
        ),
        latest_comment_url: None,
        r#type: "Discussion".to_string(),
    };

    let app = authenticated_app.await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        &settings,
        github_oauth_credential,
        None,
        None,
    )
    .await;

    let github_notifications_response = vec![*github_notification];
    let _github_notifications_mock = mock_github_notifications_service(
        &app.app.github_mock_server,
        "1",
        &github_notifications_response,
    )
    .await;

    let _github_discussion_query_mock = mock_github_discussion_query(
        &app.app.github_mock_server,
        "octokit".to_string(),
        "octokit.rb".to_string(),
        123,
        &github_discussion_123_response,
    )
    .await;
    for (page, after) in [(1, None), (2, Some("comments-cursor-1".to_string()))] {
        mock_github_discussion_comments_query(
            &app.app.github_mock_server,
            "octokit".to_string(),
            "octokit.rb".to_string(),
            123,
            after,
            &github_discussion_123_comments_response(page),
        )
        .await;
    }
    mock_github_discussion_comment_replies_query(
        &app.app.github_mock_server,
        "DC_1".to_string(),
        Some("replies-cursor-1".to_string()),
        &github_discussion_comment_1_replies_page_2_response(),
    )
    .await;

    let notifications: Vec<Notification> = sync_notifications(
        &app.client,
        &app.app.api_address,
        Some(NotificationSourceKind::Github),
        false,
    )
    .await;

    assert_eq!(notifications.len(), 1);

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
    assert_eq!(notifications[0].kind, NotificationSourceKind::Github);
    match &notifications[0].source_item.data {
        ThirdPartyItemData::GithubNotification(github_notification) => {
            match &github_notification.item {
                Some(GithubNotificationItem::GithubDiscussion(discussion)) => {
                    assert_eq!(discussion.title, "test discussion");
                    assert_eq!(
                        discussion.url,
                        "https://github.com/octocat/universal-inbox/discussions/1"
                            .parse()
                            .unwrap()
                    );
                    let category = discussion
                        .category
                        .as_ref()
                        .expect("Discussion should have a category");
                    assert_eq!(category.name, "Q&A");
                    assert_eq!(category.emoji.as_deref(), Some(":pray:"));
                    assert_eq!(category.slug, "q-a");
                    assert!(category.is_answerable);

                    // All comment and reply pages are fetched, minimized ones dropped
                    let ids: Vec<&str> =
                        discussion.comments.iter().map(|c| c.id.as_str()).collect();
                    assert_eq!(ids, vec!["DC_1", "DC_4", "DC_6"]);
                    let first = &discussion.comments[0];
                    assert_eq!(first.comment.body, "<p>first comment</p>");
                    assert!(!first.is_answer);
                    assert_eq!(first.replies_count, 3);
                    let replies: Vec<&str> =
                        first.replies.iter().map(|r| r.body.as_str()).collect();
                    assert_eq!(replies, vec!["<p>a reply</p>", "<p>a late reply</p>"]);
                    assert!(discussion.comments[1].is_answer);
                    assert!(discussion.comments[1].replies.is_empty());
                }
                _ => unreachable!("Expected a GithubDiscussion notification"),
            }
        }
        _ => unreachable!("Expected a GithubDiscussion notification"),
    }
}

#[rstest]
#[tokio::test]
async fn test_sync_discussion_notification_stops_on_non_advancing_cursor(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    mut github_notification: Box<GithubNotification>,
    github_discussion_123_response: Response<discussion_query::ResponseData>,
    github_oauth_credential: OAuthCredentialFixture,
) {
    github_notification.subject = GithubNotificationSubject {
        title: "test discussion".to_string(),
        url: Some(
            "https://api.github.com/repos/octokit/octokit.rb/discussions/123"
                .parse()
                .unwrap(),
        ),
        latest_comment_url: None,
        r#type: "Discussion".to_string(),
    };

    let app = authenticated_app.await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        &settings,
        github_oauth_credential,
        None,
        None,
    )
    .await;

    let github_notifications_response = vec![*github_notification];
    let _github_notifications_mock = mock_github_notifications_service(
        &app.app.github_mock_server,
        "1",
        &github_notifications_response,
    )
    .await;

    let _github_discussion_query_mock = mock_github_discussion_query(
        &app.app.github_mock_server,
        "octokit".to_string(),
        "octokit.rb".to_string(),
        123,
        &github_discussion_123_response,
    )
    .await;
    // The page after `comments-cursor-1` points back to `comments-cursor-1`
    for after in [None, Some("comments-cursor-1".to_string())] {
        mock_github_discussion_comments_query(
            &app.app.github_mock_server,
            "octokit".to_string(),
            "octokit.rb".to_string(),
            123,
            after,
            &github_discussion_123_comments_response(1),
        )
        .await;
    }
    mock_github_discussion_comment_replies_query(
        &app.app.github_mock_server,
        "DC_1".to_string(),
        Some("replies-cursor-1".to_string()),
        &github_discussion_comment_1_replies_page_2_response(),
    )
    .await;

    let notifications: Vec<Notification> = sync_notifications(
        &app.client,
        &app.app.api_address,
        Some(NotificationSourceKind::Github),
        false,
    )
    .await;

    assert_eq!(notifications.len(), 1);

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
    assert_eq!(notifications[0].kind, NotificationSourceKind::Github);
    match &notifications[0].source_item.data {
        ThirdPartyItemData::GithubNotification(github_notification) => {
            match &github_notification.item {
                Some(GithubNotificationItem::GithubDiscussion(discussion)) => {
                    assert_eq!(discussion.title, "test discussion");
                    assert_eq!(
                        discussion.url,
                        "https://github.com/octocat/universal-inbox/discussions/1"
                            .parse()
                            .unwrap()
                    );
                    let category = discussion
                        .category
                        .as_ref()
                        .expect("Discussion should have a category");
                    assert_eq!(category.name, "Q&A");
                    assert_eq!(category.emoji.as_deref(), Some(":pray:"));
                    assert_eq!(category.slug, "q-a");
                    assert!(category.is_answerable);

                    let ids: Vec<&str> =
                        discussion.comments.iter().map(|c| c.id.as_str()).collect();
                    assert_eq!(ids, vec!["DC_1", "DC_4", "DC_1", "DC_4"]);
                }
                _ => unreachable!("Expected a GithubDiscussion notification"),
            }
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
    github_oauth_credential: OAuthCredentialFixture,
) {
    github_notification.subject = GithubNotificationSubject {
        title: "test discussion".to_string(),
        url: Some(
            "https://api.github.com/repos/octokit/octokit.rb/discussions/123"
                .parse()
                .unwrap(),
        ),
        latest_comment_url: None,
        r#type: "Discussion".to_string(),
    };

    let app = authenticated_app.await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        &settings,
        github_oauth_credential,
        None,
        None,
    )
    .await;

    let github_notifications_response = vec![*github_notification];
    let _github_notifications_mock = mock_github_notifications_service(
        &app.app.github_mock_server,
        "1",
        &github_notifications_response,
    )
    .await;

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
    let _github_discussion_query_mock = mock_github_discussion_query(
        &app.app.github_mock_server,
        "octokit".to_string(),
        "octokit.rb".to_string(),
        123,
        &error_response,
    )
    .await;

    let response = sync_notifications_response(
        &app.client,
        &app.app.api_address,
        Some(NotificationSourceKind::Github),
        false,
    )
    .await;

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

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

    assert_eq!(notifications.len(), 0);

    let integration_connection = get_integration_connection_per_provider(
        &app,
        app.user.id,
        IntegrationProviderKind::Github,
        None,
        None,
    )
    .await
    .unwrap();
    assert!(
        integration_connection
            .last_notifications_sync_started_at
            .is_some()
    );
    assert!(
        integration_connection
            .last_notifications_sync_completed_at
            .is_none()
    );
    assert!(
        integration_connection
            .last_notifications_sync_failed_at
            .is_some()
    );
    assert_eq!(
        integration_connection
            .last_notifications_sync_failure_message
            .unwrap()
            .as_str(),
        "Failed to fetch notifications from Github"
    );
    assert_eq!(integration_connection.notifications_sync_failures, 1);
    assert_eq!(
        integration_connection.status,
        IntegrationConnectionStatus::Validated
    );
}
