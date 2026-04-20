use chrono::{Timelike, Utc};
use rstest::*;
use uuid::Uuid;

use universal_inbox::{
    HasHtmlUrl,
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::github::GithubConfig,
        integrations::ticktick::TickTickConfig, provider::IntegrationProviderKind,
    },
    notification::{Notification, NotificationStatus, NotificationWithTask},
    task::{Task, TaskCreation, TaskStatus, service::TaskPatch},
    third_party::{
        integrations::ticktick::TickTickItem,
        item::{ThirdPartyItem, ThirdPartyItemCreationResult, ThirdPartyItemData},
    },
};

use universal_inbox_api::{
    configuration::Settings,
    integrations::{oauth2::NangoConnection, ticktick::TickTickService},
};

use crate::helpers::{
    auth::{AuthenticatedApp, authenticated_app},
    integration_connection::{
        create_and_mock_integration_connection, create_ticktick_integration_connection,
        nango_github_connection,
    },
    notification::{
        create_task_from_notification,
        github::{create_notification_from_github_notification, github_notification},
    },
    rest::{create_resource, get_resource, patch_resource},
    settings,
    task::ticktick::{
        mock_ticktick_complete_task_service, mock_ticktick_delete_task_service,
        mock_ticktick_list_projects_service, mock_ticktick_update_task_service, ticktick_item,
        ticktick_projects_response,
    },
};

use universal_inbox::task::integrations::ticktick::TickTickProject;
use universal_inbox::third_party::integrations::github::GithubNotification;
use universal_inbox_api::integrations::ticktick::TickTickCreateTaskResponse;

mod patch_task {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_patch_ticktick_task_status_as_deleted(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        ticktick_item: Box<TickTickItem>,
        ticktick_projects_response: Vec<TickTickProject>,
    ) {
        let app = authenticated_app.await;
        let integration_connection = create_ticktick_integration_connection(
            &app.app,
            app.user.id,
            &settings,
            IntegrationConnectionConfig::TickTick(TickTickConfig::enabled()),
            None,
        )
        .await;
        mock_ticktick_list_projects_service(
            &app.app.ticktick_mock_server,
            &ticktick_projects_response,
        )
        .await;

        let creation: Box<ThirdPartyItemCreationResult> = create_resource(
            &app.client,
            &app.app.api_address,
            "third_party/task/items",
            Box::new(ThirdPartyItem {
                id: Uuid::new_v4().into(),
                source_id: ticktick_item.id.clone(),
                created_at: Utc::now().with_nanosecond(0).unwrap(),
                updated_at: Utc::now().with_nanosecond(0).unwrap(),
                user_id: app.user.id,
                data: ThirdPartyItemData::TickTickItem(Box::new(TickTickItem {
                    project_id: "tt_proj_1111".to_string(), // ie. "Inbox"
                    ..*ticktick_item.clone()
                })),
                integration_connection_id: integration_connection.id,
                source_item: None,
            }),
        )
        .await;
        let existing_task = creation.task.as_ref().unwrap().clone();
        assert_eq!(existing_task.status, TaskStatus::Active);
        let existing_notification = creation.notification.as_ref().unwrap().clone();

        mock_ticktick_delete_task_service(
            &app.app.ticktick_mock_server,
            &ticktick_item.project_id,
            &ticktick_item.id,
        )
        .await;

        let patched_task = patch_resource(
            &app.client,
            &app.app.api_address,
            "tasks",
            existing_task.id.into(),
            &TaskPatch {
                status: Some(TaskStatus::Deleted),
                ..Default::default()
            },
        )
        .await;

        assert_eq!(
            patched_task,
            Box::new(Task {
                status: TaskStatus::Deleted,
                ..existing_task
            })
        );

        let deleted_notification: Box<Notification> = get_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            existing_notification.id.into(),
        )
        .await;
        assert_eq!(deleted_notification.status, NotificationStatus::Deleted);
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_ticktick_task_status_as_done(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        ticktick_item: Box<TickTickItem>,
        ticktick_projects_response: Vec<TickTickProject>,
    ) {
        let app = authenticated_app.await;
        let integration_connection = create_ticktick_integration_connection(
            &app.app,
            app.user.id,
            &settings,
            IntegrationConnectionConfig::TickTick(TickTickConfig::enabled()),
            None,
        )
        .await;
        mock_ticktick_list_projects_service(
            &app.app.ticktick_mock_server,
            &ticktick_projects_response,
        )
        .await;

        let creation: Box<ThirdPartyItemCreationResult> = create_resource(
            &app.client,
            &app.app.api_address,
            "third_party/task/items",
            Box::new(ThirdPartyItem {
                id: Uuid::new_v4().into(),
                source_id: ticktick_item.id.clone(),
                created_at: Utc::now().with_nanosecond(0).unwrap(),
                updated_at: Utc::now().with_nanosecond(0).unwrap(),
                user_id: app.user.id,
                data: ThirdPartyItemData::TickTickItem(Box::new(TickTickItem {
                    project_id: "tt_proj_1111".to_string(), // ie. "Inbox"
                    ..*ticktick_item.clone()
                })),
                integration_connection_id: integration_connection.id,
                source_item: None,
            }),
        )
        .await;
        let existing_task = creation.task.as_ref().unwrap().clone();
        assert_eq!(existing_task.status, TaskStatus::Active);
        let existing_notification = creation.notification.as_ref().unwrap().clone();

        mock_ticktick_complete_task_service(
            &app.app.ticktick_mock_server,
            &ticktick_item.project_id,
            &ticktick_item.id,
        )
        .await;

        let patched_task: Box<Task> = patch_resource(
            &app.client,
            &app.app.api_address,
            "tasks",
            existing_task.id.into(),
            &TaskPatch {
                status: Some(TaskStatus::Done),
                ..Default::default()
            },
        )
        .await;

        assert!(patched_task.completed_at.is_some());
        assert_eq!(
            patched_task,
            Box::new(Task {
                status: TaskStatus::Done,
                completed_at: patched_task.completed_at,
                ..existing_task
            })
        );

        let deleted_notification: Box<Notification> = get_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            existing_notification.id.into(),
        )
        .await;
        assert_eq!(deleted_notification.status, NotificationStatus::Deleted);
    }

    #[rstest]
    #[tokio::test]
    async fn test_create_ticktick_task_from_notification(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        ticktick_item: Box<TickTickItem>,
        ticktick_projects_response: Vec<TickTickProject>,
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

        // Existing project in ticktick_projects_response
        let project = "Project2".to_string();
        let project_id = "tt_proj_2222".to_string();
        let ticktick_item = Box::new(TickTickItem {
            project_id: project_id.clone(),
            ..*ticktick_item
        });
        let body = Some(format!(
            "- [{}]({})",
            notification.title,
            notification.get_html_url().as_ref()
        ));
        let ticktick_integration_connection = create_ticktick_integration_connection(
            &app.app,
            app.user.id,
            &settings,
            IntegrationConnectionConfig::TickTick(TickTickConfig::enabled()),
            None,
        )
        .await;

        // Mock GitHub notification deletion
        wiremock::Mock::given(wiremock::matchers::method("DELETE"))
            .and(wiremock::matchers::path("/notifications/threads/1"))
            .respond_with(wiremock::ResponseTemplate::new(205))
            .mount(&app.app.github_mock_server)
            .await;

        // Mock TickTick list projects
        mock_ticktick_list_projects_service(
            &app.app.ticktick_mock_server,
            &ticktick_projects_response,
        )
        .await;

        // Mock TickTick create task
        let create_response = TickTickCreateTaskResponse {
            id: ticktick_item.id.clone(),
            project_id: ticktick_item.project_id.clone(),
            title: ticktick_item.title.clone(),
            content: ticktick_item.content.clone(),
            desc: ticktick_item.desc.clone(),
            all_day: ticktick_item.all_day,
            start_date: ticktick_item.start_date,
            due_date: ticktick_item.due_date,
            time_zone: ticktick_item.time_zone.clone(),
            priority: ticktick_item.priority,
            status: ticktick_item.status,
            tags: ticktick_item.tags.clone(),
        };
        crate::helpers::task::ticktick::mock_ticktick_create_task_service(
            &app.app.ticktick_mock_server,
            &ticktick_item.title,
            Some(&project_id),
            ticktick_item.priority,
            &create_response,
        )
        .await;

        // Mock TickTick get task (used to fetch the full item after creation)
        crate::helpers::task::ticktick::mock_ticktick_get_task_service(
            &app.app.ticktick_mock_server,
            &ticktick_item.project_id,
            &ticktick_item.id,
            &ticktick_item,
        )
        .await;

        let notification_with_task = create_task_from_notification(
            &app.client,
            &app.app.api_address,
            notification.id,
            Some(TaskCreation {
                title: ticktick_item.title.clone(),
                body,
                project_name: Some("Project2".to_string()),
                due_at: ticktick_item.get_due_date(),
                priority: ticktick_item.priority.into(),
                task_provider_kind: Some(IntegrationProviderKind::TickTick),
            }),
        )
        .await;

        let new_task_id = notification_with_task
            .as_ref()
            .unwrap()
            .task
            .as_ref()
            .unwrap()
            .id;
        assert_eq!(
            notification_with_task,
            Some(NotificationWithTask::build(
                &Notification {
                    status: NotificationStatus::Deleted,
                    ..*notification
                },
                Some(Task {
                    id: new_task_id,
                    updated_at: notification_with_task
                        .as_ref()
                        .unwrap()
                        .task
                        .as_ref()
                        .unwrap()
                        .updated_at,
                    ..(*TickTickService::build_task_with_project_name(
                        &ticktick_item,
                        project,
                        &ThirdPartyItem {
                            id: notification_with_task
                                .as_ref()
                                .unwrap()
                                .task
                                .as_ref()
                                .unwrap()
                                .source_item
                                .id,
                            source_id: ticktick_item.id.clone(),
                            created_at: notification_with_task
                                .as_ref()
                                .unwrap()
                                .task
                                .as_ref()
                                .unwrap()
                                .source_item
                                .created_at,
                            updated_at: notification_with_task
                                .as_ref()
                                .unwrap()
                                .task
                                .as_ref()
                                .unwrap()
                                .source_item
                                .updated_at,
                            user_id: app.user.id,
                            data: ThirdPartyItemData::TickTickItem(ticktick_item.clone()),
                            integration_connection_id: ticktick_integration_connection.id,
                            source_item: None,
                        },
                        app.user.id
                    )
                    .await)
                        .into()
                })
            ))
        );

        let deleted_notification: Box<Notification> = get_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            notification.id.into(),
        )
        .await;
        assert_eq!(deleted_notification.status, NotificationStatus::Deleted);
        assert_eq!(deleted_notification.task_id, Some(new_task_id));
    }

    // Regression test for universal-inbox-4mn: linking a notification to an
    // existing TickTick task used to send projectId="" in the update body,
    // which caused TickTick to duplicate the task instead of patching it.
    // The mock here rejects any request that doesn't carry the real projectId
    // from the task's third_party_item data.
    #[rstest]
    #[tokio::test]
    async fn test_link_notification_with_ticktick_task_sends_real_project_id(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        ticktick_item: Box<TickTickItem>,
        ticktick_projects_response: Vec<TickTickProject>,
        nango_github_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;

        // GitHub notification setup (source of the notification we will link).
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
        // GitHub delete side effect — respond OK so we don't fail before reaching TickTick.
        wiremock::Mock::given(wiremock::matchers::method("DELETE"))
            .and(wiremock::matchers::path("/notifications/threads/1"))
            .respond_with(wiremock::ResponseTemplate::new(205))
            .mount(&app.app.github_mock_server)
            .await;

        // TickTick connection + pre-existing task stored with a real projectId.
        let ticktick_integration_connection = create_ticktick_integration_connection(
            &app.app,
            app.user.id,
            &settings,
            IntegrationConnectionConfig::TickTick(TickTickConfig::enabled()),
            None,
        )
        .await;
        // Projects are fetched during third_party_item_into_task to resolve the project name.
        mock_ticktick_list_projects_service(
            &app.app.ticktick_mock_server,
            &ticktick_projects_response,
        )
        .await;
        let ticktick_item = Box::new(TickTickItem {
            project_id: "tt_proj_2222".to_string(),
            content: Some("existing body".to_string()),
            ..*ticktick_item
        });
        let creation: Box<ThirdPartyItemCreationResult> = create_resource(
            &app.client,
            &app.app.api_address,
            "third_party/task/items",
            Box::new(ThirdPartyItem {
                id: Uuid::new_v4().into(),
                source_id: ticktick_item.id.clone(),
                created_at: Utc::now().with_nanosecond(0).unwrap(),
                updated_at: Utc::now().with_nanosecond(0).unwrap(),
                user_id: app.user.id,
                data: ThirdPartyItemData::TickTickItem(ticktick_item.clone()),
                integration_connection_id: ticktick_integration_connection.id,
                source_item: None,
            }),
        )
        .await;
        let task = creation.task.as_ref().unwrap();

        // Mock TickTick update — the partial-body matcher will reject the
        // request if projectId is missing or mismatched, which is exactly the
        // condition that used to trigger the duplicate-creation bug.
        let update_response = TickTickCreateTaskResponse {
            id: ticktick_item.id.clone(),
            project_id: ticktick_item.project_id.clone(),
            title: ticktick_item.title.clone(),
            content: ticktick_item.content.clone(),
            desc: ticktick_item.desc.clone(),
            all_day: ticktick_item.all_day,
            start_date: ticktick_item.start_date,
            due_date: ticktick_item.due_date,
            time_zone: ticktick_item.time_zone.clone(),
            priority: ticktick_item.priority,
            status: ticktick_item.status,
            tags: ticktick_item.tags.clone(),
        };
        mock_ticktick_update_task_service(
            &app.app.ticktick_mock_server,
            &ticktick_item.id,
            &ticktick_item.project_id,
            notification.title.as_str(),
            &update_response,
        )
        .await;

        // Link the notification to the TickTick task.
        let _: Box<Notification> = patch_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            notification.id.into(),
            &universal_inbox::notification::service::NotificationPatch {
                status: Some(NotificationStatus::Deleted),
                task_id: Some(task.id),
                ..Default::default()
            },
        )
        .await;

        // Assert the task body in our DB now contains the linked notification
        // (proof that link_notification_with_task actually ran end-to-end).
        let updated: Box<Task> =
            get_resource(&app.client, &app.app.api_address, "tasks", task.id.into()).await;
        assert!(
            updated.body.contains(notification.title.as_str()),
            "expected task.body to contain the linked notification title, got: {}",
            updated.body
        );
    }
}
