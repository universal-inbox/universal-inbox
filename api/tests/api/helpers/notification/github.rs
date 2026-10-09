use chrono::{TimeZone, Utc};
use graphql_client::{GraphQLQuery, Response};
use pretty_assertions::assert_eq;
use rstest::*;
use url::Url;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use universal_inbox::{
    HasHtmlUrl,
    integration_connection::IntegrationConnectionId,
    notification::{Notification, NotificationSourceKind, NotificationStatus},
    third_party::{
        integrations::github::{GithubNotification, GithubNotificationItem},
        item::ThirdPartyItemData,
    },
    user::UserId,
};

use universal_inbox_api::integrations::github::graphql::{
    DiscussionCommentRepliesQuery, DiscussionCommentsQuery, DiscussionQuery,
    PullRequestCommentsQuery, PullRequestQuery, PullRequestReviewThreadCommentsQuery,
    PullRequestReviewThreadsQuery, discussion_comment_replies_query, discussion_comments_query,
    discussion_query, pull_request_comments_query, pull_request_query,
    pull_request_review_thread_comments_query, pull_request_review_threads_query,
};

use crate::helpers::{
    QueryParamPresent, TestedApp, load_json_fixture_file,
    notification::create_notification_from_source_item,
};

pub async fn create_notification_from_github_notification(
    app: &TestedApp,
    github_notification: &GithubNotification,
    user_id: UserId,
    github_integration_connection_id: IntegrationConnectionId,
) -> Box<Notification> {
    create_notification_from_source_item(
        app,
        github_notification.id.to_string(),
        ThirdPartyItemData::GithubNotification(Box::new(github_notification.clone())),
        app.notification_service.read().await.github_service.clone(),
        user_id,
        github_integration_connection_id,
    )
    .await
}

pub async fn mock_github_notifications_service(
    github_mock_server: &MockServer,
    page: &str,
    result: &Vec<GithubNotification>,
) {
    Mock::given(method("GET"))
        .and(path("/notifications"))
        .and(header("accept", "application/vnd.github.v3+json"))
        .and(header("authorization", "Bearer github_test_access_token"))
        .and(wiremock::matchers::query_param("page", page))
        .and(QueryParamPresent("per_page".to_string()))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(result),
        )
        .mount(github_mock_server)
        .await;
}

pub async fn mock_github_pull_request_query(
    github_mock_server: &MockServer,
    owner: String,
    repository: String,
    pr_number: i64,
    result: &Response<pull_request_query::ResponseData>,
) {
    let expected_request_body = PullRequestQuery::build_query(pull_request_query::Variables {
        owner,
        repository,
        pr_number,
    });
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_json(&expected_request_body))
        .and(header(
            "accept",
            "application/vnd.github.merge-info-preview+json",
        ))
        .and(header("authorization", "Bearer github_test_access_token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(result),
        )
        .mount(github_mock_server)
        .await;
}

pub async fn mock_github_discussion_query(
    github_mock_server: &MockServer,
    owner: String,
    repository: String,
    discussion_number: i64,
    result: &Response<discussion_query::ResponseData>,
) {
    let expected_request_body = DiscussionQuery::build_query(discussion_query::Variables {
        owner,
        repository,
        discussion_number,
    });
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_json(&expected_request_body))
        .and(header(
            "accept",
            "application/vnd.github.merge-info-preview+json",
        ))
        .and(header("authorization", "Bearer github_test_access_token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(result),
        )
        .mount(github_mock_server)
        .await;
}

pub async fn mock_github_discussion_comments_query(
    github_mock_server: &MockServer,
    owner: String,
    repository: String,
    discussion_number: i64,
    after: Option<String>,
    result: &Response<discussion_comments_query::ResponseData>,
) {
    let expected_request_body =
        DiscussionCommentsQuery::build_query(discussion_comments_query::Variables {
            owner,
            repository,
            discussion_number,
            after,
        });
    mock_github_graphql_query(github_mock_server, &expected_request_body, result).await;
}

pub async fn mock_github_discussion_comment_replies_query(
    github_mock_server: &MockServer,
    comment_id: String,
    after: Option<String>,
    result: &Response<discussion_comment_replies_query::ResponseData>,
) {
    let expected_request_body =
        DiscussionCommentRepliesQuery::build_query(discussion_comment_replies_query::Variables {
            comment_id,
            after,
        });
    mock_github_graphql_query(github_mock_server, &expected_request_body, result).await;
}

pub async fn mock_github_pull_request_comments_query(
    github_mock_server: &MockServer,
    owner: String,
    repository: String,
    pr_number: i64,
    after: Option<String>,
    result: &Response<pull_request_comments_query::ResponseData>,
) {
    let expected_request_body =
        PullRequestCommentsQuery::build_query(pull_request_comments_query::Variables {
            owner,
            repository,
            pr_number,
            after,
        });
    mock_github_graphql_query(github_mock_server, &expected_request_body, result).await;
}

pub async fn mock_github_pull_request_review_threads_query(
    github_mock_server: &MockServer,
    owner: String,
    repository: String,
    pr_number: i64,
    after: Option<String>,
    result: &Response<pull_request_review_threads_query::ResponseData>,
) {
    let expected_request_body =
        PullRequestReviewThreadsQuery::build_query(pull_request_review_threads_query::Variables {
            owner,
            repository,
            pr_number,
            after,
        });
    mock_github_graphql_query(github_mock_server, &expected_request_body, result).await;
}

pub async fn mock_github_pull_request_review_thread_comments_query(
    github_mock_server: &MockServer,
    thread_id: String,
    after: Option<String>,
    result: &Response<pull_request_review_thread_comments_query::ResponseData>,
) {
    let expected_request_body = PullRequestReviewThreadCommentsQuery::build_query(
        pull_request_review_thread_comments_query::Variables { thread_id, after },
    );
    mock_github_graphql_query(github_mock_server, &expected_request_body, result).await;
}

async fn mock_github_graphql_query<B: serde::Serialize, R: serde::Serialize>(
    github_mock_server: &MockServer,
    expected_request_body: &B,
    result: &R,
) {
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_json(expected_request_body))
        .and(header(
            "accept",
            "application/vnd.github.merge-info-preview+json",
        ))
        .and(header("authorization", "Bearer github_test_access_token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(result),
        )
        .mount(github_mock_server)
        .await;
}

#[fixture]
pub fn sync_github_notifications() -> Vec<GithubNotification> {
    load_json_fixture_file("sync_github_notifications.json")
}

#[fixture]
pub fn github_pull_request_123_response() -> Response<pull_request_query::ResponseData> {
    load_json_fixture_file("github_pull_request_123_response.json")
}

#[fixture]
pub fn github_pull_request_123_no_commits_response() -> Response<pull_request_query::ResponseData> {
    load_json_fixture_file("github_pull_request_123_no_commits_response.json")
}

#[fixture]
pub fn github_pull_request_123_with_comments_response() -> Response<pull_request_query::ResponseData>
{
    load_json_fixture_file("github_pull_request_123_with_comments_response.json")
}

#[fixture]
pub fn github_pull_request_123_comments_page_2_response()
-> Response<pull_request_comments_query::ResponseData> {
    load_json_fixture_file("github_pull_request_123_comments_page_2_response.json")
}

#[fixture]
pub fn github_pull_request_123_review_threads_page_2_response()
-> Response<pull_request_review_threads_query::ResponseData> {
    load_json_fixture_file("github_pull_request_123_review_threads_page_2_response.json")
}

#[fixture]
pub fn github_pull_request_review_thread_1_comments_page_2_response()
-> Response<pull_request_review_thread_comments_query::ResponseData> {
    load_json_fixture_file("github_pull_request_review_thread_1_comments_page_2_response.json")
}

#[fixture]
pub fn github_discussion_123_response() -> Response<discussion_query::ResponseData> {
    load_json_fixture_file("github_discussion_123_response.json")
}

pub fn github_discussion_123_comments_response(
    page: usize,
) -> Response<discussion_comments_query::ResponseData> {
    load_json_fixture_file(&format!(
        "github_discussion_123_comments_page_{page}_response.json"
    ))
}

#[fixture]
pub fn github_discussion_comment_1_replies_page_2_response()
-> Response<discussion_comment_replies_query::ResponseData> {
    load_json_fixture_file("github_discussion_comment_1_replies_page_2_response.json")
}

pub fn assert_sync_notifications(
    notifications: &[Notification],
    sync_github_notifications: &[GithubNotification],
    expected_user_id: UserId,
    expected_notification_123_item: Option<GithubNotificationItem>,
) {
    for notification in notifications.iter() {
        assert_eq!(notification.user_id, expected_user_id);
        assert_eq!(notification.kind, NotificationSourceKind::Github);
        match notification.source_item.source_id.as_ref() {
            "123" => {
                assert_eq!(notification.title, "Add passkey authentication".to_string());
                assert_eq!(notification.status, NotificationStatus::Unread);
                assert_eq!(
                    notification.get_html_url(),
                    "https://github.com/octokit/octokit.rb/pull/123"
                        .parse::<Url>()
                        .unwrap()
                );
                assert_eq!(
                    notification.last_read_at,
                    Some(Utc.with_ymd_and_hms(2014, 11, 7, 22, 2, 45).unwrap())
                );
                assert_eq!(
                    notification.source_item.data,
                    ThirdPartyItemData::GithubNotification(Box::new(GithubNotification {
                        item: expected_notification_123_item.clone(),
                        ..sync_github_notifications[0].clone()
                    }))
                );
            }
            // This notification should be updated
            "456" => {
                assert_eq!(
                    notification.title,
                    "Load custom emoji from Slack".to_string()
                );
                assert_eq!(notification.status, NotificationStatus::Read);
                assert_eq!(
                    notification.get_html_url(),
                    "https://github.com/octokit/octokit.rb/issues/456"
                        .parse::<Url>()
                        .unwrap()
                );
                assert_eq!(
                    notification.last_read_at,
                    Some(Utc.with_ymd_and_hms(2014, 11, 7, 23, 2, 45).unwrap())
                );
                assert_eq!(
                    notification.source_item.data,
                    ThirdPartyItemData::GithubNotification(Box::new(GithubNotification {
                        item: None,
                        ..sync_github_notifications[1].clone()
                    }))
                );
            }
            _ => {}
        }
    }
}

#[fixture]
pub fn github_notification() -> Box<GithubNotification> {
    load_json_fixture_file("github_notification.json")
}
