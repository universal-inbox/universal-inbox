use std::{env, fs};

use chrono::{TimeZone, Utc};
use format_serde_error::SerdeError;
use http::Uri;
use httpmock::{Method::GET, Mock, MockServer};
use rstest::*;
use uuid::Uuid;

use universal_inbox::notification::{
    integrations::github::GithubNotification, Notification, NotificationMetadata,
    NotificationStatus,
};
use universal_inbox_api::integrations::github;

use crate::helpers::{load_json_fixture_file, rest::create_resource};

pub async fn create_notification_from_github_notification(
    app_address: &str,
    github_notification: &GithubNotification,
) -> Box<Notification> {
    create_resource(
        app_address,
        "notifications",
        Box::new(Notification {
            id: Uuid::new_v4().into(),
            title: github_notification.subject.title.clone(),
            source_id: github_notification.id.to_string(),
            source_html_url: github::get_html_url_from_api_url(&github_notification.subject.url),
            status: if github_notification.unread {
                NotificationStatus::Unread
            } else {
                NotificationStatus::Read
            },
            metadata: NotificationMetadata::Github(github_notification.clone()),
            updated_at: github_notification.updated_at,
            last_read_at: github_notification.last_read_at,
            snoozed_until: None,
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
            .header("authorization", "token github_test_token")
            .query_param("page", page)
            .query_param_exists("per_page");
        then.status(200)
            .header("content-type", "application/json")
            .json_body_obj(result);
    })
}

#[fixture]
pub fn sync_github_notifications() -> Vec<GithubNotification> {
    load_json_fixture_file("/tests/api/fixtures/sync_github_notifications.json")
}

pub fn assert_sync_notifications(
    notifications: &[Notification],
    sync_github_notifications: &[GithubNotification],
) {
    for notification in notifications.iter() {
        match notification.source_id.as_ref() {
            "123" => {
                assert_eq!(notification.title, "Greetings 1".to_string());
                assert_eq!(notification.status, NotificationStatus::Unread);
                assert_eq!(
                    notification.source_html_url,
                    Some(
                        "https://github.com/octokit/octokit.rb/issues/123"
                            .parse::<Uri>()
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
                    NotificationMetadata::Github(sync_github_notifications[0].clone())
                );
            }
            // This notification should be updated
            "456" => {
                assert_eq!(notification.title, "Greetings 2".to_string());
                assert_eq!(notification.status, NotificationStatus::Read);
                assert_eq!(
                    notification.source_html_url,
                    Some(
                        "https://github.com/octokit/octokit.rb/issues/456"
                            .parse::<Uri>()
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
                    NotificationMetadata::Github(sync_github_notifications[1].clone())
                );
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
    serde_json::from_str(&input_str)
        .map_err(|err| SerdeError::new(input_str, err))
        .unwrap()
}
