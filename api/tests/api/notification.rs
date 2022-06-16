#![allow(clippy::useless_conversion)]

use crate::helpers::app_address;
use ::universal_inbox::NotificationKind;
use ::universal_inbox::NotificationStatus;
use ::universal_inbox::{GithubNotification, Notification};
use chrono::{TimeZone, Utc};
use http::StatusCode;
use reqwest::Response;
use rstest::*;
use serde_json::json;

async fn create_notification(app_address: &str, notification: &Notification) -> Response {
    reqwest::Client::new()
        .post(&format!("{}/notifications", &app_address))
        .json(notification)
        .send()
        .await
        .expect("Failed to execute request")
}

async fn get_notification(app_address: &str, id: uuid::Uuid) -> Response {
    reqwest::Client::new()
        .get(&format!("{}/notifications/{}", &app_address, id))
        .send()
        .await
        .expect("Failed to execute request")
}

async fn list_notifications(app_address: &str) -> Response {
    reqwest::Client::new()
        .get(&format!("{}/notifications", &app_address))
        .send()
        .await
        .expect("Failed to execute request")
}

mod list_notifications {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_empty_list_notifications(#[future] app_address: String) {
        let address: String = app_address.await.into();
        let notifications: Vec<Notification> = list_notifications(&address)
            .await
            .json()
            .await
            .expect("Cannot parse JSON result");

        assert_eq!(notifications.len(), 0);
    }

    #[rstest]
    #[tokio::test]
    async fn test_list_notifications(#[future] app_address: String) {
        let address: String = app_address.await.into();
        let expected_notification1: Notification = create_notification(
            &address,
            &Notification {
                id: uuid::Uuid::new_v4(),
                title: "notif1".to_string(),
                kind: NotificationKind::Github,
                status: NotificationStatus::Unread,
                metadata: GithubNotification {
                    test: "Hello".to_string(),
                    num: 42,
                },
                updated_at: Utc.ymd(2022, 1, 1).and_hms(0, 0, 0),
                last_read_at: None,
            },
        )
        .await
        .json()
        .await
        .expect("Cannot parse JSON result");
        let expected_notification2: Notification = create_notification(
            &address,
            &Notification {
                id: uuid::Uuid::new_v4(),
                title: "notif2".to_string(),
                kind: NotificationKind::Github,
                status: NotificationStatus::Read,
                metadata: GithubNotification {
                    test: "World".to_string(),
                    num: 43,
                },
                updated_at: Utc.ymd(2022, 2, 1).and_hms(0, 0, 0),
                last_read_at: Some(Utc.ymd(2022, 2, 1).and_hms(1, 0, 0)),
            },
        )
        .await
        .json()
        .await
        .expect("Cannot parse JSON result");

        let notifications: Vec<Notification> = list_notifications(&address)
            .await
            .json()
            .await
            .expect("Cannot parse JSON result");

        assert_eq!(notifications.len(), 2);
        assert_eq!(notifications[0], expected_notification1);
        assert_eq!(notifications[1], expected_notification2);
    }
}

mod create_notification {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_create_notification(#[future] app_address: String) {
        let address: String = app_address.await.into();
        let expected_notification = Notification {
            id: uuid::Uuid::new_v4(),
            title: "notif1".to_string(),
            kind: NotificationKind::Github,
            status: NotificationStatus::Unread,
            metadata: GithubNotification {
                test: "Hello".to_string(),
                num: 42,
            },
            updated_at: Utc.ymd(2022, 1, 1).and_hms(0, 0, 0),
            last_read_at: None,
        };
        let created_notification: Notification =
            create_notification(&address, &expected_notification)
                .await
                .json()
                .await
                .expect("Cannot parse JSON result");

        assert_eq!(created_notification, expected_notification);

        let notification: Notification = get_notification(&address, created_notification.id)
            .await
            .json()
            .await
            .expect("Cannot parse JSON result");

        assert_eq!(notification, expected_notification);
    }

    #[rstest]
    #[tokio::test]
    async fn test_create_notification_duplicate_notification(#[future] app_address: String) {
        let address: String = app_address.await.into();
        let expected_notification = Notification {
            id: uuid::Uuid::new_v4(),
            title: "notif1".to_string(),
            kind: NotificationKind::Github,
            status: NotificationStatus::Unread,
            metadata: GithubNotification {
                test: "Hello".to_string(),
                num: 42,
            },
            updated_at: Utc.ymd(2022, 1, 1).and_hms(0, 0, 0),
            last_read_at: None,
        };
        let created_notification: Notification =
            create_notification(&address, &expected_notification)
                .await
                .json()
                .await
                .expect("Cannot parse JSON result");

        assert_eq!(created_notification, expected_notification);

        let response = create_notification(&address, &expected_notification).await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = response.text().await.expect("Cannot get response body");
        assert_eq!(
            body,
            json!({ "message": format!("The entity {} already exists", created_notification.id) })
                .to_string()
        );
    }
}
