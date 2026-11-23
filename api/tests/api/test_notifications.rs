use chrono::{TimeZone, Utc};
use http::StatusCode;
use rstest::*;
use tokio::time::{Duration, sleep};
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::github::GithubConfig,
    },
    notification::{
        Notification, NotificationSourceKind, NotificationStatus, service::NotificationPatch,
    },
    third_party::integrations::github::GithubNotification,
};

use wiremock::{
    Mock, ResponseTemplate,
    matchers::{method, path},
};

use universal_inbox_api::configuration::Settings;

use crate::helpers::integration_connection::OAuthCredentialFixture;
use crate::helpers::{
    auth::{AuthenticatedApp, authenticated_app},
    integration_connection::{create_and_mock_integration_connection, github_oauth_credential},
    notification::{
        github::{create_notification_from_github_notification, github_notification},
        list_notifications, list_only_snoozed_notifications,
    },
    rest::patch_resource_collection,
    settings,
};

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
        github_oauth_credential: OAuthCredentialFixture,
    ) {
        let mut app = authenticated_app.await;
        let github_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Github(GithubConfig::enabled()),
            &settings,
            github_oauth_credential,
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

    /// A bulk patch carrying only a `snoozed_until` used to answer a request
    /// that asked for a write with an empty list: the repository early-returned
    /// on a status-less patch and only ever set `status`. The `SET` clauses are
    /// now derived from the patch, so the snooze is a real write.
    #[rstest]
    #[tokio::test]
    async fn test_patch_notifications_bulk_snooze_only(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        github_oauth_credential: OAuthCredentialFixture,
    ) {
        let mut app = authenticated_app.await;
        let github_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Github(GithubConfig::enabled()),
            &settings,
            github_oauth_credential,
            None,
            None,
        )
        .await;

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

        assert_eq!(notification1.snoozed_until, None);
        assert_eq!(notification2.snoozed_until, None);

        let snoozed_time = Utc.with_ymd_and_hms(2050, 1, 1, 1, 2, 3).unwrap();
        let patch_request = PatchNotificationsRequest {
            status: vec![NotificationStatus::Unread],
            sources: vec![NotificationSourceKind::Github],
            patch: NotificationPatch {
                status: None,
                snoozed_until: Some(snoozed_time),
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
        // The patch carries no status, so the rows keep the one they had.
        assert!(
            result
                .iter()
                .all(|elt| elt.snoozed_until == Some(snoozed_time)
                    && elt.status == NotificationStatus::Unread)
        );

        // Snoozed rows leave the inbox until the timestamp passes.
        let snoozed_notifications = list_only_snoozed_notifications(
            &app.client,
            &app.app.api_address,
            vec![NotificationStatus::Unread],
        )
        .await;

        assert_eq!(snoozed_notifications.len(), 2);
        assert!(
            snoozed_notifications
                .iter()
                .any(|elt| elt.id == notification1.id)
        );
        assert!(
            snoozed_notifications
                .iter()
                .any(|elt| elt.id == notification2.id)
        );

        // Wait a bit to ensure the queued side effect jobs are processed
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

mod create_task_from_notification_validation {
    use super::*;
    use pretty_assertions::assert_eq;
    use universal_inbox::task::{TASK_TITLE_MAX_LENGTH, TaskCreation, TaskPriority};

    use crate::helpers::notification::create_task_from_notification_response;

    // Validation runs before the notification lookup: an unknown id is enough.
    #[rstest]
    #[tokio::test]
    async fn test_create_task_from_notification_rejects_too_long_title(
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;

        let response = create_task_from_notification_response(
            &app.client,
            &app.app.api_address,
            Uuid::new_v4().into(),
            Some(TaskCreation {
                title: "a".repeat(TASK_TITLE_MAX_LENGTH as usize + 1),
                body: None,
                project_name: None,
                due_at: None,
                priority: TaskPriority::P4,
                task_provider_kind: None,
                time_config: None,
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
