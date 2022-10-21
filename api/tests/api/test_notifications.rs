#![allow(clippy::useless_conversion)]

use std::env;
use std::fs;

use crate::helpers::{
    create_notification, get_notification, list_notifications, tested_app, TestedApp,
};
use ::universal_inbox::{
    integrations::github::GithubNotification, Notification, NotificationKind, NotificationStatus,
};
use chrono::{TimeZone, Utc};
use format_serde_error::SerdeError;
use http::StatusCode;
use httpmock::Method::PATCH;
use rstest::*;
use serde_json::json;

#[fixture]
fn github_notification() -> Box<GithubNotification> {
    let fixture_path = format!(
        "{}/tests/api/fixtures/github_notification.json",
        env::var("CARGO_MANIFEST_DIR").unwrap(),
    );
    let input_str = fs::read_to_string(fixture_path).unwrap();
    serde_json::from_str(&input_str)
        .map_err(|err| SerdeError::new(input_str, err))
        .unwrap()
}

mod list_notifications {
    use crate::helpers::create_notification;

    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_empty_list_notifications(#[future] tested_app: TestedApp) {
        let address: String = tested_app.await.app_address.into();
        let notifications = list_notifications(&address).await;

        assert_eq!(notifications.len(), 0);
    }

    #[rstest]
    #[tokio::test]
    async fn test_list_notifications(
        #[future] tested_app: TestedApp,
        github_notification: Box<GithubNotification>,
    ) {
        let mut github_notification2 = github_notification.clone();
        github_notification2.id = "43".to_string();

        let address: String = tested_app.await.app_address.into();
        let expected_notification1 = create_notification(
            &address,
            &Notification {
                id: uuid::Uuid::new_v4(),
                title: "notif1".to_string(),
                kind: NotificationKind::Github,
                status: NotificationStatus::Unread,
                source_id: "1234".to_string(),
                metadata: *github_notification,
                updated_at: Utc.ymd(2022, 1, 1).and_hms(0, 0, 0),
                last_read_at: None,
            },
        )
        .await;

        let expected_notification2 = create_notification(
            &address,
            &Notification {
                id: uuid::Uuid::new_v4(),
                title: "notif2".to_string(),
                kind: NotificationKind::Github,
                status: NotificationStatus::Read,
                source_id: "5678".to_string(),
                metadata: *github_notification2,
                updated_at: Utc.ymd(2022, 2, 1).and_hms(0, 0, 0),
                last_read_at: Some(Utc.ymd(2022, 2, 1).and_hms(1, 0, 0)),
            },
        )
        .await;

        let notifications = list_notifications(&address).await;

        assert_eq!(notifications.len(), 2);
        assert_eq!(notifications[0], *expected_notification1);
        assert_eq!(notifications[1], *expected_notification2);
    }
}

mod create_notification {
    use crate::helpers::create_notification_response;

    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_create_notification(
        #[future] tested_app: TestedApp,
        github_notification: Box<GithubNotification>,
    ) {
        let address: String = tested_app.await.app_address.into();
        let expected_notification = Box::new(Notification {
            id: uuid::Uuid::new_v4(),
            title: "notif1".to_string(),
            kind: NotificationKind::Github,
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            metadata: *github_notification,
            updated_at: Utc.ymd(2022, 1, 1).and_hms(0, 0, 0),
            last_read_at: None,
        });
        let created_notification = create_notification(&address, &expected_notification).await;

        assert_eq!(created_notification, expected_notification);

        let notification = get_notification(&address, created_notification.id).await;

        assert_eq!(notification, expected_notification);
    }

    #[rstest]
    #[tokio::test]
    async fn test_create_notification_duplicate_notification(
        #[future] tested_app: TestedApp,
        github_notification: Box<GithubNotification>,
    ) {
        let address: String = tested_app.await.app_address.into();
        let expected_notification = Box::new(Notification {
            id: uuid::Uuid::new_v4(),
            title: "notif1".to_string(),
            kind: NotificationKind::Github,
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            metadata: *github_notification,
            updated_at: Utc.ymd(2022, 1, 1).and_hms(0, 0, 0),
            last_read_at: None,
        });
        let created_notification = create_notification(&address, &expected_notification).await;

        assert_eq!(created_notification, expected_notification);

        let response = create_notification_response(&address, &expected_notification).await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = response.text().await.expect("Cannot get response body");
        assert_eq!(
            body,
            json!({ "message": format!("The entity {} already exists", created_notification.id) })
                .to_string()
        );
    }
}
mod get_notification {
    use uuid::Uuid;

    use crate::helpers::{get_notification, get_notification_response};

    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_get_existing_notification(
        #[future] tested_app: TestedApp,
        github_notification: Box<GithubNotification>,
    ) {
        let address: String = tested_app.await.app_address.into();
        let expected_notification = Box::new(Notification {
            id: uuid::Uuid::new_v4(),
            title: "notif1".to_string(),
            kind: NotificationKind::Github,
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            metadata: *github_notification,
            updated_at: Utc.ymd(2022, 1, 1).and_hms(0, 0, 0),
            last_read_at: None,
        });
        let created_notification = create_notification(&address, &expected_notification).await;

        assert_eq!(created_notification, expected_notification);

        let notification = get_notification(&address, created_notification.id).await;

        assert_eq!(notification, created_notification);
    }

    #[rstest]
    #[tokio::test]
    async fn test_get_unknown_notification(#[future] tested_app: TestedApp) {
        let address: String = tested_app.await.app_address.into();
        let unknown_notification_id = Uuid::new_v4();

        let response = get_notification_response(&address, unknown_notification_id).await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = response.text().await.expect("Cannot get response body");
        assert_eq!(
            body,
            json!({ "message": format!("Cannot find notification {}", unknown_notification_id) })
                .to_string()
        );
    }
}

mod patch_notification {
    use universal_inbox::NotificationPatch;
    use uuid::Uuid;

    use crate::helpers::{patch_notification, patch_notification_response};

    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_patch_notification_status(
        #[future] tested_app: TestedApp,
        github_notification: Box<GithubNotification>,
        #[values(205, 304, 404)] github_status_code: u16,
    ) {
        let app = tested_app.await;
        let github_mark_thread_as_read_mock = app.github_mock_server.mock(|when, then| {
            when.method(PATCH)
                .path("/notifications/threads/1234")
                .header("accept", "application/vnd.github.v3+json");
            then.status(github_status_code);
        });
        let address: String = app.app_address.into();
        let expected_notification = Box::new(Notification {
            id: uuid::Uuid::new_v4(),
            title: "notif1".to_string(),
            kind: NotificationKind::Github,
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            metadata: *github_notification,
            updated_at: Utc.ymd(2022, 1, 1).and_hms(0, 0, 0),
            last_read_at: None,
        });
        let created_notification = create_notification(&address, &expected_notification).await;

        assert_eq!(created_notification, expected_notification);

        let patched_notification = patch_notification(
            &address,
            created_notification.id,
            &NotificationPatch {
                status: Some(NotificationStatus::Done),
            },
        )
        .await;

        assert_eq!(
            patched_notification,
            Box::new(Notification {
                status: NotificationStatus::Done,
                ..*created_notification
            })
        );
        github_mark_thread_as_read_mock.assert();
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_notification_status_with_github_api_error(
        #[future] tested_app: TestedApp,
        github_notification: Box<GithubNotification>,
    ) {
        let app = tested_app.await;
        let github_mark_thread_as_read_mock = app.github_mock_server.mock(|when, then| {
            when.method(PATCH)
                .path("/notifications/threads/1234")
                .header("accept", "application/vnd.github.v3+json");
            then.status(403);
        });
        let address: String = app.app_address.into();
        let expected_notification = Box::new(Notification {
            id: uuid::Uuid::new_v4(),
            title: "notif1".to_string(),
            kind: NotificationKind::Github,
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            metadata: *github_notification,
            updated_at: Utc.ymd(2022, 1, 1).and_hms(0, 0, 0),
            last_read_at: None,
        });
        let created_notification = create_notification(&address, &expected_notification).await;

        assert_eq!(created_notification, expected_notification);

        let response = patch_notification_response(
            &address,
            created_notification.id,
            &NotificationPatch {
                status: Some(NotificationStatus::Done),
            },
        )
        .await;
        assert_eq!(response.status(), 500);

        let body = response.text().await.expect("Cannot get response body");
        assert_eq!(
            body,
            json!({ "message": format!("Failed to mark Github notification `1234` as read") })
                .to_string()
        );
        github_mark_thread_as_read_mock.assert();
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_notification_status_without_modification(
        #[future] tested_app: TestedApp,
        github_notification: Box<GithubNotification>,
    ) {
        let app = tested_app.await;
        let github_api_mock = app.github_mock_server.mock(|when, then| {
            when.any_request();
            then.status(200);
        });
        let address: String = app.app_address.into();
        let expected_notification = Box::new(Notification {
            id: uuid::Uuid::new_v4(),
            title: "notif1".to_string(),
            kind: NotificationKind::Github,
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            metadata: *github_notification,
            updated_at: Utc.ymd(2022, 1, 1).and_hms(0, 0, 0),
            last_read_at: None,
        });
        let created_notification = create_notification(&address, &expected_notification).await;

        assert_eq!(created_notification, expected_notification);

        let response = patch_notification_response(
            &address,
            created_notification.id,
            &NotificationPatch {
                status: Some(created_notification.status),
            },
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
        github_api_mock.assert_hits(0);
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_notification_status_without_value(
        #[future] tested_app: TestedApp,
        github_notification: Box<GithubNotification>,
    ) {
        let app = tested_app.await;
        let github_api_mock = app.github_mock_server.mock(|when, then| {
            when.any_request();
            then.status(200);
        });
        let address: String = app.app_address.into();
        let expected_notification = Box::new(Notification {
            id: uuid::Uuid::new_v4(),
            title: "notif1".to_string(),
            kind: NotificationKind::Github,
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            metadata: *github_notification,
            updated_at: Utc.ymd(2022, 1, 1).and_hms(0, 0, 0),
            last_read_at: None,
        });
        let created_notification = create_notification(&address, &expected_notification).await;

        assert_eq!(created_notification, expected_notification);

        let response = patch_notification_response(
            &address,
            created_notification.id,
            &NotificationPatch { status: None },
        )
        .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        github_api_mock.assert_hits(0);
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_unknown_notification(#[future] tested_app: TestedApp) {
        let app = tested_app.await;
        let github_api_mock = app.github_mock_server.mock(|when, then| {
            when.any_request();
            then.status(200);
        });
        let address: String = app.app_address.into();
        let unknown_notification_id = Uuid::new_v4();

        let response = patch_notification_response(
            &address,
            unknown_notification_id,
            &NotificationPatch {
                status: Some(NotificationStatus::Done),
            },
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = response.text().await.expect("Cannot get response body");
        assert_eq!(
            body,
            json!({
                "message":
                    format!(
                        "Cannot update unknown notification {}",
                        unknown_notification_id
                    )
            })
            .to_string()
        );
        github_api_mock.assert_hits(0);
    }
}
