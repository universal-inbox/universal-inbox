use chrono::{TimeZone, Utc};
use http::StatusCode;
use httpmock::Method::{PATCH, PUT};
use rstest::*;
use serde_json::json;
use uuid::Uuid;

use universal_inbox::{
    integrations::github::GithubNotification, Notification, NotificationMetadata,
    NotificationPatch, NotificationStatus,
};
use universal_inbox_api::integrations::github;

use crate::helpers::{
    create_notification, get_notification, github::github_notification, patch_notification,
    patch_notification_response, tested_app, TestedApp,
};

mod patch_notification {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_patch_notification_status_as_deleted(
        #[future] tested_app: TestedApp,
        github_notification: Box<GithubNotification>,
        #[values(205, 304, 404)] github_status_code: u16,
    ) {
        let app = tested_app.await;
        let github_mark_thread_as_read_mock = app.github_mock_server.mock(|when, then| {
            when.method(PATCH)
                .path("/notifications/threads/1234")
                .header("accept", "application/vnd.github.v3+json")
                .header("authorization", "token github_test_token");
            then.status(github_status_code);
        });
        let expected_notification = Box::new(Notification {
            id: uuid::Uuid::new_v4(),
            title: "notif1".to_string(),
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            source_html_url: github::get_html_url_from_api_url(&github_notification.subject.url),
            metadata: NotificationMetadata::Github(*github_notification),
            updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
        });
        let created_notification =
            create_notification(&app.app_address, expected_notification.clone()).await;

        assert_eq!(created_notification, expected_notification);

        let patched_notification = patch_notification(
            &app.app_address,
            created_notification.id,
            &NotificationPatch {
                status: Some(NotificationStatus::Deleted),
                ..Default::default()
            },
        )
        .await;

        assert_eq!(
            patched_notification,
            Box::new(Notification {
                status: NotificationStatus::Deleted,
                ..*created_notification
            })
        );
        github_mark_thread_as_read_mock.assert();
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_notification_status_as_unsubscribed(
        #[future] tested_app: TestedApp,
        github_notification: Box<GithubNotification>,
        #[values(205, 304, 404)] github_status_code: u16,
    ) {
        let app = tested_app.await;
        let github_mark_thread_as_read_mock = app.github_mock_server.mock(|when, then| {
            when.method(PUT)
                .path("/notifications/threads/1234/subscription")
                .header("accept", "application/vnd.github.v3+json")
                .header("authorization", "token github_test_token")
                .json_body(json!({"ignored": true}));
            then.status(github_status_code);
        });
        let expected_notification = Box::new(Notification {
            id: uuid::Uuid::new_v4(),
            title: "notif1".to_string(),
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            source_html_url: github::get_html_url_from_api_url(&github_notification.subject.url),
            metadata: NotificationMetadata::Github(*github_notification),
            updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
        });
        let created_notification =
            create_notification(&app.app_address, expected_notification.clone()).await;

        assert_eq!(created_notification, expected_notification);

        let patched_notification = patch_notification(
            &app.app_address,
            created_notification.id,
            &NotificationPatch {
                status: Some(NotificationStatus::Unsubscribed),
                ..Default::default()
            },
        )
        .await;

        assert_eq!(
            patched_notification,
            Box::new(Notification {
                status: NotificationStatus::Unsubscribed,
                ..*created_notification
            })
        );
        github_mark_thread_as_read_mock.assert();
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_notification_status_as_deleted_with_github_api_error(
        #[future] tested_app: TestedApp,
        github_notification: Box<GithubNotification>,
    ) {
        let app = tested_app.await;
        let github_mark_thread_as_read_mock = app.github_mock_server.mock(|when, then| {
            when.method(PATCH)
                .path("/notifications/threads/1234")
                .header("accept", "application/vnd.github.v3+json")
                .header("authorization", "token github_test_token");
            then.status(403);
        });
        let expected_notification = Box::new(Notification {
            id: uuid::Uuid::new_v4(),
            title: "notif1".to_string(),
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            source_html_url: github::get_html_url_from_api_url(&github_notification.subject.url),
            metadata: NotificationMetadata::Github(*github_notification),
            updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
        });
        let created_notification =
            create_notification(&app.app_address, expected_notification.clone()).await;

        assert_eq!(created_notification, expected_notification);

        let response = patch_notification_response(
            &app.app_address,
            created_notification.id,
            &NotificationPatch {
                status: Some(NotificationStatus::Deleted),
                ..Default::default()
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

        let notification = get_notification(&app.app_address, created_notification.id).await;
        assert_eq!(notification.status, NotificationStatus::Unread);
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_notification_snoozed_until(
        #[future] tested_app: TestedApp,
        github_notification: Box<GithubNotification>,
    ) {
        let app = tested_app.await;
        let expected_notification = Box::new(Notification {
            id: uuid::Uuid::new_v4(),
            title: "notif1".to_string(),
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            source_html_url: github::get_html_url_from_api_url(&github_notification.subject.url),
            metadata: NotificationMetadata::Github(*github_notification),
            updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
        });
        let snoozed_time = Utc.with_ymd_and_hms(2022, 1, 1, 1, 2, 3).unwrap();
        let created_notification =
            create_notification(&app.app_address, expected_notification.clone()).await;

        assert_eq!(created_notification, expected_notification);

        let patched_notification = patch_notification(
            &app.app_address,
            created_notification.id,
            &NotificationPatch {
                snoozed_until: Some(snoozed_time),
                ..Default::default()
            },
        )
        .await;

        assert_eq!(
            patched_notification,
            Box::new(Notification {
                snoozed_until: Some(snoozed_time),
                ..*created_notification
            })
        );
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
        let snoozed_time = Utc.with_ymd_and_hms(2022, 1, 1, 1, 2, 3).unwrap();
        let expected_notification = Box::new(Notification {
            id: uuid::Uuid::new_v4(),
            title: "notif1".to_string(),
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            source_html_url: github::get_html_url_from_api_url(&github_notification.subject.url),
            metadata: NotificationMetadata::Github(*github_notification),
            updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: Some(snoozed_time),
        });
        let created_notification =
            create_notification(&app.app_address, expected_notification.clone()).await;

        assert_eq!(created_notification, expected_notification);

        let response = patch_notification_response(
            &app.app_address,
            created_notification.id,
            &NotificationPatch {
                status: Some(created_notification.status),
                snoozed_until: Some(snoozed_time),
            },
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
        github_api_mock.assert_hits(0);
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_notification_without_values_to_update(
        #[future] tested_app: TestedApp,
        github_notification: Box<GithubNotification>,
    ) {
        let app = tested_app.await;
        let github_api_mock = app.github_mock_server.mock(|when, then| {
            when.any_request();
            then.status(200);
        });
        let expected_notification = Box::new(Notification {
            id: uuid::Uuid::new_v4(),
            title: "notif1".to_string(),
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            source_html_url: github::get_html_url_from_api_url(&github_notification.subject.url),
            metadata: NotificationMetadata::Github(*github_notification),
            updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
        });
        let created_notification =
            create_notification(&app.app_address, expected_notification.clone()).await;

        assert_eq!(created_notification, expected_notification);

        let response = patch_notification_response(
            &app.app_address,
            created_notification.id,
            &NotificationPatch {
                ..Default::default()
            },
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
        let unknown_notification_id = Uuid::new_v4();

        let response = patch_notification_response(
            &app.app_address,
            unknown_notification_id,
            &NotificationPatch {
                status: Some(NotificationStatus::Deleted),
                ..Default::default()
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
