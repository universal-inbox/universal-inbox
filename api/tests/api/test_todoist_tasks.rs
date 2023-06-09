use chrono::NaiveDate;
use rstest::*;

use universal_inbox::{
    integration_connection::IntegrationProviderKind,
    notification::{
        integrations::github::GithubNotification, service::NotificationPatch, Notification,
        NotificationStatus, NotificationWithTask,
    },
    task::{
        integrations::todoist::{TodoistItem, TodoistItemDue, TodoistItemPriority},
        service::TaskPatch,
        DueDate, Task, TaskCreation, TaskPriority, TaskProject, TaskStatus,
    },
};

use universal_inbox_api::{
    configuration::Settings,
    integrations::{
        oauth2::NangoConnection,
        todoist::{
            TodoistService, TodoistSyncCommandItemMoveArgs, TodoistSyncCommandItemUpdateArgs,
            TodoistSyncResponse,
        },
    },
};

use crate::helpers::{
    auth::{authenticated_app, AuthenticatedApp},
    integration_connection::{
        create_and_mock_integration_connection, nango_github_connection, nango_todoist_connection,
    },
    notification::{
        create_task_from_notification,
        github::{create_notification_from_github_notification, github_notification},
    },
    rest::{get_resource, patch_resource},
    settings,
    task::todoist::{
        create_task_from_todoist_item, mock_todoist_complete_item_service,
        mock_todoist_delete_item_service, mock_todoist_get_item_service,
        mock_todoist_item_add_service, mock_todoist_sync_project_add,
        mock_todoist_sync_resources_service, mock_todoist_sync_service,
        sync_todoist_projects_response, todoist_item, TodoistSyncPartialCommand,
    },
};

mod patch_task {

    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_patch_todoist_task_status_as_deleted(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        todoist_item: Box<TodoistItem>,
        nango_todoist_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;
        let existing_todoist_task_creation = create_task_from_todoist_item(
            &app.client,
            &app.app_address,
            &todoist_item,
            "Inbox".to_string(),
            app.user.id,
        )
        .await;
        let existing_todoist_task = existing_todoist_task_creation.task;
        assert_eq!(existing_todoist_task.status, TaskStatus::Active);
        let existing_todoist_notification = existing_todoist_task_creation.notification.unwrap();
        create_and_mock_integration_connection(
            &app,
            IntegrationProviderKind::Todoist,
            &settings,
            nango_todoist_connection,
        )
        .await;

        let todoist_mock = mock_todoist_delete_item_service(
            &app.todoist_mock_server,
            &existing_todoist_task.source_id,
        );

        let patched_task = patch_resource(
            &app.client,
            &app.app_address,
            "tasks",
            existing_todoist_task.id.into(),
            &TaskPatch {
                status: Some(TaskStatus::Deleted),
                ..Default::default()
            },
        )
        .await;

        todoist_mock.assert();
        assert_eq!(
            patched_task,
            Box::new(Task {
                status: TaskStatus::Deleted,
                ..existing_todoist_task
            })
        );

        let deleted_notification: Box<Notification> = get_resource(
            &app.client,
            &app.app_address,
            "notifications",
            existing_todoist_notification.id.into(),
        )
        .await;
        assert_eq!(deleted_notification.status, NotificationStatus::Deleted);
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_todoist_task_status_as_done(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        todoist_item: Box<TodoistItem>,
        nango_todoist_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;
        let existing_todoist_task_creation = create_task_from_todoist_item(
            &app.client,
            &app.app_address,
            &todoist_item,
            "Inbox".to_string(),
            app.user.id,
        )
        .await;
        let existing_todoist_task = existing_todoist_task_creation.task;
        assert_eq!(existing_todoist_task.status, TaskStatus::Active);
        let existing_todoist_notification = existing_todoist_task_creation.notification.unwrap();
        create_and_mock_integration_connection(
            &app,
            IntegrationProviderKind::Todoist,
            &settings,
            nango_todoist_connection,
        )
        .await;

        let todoist_mock = mock_todoist_complete_item_service(
            &app.todoist_mock_server,
            &existing_todoist_task.source_id,
        );

        let patched_task: Box<Task> = patch_resource(
            &app.client,
            &app.app_address,
            "tasks",
            existing_todoist_task.id.into(),
            &TaskPatch {
                status: Some(TaskStatus::Done),
                ..Default::default()
            },
        )
        .await;

        todoist_mock.assert();
        assert!(patched_task.completed_at.is_some());
        assert_eq!(
            patched_task,
            Box::new(Task {
                status: TaskStatus::Done,
                completed_at: patched_task.completed_at,
                ..existing_todoist_task
            })
        );

        let deleted_notification: Box<Notification> = get_resource(
            &app.client,
            &app.app_address,
            "notifications",
            existing_todoist_notification.id.into(),
        )
        .await;
        assert_eq!(deleted_notification.status, NotificationStatus::Deleted);
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_todoist_task_to_plan_to_new_project(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        todoist_item: Box<TodoistItem>,
        sync_todoist_projects_response: TodoistSyncResponse,
        nango_todoist_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;
        let existing_todoist_task_creation = create_task_from_todoist_item(
            &app.client,
            &app.app_address,
            &todoist_item,
            "Inbox".to_string(),
            app.user.id,
        )
        .await;
        let existing_todoist_task = existing_todoist_task_creation.task;
        assert_eq!(
            existing_todoist_task.due_at,
            Some(DueDate::Date(NaiveDate::from_ymd_opt(2016, 9, 1).unwrap()))
        );
        assert_eq!(existing_todoist_task.priority, TaskPriority::P4);
        assert_eq!(existing_todoist_task.project, "Inbox".to_string());
        let existing_todoist_notification = existing_todoist_task_creation.notification.unwrap();

        let new_due_at = DueDate::Date(NaiveDate::from_ymd_opt(2022, 1, 1).unwrap());
        let new_priority = TodoistItemPriority::P2;
        let new_project = "Project1".to_string();
        let new_project_id = "3333".to_string();
        create_and_mock_integration_connection(
            &app,
            IntegrationProviderKind::Todoist,
            &settings,
            nango_todoist_connection,
        )
        .await;

        let todoist_projects_mock = mock_todoist_sync_resources_service(
            &app.todoist_mock_server,
            "projects",
            &sync_todoist_projects_response,
        );
        let todoist_project_add_mock =
            mock_todoist_sync_project_add(&app.todoist_mock_server, &new_project, &new_project_id);
        let todoist_sync_mock = mock_todoist_sync_service(
            &app.todoist_mock_server,
            vec![
                TodoistSyncPartialCommand::ItemMove {
                    args: TodoistSyncCommandItemMoveArgs {
                        id: existing_todoist_task.source_id.clone(),
                        project_id: new_project_id,
                    },
                },
                TodoistSyncPartialCommand::ItemUpdate {
                    args: TodoistSyncCommandItemUpdateArgs {
                        id: existing_todoist_task.source_id.clone(),
                        due: Some(Some(TodoistItemDue {
                            string: "".to_string(),
                            date: new_due_at.clone(),
                            is_recurring: false,
                            timezone: None,
                            lang: "en".to_string(),
                        })),
                        priority: Some(new_priority),
                        description: None,
                    },
                },
            ],
            None,
        );

        let patched_task = patch_resource(
            &app.client,
            &app.app_address,
            "tasks",
            existing_todoist_task.id.into(),
            &TaskPatch {
                project: Some(new_project.clone()),
                due_at: Some(Some(new_due_at.clone())),
                priority: Some(new_priority.into()),
                ..Default::default()
            },
        )
        .await;

        todoist_projects_mock.assert();
        todoist_project_add_mock.assert();
        todoist_sync_mock.assert();

        assert_eq!(
            patched_task,
            Box::new(Task {
                project: new_project,
                due_at: Some(new_due_at),
                priority: new_priority.into(),
                ..existing_todoist_task
            })
        );

        let deleted_notification: Box<Notification> = get_resource(
            &app.client,
            &app.app_address,
            "notifications",
            existing_todoist_notification.id.into(),
        )
        .await;
        assert_eq!(deleted_notification.status, NotificationStatus::Deleted);
    }

    // Cannot test project creation as it will fetch projects more than once
    // and httpmock does not support mocking the same URL with different results
    #[rstest]
    #[tokio::test]
    async fn test_create_todoist_task_from_notification(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        sync_todoist_projects_response: TodoistSyncResponse,
        todoist_item: Box<TodoistItem>,
        nango_todoist_connection: Box<NangoConnection>,
        nango_github_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;

        let notification = create_notification_from_github_notification(
            &app.client,
            &app.app_address,
            &github_notification,
            app.user.id,
        )
        .await;
        // Existing project in sync_todoist_projects_response
        let project = "Project2".to_string();
        let project_id = "2222".to_string();
        let todoist_item = Box::new(TodoistItem {
            project_id: project_id.clone(),
            ..(*todoist_item).clone()
        });
        let due_at: Option<DueDate> = todoist_item.due.as_ref().map(|due| due.into());
        let body = Some(format!(
            "- [{}]({})",
            notification.title,
            notification.source_html_url.as_ref().unwrap()
        ));
        create_and_mock_integration_connection(
            &app,
            IntegrationProviderKind::Todoist,
            &settings,
            nango_todoist_connection,
        )
        .await;
        create_and_mock_integration_connection(
            &app,
            IntegrationProviderKind::Github,
            &settings,
            nango_github_connection,
        )
        .await;

        let todoist_projects_mock = mock_todoist_sync_resources_service(
            &app.todoist_mock_server,
            "projects",
            &sync_todoist_projects_response,
        );
        let todoist_item_add_mock = mock_todoist_item_add_service(
            &app.todoist_mock_server,
            &todoist_item.id,
            todoist_item.content.clone(),
            body.clone(),
            todoist_item.project_id.clone(),
            due_at.as_ref().map(|due_at| due_at.into()),
            todoist_item.priority,
        );
        let todoist_get_item_mock =
            mock_todoist_get_item_service(&app.todoist_mock_server, todoist_item.clone());

        let notification_with_task = create_task_from_notification(
            &app.client,
            &app.app_address,
            notification.id,
            &TaskCreation {
                title: todoist_item.content.clone(),
                body,
                project: project.parse::<TaskProject>().unwrap(),
                due_at,
                priority: todoist_item.priority.into(),
            },
        )
        .await;

        todoist_projects_mock.assert();
        todoist_item_add_mock.assert();
        todoist_get_item_mock.assert();

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
                    ..*TodoistService::build_task_with_project_name(
                        &todoist_item,
                        project,
                        app.user.id
                    )
                    .await
                })
            ))
        );

        let deleted_notification: Box<Notification> = get_resource(
            &app.client,
            &app.app_address,
            "notifications",
            notification.id.into(),
        )
        .await;
        assert_eq!(deleted_notification.status, NotificationStatus::Deleted);
    }
}

mod patch_notification {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_patch_notification_to_associate_with_task(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        todoist_item: Box<TodoistItem>,
        nango_todoist_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;
        let notification = create_notification_from_github_notification(
            &app.client,
            &app.app_address,
            &github_notification,
            app.user.id,
        )
        .await;
        let existing_todoist_task = create_task_from_todoist_item(
            &app.client,
            &app.app_address,
            &todoist_item,
            "Project2".to_string(),
            app.user.id,
        )
        .await;
        create_and_mock_integration_connection(
            &app,
            IntegrationProviderKind::Todoist,
            &settings,
            nango_todoist_connection,
        )
        .await;
        let todoist_sync_mock = mock_todoist_sync_service(
            &app.todoist_mock_server,
            vec![TodoistSyncPartialCommand::ItemUpdate {
                args: TodoistSyncCommandItemUpdateArgs {
                    id: existing_todoist_task.task.source_id.clone(),
                    description: Some(format!(
                        "\n- [{}]({})",
                        notification.title,
                        notification.source_html_url.as_ref().unwrap()
                    )),
                    ..Default::default()
                },
            }],
            None,
        );

        let patched_notification = patch_resource(
            &app.client,
            &app.app_address,
            "notifications",
            notification.id.into(),
            &NotificationPatch {
                task_id: Some(existing_todoist_task.task.id),
                ..Default::default()
            },
        )
        .await;

        todoist_sync_mock.assert();
        assert_eq!(
            patched_notification,
            Box::new(Notification {
                task_id: Some(existing_todoist_task.task.id),
                ..*notification
            })
        );
    }
}
