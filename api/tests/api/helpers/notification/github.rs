use std::{env, fs};

use chrono::{TimeZone, Utc};
use graphql_client::{GraphQLQuery, Response};
use httpmock::{
    Method::{GET, POST},
    Mock, MockServer,
};
use reqwest::Client;
use rstest::*;
use url::Url;
use uuid::Uuid;

use universal_inbox::{
    notification::{
        integrations::github::GithubNotification, Notification, NotificationDetails,
        NotificationMetadata, NotificationStatus,
    },
    user::UserId,
};

use universal_inbox_api::integrations::github::graphql::{
    discussions_search_query, pull_request_query, DiscussionsSearchQuery, PullRequestQuery,
};

use crate::helpers::{load_json_fixture_file, rest::create_resource};

pub async fn create_notification_from_github_notification(
    client: &Client,
    api_address: &str,
    github_notification: &GithubNotification,
    user_id: UserId,
) -> Box<Notification> {
    create_resource(
        client,
        api_address,
        "notifications",
        Box::new(Notification {
            id: Uuid::new_v4().into(),
            title: github_notification.subject.title.clone(),
            source_id: github_notification.id.to_string(),
            source_html_url: Some(github_notification.get_html_url_from_metadata()),
            status: if github_notification.unread {
                NotificationStatus::Unread
            } else {
                NotificationStatus::Read
            },
            metadata: NotificationMetadata::Github(Box::new(github_notification.clone())),
            updated_at: github_notification.updated_at,
            last_read_at: github_notification.last_read_at,
            snoozed_until: None,
            user_id,
            details: None,
            task_id: None,
        }),
    )
    .await
}

pub fn mock_github_notifications_service<'a>(
    github_mock_server: &'a MockServer,
    page: &'a str,
    result: &'a Vec<GithubNotification>,
) -> Mock<'a> {
    github_mock_server.mock(|when, then| {
        when.method(GET)
            .path("/notifications")
            .header("accept", "application/vnd.github.v3+json")
            .header("authorization", "Bearer github_test_access_token")
            .query_param("page", page)
            .query_param_exists("per_page");
        then.status(200)
            .header("content-type", "application/json")
            .json_body_obj(result);
    })
}

pub fn mock_github_pull_request_query<'a>(
    github_mock_server: &'a MockServer,
    owner: String,
    repository: String,
    pr_number: i64,
    result: &'a Response<pull_request_query::ResponseData>,
) -> Mock<'a> {
    let expected_request_body = PullRequestQuery::build_query(pull_request_query::Variables {
        owner,
        repository,
        pr_number,
    });
    github_mock_server.mock(|when, then| {
        when.method(POST)
            .path("/graphql")
            .json_body_obj(&expected_request_body)
            .header("accept", "application/vnd.github.merge-info-preview+json")
            .header("authorization", "Bearer github_test_access_token");
        then.status(200)
            .header("content-type", "application/json")
            .json_body_obj(result);
    })
}

pub fn mock_github_discussions_search_query<'a>(
    github_mock_server: &'a MockServer,
    search_query: &str,
    result: &'a Response<discussions_search_query::ResponseData>,
) -> Mock<'a> {
    let expected_request_body =
        DiscussionsSearchQuery::build_query(discussions_search_query::Variables {
            search_query: search_query.to_string(),
        });
    github_mock_server.mock(|when, then| {
        when.method(POST)
            .path("/graphql")
            .json_body_obj(&expected_request_body)
            .header("accept", "application/vnd.github.merge-info-preview+json")
            .header("authorization", "Bearer github_test_access_token");
        then.status(200)
            .header("content-type", "application/json")
            .json_body_obj(result);
    })
}

#[fixture]
pub fn sync_github_notifications() -> Vec<GithubNotification> {
    load_json_fixture_file("/tests/api/fixtures/sync_github_notifications.json")
}

#[fixture]
pub fn github_pull_request_123_response() -> Response<pull_request_query::ResponseData> {
    load_json_fixture_file("/tests/api/fixtures/github_pull_request_123_response.json")
}

#[fixture]
pub fn github_discussion_123_response() -> Response<discussions_search_query::ResponseData> {
    load_json_fixture_file("/tests/api/fixtures/github_discussion_123_response.json")
}

pub fn assert_sync_notifications(
    notifications: &[Notification],
    sync_github_notifications: &[GithubNotification],
    expected_user_id: UserId,
    expected_notification_123_details: Option<NotificationDetails>,
) {
    for notification in notifications.iter() {
        assert_eq!(notification.user_id, expected_user_id);
        match notification.source_id.as_ref() {
            "123" => {
                assert_eq!(notification.title, "Greetings 1".to_string());
                assert_eq!(notification.status, NotificationStatus::Unread);
                assert_eq!(
                    notification.source_html_url,
                    Some(
                        "https://github.com/octokit/octokit.rb/pull/123"
                            .parse::<Url>()
                            .unwrap()
                    )
                );
                assert_eq!(
                    notification.updated_at,
                    Utc.with_ymd_and_hms(2014, 11, 7, 22, 1, 45).unwrap()
                );
                assert_eq!(
                    notification.last_read_at,
                    Some(Utc.with_ymd_and_hms(2014, 11, 7, 22, 2, 45).unwrap())
                );
                assert_eq!(
                    notification.metadata,
                    NotificationMetadata::Github(Box::new(sync_github_notifications[0].clone()))
                );
                if let Some(ref details) = expected_notification_123_details {
                    assert_eq!(notification.details, Some(details.clone()));
                }
            }
            // This notification should be updated
            "456" => {
                assert_eq!(notification.title, "Greetings 2".to_string());
                assert_eq!(notification.status, NotificationStatus::Read);
                assert_eq!(
                    notification.source_html_url,
                    Some(
                        "https://github.com/octokit/octokit.rb/issues/456"
                            .parse::<Url>()
                            .unwrap()
                    )
                );
                assert_eq!(
                    notification.updated_at,
                    Utc.with_ymd_and_hms(2014, 11, 7, 23, 1, 45).unwrap()
                );
                assert_eq!(
                    notification.last_read_at,
                    Some(Utc.with_ymd_and_hms(2014, 11, 7, 23, 2, 45).unwrap())
                );
                assert_eq!(
                    notification.metadata,
                    NotificationMetadata::Github(Box::new(sync_github_notifications[1].clone()))
                );
                assert!(notification.details.is_none());
            }
            _ => {
                unreachable!("Unexpected notification title '{}'", &notification.title);
            }
        }
    }
}

#[fixture]
pub fn github_notification() -> Box<GithubNotification> {
    let fixture_path = format!(
        "{}/tests/api/fixtures/github_notification.json",
        env::var("CARGO_MANIFEST_DIR").unwrap(),
    );
    let input_str = fs::read_to_string(fixture_path).unwrap();
    serde_json::from_str(&input_str).unwrap()
}
