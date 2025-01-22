use chrono::{TimeZone, Utc};
use graphql_client::{GraphQLQuery, Response};
use httpmock::{
    Method::{GET, POST},
    Mock, MockServer,
};
use pretty_assertions::assert_eq;
use rstest::*;
use url::Url;

use universal_inbox::{
    integration_connection::IntegrationConnectionId,
    notification::{Notification, NotificationSourceKind, NotificationStatus},
    third_party::{
        integrations::github::{GithubNotification, GithubNotificationItem},
        item::ThirdPartyItemData,
    },
    user::UserId,
    HasHtmlUrl,
};

use universal_inbox_api::integrations::github::graphql::{
    discussion_query, pull_request_query, DiscussionQuery, PullRequestQuery,
};

use crate::helpers::{
    load_json_fixture_file, notification::create_notification_from_source_item, TestedApp,
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

pub fn mock_github_discussion_query<'a>(
    github_mock_server: &'a MockServer,
    owner: String,
    repository: String,
    discussion_number: i64,
    result: &'a Response<discussion_query::ResponseData>,
) -> Mock<'a> {
    let expected_request_body = DiscussionQuery::build_query(discussion_query::Variables {
        owner,
        repository,
        discussion_number,
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
    load_json_fixture_file("sync_github_notifications.json")
}

#[fixture]
pub fn github_pull_request_123_response() -> Response<pull_request_query::ResponseData> {
    load_json_fixture_file("github_pull_request_123_response.json")
}

#[fixture]
pub fn github_discussion_123_response() -> Response<discussion_query::ResponseData> {
    load_json_fixture_file("github_discussion_123_response.json")
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
                assert_eq!(notification.title, "Greetings 1".to_string());
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
                assert_eq!(notification.title, "Greetings 2".to_string());
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
