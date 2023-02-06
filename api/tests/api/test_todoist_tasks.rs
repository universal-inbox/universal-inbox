use chrono::NaiveDate;
use rstest::*;

use universal_inbox::{
    notification::{Notification, NotificationStatus},
    task::{
        integrations::todoist::{TodoistItem, TodoistItemDue, TodoistItemPriority},
        DueDate, Task, TaskPatch, TaskPriority, TaskStatus,
    },
};

use universal_inbox_api::integrations::todoist::{
    TodoistSyncCommandItemMoveArgs, TodoistSyncCommandItemUpdateArgs, TodoistSyncResponse,
};

use crate::helpers::{
    rest::{get_resource, patch_resource},
    task::todoist::{
        create_task_from_todoist_item, mock_todoist_complete_item_service,
        mock_todoist_delete_item_service, mock_todoist_sync_project_add,
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
}
