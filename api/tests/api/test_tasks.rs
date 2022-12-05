use chrono::{TimeZone, Utc};
use http::{StatusCode, Uri};
use rstest::*;
use serde_json::json;
use uuid::Uuid;

use universal_inbox::task::{
    integrations::todoist::TodoistItem, Task, TaskMetadata, TaskPatch, TaskPriority, TaskStatus,
};

use crate::helpers::{
    rest::{
        create_resource, create_resource_response, get_resource, get_resource_response,
        patch_resource_response,
    },
    task::{list_tasks, todoist::todoist_item},
    tested_app, TestedApp,
};

mod create_task {
    use chrono::NaiveDate;

    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_create_task(#[future] tested_app: TestedApp, todoist_item: Box<TodoistItem>) {
        let app = tested_app.await;
        let expected_minimal_task = Box::new(Task {
            id: uuid::Uuid::new_v4(),
            source_id: "1234".to_string(),
            title: "task 1".to_string(),
            body: "more details".to_string(),
            status: TaskStatus::Active,
            completed_at: None,
            priority: TaskPriority::P4,
            due_at: None,
            source_html_url: "https://todoist.com/showTask?id=1234".parse::<Uri>().ok(),
            tags: vec![],
            parent_id: None,
            project: "Inbox".to_string(),
            is_recurring: false,
            created_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            metadata: TaskMetadata::Todoist(*todoist_item.clone()),
        });

        let created_task =
            create_resource(&app.app_address, "tasks", expected_minimal_task.clone()).await;

        assert_eq!(created_task, expected_minimal_task);

        let task = get_resource(&app.app_address, "tasks", created_task.id).await;

        assert_eq!(task, expected_minimal_task);

        let expected_task = Box::new(Task {
            id: uuid::Uuid::new_v4(),
            source_id: "5678".to_string(),
            title: "task 2".to_string(),
            body: "more details 2".to_string(),
            status: TaskStatus::Done,
            completed_at: Some(Utc.with_ymd_and_hms(2022, 1, 3, 0, 0, 0).unwrap()),
            priority: TaskPriority::P3,
            due_at: Some(universal_inbox::task::DueDate::Date(
                NaiveDate::from_ymd_opt(2016, 9, 1).unwrap(),
            )),
            source_html_url: "https://todoist.com/showTask?id=5678".parse::<Uri>().ok(),
            tags: vec!["tag1".to_string(), "tag2".to_string()],
            parent_id: Some(expected_minimal_task.id),
            project: "project 1".to_string(),
            is_recurring: true,
            created_at: Utc.with_ymd_and_hms(2022, 1, 2, 0, 0, 0).unwrap(),
            metadata: TaskMetadata::Todoist(*todoist_item),
        });

        let created_task = create_resource(&app.app_address, "tasks", expected_task.clone()).await;

        assert_eq!(created_task, expected_task);

        let task = get_resource(&app.app_address, "tasks", created_task.id).await;

        assert_eq!(task, expected_task);
    }

    #[rstest]
    #[tokio::test]
    async fn test_create_task_as_done_with_not_completed_at_value(
        #[future] tested_app: TestedApp,
        todoist_item: Box<TodoistItem>,
    ) {
        let app = tested_app.await;
        let task_done = Box::new(Task {
            id: uuid::Uuid::new_v4(),
            source_id: "5678".to_string(),
            title: "task 2".to_string(),
            body: "more details 2".to_string(),
            status: TaskStatus::Done,
            completed_at: None,
            priority: TaskPriority::P3,
            due_at: Some(universal_inbox::task::DueDate::Date(
                NaiveDate::from_ymd_opt(2022, 1, 3).unwrap(),
            )),
            source_html_url: "https://todoist.com/showTask?id=5678".parse::<Uri>().ok(),
            tags: vec!["tag1".to_string(), "tag2".to_string()],
            parent_id: None,
            project: "project 1".to_string(),
            is_recurring: true,
            created_at: Utc.with_ymd_and_hms(2022, 1, 2, 0, 0, 0).unwrap(),
            metadata: TaskMetadata::Todoist(*todoist_item),
        });

        let response = create_resource_response(&app.app_address, "tasks", task_done.clone()).await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = response.text().await.expect("Cannot get response body");
        assert_eq!(
            body,
            json!({ "message": "Invalid input data: Submitted task is invalid" }).to_string()
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_create_task_duplicate_task(
        #[future] tested_app: TestedApp,
        todoist_item: Box<TodoistItem>,
    ) {
        let app = tested_app.await;
        let expected_task = Box::new(Task {
            id: uuid::Uuid::new_v4(),
            source_id: "1234".to_string(),
            title: "task 1".to_string(),
            body: "more details".to_string(),
            status: TaskStatus::Active,
            completed_at: None,
            priority: TaskPriority::P4,
            due_at: None,
            source_html_url: "https://todoist.com/showTask?id=1234".parse::<Uri>().ok(),
            tags: vec![],
            parent_id: None,
            project: "Inbox".to_string(),
            is_recurring: false,
            created_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            metadata: TaskMetadata::Todoist(*todoist_item.clone()),
        });

        let created_task = create_resource(&app.app_address, "tasks", expected_task.clone()).await;

        assert_eq!(created_task, expected_task);

        let response = create_resource_response(&app.app_address, "tasks", expected_task).await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = response.text().await.expect("Cannot get response body");
        assert_eq!(
            body,
            json!({ "message": format!("The entity {} already exists", created_task.id) })
                .to_string()
        );
    }
}

mod get_task {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_get_unknown_task(#[future] tested_app: TestedApp) {
        let app = tested_app.await;
        let unknown_task_id = Uuid::new_v4();

        let response = get_resource_response(&app.app_address, "tasks", unknown_task_id).await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = response.text().await.expect("Cannot get response body");
        assert_eq!(
            body,
            json!({ "message": format!("Cannot find task {}", unknown_task_id) }).to_string()
        );
    }
}

mod list_tasks {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_empty_list_tasks(#[future] tested_app: TestedApp) {
        let app = tested_app.await;
        let tasks = list_tasks(&app.app_address, TaskStatus::Active).await;

        assert_eq!(tasks.len(), 0);
    }

    #[rstest]
    #[tokio::test]
    async fn test_list_tasks(#[future] tested_app: TestedApp, todoist_item: Box<TodoistItem>) {
        let mut todoist_item_ = todoist_item.clone();
        todoist_item_.id = "43".to_string();

        let app = tested_app.await;
        let task_active = create_resource(
            &app.app_address,
            "tasks",
            Box::new(Task {
                id: uuid::Uuid::new_v4(),
                source_id: "1234".to_string(),
                title: "task 1".to_string(),
                body: "more details".to_string(),
                status: TaskStatus::Active,
                completed_at: None,
                priority: TaskPriority::P4,
                due_at: None,
                source_html_url: "https://todoist.com/showTask?id=1234".parse::<Uri>().ok(),
                tags: vec![],
                parent_id: None,
                project: "Inbox".to_string(),
                is_recurring: false,
                created_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
                metadata: TaskMetadata::Todoist(*todoist_item.clone()),
            }),
        )
        .await;

        let task_done = create_resource(
            &app.app_address,
            "tasks",
            Box::new(Task {
                id: uuid::Uuid::new_v4(),
                source_id: "5678".to_string(),
                title: "task 2".to_string(),
                body: "more details".to_string(),
                status: TaskStatus::Done,
                completed_at: Some(Utc.with_ymd_and_hms(2022, 1, 2, 0, 0, 0).unwrap()),
                priority: TaskPriority::P4,
                due_at: None,
                source_html_url: "https://todoist.com/showTask?id=5678".parse::<Uri>().ok(),
                tags: vec![],
                parent_id: None,
                project: "Inbox".to_string(),
                is_recurring: false,
                created_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
                metadata: TaskMetadata::Todoist(*todoist_item.clone()),
            }),
        )
        .await;

        let tasks = list_tasks(&app.app_address, TaskStatus::Active).await;

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0], *task_active);

        let tasks = list_tasks(&app.app_address, TaskStatus::Done).await;

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0], *task_done);
    }
}

mod patch_task {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_patch_task_status_without_modification(
        #[future] tested_app: TestedApp,
        todoist_item: Box<TodoistItem>,
    ) {
        let app = tested_app.await;

        let created_task = create_resource(
            &app.app_address,
            "tasks",
            Box::new(Task {
                id: uuid::Uuid::new_v4(),
                source_id: "1234".to_string(),
                title: "task 1".to_string(),
                body: "more details".to_string(),
                status: TaskStatus::Active,
                completed_at: None,
                priority: TaskPriority::P4,
                due_at: None,
                source_html_url: "https://todoist.com/showTask?id=1234".parse::<Uri>().ok(),
                tags: vec![],
                parent_id: None,
                project: "Inbox".to_string(),
                is_recurring: false,
                created_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
                metadata: TaskMetadata::Todoist(*todoist_item.clone()),
            }),
        )
        .await;

        let response = patch_resource_response(
            &app.app_address,
            "tasks",
            created_task.id,
            &TaskPatch {
                status: Some(created_task.status),
            },
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_task_without_values_to_update(
        #[future] tested_app: TestedApp,
        todoist_item: Box<TodoistItem>,
    ) {
        let app = tested_app.await;
        let created_task = create_resource(
            &app.app_address,
            "tasks",
            Box::new(Task {
                id: uuid::Uuid::new_v4(),
                source_id: "1234".to_string(),
                title: "task 1".to_string(),
                body: "more details".to_string(),
                status: TaskStatus::Active,
                completed_at: None,
                priority: TaskPriority::P4,
                due_at: None,
                source_html_url: "https://todoist.com/showTask?id=1234".parse::<Uri>().ok(),
                tags: vec![],
                parent_id: None,
                project: "Inbox".to_string(),
                is_recurring: false,
                created_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
                metadata: TaskMetadata::Todoist(*todoist_item.clone()),
            }),
        )
        .await;

        let response = patch_resource_response(
            &app.app_address,
            "tasks",
            created_task.id,
            &TaskPatch {
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
                        "Invalid input data: Missing `status` field value to update task {}",
                        created_task.id
                    )
            })
            .to_string()
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_unknown_task(#[future] tested_app: TestedApp) {
        let app = tested_app.await;
        let unknown_task_id = Uuid::new_v4();

        let response = patch_resource_response(
            &app.app_address,
            "tasks",
            unknown_task_id,
            &TaskPatch {
                status: Some(TaskStatus::Active),
            },
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = response.text().await.expect("Cannot get response body");
        assert_eq!(
            body,
            json!({ "message": format!("Cannot update unknown task {}", unknown_task_id) })
                .to_string()
        );
    }
}
