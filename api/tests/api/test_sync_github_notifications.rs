#![allow(clippy::too_many_arguments)]
use graphql_client::Response;
use rstest::*;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::github::GithubConfig,
    },
    notification::{
        Notification, NotificationSourceKind, NotificationStatus, NotificationWithTask,
    },
    third_party::integrations::github::{GithubNotification, GithubNotificationItem},
};

use universal_inbox_api::{
    configuration::Settings, integrations::github::graphql::pull_request_query,
};

use crate::helpers::integration_connection::OAuthCredentialFixture;
use crate::helpers::{
    TestedApp,
    integration_connection::{create_and_mock_integration_connection, github_oauth_credential},
    notification::{
        github::{
            assert_sync_notifications, create_notification_from_github_notification,
            github_pull_request_123_response, mock_github_notifications_service,
            mock_github_pull_request_query, sync_github_notifications,
        },
        sync_notifications,
    },
    rest::get_resource,
    settings, tested_app_with_local_auth,
    user::create_user_and_login,
};

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
