use chrono::{TimeDelta, TimeZone, Timelike, Utc};
use graphql_client::Response;
use http::StatusCode;
use rstest::*;
use serde_json::json;
use uuid::Uuid;

use universal_inbox::notification::{
    integrations::{github::GithubNotification, linear::LinearNotification},
    service::NotificationPatch,
    Notification, NotificationMetadata, NotificationSourceKind, NotificationStatus,
};

use universal_inbox_api::integrations::linear::graphql::notifications_query;

use crate::helpers::{
    auth::{authenticate_user, authenticated_app, AuthenticatedApp},
    notification::{
        github::{create_notification_from_github_notification, github_notification},
        linear::sync_linear_notifications_response,
        list_notifications,
    },
    rest::{
        create_resource, create_resource_response, get_resource, get_resource_response,
        patch_resource, patch_resource_response,
    },
};

mod list_notifications {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_empty_list_notifications(#[future] authenticated_app: AuthenticatedApp) {
        let app = authenticated_app.await;
        let result = list_notifications(
            &app.client,
            &app.app.api_address,
            vec![NotificationStatus::Unread],
            false,
            None,
            None,
            false,
        )
        .await;

        assert!(result.is_empty());
    }

    #[rstest]
    #[tokio::test]
    async fn test_list_notifications(
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
    ) {
        let mut github_notification2 = github_notification.clone();
        github_notification2.id = "43".to_string();

        let app = authenticated_app.await;
        let expected_notification1: Box<Notification> = create_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            Box::new(Notification {
                id: Uuid::new_v4().into(),
                title: "notif1".to_string(),
                status: NotificationStatus::Unread,
                source_id: "1234".to_string(),
                metadata: NotificationMetadata::Github(github_notification.clone()),
                updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
                last_read_at: None,
                snoozed_until: None,
                user_id: app.user.id,
                details: None,
                task_id: None,
            }),
        )
        .await;

        let expected_notification2: Box<Notification> = create_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            Box::new(Notification {
                id: Uuid::new_v4().into(),
                title: "notif2".to_string(),
                status: NotificationStatus::Read,
                source_id: "5678".to_string(),
                metadata: NotificationMetadata::Github(github_notification2.clone()),
                updated_at: Utc.with_ymd_and_hms(2022, 2, 1, 0, 0, 0).unwrap(),
                last_read_at: Some(Utc.with_ymd_and_hms(2022, 2, 1, 1, 0, 0).unwrap()),
                // Snooze time has expired
                snoozed_until: Some(
                    Utc::now().with_nanosecond(0).unwrap() - TimeDelta::try_minutes(1).unwrap(),
                ),
                user_id: app.user.id,
                details: None,
                task_id: None,
            }),
        )
        .await;

        let deleted_notification: Box<Notification> = create_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            Box::new(Notification {
                id: Uuid::new_v4().into(),
                title: "notif3".to_string(),
                status: NotificationStatus::Deleted,
                source_id: "9012".to_string(),
                metadata: NotificationMetadata::Github(github_notification),
                updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
                last_read_at: None,
                snoozed_until: None,
                user_id: app.user.id,
                details: None,
                task_id: None,
            }),
        )
        .await;

        let snoozed_notification: Box<Notification> = create_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            Box::new(Notification {
                id: Uuid::new_v4().into(),
                title: "notif4".to_string(),
                status: NotificationStatus::Unread,
                source_id: "3456".to_string(),
                metadata: NotificationMetadata::Github(github_notification2),
                updated_at: Utc.with_ymd_and_hms(2022, 2, 1, 0, 0, 0).unwrap(),
                last_read_at: Some(Utc.with_ymd_and_hms(2022, 2, 1, 1, 0, 0).unwrap()),
                // Snooze time in the future
                snoozed_until: Some(
                    Utc::now().with_nanosecond(0).unwrap() + TimeDelta::try_minutes(1).unwrap(),
                ),
                user_id: app.user.id,
                details: None,
                task_id: None,
            }),
        )
        .await;

        let result = list_notifications(
            &app.client,
            &app.app.api_address,
            vec![NotificationStatus::Unread],
            false,
            None,
            None,
            false,
        )
        .await;

        assert_eq!(result.len(), 1);
        assert_eq!(result[0], *expected_notification1);

        let result = list_notifications(
            &app.client,
            &app.app.api_address,
            vec![NotificationStatus::Read, NotificationStatus::Unread],
            false,
            None,
            None,
            false,
        )
        .await;

        assert_eq!(result.len(), 2);
        assert_eq!(result[0], *expected_notification1);
        assert_eq!(result[1], *expected_notification2);

        let result = list_notifications(
            &app.client,
            &app.app.api_address,
            vec![NotificationStatus::Read, NotificationStatus::Unread],
            true,
            None,
            None,
            false,
        )
        .await;

        assert_eq!(result.len(), 3);
        assert_eq!(result[0], *expected_notification1);
        assert_eq!(result[1], *expected_notification2);
        assert_eq!(result[2], *snoozed_notification);

        let result = list_notifications(
            &app.client,
            &app.app.api_address,
            vec![NotificationStatus::Deleted],
            false,
            None,
            None,
            false,
        )
        .await;

        assert_eq!(result.len(), 1);
        assert_eq!(result[0], *deleted_notification);

        let result = list_notifications(
            &app.client,
            &app.app.api_address,
            vec![NotificationStatus::Unsubscribed],
            false,
            None,
            None,
            false,
        )
        .await;

        assert!(result.is_empty());

        // Test listing notifications of another user
        let (client, _user) =
            authenticate_user(&app.app, "5678", "Jane", "Doe", "jane@example.com").await;

        let result = list_notifications(
            &client,
            &app.app.api_address,
            vec![NotificationStatus::Unread],
            false,
            None,
            None,
            false,
        )
        .await;

        assert_eq!(result.len(), 0);
    }

    #[rstest]
    #[tokio::test]
    async fn test_list_notifications_filtered_by_kind(
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        sync_linear_notifications_response: Response<notifications_query::ResponseData>,
    ) {
        let mut github_notification2 = github_notification.clone();
        github_notification2.id = "43".to_string();

        let app = authenticated_app.await;
        let expected_notification1: Box<Notification> = create_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            Box::new(Notification {
                id: Uuid::new_v4().into(),
                title: "notif1".to_string(),
                status: NotificationStatus::Unread,
                source_id: "1234".to_string(),
                metadata: NotificationMetadata::Github(github_notification.clone()),
                updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
                last_read_at: None,
                snoozed_until: None,
                user_id: app.user.id,
                details: None,
                task_id: None,
            }),
        )
        .await;

        let linear_notifications: Vec<LinearNotification> = sync_linear_notifications_response
            .data
            .unwrap()
            .try_into()
            .unwrap();
        let linear_notification = linear_notifications[2].clone(); // Get an IssueNotification
        let expected_notification2: Box<Notification> = create_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            Box::new(linear_notification.into_notification(app.user.id)),
        )
        .await;

        let result = list_notifications(
            &app.client,
            &app.app.api_address,
            vec![NotificationStatus::Unread, NotificationStatus::Read],
            false,
            None,
            Some(NotificationSourceKind::Github),
            false,
        )
        .await;

        assert_eq!(result.len(), 1);
        assert_eq!(result[0], *expected_notification1);

        let result = list_notifications(
            &app.client,
            &app.app.api_address,
            vec![NotificationStatus::Unread, NotificationStatus::Read],
            false,
            None,
            Some(NotificationSourceKind::Linear),
            false,
        )
        .await;

        assert_eq!(result.len(), 1);
        assert_eq!(result[0], *expected_notification2);

        let result = list_notifications(
            &app.client,
            &app.app.api_address,
            vec![NotificationStatus::Unread, NotificationStatus::Read],
            false,
            None,
            Some(NotificationSourceKind::Todoist),
            false,
        )
        .await;

        assert!(result.is_empty());
    }
}

mod create_notification {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_create_notification(
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
    ) {
        let app = authenticated_app.await;
        let expected_notification = Box::new(Notification {
            id: Uuid::new_v4().into(),
            title: "notif1".to_string(),
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            metadata: NotificationMetadata::Github(github_notification),
            updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
            user_id: app.user.id,
            details: None,
            task_id: None,
        });
        let created_notification: Box<Notification> = create_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            expected_notification.clone(),
        )
        .await;

        assert_eq!(created_notification, expected_notification);

        let notification = get_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            created_notification.id.into(),
        )
        .await;

        assert_eq!(notification, expected_notification);
    }

    #[rstest]
    #[tokio::test]
    async fn test_create_notification_duplicate_notification(
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
    ) {
        let app = authenticated_app.await;
        let expected_notification = Box::new(Notification {
            id: Uuid::new_v4().into(),
            title: "notif1".to_string(),
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            metadata: NotificationMetadata::Github(github_notification),
            updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
            user_id: app.user.id,
            details: None,
            task_id: None,
        });
        let created_notification: Box<Notification> = create_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            expected_notification.clone(),
        )
        .await;

        assert_eq!(created_notification, expected_notification);

        let response = create_resource_response(
            &app.client,
            &app.app.api_address,
            "notifications",
            expected_notification,
        )
        .await;

        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body = response.text().await.expect("Cannot get response body");
        assert_eq!(
            body,
            json!({ "message": format!("The entity {} already exists", created_notification.id) })
                .to_string()
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_create_notification_with_wrong_user_id(
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
    ) {
        let app = authenticated_app.await;

        let expected_notification = Box::new(Notification {
            id: Uuid::new_v4().into(),
            title: "notif1".to_string(),
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            metadata: NotificationMetadata::Github(github_notification),
            updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
            user_id: Uuid::new_v4().into(),
            details: None,
            task_id: None,
        });

        let response = create_resource_response(
            &app.client,
            &app.app.api_address,
            "notifications",
            expected_notification.clone(),
        )
        .await;

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}

mod get_notification {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_get_notification_of_another_user(
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
    ) {
        let app = authenticated_app.await;

        let notification = create_notification_from_github_notification(
            &app.client,
            &app.app.api_address,
            &github_notification,
            app.user.id,
        )
        .await;

        let (client, _user) =
            authenticate_user(&app.app, "5678", "Jane", "Doe", "jane@example.com").await;
        let response = get_resource_response(
            &client,
            &app.app.api_address,
            "notifications",
            notification.id.0,
        )
        .await;

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let body = response.text().await.expect("Cannot get response body");
        assert_eq!(
            body,
            json!({
                "message":
                    format!(
                        "Forbidden access: Only the owner of the notification {} can access it",
                        notification.id
                    )
            })
            .to_string()
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_get_unknown_notification(#[future] authenticated_app: AuthenticatedApp) {
        let app = authenticated_app.await;
        let unknown_notification_id = Uuid::new_v4();

        let response = get_resource_response(
            &app.client,
            &app.app.api_address,
            "notifications",
            unknown_notification_id,
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = response.text().await.expect("Cannot get response body");
        assert_eq!(
            body,
            json!({ "message": format!("Cannot find notification {unknown_notification_id}") })
                .to_string()
        );
    }
}

mod patch_notification {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_patch_notification_snoozed_until(
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
    ) {
        let app = authenticated_app.await;
        let expected_notification = Box::new(Notification {
            id: Uuid::new_v4().into(),
            title: "notif1".to_string(),
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            metadata: NotificationMetadata::Github(github_notification),
            updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
            user_id: app.user.id,
            details: None,
            task_id: None,
        });
        let snoozed_time = Utc.with_ymd_and_hms(2022, 1, 1, 1, 2, 3).unwrap();
        let created_notification: Box<Notification> = create_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            expected_notification.clone(),
        )
        .await;

        assert_eq!(created_notification, expected_notification);

        let patched_notification = patch_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            created_notification.id.into(),
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
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
    ) {
        let app = authenticated_app.await;
        let snoozed_time = Utc.with_ymd_and_hms(2022, 1, 1, 1, 2, 3).unwrap();
        let expected_notification = Box::new(Notification {
            id: Uuid::new_v4().into(),
            title: "notif1".to_string(),
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            metadata: NotificationMetadata::Github(github_notification),
            updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: Some(snoozed_time),
            user_id: app.user.id,
            details: None,
            task_id: None,
        });
        let created_notification: Box<Notification> = create_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            expected_notification.clone(),
        )
        .await;

        assert_eq!(created_notification, expected_notification);

        let response = patch_resource_response(
            &app.client,
            &app.app.api_address,
            "notifications",
            created_notification.id.into(),
            &NotificationPatch {
                status: Some(created_notification.status),
                snoozed_until: Some(snoozed_time),
                ..Default::default()
            },
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_notification_of_another_user(
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
    ) {
        let app = authenticated_app.await;
        let notification = create_notification_from_github_notification(
            &app.client,
            &app.app.api_address,
            &github_notification,
            app.user.id,
        )
        .await;
        let (client, _user) =
            authenticate_user(&app.app, "5678", "Jane", "Doe", "jane@example.com").await;

        let response = patch_resource_response(
            &client,
            &app.app.api_address,
            "notifications",
            notification.id.into(),
            &NotificationPatch {
                status: Some(NotificationStatus::Deleted),
                ..Default::default()
            },
        )
        .await;

        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        // Verify notification has not been updated
        let notification_from_db: Box<Notification> = get_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            notification.id.into(),
        )
        .await;

        assert_eq!(notification_from_db.status, NotificationStatus::Unread);
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_notification_without_values_to_update(
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
    ) {
        let app = authenticated_app.await;
        let expected_notification = Box::new(Notification {
            id: Uuid::new_v4().into(),
            title: "notif1".to_string(),
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            metadata: NotificationMetadata::Github(github_notification),
            updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
            user_id: app.user.id,
            details: None,
            task_id: None,
        });
        let created_notification: Box<Notification> = create_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            expected_notification.clone(),
        )
        .await;

        assert_eq!(created_notification, expected_notification);

        let response = patch_resource_response(
            &app.client,
            &app.app.api_address,
            "notifications",
            created_notification.id.into(),
            &NotificationPatch {
                ..Default::default()
            },
        )
        .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = response.text().await.expect("Cannot get response body");
        assert_eq!(
            body,
            json!({
                "message":
                format!(
                    "Invalid input data: Missing `status` field value to update notification {}", expected_notification.id
                )
            })
                .to_string()
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_unknown_notification(#[future] authenticated_app: AuthenticatedApp) {
        let app = authenticated_app.await;
        let unknown_notification_id = Uuid::new_v4();

        let response = patch_resource_response(
            &app.client,
            &app.app.api_address,
            "notifications",
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
                "message": format!("Cannot update unknown notification {unknown_notification_id}")
            })
            .to_string()
        );
    }
}
