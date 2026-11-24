#![allow(clippy::too_many_arguments)]
use chrono::{TimeZone, Utc};
use graphql_client::{Error, Response};
use http::StatusCode;
use rstest::*;

use universal_inbox::{
    integration_connection::{
        IntegrationConnectionStatus, config::IntegrationConnectionConfig,
        integrations::github::GithubConfig, provider::IntegrationProviderKind,
    },
    notification::{
        Notification, NotificationSourceKind, NotificationStatus, NotificationWithTask,
    },
    third_party::{
        integrations::github::{
            GithubNotification, GithubNotificationItem, GithubNotificationSubject,
        },
        item::ThirdPartyItemData,
    },
};

use universal_inbox_api::{
    configuration::Settings,
    integrations::github::graphql::{
        discussion_query, pull_request_comments_query, pull_request_query,
        pull_request_review_thread_comments_query, pull_request_review_threads_query,
    },
};

use crate::helpers::integration_connection::OAuthCredentialFixture;
use crate::helpers::{
    TestedApp,
    auth::{AuthenticatedApp, authenticated_app},
    integration_connection::{
        create_and_mock_integration_connection, get_integration_connection_per_provider,
        github_oauth_credential,
    },
    notification::{
        github::{
            assert_sync_notifications, create_notification_from_github_notification,
            github_discussion_123_comments_response, github_discussion_123_response,
            github_discussion_comment_1_replies_page_2_response, github_notification,
            github_pull_request_123_comments_page_2_response, github_pull_request_123_response,
            github_pull_request_123_review_threads_page_2_response,
            github_pull_request_123_with_comments_response,
            github_pull_request_review_thread_1_comments_page_2_response,
            mock_github_discussion_comment_replies_query, mock_github_discussion_comments_query,
            mock_github_discussion_query, mock_github_notifications_service,
            mock_github_pull_request_comments_query, mock_github_pull_request_query,
            mock_github_pull_request_review_thread_comments_query,
            mock_github_pull_request_review_threads_query, sync_github_notifications,
        },
        list_notifications, sync_notifications, sync_notifications_response,
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

#[rstest]
#[tokio::test]
async fn test_sync_pull_request_notification_with_comments_and_review_threads(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    mut github_notification: Box<GithubNotification>,
    github_pull_request_123_with_comments_response: Response<pull_request_query::ResponseData>,
    github_pull_request_123_comments_page_2_response: Response<
        pull_request_comments_query::ResponseData,
    >,
    github_pull_request_123_review_threads_page_2_response: Response<
        pull_request_review_threads_query::ResponseData,
    >,
    github_pull_request_review_thread_1_comments_page_2_response: Response<
        pull_request_review_thread_comments_query::ResponseData,
    >,
    github_oauth_credential: OAuthCredentialFixture,
) {
    github_notification.subject.url = Some(
        "https://api.github.com/repos/octokit/octokit.rb/pulls/123"
            .parse()
            .unwrap(),
    );
    github_notification.subject.r#type = "PullRequest".to_string();

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
    mock_github_pull_request_query(
        &app.app.github_mock_server,
        "octokit".to_string(),
        "octokit.rb".to_string(),
        123,
        &github_pull_request_123_with_comments_response,
    )
    .await;
    mock_github_pull_request_comments_query(
        &app.app.github_mock_server,
        "octokit".to_string(),
        "octokit.rb".to_string(),
        123,
        Some("comments-cursor-1".to_string()),
        &github_pull_request_123_comments_page_2_response,
    )
    .await;
    mock_github_pull_request_review_threads_query(
        &app.app.github_mock_server,
        "octokit".to_string(),
        "octokit.rb".to_string(),
        123,
        Some("threads-cursor-1".to_string()),
        &github_pull_request_123_review_threads_page_2_response,
    )
    .await;
    mock_github_pull_request_review_thread_comments_query(
        &app.app.github_mock_server,
        "PRRT_1".to_string(),
        Some("thread-comments-cursor-1".to_string()),
        &github_pull_request_review_thread_1_comments_page_2_response,
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
    let ThirdPartyItemData::GithubNotification(github_notification) =
        &notifications[0].source_item.data
    else {
        unreachable!("Expected a Github notification");
    };
    let Some(GithubNotificationItem::GithubPullRequest(pull_request)) = &github_notification.item
    else {
        unreachable!("Expected a GithubPullRequest notification");
    };

    // All comment pages are fetched, minimized comments dropped
    let comments: Vec<(Option<&str>, &str)> = pull_request
        .comments
        .iter()
        .map(|comment| (comment.id.as_deref(), comment.body.as_str()))
        .collect();
    assert_eq!(
        comments,
        vec![
            (Some("IC_1"), "<p>first comment</p>"),
            (Some("IC_3"), "<p>a late comment</p>")
        ]
    );

    assert_eq!(pull_request.reviews.len(), 1);
    assert_eq!(pull_request.reviews[0].id.as_deref(), Some("PRR_1"));
    assert_eq!(pull_request.reviews[0].body, "<p>Looks good overall</p>");
    assert_eq!(
        pull_request.reviews[0].submitted_at,
        Some(Utc.with_ymd_and_hms(2014, 11, 7, 22, 3, 0).unwrap())
    );

    // All review thread pages and thread comment pages are fetched
    let threads: Vec<(&str, Vec<&str>)> = pull_request
        .review_threads
        .iter()
        .map(|thread| {
            (
                thread.id.as_str(),
                thread.comments.iter().map(|c| c.id.as_str()).collect(),
            )
        })
        .collect();
    assert_eq!(
        threads,
        vec![("PRRT_1", vec!["RC_1", "RC_2"]), ("PRRT_2", vec!["RC_3"])]
    );
    let first_thread = &pull_request.review_threads[0];
    assert_eq!(first_thread.path, "lib/octokit/authenticator.rb");
    assert_eq!(
        (first_thread.start_line, first_thread.line),
        (Some(13), Some(15))
    );
    assert!(first_thread.diff_hunk.starts_with("@@ -10,6 +10,8 @@"));
    assert!(!first_thread.is_resolved);
    // An outdated thread falls back to its original lines
    let second_thread = &pull_request.review_threads[1];
    assert!(second_thread.is_resolved && second_thread.is_outdated);
    assert_eq!(second_thread.line, Some(20));
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
