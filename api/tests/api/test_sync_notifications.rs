#![allow(clippy::useless_conversion)]

use std::{env, fs};

use crate::helpers::{create_notification, get_notification, tested_app, TestedApp};
use chrono::{TimeZone, Utc};
use format_serde_error::SerdeError;
use http::Uri;
use httpmock::{Method::GET, Mock, MockServer};
use reqwest::Response;
use rstest::*;
use serde_json::json;
use universal_inbox::{
    integrations::github::GithubNotification, Notification, NotificationKind, NotificationStatus,
};
use universal_inbox_api::{
    integrations::github, universal_inbox::notification::source::NotificationSource,
};

async fn sync_notifications(app_address: &str, source: NotificationSource) -> Response {
    reqwest::Client::new()
        .post(&format!("{}/notifications/sync", &app_address))
        .json(&json!({"source": source.to_string()}))
        .send()
        .await
        .expect("Failed to execute request")
}

async fn create_notification_from_github_notification(
    app_address: &str,
    github_notification: Box<GithubNotification>,
) -> Box<Notification> {
    create_notification(
        app_address,
        &Notification {
            id: uuid::Uuid::new_v4(),
            title: github_notification.subject.title.clone(),
            kind: NotificationKind::Github,
            source_id: github_notification.id.to_string(),
            source_html_url: github::get_html_url_from_api_url(&github_notification.subject.url),
            status: if github_notification.unread {
                NotificationStatus::Unread
            } else {
                NotificationStatus::Read
            },
            metadata: *github_notification.clone(),
            updated_at: github_notification.updated_at,
            last_read_at: github_notification.last_read_at,
        },
    )
    .await
}

fn mock_github_notifications_service<'a>(
    github_mock_server: &'a MockServer,
    page: &'a str,
    result: &'a Vec<Box<GithubNotification>>,
) -> Mock<'a> {
    github_mock_server.mock(|when, then| {
        when.method(GET)
            .path("/notifications")
            .header("accept", "application/vnd.github.v3+json")
            .query_param("page", page)
            .query_param_exists("per_page");
        then.status(200)
            .header("content-type", "application/json")
            .json_body_obj(result);
    })
}

#[fixture]
pub fn sync_github_notifications() -> Vec<Box<GithubNotification>> {
    let fixture_path = format!(
        "{}/tests/api/fixtures/sync_github_notifications.json",
        env::var("CARGO_MANIFEST_DIR").unwrap(),
    );
    let input_str = fs::read_to_string(fixture_path).unwrap();
    serde_json::from_str(&input_str)
        .map_err(|err| SerdeError::new(input_str, err))
        .unwrap()
}

#[rstest]
#[tokio::test]
async fn test_sync_notifications_should_add_new_notification_and_update_existing_one(
    #[future] tested_app: TestedApp,
    // Vec[GithubNotification { source_id: "123", ... }, GithubNotification { source_id: "456", ... } ]
    sync_github_notifications: Vec<Box<GithubNotification>>,
) {
    let app = tested_app.await;
    let existing_notification = create_notification(
        &app.app_address,
        &Notification {
            id: uuid::Uuid::new_v4(),
            title: "Greetings 2".to_string(),
            kind: NotificationKind::Github,
            status: NotificationStatus::Unread,
            source_id: "456".to_string(),
            source_html_url: github::get_html_url_from_api_url(
                &sync_github_notifications[1].subject.url,
            ),
            metadata: *sync_github_notifications[1].clone(),
            updated_at: Utc.ymd(2014, 11, 6).and_hms(0, 0, 0),
            last_read_at: None,
        },
    )
    .await;

    let github_notifications_mock =
        mock_github_notifications_service(&app.github_mock_server, "1", &sync_github_notifications);
    let empty_result = Vec::<Box<GithubNotification>>::new();
    let github_notifications_mock2 =
        mock_github_notifications_service(&app.github_mock_server, "2", &empty_result);

    let notifications: Vec<Notification> =
        sync_notifications(&app.app_address, NotificationSource::Github)
            .await
            .json()
            .await
            .expect("Cannot parse JSON result");

    assert_eq!(notifications.len(), sync_github_notifications.len());
    assert_sync_notifications(notifications, &sync_github_notifications);
    github_notifications_mock.assert();
    github_notifications_mock2.assert();

    let updated_notification = get_notification(&app.app_address, existing_notification.id).await;
    assert_eq!(updated_notification.id, existing_notification.id);
    assert_eq!(
        updated_notification.source_id,
        existing_notification.source_id
    );
    assert_eq!(updated_notification.status, NotificationStatus::Read);
    assert_eq!(
        updated_notification.updated_at,
        Utc.ymd(2014, 11, 7).and_hms(23, 1, 45)
    );
    assert_eq!(
        updated_notification.last_read_at,
        Some(Utc.ymd(2014, 11, 7).and_hms(23, 2, 45))
    );
    assert_eq!(updated_notification.metadata, *sync_github_notifications[1]);
}

#[rstest]
#[tokio::test]
async fn test_sync_notifications_should_mark_unsubscribed_notification_without_subscription(
    #[future] tested_app: TestedApp,
    // Vec[GithubNotification { source_id: "123", ... }, GithubNotification { source_id: "456", ... } ]
    sync_github_notifications: Vec<Box<GithubNotification>>,
) {
    let app = tested_app.await;
    for github_notification in sync_github_notifications.iter() {
        create_notification_from_github_notification(&app.app_address, github_notification.clone())
            .await;
    }
    // to be unsubscribed during sync
    let existing_notification = create_notification(
        &app.app_address,
        &Notification {
            id: uuid::Uuid::new_v4(),
            title: "Greetings 3".to_string(),
            kind: NotificationKind::Github,
            status: NotificationStatus::Unread,
            source_id: "789".to_string(),
            source_html_url: github::get_html_url_from_api_url(
                &sync_github_notifications[1].subject.url,
            ),
            metadata: *sync_github_notifications[1].clone(), // reusing github notification but not useful
            updated_at: Utc.ymd(2014, 11, 6).and_hms(0, 0, 0),
            last_read_at: None,
        },
    )
    .await;

    let github_notifications_mock =
        mock_github_notifications_service(&app.github_mock_server, "1", &sync_github_notifications);
    let empty_result = Vec::<Box<GithubNotification>>::new();
    let github_notifications_mock2 =
        mock_github_notifications_service(&app.github_mock_server, "2", &empty_result);

    let notifications: Vec<Notification> =
        sync_notifications(&app.app_address, NotificationSource::Github)
            .await
            .json()
            .await
            .expect("Cannot parse JSON result");

    assert_eq!(notifications.len(), sync_github_notifications.len());
    assert_sync_notifications(notifications, &sync_github_notifications);
    github_notifications_mock.assert();
    github_notifications_mock2.assert();

    let unsubscribed_notification =
        get_notification(&app.app_address, existing_notification.id).await;
    assert_eq!(unsubscribed_notification.id, existing_notification.id);
    assert_eq!(
        unsubscribed_notification.status,
        NotificationStatus::Deleted
    );
}

fn assert_sync_notifications(
    notifications: Vec<Notification>,
    sync_github_notifications: &[Box<GithubNotification>],
) {
    for notification in notifications.iter() {
        match notification.source_id.as_ref() {
            "123" => {
                assert_eq!(notification.title, "Greetings 1".to_string());
                assert_eq!(notification.kind, NotificationKind::Github);
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
                    Utc.ymd(2014, 11, 7).and_hms(22, 1, 45)
                );
                assert_eq!(
                    notification.last_read_at,
                    Some(Utc.ymd(2014, 11, 7).and_hms(22, 2, 45))
                );
                assert_eq!(notification.metadata, *sync_github_notifications[0]);
            }
            // This notification should be updated
            "456" => {
                assert_eq!(notification.title, "Greetings 2".to_string());
                assert_eq!(notification.kind, NotificationKind::Github);
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
                    Utc.ymd(2014, 11, 7).and_hms(23, 1, 45)
                );
                assert_eq!(
                    notification.last_read_at,
                    Some(Utc.ymd(2014, 11, 7).and_hms(23, 2, 45))
                );
                assert_eq!(notification.metadata, *sync_github_notifications[1]);
            }
            _ => {
                unreachable!("Unexpected notification title '{}'", &notification.title);
            }
        }
    }
}
