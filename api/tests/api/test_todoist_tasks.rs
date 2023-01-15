use std::collections::HashMap;

use chrono::{TimeZone, Utc};
use rstest::*;
use uuid::Uuid;

use universal_inbox::{
    notification::{Notification, NotificationStatus},
    task::{
        integrations::todoist::{get_task_html_url, TodoistItem},
        Task, TaskMetadata, TaskPatch, TaskPriority, TaskStatus,
    },
};

use universal_inbox_api::{
    integrations::todoist::{TodoistCommandStatus, TodoistSyncStatusResponse},
    universal_inbox::task::TaskCreationResult,
};

use crate::helpers::{
    rest::{create_resource, get_resource, patch_resource},
    task::todoist::{mock_todoist_delete_item_service, todoist_item},
    tested_app, TestedApp,
};

mod patch_task {
    use crate::helpers::task::todoist::mock_todoist_complete_item_service;

    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_patch_todoist_task_status_as_deleted(
        #[future] tested_app: TestedApp,
        todoist_item: Box<TodoistItem>,
    ) {
        let app = tested_app.await;
        let existing_todoist_task_creation: Box<TaskCreationResult> = create_resource(
            &app.app_address,
            "tasks",
            Box::new(Task {
                id: Uuid::new_v4().into(),
                source_id: todoist_item.id.clone(),
                title: todoist_item.content.clone(),
                body: todoist_item.description.clone(),
                status: TaskStatus::Active,
                completed_at: None,
                priority: TaskPriority::P4,
                due_at: None,
                source_html_url: get_task_html_url(&todoist_item.id),
                tags: vec!["tag1".to_string()],
                parent_id: None,
                project: "Inbox".to_string(),
                is_recurring: false,
                created_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
                metadata: TaskMetadata::Todoist(*todoist_item),
            }),
        )
        .await;
        let existing_todoist_task = existing_todoist_task_creation.task;
        let existing_todoist_notification = existing_todoist_task_creation.notification.unwrap();
        let sync_todoist_response = TodoistSyncStatusResponse {
            sync_status: HashMap::from([(
                Uuid::new_v4(),
                TodoistCommandStatus::Ok("ok".to_string()),
            )]),
            full_sync: false,
            temp_id_mapping: HashMap::new(),
            sync_token: "sync token".to_string(),
        };
        let todoist_mock = mock_todoist_delete_item_service(
            &app.todoist_mock_server,
            &existing_todoist_task.source_id,
            &sync_todoist_response,
        );

        let patched_task = patch_resource(
            &app.app_address,
            "tasks",
            existing_todoist_task.id.into(),
            &TaskPatch {
                status: Some(TaskStatus::Deleted),
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
        let existing_todoist_task_creation: Box<TaskCreationResult> = create_resource(
            &app.app_address,
            "tasks",
            Box::new(Task {
                id: Uuid::new_v4().into(),
                source_id: todoist_item.id.clone(),
                title: todoist_item.content.clone(),
                body: todoist_item.description.clone(),
                status: TaskStatus::Active,
                completed_at: None,
                priority: TaskPriority::P4,
                due_at: None,
                source_html_url: get_task_html_url(&todoist_item.id),
                tags: vec!["tag1".to_string()],
                parent_id: None,
                project: "Inbox".to_string(),
                is_recurring: false,
                created_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
                metadata: TaskMetadata::Todoist(*todoist_item),
            }),
        )
        .await;
        let existing_todoist_task = existing_todoist_task_creation.task;
        let existing_todoist_notification = existing_todoist_task_creation.notification.unwrap();
        let sync_todoist_response = TodoistSyncStatusResponse {
            sync_status: HashMap::from([(
                Uuid::new_v4(),
                TodoistCommandStatus::Ok("ok".to_string()),
            )]),
            full_sync: false,
            temp_id_mapping: HashMap::new(),
            sync_token: "sync token".to_string(),
        };
        let todoist_mock = mock_todoist_complete_item_service(
            &app.todoist_mock_server,
            &existing_todoist_task.source_id,
            &sync_todoist_response,
        );

        let patched_task: Box<Task> = patch_resource(
            &app.app_address,
            "tasks",
            existing_todoist_task.id.into(),
            &TaskPatch {
                status: Some(TaskStatus::Done),
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
}
