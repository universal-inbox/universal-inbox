use chrono::{TimeDelta, TimeZone, Timelike, Utc};
use graphql_client::Response;
use http::StatusCode;
use rstest::*;
use serde_json::json;
use tokio::time::{Duration, sleep};
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::{github::GithubConfig, linear::LinearConfig},
    },
    notification::{
        Notification, NotificationSourceKind, NotificationStatus, service::NotificationPatch,
    },
    third_party::integrations::{github::GithubNotification, linear::LinearNotification},
};

use wiremock::{
    Mock, ResponseTemplate,
    matchers::{method, path},
};

use universal_inbox_api::{
    configuration::Settings, integrations::linear::graphql::notifications_query,
    integrations::oauth2::NangoConnection,
};

use crate::helpers::{
    auth::{AuthenticatedApp, authenticate_user, authenticated_app},
    integration_connection::{
        create_and_mock_integration_connection, nango_github_connection, nango_linear_connection,
    },
    notification::{
        github::{create_notification_from_github_notification, github_notification},
        linear::{
            create_notification_from_linear_notification, sync_linear_notifications_response,
        },
        list_notifications, update_notification,
    },
    rest::{
        get_resource, get_resource_response, patch_resource, patch_resource_collection,
        patch_resource_response,
    },
    settings,
};

mod list_notifications {
    use super::*;

    use pretty_assertions::assert_eq;

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
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        nango_github_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;

        let github_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Github(GithubConfig::enabled()),
            &settings,
            nango_github_connection,
            None,
            None,
        )
        .await;

        let expected_notification1 = create_notification_from_github_notification(
            &app.app,
            &github_notification,
            app.user.id,
            github_integration_connection.id,
        )
        .await;

        let mut github_notification2 = github_notification.clone();
        github_notification2.id = "43".to_string();
        let expected_notification2 = create_notification_from_github_notification(
            &app.app,
            &github_notification2,
            app.user.id,
            github_integration_connection.id,
        )
        .await;
        let expected_notification2 = update_notification(
            &app,
            expected_notification2.id,
            &NotificationPatch {
                status: Some(NotificationStatus::Read),
                ..NotificationPatch::default()
            },
            app.user.id,
        )
        .await;

        let mut github_notification_deleted = github_notification.clone();
        github_notification_deleted.id = "54".to_string();
        let deleted_notification = create_notification_from_github_notification(
            &app.app,
            &github_notification_deleted,
            app.user.id,
            github_integration_connection.id,
        )
        .await;
        let deleted_notification = update_notification(
            &app,
            deleted_notification.id,
            &NotificationPatch {
                status: Some(NotificationStatus::Deleted),
                ..NotificationPatch::default()
            },
            app.user.id,
        )
        .await;

        let mut github_notification_snoozed = github_notification.clone();
        github_notification_snoozed.id = "65".to_string();
        let snoozed_notification = create_notification_from_github_notification(
            &app.app,
            &github_notification_snoozed,
            app.user.id,
            github_integration_connection.id,
        )
        .await;
        let snoozed_notification = update_notification(
            &app,
            snoozed_notification.id,
            &NotificationPatch {
                snoozed_until: Some(
                    Utc::now().with_nanosecond(0).unwrap() + TimeDelta::try_minutes(1).unwrap(),
                ),
                ..NotificationPatch::default()
            },
            app.user.id,
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
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        sync_linear_notifications_response: Response<notifications_query::ResponseData>,
        nango_github_connection: Box<NangoConnection>,
        nango_linear_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;
        let github_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Github(GithubConfig::enabled()),
            &settings,
            nango_github_connection,
            None,
            None,
        )
        .await;

        let expected_notification1 = create_notification_from_github_notification(
            &app.app,
            &github_notification,
            app.user.id,
            github_integration_connection.id,
        )
        .await;

        let linear_notifications: Vec<LinearNotification> = sync_linear_notifications_response
            .data
            .unwrap()
            .try_into()
            .unwrap();
        let linear_notification = linear_notifications[2].clone(); // Get an IssueNotification
        let linear_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Linear(LinearConfig::enabled()),
            &settings,
            nango_linear_connection,
            None,
            None,
        )
        .await;
        let expected_notification2 = create_notification_from_linear_notification(
            &app.app,
            &linear_notification,
            app.user.id,
            linear_integration_connection.id,
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

mod get_notification {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_get_notification_of_another_user(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        nango_github_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;

        let github_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Github(GithubConfig::enabled()),
            &settings,
            nango_github_connection,
            None,
            None,
        )
        .await;

        let notification = create_notification_from_github_notification(
            &app.app,
            &github_notification,
            app.user.id,
            github_integration_connection.id,
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

mod patch_notifications_bulk {
    use apalis::prelude::Storage;
    use universal_inbox::notification::service::PatchNotificationsRequest;

    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_patch_notifications_bulk_delete_all(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        nango_github_connection: Box<NangoConnection>,
    ) {
        let mut app = authenticated_app.await;
        let github_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Github(GithubConfig::enabled()),
            &settings,
            nango_github_connection,
            None,
            None,
        )
        .await;
        Mock::given(method("DELETE"))
            .and(path("/notifications/threads/1"))
            .respond_with(ResponseTemplate::new(205))
            .mount(&app.app.github_mock_server)
            .await;
        Mock::given(method("DELETE"))
            .and(path("/notifications/threads/2"))
            .respond_with(ResponseTemplate::new(205))
            .mount(&app.app.github_mock_server)
            .await;

        // Create multiple notifications
        let notification1 = create_notification_from_github_notification(
            &app.app,
            &github_notification,
            app.user.id,
            github_integration_connection.id,
        )
        .await;

        let mut github_notification2 = *github_notification.clone();
        github_notification2.id = "2".to_string();
        let notification2 = create_notification_from_github_notification(
            &app.app,
            &Box::new(github_notification2),
            app.user.id,
            github_integration_connection.id,
        )
        .await;

        // Verify both notifications exist and are unread
        assert_eq!(notification1.status, NotificationStatus::Unread);
        assert_eq!(notification2.status, NotificationStatus::Unread);

        // Perform bulk patch to delete all notifications
        let patch_request = PatchNotificationsRequest {
            status: vec![NotificationStatus::Unread, NotificationStatus::Read],
            sources: vec![NotificationSourceKind::Github],
            patch: NotificationPatch {
                status: Some(NotificationStatus::Deleted),
                snoozed_until: None,
                task_id: None,
            },
        };

        let result: Vec<Notification> = patch_resource_collection(
            &app.client,
            &app.app.api_address,
            "notifications",
            &patch_request,
        )
        .await;

        assert_eq!(result.len(), 2);

        // Verify notifications are now deleted in the database
        let result = list_notifications(
            &app.client,
            &app.app.api_address,
            vec![NotificationStatus::Deleted],
            false,
            None,
            Some(NotificationSourceKind::Github),
            false,
        )
        .await;

        assert_eq!(result.len(), 2);
        assert!(result.iter().any(|elt| elt.id == notification1.id));
        assert!(result.iter().any(|elt| elt.id == notification2.id));

        // Wait a bit to ensure the job is processed
        sleep(Duration::from_millis(1000)).await;

        let job_count = app
            .app
            .redis_storage
            .len()
            .await
            .expect("Failed to get job count");
        assert_eq!(job_count, 0);
    }
}

mod patch_notification {

    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_patch_notification_snoozed_until(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        nango_github_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;
        let github_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Github(GithubConfig::enabled()),
            &settings,
            nango_github_connection,
            None,
            None,
        )
        .await;
        let expected_notification = create_notification_from_github_notification(
            &app.app,
            &github_notification,
            app.user.id,
            github_integration_connection.id,
        )
        .await;
        let snoozed_time = Utc.with_ymd_and_hms(2022, 1, 1, 1, 2, 3).unwrap();

        let patched_notification = patch_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            expected_notification.id.into(),
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
                ..*expected_notification
            })
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_notification_status_without_modification(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        nango_github_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;
        let snoozed_time = Utc.with_ymd_and_hms(2022, 1, 1, 1, 2, 3).unwrap();
        let github_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Github(GithubConfig::enabled()),
            &settings,
            nango_github_connection,
            None,
            None,
        )
        .await;
        let expected_notification = create_notification_from_github_notification(
            &app.app,
            &github_notification,
            app.user.id,
            github_integration_connection.id,
        )
        .await;
        let expected_notification = update_notification(
            &app,
            expected_notification.id,
            &NotificationPatch {
                snoozed_until: Some(snoozed_time),
                ..NotificationPatch::default()
            },
            app.user.id,
        )
        .await;

        let response = patch_resource_response(
            &app.client,
            &app.app.api_address,
            "notifications",
            expected_notification.id.into(),
            &NotificationPatch {
                status: Some(expected_notification.status),
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
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        nango_github_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;

        let github_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Github(GithubConfig::enabled()),
            &settings,
            nango_github_connection,
            None,
            None,
        )
        .await;

        let notification = create_notification_from_github_notification(
            &app.app,
            &github_notification,
            app.user.id,
            github_integration_connection.id,
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
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        nango_github_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;
        let github_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Github(GithubConfig::enabled()),
            &settings,
            nango_github_connection,
            None,
            None,
        )
        .await;
        let expected_notification = create_notification_from_github_notification(
            &app.app,
            &github_notification,
            app.user.id,
            github_integration_connection.id,
        )
        .await;

        let response = patch_resource_response(
            &app.client,
            &app.app.api_address,
            "notifications",
            expected_notification.id.into(),
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
