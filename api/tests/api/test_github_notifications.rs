use chrono::{TimeZone, Utc};
use graphql_client::Response;
use http::StatusCode;
use httpmock::Method::{PATCH, PUT};
use rstest::*;
use serde_json::json;
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::github::GithubConfig,
    },
    notification::{
        integrations::github::GithubNotification, service::NotificationPatch, Notification,
        NotificationDetails, NotificationMetadata, NotificationStatus,
    },
    task::{integrations::todoist::TodoistItem, Task},
};
use universal_inbox_api::{
    configuration::Settings,
    integrations::{github::graphql::pull_request_query, oauth2::NangoConnection},
};

use crate::helpers::{
    auth::{authenticated_app, AuthenticatedApp},
    integration_connection::{create_and_mock_integration_connection, nango_github_connection},
    notification::{
        create_or_update_notification_details,
        github::{github_notification, github_pull_request_123_response},
    },
    rest::{create_resource, get_resource, patch_resource, patch_resource_response},
    settings,
    task::todoist::{create_task_from_todoist_item, todoist_item},
};

mod patch_resource {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_patch_github_notification_status_as_deleted(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        nango_github_connection: Box<NangoConnection>,
        github_notification: Box<GithubNotification>,
        todoist_item: Box<TodoistItem>,
        github_pull_request_123_response: Response<pull_request_query::ResponseData>,
        #[values(205, 304, 404)] github_status_code: u16,
    ) {
        let app = authenticated_app.await;
        create_and_mock_integration_connection(
            &app,
            &settings.integrations.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Github(GithubConfig::enabled()),
            &settings,
            nango_github_connection,
        )
        .await;

        let github_mark_thread_as_read_mock = app.github_mock_server.mock(|when, then| {
            when.method(PATCH)
                .path("/notifications/threads/1234")
                .header("accept", "application/vnd.github.v3+json")
                .header("authorization", "Bearer github_test_access_token");
            then.status(github_status_code);
        });
        let existing_todoist_task = create_task_from_todoist_item(
            &app.client,
            &app.api_address,
            &todoist_item,
            "Project2".to_string(),
            app.user.id,
        )
        .await;
        let notification_details: NotificationDetails = github_pull_request_123_response
            .data
            .clone()
            .unwrap()
            .try_into()
            .unwrap();
        let expected_notification = Box::new(Notification {
            id: Uuid::new_v4().into(),
            title: "notif1".to_string(),
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            source_html_url: Some(github_notification.get_html_url_from_metadata()),
            metadata: NotificationMetadata::Github(github_notification),
            updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
            user_id: app.user.id,
            details: Some(notification_details.clone()),
            task_id: Some(existing_todoist_task.task.id),
        });
        let created_notification: Box<Notification> = create_resource(
            &app.client,
            &app.api_address,
            "notifications",
            expected_notification.clone(),
        )
        .await;
        create_or_update_notification_details(
            &app,
            created_notification.id,
            notification_details.clone(),
        )
        .await;

        assert_eq!(created_notification, expected_notification);

        let patched_notification = patch_resource(
            &app.client,
            &app.api_address,
            "notifications",
            created_notification.id.into(),
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

        let task: Box<Task> = get_resource(
            &app.client,
            &app.api_address,
            "tasks",
            existing_todoist_task.task.id.into(),
        )
        .await;
        assert_eq!(task.status, existing_todoist_task.task.status);
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_github_notification_status_as_unsubscribed(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        nango_github_connection: Box<NangoConnection>,
        #[values(205, 304, 404)] github_status_code: u16,
    ) {
        let app = authenticated_app.await;
        create_and_mock_integration_connection(
            &app,
            &settings.integrations.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Github(GithubConfig::enabled()),
            &settings,
            nango_github_connection,
        )
        .await;

        let github_mark_thread_as_read_mock = app.github_mock_server.mock(|when, then| {
            when.method(PATCH)
                .path("/notifications/threads/1234")
                .header("accept", "application/vnd.github.v3+json")
                .header("authorization", "Bearer github_test_access_token");
            then.status(github_status_code);
        });
        let github_unsubscribed_mock = app.github_mock_server.mock(|when, then| {
            when.method(PUT)
                .path("/notifications/threads/1234/subscription")
                .header("accept", "application/vnd.github.v3+json")
                .header("authorization", "Bearer github_test_access_token")
                .json_body(json!({"ignored": true}));
            then.status(github_status_code);
        });

        let expected_notification = Box::new(Notification {
            id: Uuid::new_v4().into(),
            title: "notif1".to_string(),
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            source_html_url: Some(github_notification.get_html_url_from_metadata()),
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
            &app.api_address,
            "notifications",
            expected_notification.clone(),
        )
        .await;

        assert_eq!(created_notification, expected_notification);

        let patched_notification = patch_resource(
            &app.client,
            &app.api_address,
            "notifications",
            created_notification.id.into(),
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
        github_unsubscribed_mock.assert();
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_github_notification_status_as_deleted_with_github_api_error(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        nango_github_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;
        create_and_mock_integration_connection(
            &app,
            &settings.integrations.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Github(GithubConfig::enabled()),
            &settings,
            nango_github_connection,
        )
        .await;

        let github_mark_thread_as_read_mock = app.github_mock_server.mock(|when, then| {
            when.method(PATCH)
                .path("/notifications/threads/1234")
                .header("accept", "application/vnd.github.v3+json")
                .header("authorization", "Bearer github_test_access_token");
            then.status(403);
        });
        let expected_notification = Box::new(Notification {
            id: Uuid::new_v4().into(),
            title: "notif1".to_string(),
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            source_html_url: Some(github_notification.get_html_url_from_metadata()),
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
            &app.api_address,
            "notifications",
            expected_notification.clone(),
        )
        .await;

        assert_eq!(created_notification, expected_notification);

        let response = patch_resource_response(
            &app.client,
            &app.api_address,
            "notifications",
            created_notification.id.into(),
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

        let notification: Box<Notification> = get_resource(
            &app.client,
            &app.api_address,
            "notifications",
            created_notification.id.into(),
        )
        .await;
        assert_eq!(notification.status, NotificationStatus::Unread);
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_github_notification_snoozed_until(
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
    ) {
        let app = authenticated_app.await;
        let expected_notification = Box::new(Notification {
            id: Uuid::new_v4().into(),
            title: "notif1".to_string(),
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            source_html_url: Some(github_notification.get_html_url_from_metadata()),
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
            &app.api_address,
            "notifications",
            expected_notification.clone(),
        )
        .await;

        assert_eq!(created_notification, expected_notification);

        let patched_notification = patch_resource(
            &app.client,
            &app.api_address,
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
    async fn test_patch_github_notification_status_without_modification(
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
    ) {
        let app = authenticated_app.await;
        let github_api_mock = app.github_mock_server.mock(|when, then| {
            when.any_request();
            then.status(200);
        });
        let snoozed_time = Utc.with_ymd_and_hms(2022, 1, 1, 1, 2, 3).unwrap();
        let expected_notification = Box::new(Notification {
            id: Uuid::new_v4().into(),
            title: "notif1".to_string(),
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            source_html_url: Some(github_notification.get_html_url_from_metadata()),
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
            &app.api_address,
            "notifications",
            expected_notification.clone(),
        )
        .await;

        assert_eq!(created_notification, expected_notification);

        let response = patch_resource_response(
            &app.client,
            &app.api_address,
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
        github_api_mock.assert_hits(0);
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_github_notification_without_values_to_update(
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
    ) {
        let app = authenticated_app.await;
        let github_api_mock = app.github_mock_server.mock(|when, then| {
            when.any_request();
            then.status(200);
        });
        let expected_notification = Box::new(Notification {
            id: Uuid::new_v4().into(),
            title: "notif1".to_string(),
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            source_html_url: Some(github_notification.get_html_url_from_metadata()),
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
            &app.api_address,
            "notifications",
            expected_notification.clone(),
        )
        .await;

        assert_eq!(created_notification, expected_notification);

        let response = patch_resource_response(
            &app.client,
            &app.api_address,
            "notifications",
            created_notification.id.into(),
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
    async fn test_patch_unknown_notification(#[future] authenticated_app: AuthenticatedApp) {
        let app = authenticated_app.await;
        let github_api_mock = app.github_mock_server.mock(|when, then| {
            when.any_request();
            then.status(200);
        });
        let unknown_notification_id = Uuid::new_v4();

        let response = patch_resource_response(
            &app.client,
            &app.api_address,
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
        github_api_mock.assert_hits(0);
    }
}
