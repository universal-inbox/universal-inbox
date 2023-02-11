use chrono::NaiveDate;
use rstest::*;

use universal_inbox::{
    notification::{
        integrations::github::GithubNotification, Notification, NotificationStatus,
        NotificationWithTask,
    },
    task::{
        integrations::todoist::{TodoistItem, TodoistItemDue, TodoistItemPriority},
        DueDate, Task, TaskCreation, TaskPatch, TaskPriority, TaskProject, TaskStatus,
    },
};

use universal_inbox_api::integrations::todoist::{
    TodoistService, TodoistSyncCommandItemMoveArgs, TodoistSyncCommandItemUpdateArgs,
    TodoistSyncResponse,
};

use crate::helpers::{
    notification::{
        create_task_from_notification,
        github::{create_notification_from_github_notification, github_notification},
    },
    rest::{get_resource, patch_resource},
    task::todoist::{
        create_task_from_todoist_item, mock_todoist_complete_item_service,
        mock_todoist_delete_item_service, mock_todoist_get_item_service,
        mock_todoist_item_add_service, mock_todoist_sync_project_add,
        mock_todoist_sync_resources_service, mock_todoist_sync_service,
        sync_todoist_projects_response, todoist_item, TodoistSyncPartialCommand,
    },
    tested_app, TestedApp,
};

mod patch_task {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_patch_todoist_task_status_as_deleted(
        #[future] tested_app: TestedApp,
        todoist_item: Box<TodoistItem>,
    ) {
        let app = tested_app.await;
        let existing_todoist_task_creation =
            create_task_from_todoist_item(&app.app_address, &todoist_item).await;
        let existing_todoist_task = existing_todoist_task_creation.task;
        assert_eq!(existing_todoist_task.status, TaskStatus::Active);
        let existing_todoist_notification = existing_todoist_task_creation.notification.unwrap();
        let todoist_mock = mock_todoist_delete_item_service(
            &app.todoist_mock_server,
            &existing_todoist_task.source_id,
        );

        let patched_task = patch_resource(
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
        #[future] tested_app: TestedApp,
        todoist_item: Box<TodoistItem>,
    ) {
        let app = tested_app.await;
        let existing_todoist_task_creation =
            create_task_from_todoist_item(&app.app_address, &todoist_item).await;
        let existing_todoist_task = existing_todoist_task_creation.task;
        assert_eq!(existing_todoist_task.status, TaskStatus::Active);
        let existing_todoist_notification = existing_todoist_task_creation.notification.unwrap();
        let todoist_mock = mock_todoist_complete_item_service(
            &app.todoist_mock_server,
            &existing_todoist_task.source_id,
        );

        let patched_task: Box<Task> = patch_resource(
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
        #[future] tested_app: TestedApp,
        todoist_item: Box<TodoistItem>,
        sync_todoist_projects_response: TodoistSyncResponse,
    ) {
        let app = tested_app.await;
        let existing_todoist_task_creation =
            create_task_from_todoist_item(&app.app_address, &todoist_item).await;
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
                    },
                },
            ],
            None,
        );

        let patched_task = patch_resource(
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
        #[future] tested_app: TestedApp,
        github_notification: Box<GithubNotification>,
        sync_todoist_projects_response: TodoistSyncResponse,
        todoist_item: Box<TodoistItem>,
    ) {
        let app = tested_app.await;

        let notification =
            create_notification_from_github_notification(&app.app_address, &github_notification)
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
                    ..*TodoistService::build_task_with_project_name(&todoist_item, project).await
                })
            ))
        );

        let deleted_notification: Box<Notification> =
            get_resource(&app.app_address, "notifications", notification.id.into()).await;
        assert_eq!(deleted_notification.status, NotificationStatus::Deleted);
    }
}