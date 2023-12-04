use chrono::{NaiveDate, TimeZone, Utc};
use http::StatusCode;
use rstest::*;
use serde_json::json;
use url::Url;
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::todoist::TodoistConfig,
    },
    notification::{NotificationStatus, NotificationWithTask},
    task::{
        integrations::todoist::TodoistItem, service::TaskPatch, ProjectSummary, Task, TaskMetadata,
        TaskPriority, TaskStatus,
    },
};

use universal_inbox_api::{
    configuration::Settings,
    integrations::{oauth2::NangoConnection, todoist::TodoistSyncResponse},
    universal_inbox::task::TaskCreationResult,
};

use crate::helpers::{
    auth::{authenticate_user, authenticated_app, AuthenticatedApp},
    integration_connection::{create_and_mock_integration_connection, nango_todoist_connection},
    notification::list_notifications_with_tasks,
    rest::{
        create_resource, create_resource_response, get_resource, get_resource_response,
        patch_resource_response,
    },
    settings,
    task::{
        list_tasks, search_projects,
        todoist::{
            create_task_from_todoist_item, mock_todoist_sync_resources_service,
            sync_todoist_projects_response, todoist_item,
        },
    },
    tested_app, TestedApp,
};

mod create_task {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_create_task_in_inbox(
        #[future] authenticated_app: AuthenticatedApp,
        todoist_item: Box<TodoistItem>,
    ) {
        let app = authenticated_app.await;
        let expected_minimal_task = Box::new(Task {
            id: Uuid::new_v4().into(),
            source_id: "1234".to_string(),
            title: "task 1".to_string(),
            body: "more details".to_string(),
            status: TaskStatus::Active,
            completed_at: None,
            priority: TaskPriority::P4,
            due_at: None,
            source_html_url: "https://todoist.com/showTask?id=1234".parse::<Url>().ok(),
            tags: vec![],
            parent_id: None,
            project: "Inbox".to_string(),
            is_recurring: false,
            created_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            metadata: TaskMetadata::Todoist(*todoist_item.clone()),
            user_id: app.user.id,
        });

        let creation_result: Box<TaskCreationResult> = create_resource(
            &app.client,
            &app.api_address,
            "tasks",
            expected_minimal_task.clone(),
        )
        .await;

        assert_eq!(creation_result.task, *expected_minimal_task);
        // A notification should have been created for tasks in the inbox (project)
        assert!(creation_result.notification.is_some());
        let created_notification = creation_result.notification.unwrap();
        assert_eq!(created_notification.task_id, Some(creation_result.task.id));

        let task = get_resource(
            &app.client,
            &app.api_address,
            "tasks",
            creation_result.task.id.into(),
        )
        .await;
        assert_eq!(task, expected_minimal_task);

        let result = list_notifications_with_tasks(
            &app.client,
            &app.api_address,
            vec![NotificationStatus::Unread],
            false,
            Some(creation_result.task.id),
        )
        .await;
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0],
            NotificationWithTask {
                id: created_notification.id,
                ..(*task).clone().into()
            }
        );
        assert_eq!(result[0].task, Some(*task));
    }

    #[rstest]
    #[tokio::test]
    async fn test_create_task(
        #[future] authenticated_app: AuthenticatedApp,
        todoist_item: Box<TodoistItem>,
    ) {
        let app = authenticated_app.await;
        let expected_task = Box::new(Task {
            id: Uuid::new_v4().into(),
            source_id: "5678".to_string(),
            title: "task 2".to_string(),
            body: "more details 2".to_string(),
            status: TaskStatus::Done,
            completed_at: Some(Utc.with_ymd_and_hms(2022, 1, 3, 0, 0, 0).unwrap()),
            priority: TaskPriority::P3,
            due_at: Some(universal_inbox::task::DueDate::Date(
                NaiveDate::from_ymd_opt(2016, 9, 1).unwrap(),
            )),
            source_html_url: "https://todoist.com/showTask?id=5678".parse::<Url>().ok(),
            tags: vec!["tag1".to_string(), "tag2".to_string()],
            parent_id: None,
            project: "project 1".to_string(),
            is_recurring: true,
            created_at: Utc.with_ymd_and_hms(2022, 1, 2, 0, 0, 0).unwrap(),
            metadata: TaskMetadata::Todoist(*todoist_item),
            user_id: app.user.id,
        });

        let creation_result: Box<TaskCreationResult> = create_resource(
            &app.client,
            &app.api_address,
            "tasks",
            expected_task.clone(),
        )
        .await;

        assert_eq!(creation_result.task, *expected_task);
        assert!(creation_result.notification.is_none());

        let task = get_resource(
            &app.client,
            &app.api_address,
            "tasks",
            creation_result.task.id.into(),
        )
        .await;

        assert_eq!(task, expected_task);

        let result = list_notifications_with_tasks(
            &app.client,
            &app.api_address,
            vec![NotificationStatus::Unread],
            false,
            Some(creation_result.task.id),
        )
        .await;
        assert!(result.is_empty());
    }

    #[rstest]
    #[tokio::test]
    async fn test_create_task_with_wrong_user_id(
        #[future] authenticated_app: AuthenticatedApp,
        todoist_item: Box<TodoistItem>,
    ) {
        let app = authenticated_app.await;
        let expected_task = Box::new(Task {
            id: Uuid::new_v4().into(),
            source_id: "5678".to_string(),
            title: "task 2".to_string(),
            body: "more details 2".to_string(),
            status: TaskStatus::Done,
            completed_at: Some(Utc.with_ymd_and_hms(2022, 1, 3, 0, 0, 0).unwrap()),
            priority: TaskPriority::P3,
            due_at: Some(universal_inbox::task::DueDate::Date(
                NaiveDate::from_ymd_opt(2016, 9, 1).unwrap(),
            )),
            source_html_url: "https://todoist.com/showTask?id=5678".parse::<Url>().ok(),
            tags: vec!["tag1".to_string(), "tag2".to_string()],
            parent_id: None,
            project: "project 1".to_string(),
            is_recurring: true,
            created_at: Utc.with_ymd_and_hms(2022, 1, 2, 0, 0, 0).unwrap(),
            metadata: TaskMetadata::Todoist(*todoist_item),
            user_id: Uuid::new_v4().into(),
        });

        let response = create_resource_response(
            &app.client,
            &app.api_address,
            "tasks",
            expected_task.clone(),
        )
        .await;

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[rstest]
    #[tokio::test]
    async fn test_create_task_as_done_with_not_completed_at_value(
        #[future] authenticated_app: AuthenticatedApp,
        todoist_item: Box<TodoistItem>,
    ) {
        let app = authenticated_app.await;
        let task_done = Box::new(Task {
            id: Uuid::new_v4().into(),
            source_id: "5678".to_string(),
            title: "task 2".to_string(),
            body: "more details 2".to_string(),
            status: TaskStatus::Done,
            completed_at: None,
            priority: TaskPriority::P3,
            due_at: Some(universal_inbox::task::DueDate::Date(
                NaiveDate::from_ymd_opt(2022, 1, 3).unwrap(),
            )),
            source_html_url: "https://todoist.com/showTask?id=5678".parse::<Url>().ok(),
            tags: vec!["tag1".to_string(), "tag2".to_string()],
            parent_id: None,
            project: "project 1".to_string(),
            is_recurring: true,
            created_at: Utc.with_ymd_and_hms(2022, 1, 2, 0, 0, 0).unwrap(),
            metadata: TaskMetadata::Todoist(*todoist_item),
            user_id: app.user.id,
        });

        let response =
            create_resource_response(&app.client, &app.api_address, "tasks", task_done.clone())
                .await;

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
        #[future] authenticated_app: AuthenticatedApp,
        todoist_item: Box<TodoistItem>,
    ) {
        let app = authenticated_app.await;
        let expected_task = Box::new(Task {
            id: Uuid::new_v4().into(),
            source_id: "1234".to_string(),
            title: "task 1".to_string(),
            body: "more details".to_string(),
            status: TaskStatus::Active,
            completed_at: None,
            priority: TaskPriority::P4,
            due_at: None,
            source_html_url: "https://todoist.com/showTask?id=1234".parse::<Url>().ok(),
            tags: vec![],
            parent_id: None,
            project: "Inbox".to_string(),
            is_recurring: false,
            created_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            metadata: TaskMetadata::Todoist(*todoist_item.clone()),
            user_id: app.user.id,
        });

        let creation_result: Box<TaskCreationResult> = create_resource(
            &app.client,
            &app.api_address,
            "tasks",
            expected_task.clone(),
        )
        .await;

        assert_eq!(creation_result.task, *expected_task);

        let response =
            create_resource_response(&app.client, &app.api_address, "tasks", expected_task).await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = response.text().await.expect("Cannot get response body");
        assert_eq!(
            body,
            json!({ "message": format!("The entity {} already exists", creation_result.task.id) })
                .to_string()
        );
    }
}

mod get_task {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_get_unknown_task(#[future] authenticated_app: AuthenticatedApp) {
        let app = authenticated_app.await;
        let unknown_task_id = Uuid::new_v4();

        let response =
            get_resource_response(&app.client, &app.api_address, "tasks", unknown_task_id).await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = response.text().await.expect("Cannot get response body");
        assert_eq!(
            body,
            json!({ "message": format!("Cannot find task {unknown_task_id}") }).to_string()
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_get_task_of_another_user(
        #[future] tested_app: TestedApp,
        #[future] authenticated_app: AuthenticatedApp,
        todoist_item: Box<TodoistItem>,
    ) {
        let app = authenticated_app.await;

        let creation_result = create_task_from_todoist_item(
            &app.client,
            &app.api_address,
            &todoist_item,
            "Inbox".to_string(),
            app.user.id,
        )
        .await;
        let task_id = creation_result.task.id.0;

        let (client, _user) =
            authenticate_user(&tested_app.await, "5678", "Jane", "Doe", "jane@example.com").await;
        let response = get_resource_response(&client, &app.api_address, "tasks", task_id).await;

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let body = response.text().await.expect("Cannot get response body");
        assert_eq!(
            body,
            json!({
                "message":
                    format!("Forbidden access: Only the owner of the task {task_id} can access it")
            })
            .to_string()
        );
    }
}

mod list_tasks {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_empty_list_tasks(#[future] authenticated_app: AuthenticatedApp) {
        let app = authenticated_app.await;
        let tasks = list_tasks(&app.client, &app.api_address, TaskStatus::Active).await;

        assert!(tasks.is_empty());
    }

    #[rstest]
    #[tokio::test]
    async fn test_list_tasks(
        #[future] tested_app: TestedApp,
        #[future] authenticated_app: AuthenticatedApp,
        todoist_item: Box<TodoistItem>,
    ) {
        let app = authenticated_app.await;
        let task_active = create_task_from_todoist_item(
            &app.client,
            &app.api_address,
            &todoist_item,
            "Inbox".to_string(),
            app.user.id,
        )
        .await;
        assert_eq!(task_active.task.status, TaskStatus::Active);

        let mut todoist_item_done = todoist_item.clone();
        todoist_item_done.id = "5678".to_string();
        todoist_item_done.checked = true;
        todoist_item_done.completed_at = Some(Utc.with_ymd_and_hms(2022, 1, 2, 0, 0, 0).unwrap());

        let task_done = create_task_from_todoist_item(
            &app.client,
            &app.api_address,
            &todoist_item_done,
            "Inbox".to_string(),
            app.user.id,
        )
        .await;
        assert_eq!(task_done.task.status, TaskStatus::Done);

        let tasks = list_tasks(&app.client, &app.api_address, TaskStatus::Active).await;

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0], task_active.task);

        let tasks = list_tasks(&app.client, &app.api_address, TaskStatus::Done).await;

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0], task_done.task);

        // Test listing tasks of another user
        let (client, _user) =
            authenticate_user(&tested_app.await, "5678", "Jane", "Doe", "jane@example.com").await;

        let result = list_tasks(&client, &app.api_address, TaskStatus::Done).await;

        assert_eq!(result.len(), 0);
    }
}

mod patch_task {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_patch_task_status_without_modification(
        #[future] authenticated_app: AuthenticatedApp,
        todoist_item: Box<TodoistItem>,
    ) {
        let app = authenticated_app.await;

        let creation_result = create_task_from_todoist_item(
            &app.client,
            &app.api_address,
            &todoist_item,
            "Inbox".to_string(),
            app.user.id,
        )
        .await;

        let response = patch_resource_response(
            &app.client,
            &app.api_address,
            "tasks",
            creation_result.task.id.into(),
            &TaskPatch {
                status: Some(creation_result.task.status),
                ..Default::default()
            },
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_task_of_another_user(
        #[future] tested_app: TestedApp,
        #[future] authenticated_app: AuthenticatedApp,
        todoist_item: Box<TodoistItem>,
    ) {
        let app = authenticated_app.await;

        let creation_result = create_task_from_todoist_item(
            &app.client,
            &app.api_address,
            &todoist_item,
            "Inbox".to_string(),
            app.user.id,
        )
        .await;
        let (client, _user) =
            authenticate_user(&tested_app.await, "5678", "Jane", "Doe", "jane@example.com").await;

        let response = patch_resource_response(
            &client,
            &app.api_address,
            "tasks",
            creation_result.task.id.into(),
            &TaskPatch {
                status: Some(TaskStatus::Done),
                ..Default::default()
            },
        )
        .await;

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_task_without_values_to_update(
        #[future] authenticated_app: AuthenticatedApp,
        todoist_item: Box<TodoistItem>,
    ) {
        let app = authenticated_app.await;
        let creation_result = create_task_from_todoist_item(
            &app.client,
            &app.api_address,
            &todoist_item,
            "Inbox".to_string(),
            app.user.id,
        )
        .await;

        let response = patch_resource_response(
            &app.client,
            &app.api_address,
            "tasks",
            creation_result.task.id.into(),
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
                        creation_result.task.id
                    )
            })
            .to_string()
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_unknown_task(#[future] authenticated_app: AuthenticatedApp) {
        let app = authenticated_app.await;
        let unknown_task_id = Uuid::new_v4();

        let response = patch_resource_response(
            &app.client,
            &app.api_address,
            "tasks",
            unknown_task_id,
            &TaskPatch {
                status: Some(TaskStatus::Active),
                ..Default::default()
            },
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = response.text().await.expect("Cannot get response body");
        assert_eq!(
            body,
            json!({ "message": format!("Cannot update unknown task {unknown_task_id}") })
                .to_string()
        );
    }
}

mod search_tasks {
    use crate::helpers::task::search_tasks;

    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_empty_search_tasks(#[future] authenticated_app: AuthenticatedApp) {
        let app = authenticated_app.await;
        let tasks = search_tasks(&app.client, &app.api_address, "").await;

        assert!(tasks.is_empty());
    }

    #[rstest]
    #[tokio::test]
    async fn test_search_tasks(
        #[future] tested_app: TestedApp,
        #[future] authenticated_app: AuthenticatedApp,
        todoist_item: Box<TodoistItem>,
    ) {
        let app = authenticated_app.await;
        let task1 = create_task_from_todoist_item(
            &app.client,
            &app.api_address,
            &todoist_item,
            "Inbox".to_string(),
            app.user.id,
        )
        .await;
        assert_eq!(task1.task.title, "Task 1".to_string());

        let mut other_todoist_item = todoist_item.clone();
        other_todoist_item.id = "5678".to_string();
        other_todoist_item.content = "Other todo".to_string();
        other_todoist_item.description = "fill the form".to_string();

        let task2 = create_task_from_todoist_item(
            &app.client,
            &app.api_address,
            &other_todoist_item,
            "Inbox".to_string(),
            app.user.id,
        )
        .await;
        assert_eq!(task2.task.title, "Other todo".to_string());

        // Search by task title
        let tasks = search_tasks(&app.client, &app.api_address, "Task").await;

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, task1.task.id);

        let tasks = search_tasks(&app.client, &app.api_address, "todo").await;

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, task2.task.id);

        // Search by task description
        let tasks = search_tasks(&app.client, &app.api_address, "form").await;

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, task2.task.id);

        // Search by task tags
        let tasks = search_tasks(&app.client, &app.api_address, "Food").await;

        assert_eq!(tasks.len(), 2);
        assert!(tasks.iter().any(|t| t.id == task1.task.id));
        assert!(tasks.iter().any(|t| t.id == task2.task.id));

        // Test searching tasks of another user
        let (client, _user) =
            authenticate_user(&tested_app.await, "5678", "Jane", "Doe", "jane@example.com").await;

        let result = search_tasks(&client, &app.api_address, "Task").await;

        assert_eq!(result.len(), 0);
    }
}

mod search_projects {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_search_projects(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        sync_todoist_projects_response: TodoistSyncResponse,
        nango_todoist_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;
        create_and_mock_integration_connection(
            &app,
            &settings.integrations.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
            &settings,
            nango_todoist_connection,
        )
        .await;
        let todoist_projects_mock = mock_todoist_sync_resources_service(
            &app.todoist_mock_server,
            "projects",
            &sync_todoist_projects_response,
            None,
        );

        let projects = search_projects(&app.client, &app.api_address, "in").await;

        assert_eq!(projects.len(), 1);
        assert_eq!(
            projects[0],
            ProjectSummary {
                source_id: "1111".to_string(),
                name: "Inbox".to_string()
            }
        );
        let projects = search_projects(&app.client, &app.api_address, "box").await;

        assert_eq!(projects.len(), 1);
        assert_eq!(
            projects[0],
            ProjectSummary {
                source_id: "1111".to_string(),
                name: "Inbox".to_string()
            }
        );

        let projects = search_projects(&app.client, &app.api_address, "jec").await;

        assert_eq!(projects.len(), 1);
        assert_eq!(
            projects[0],
            ProjectSummary {
                source_id: "2222".to_string(),
                name: "Project2".to_string()
            }
        );

        todoist_projects_mock.assert();
        // Todoist API calls should have been cached
        todoist_projects_mock.assert_hits(1);
    }
}
