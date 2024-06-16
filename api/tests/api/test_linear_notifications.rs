use chrono::{TimeZone, Timelike, Utc};
use graphql_client::{Error, Response};
use rstest::*;
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::{linear::LinearConfig, todoist::TodoistConfig},
    },
    notification::{
        integrations::linear::LinearNotification, service::NotificationPatch, Notification,
        NotificationStatus,
    },
    task::Task,
    third_party::{
        integrations::todoist::TodoistItem,
        item::{ThirdPartyItem, ThirdPartyItemCreationResult, ThirdPartyItemData},
    },
};

use universal_inbox_api::{
    configuration::Settings,
    integrations::{
        linear::graphql::notifications_query, oauth2::NangoConnection, todoist::TodoistSyncResponse,
    },
};

use crate::helpers::{
    auth::{authenticated_app, AuthenticatedApp},
    integration_connection::{
        create_and_mock_integration_connection, nango_linear_connection, nango_todoist_connection,
    },
    notification::linear::{
        mock_linear_archive_notification_query, mock_linear_issue_notification_subscribers_query,
        mock_linear_project_notification_subscribers_query,
        mock_linear_update_issue_subscribers_query,
        mock_linear_update_notification_snoozed_until_at_query, sync_linear_notifications_response,
    },
    rest::{create_resource, get_resource, patch_resource, patch_resource_response},
    settings,
    task::todoist::{
        mock_todoist_sync_resources_service, sync_todoist_projects_response, todoist_item,
    },
};

mod patch_resource {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_patch_linear_notification_status_as_deleted(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        nango_linear_connection: Box<NangoConnection>,
        sync_linear_notifications_response: Response<notifications_query::ResponseData>,
        sync_todoist_projects_response: TodoistSyncResponse,
        todoist_item: Box<TodoistItem>,
        nango_todoist_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;
        let linear_notifications: Vec<LinearNotification> = sync_linear_notifications_response
            .data
            .unwrap()
            .try_into()
            .unwrap();
        let linear_notification = linear_notifications[2].clone(); // Get an IssueNotification
        create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.integrations.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Linear(LinearConfig::enabled()),
            &settings,
            nango_linear_connection,
            None,
        )
        .await;
        let integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.integrations.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
            &settings,
            nango_todoist_connection,
            None,
        )
        .await;
        mock_todoist_sync_resources_service(
            &app.app.todoist_mock_server,
            "projects",
            &sync_todoist_projects_response,
            None,
        );

        let creation: Box<ThirdPartyItemCreationResult> = create_resource(
            &app.client,
            &app.app.api_address,
            "third_party/items",
            Box::new(ThirdPartyItem {
                id: Uuid::new_v4().into(),
                source_id: todoist_item.id.clone(),
                created_at: Utc::now().with_nanosecond(0).unwrap(),
                updated_at: Utc::now().with_nanosecond(0).unwrap(),
                user_id: app.user.id,
                data: ThirdPartyItemData::TodoistItem(TodoistItem {
                    project_id: "2222".to_string(), // ie. "Project2"
                    added_at: Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
                    ..*todoist_item.clone()
                }),
                integration_connection_id: integration_connection.id,
            }),
        )
        .await;
        let existing_todoist_task = creation.task.as_ref().unwrap();

        let expected_notification = Box::new(linear_notification.into_notification(app.user.id));
        let linear_archive_notification_mock = mock_linear_archive_notification_query(
            &app.app.linear_mock_server,
            expected_notification.source_id.clone(),
            true,
            None,
        );

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
        linear_archive_notification_mock.assert();

        let task: Box<Task> = get_resource(
            &app.client,
            &app.app.api_address,
            "tasks",
            existing_todoist_task.id.into(),
        )
        .await;
        assert_eq!(task.status, existing_todoist_task.status);
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_linear_notification_status_as_deleted_with_linear_error_response(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        nango_linear_connection: Box<NangoConnection>,
        sync_linear_notifications_response: Response<notifications_query::ResponseData>,
    ) {
        let app = authenticated_app.await;
        let linear_notifications: Vec<LinearNotification> = sync_linear_notifications_response
            .data
            .unwrap()
            .try_into()
            .unwrap();
        let linear_notification = linear_notifications[2].clone(); // Get an IssueNotification
        create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.integrations.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Linear(LinearConfig::enabled()),
            &settings,
            nango_linear_connection,
            None,
        )
        .await;

        let expected_notification = Box::new(linear_notification.into_notification(app.user.id));
        let linear_archive_notification_mock = mock_linear_archive_notification_query(
            &app.app.linear_mock_server,
            expected_notification.source_id.clone(),
            true,
            Some(vec![Error {
                message: "Entity not found".to_string(),
                path: None,
                locations: None,
                extensions: None,
            }]),
        );

        let created_notification: Box<Notification> = create_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            expected_notification.clone(),
        )
        .await;

        assert_eq!(created_notification, expected_notification);

        let patch_response = patch_resource_response(
            &app.client,
            &app.app.api_address,
            "notifications",
            created_notification.id.into(),
            &NotificationPatch {
                status: Some(NotificationStatus::Deleted),
                ..Default::default()
            },
        )
        .await;

        assert_eq!(patch_response.status(), 500);
        let body = patch_response.text().await.unwrap();
        assert_eq!(
            body,
            r#"{"message":"Recoverable error: Errors occured while querying Linear API: Entity not found"}"#
        );
        linear_archive_notification_mock.assert();
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_linear_notification_status_as_deleted_with_linear_unsuccessful_response(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        nango_linear_connection: Box<NangoConnection>,
        sync_linear_notifications_response: Response<notifications_query::ResponseData>,
    ) {
        let app = authenticated_app.await;
        let linear_notifications: Vec<LinearNotification> = sync_linear_notifications_response
            .data
            .unwrap()
            .try_into()
            .unwrap();
        let linear_notification = linear_notifications[2].clone(); // Get an IssueNotification
        create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.integrations.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Linear(LinearConfig::enabled()),
            &settings,
            nango_linear_connection,
            None,
        )
        .await;

        let expected_notification = Box::new(linear_notification.into_notification(app.user.id));
        let linear_archive_notification_mock = mock_linear_archive_notification_query(
            &app.app.linear_mock_server,
            expected_notification.source_id.clone(),
            false,
            None,
        );

        let created_notification: Box<Notification> = create_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            expected_notification.clone(),
        )
        .await;

        assert_eq!(created_notification, expected_notification);

        let patch_response = patch_resource_response(
            &app.client,
            &app.app.api_address,
            "notifications",
            created_notification.id.into(),
            &NotificationPatch {
                status: Some(NotificationStatus::Deleted),
                ..Default::default()
            },
        )
        .await;

        assert_eq!(patch_response.status(), 500);
        let body = patch_response.text().await.unwrap();
        assert_eq!(
            body,
            r#"{"message":"Linear API call failed with an unknown error"}"#
        );
        linear_archive_notification_mock.assert();
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_linear_issue_notification_status_as_unsubscribed(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        nango_linear_connection: Box<NangoConnection>,
        sync_linear_notifications_response: Response<notifications_query::ResponseData>,
    ) {
        let app = authenticated_app.await;
        let linear_notifications: Vec<LinearNotification> = sync_linear_notifications_response
            .data
            .unwrap()
            .try_into()
            .unwrap();
        let linear_notification = linear_notifications[2].clone(); // Get an IssueNotification
        create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.integrations.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Linear(LinearConfig::enabled()),
            &settings,
            nango_linear_connection,
            None,
        )
        .await;

        let expected_notification = Box::new(linear_notification.into_notification(app.user.id));

        let linear_query_notification_subscribers_mock =
            mock_linear_issue_notification_subscribers_query(
                &app.app.linear_mock_server,
                expected_notification.source_id.clone(),
                "user_id".to_string(),
                vec!["user_id".to_string(), "other_user_id".to_string()],
            );

        let linear_update_issue_subscribers_mock = mock_linear_update_issue_subscribers_query(
            &app.app.linear_mock_server,
            expected_notification.source_id.clone(),
            vec!["other_user_id".to_string()],
            true,
            None,
        );

        let linear_archive_notification_mock = mock_linear_archive_notification_query(
            &app.app.linear_mock_server,
            expected_notification.source_id.clone(),
            true,
            None,
        );

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

        linear_query_notification_subscribers_mock.assert();
        linear_update_issue_subscribers_mock.assert();
        linear_archive_notification_mock.assert();
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_linear_project_notification_status_as_unsubscribed(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        nango_linear_connection: Box<NangoConnection>,
        sync_linear_notifications_response: Response<notifications_query::ResponseData>,
    ) {
        // ProjectNotification has no subscriber list. Notification should not be updated, just archived
        let app = authenticated_app.await;
        let linear_notifications: Vec<LinearNotification> = sync_linear_notifications_response
            .data
            .unwrap()
            .try_into()
            .unwrap();
        let linear_notification = linear_notifications[0].clone(); // Get a ProjectNotification
        create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.integrations.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Linear(LinearConfig::enabled()),
            &settings,
            nango_linear_connection,
            None,
        )
        .await;

        let expected_notification = Box::new(linear_notification.into_notification(app.user.id));

        let linear_query_notification_subscribers_mock =
            mock_linear_project_notification_subscribers_query(
                &app.app.linear_mock_server,
                expected_notification.source_id.clone(),
            );

        let linear_archive_notification_mock = mock_linear_archive_notification_query(
            &app.app.linear_mock_server,
            expected_notification.source_id.clone(),
            true,
            None,
        );

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

        linear_query_notification_subscribers_mock.assert();
        linear_archive_notification_mock.assert();
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_linear_notification_snoozed_until(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        nango_linear_connection: Box<NangoConnection>,
        sync_linear_notifications_response: Response<notifications_query::ResponseData>,
    ) {
        let app = authenticated_app.await;
        let linear_notifications: Vec<LinearNotification> = sync_linear_notifications_response
            .data
            .unwrap()
            .try_into()
            .unwrap();
        let linear_notification = linear_notifications[0].clone(); // Get a ProjectNotification
        create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.integrations.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Linear(LinearConfig::enabled()),
            &settings,
            nango_linear_connection,
            None,
        )
        .await;

        let expected_notification = Box::new(linear_notification.into_notification(app.user.id));
        let snoozed_time = Utc.with_ymd_and_hms(2022, 1, 1, 1, 2, 3).unwrap();
        let linear_update_notification_snoozed_until_at_mock =
            mock_linear_update_notification_snoozed_until_at_query(
                &app.app.linear_mock_server,
                expected_notification.source_id.clone(),
                snoozed_time,
            );

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

        linear_update_notification_snoozed_until_at_mock.assert();
    }
}
