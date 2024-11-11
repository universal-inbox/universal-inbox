use chrono::{TimeZone, Timelike, Utc};
use http::StatusCode;
use rstest::*;
use serde_json::json;
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::todoist::TodoistConfig,
    },
    task::{service::TaskPatch, ProjectSummary, TaskStatus},
    third_party::{
        integrations::todoist::TodoistItem,
        item::{ThirdPartyItem, ThirdPartyItemCreationResult, ThirdPartyItemData},
    },
};

use universal_inbox_api::{
    configuration::Settings,
    integrations::{oauth2::NangoConnection, todoist::TodoistSyncResponse},
};

use crate::helpers::{
    auth::{authenticate_user, authenticated_app, AuthenticatedApp},
    integration_connection::{create_and_mock_integration_connection, nango_todoist_connection},
    rest::{create_resource, get_resource_response, patch_resource_response},
    settings,
    task::{
        list_tasks, search_projects,
        todoist::{
            mock_todoist_sync_resources_service, sync_todoist_projects_response, todoist_item,
        },
    },
};

mod get_task {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_get_unknown_task(#[future] authenticated_app: AuthenticatedApp) {
        let app = authenticated_app.await;
        let unknown_task_id = Uuid::new_v4();

        let response =
            get_resource_response(&app.client, &app.app.api_address, "tasks", unknown_task_id)
                .await;

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
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        todoist_item: Box<TodoistItem>,
        sync_todoist_projects_response: TodoistSyncResponse,
        nango_todoist_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;
        let integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
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
            "third_party/task/items",
            Box::new(ThirdPartyItem {
                id: Uuid::new_v4().into(),
                source_id: todoist_item.id.clone(),
                created_at: Utc::now().with_nanosecond(0).unwrap(),
                updated_at: Utc::now().with_nanosecond(0).unwrap(),
                user_id: app.user.id,
                data: ThirdPartyItemData::TodoistItem(Box::new(TodoistItem {
                    project_id: "1111".to_string(), // ie. "Inbox"
                    added_at: Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
                    ..*todoist_item.clone()
                })),
                integration_connection_id: integration_connection.id,
            }),
        )
        .await;
        let task_id = creation.task.as_ref().unwrap().id;

        let (client, _user) =
            authenticate_user(&app.app, "5678", "Jane", "Doe", "jane@example.com").await;
        let response =
            get_resource_response(&client, &app.app.api_address, "tasks", task_id.into()).await;

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
        let tasks = list_tasks(&app.client, &app.app.api_address, TaskStatus::Active, false).await;

        assert!(tasks.is_empty());
    }

    #[rstest]
    #[tokio::test]
    async fn test_list_tasks(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        todoist_item: Box<TodoistItem>,
        sync_todoist_projects_response: TodoistSyncResponse,
        nango_todoist_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;
        let integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
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
            "third_party/task/items",
            Box::new(ThirdPartyItem {
                id: Uuid::new_v4().into(),
                source_id: todoist_item.id.clone(),
                created_at: Utc::now().with_nanosecond(0).unwrap(),
                updated_at: Utc::now().with_nanosecond(0).unwrap(),
                user_id: app.user.id,
                data: ThirdPartyItemData::TodoistItem(Box::new(TodoistItem {
                    project_id: "1111".to_string(), // ie. "Inbox"
                    added_at: Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
                    ..*todoist_item.clone()
                })),
                integration_connection_id: integration_connection.id,
            }),
        )
        .await;
        let task_active = creation.task.as_ref().unwrap().clone();
        assert_eq!(task_active.status, TaskStatus::Active);

        let mut todoist_item_done = todoist_item.clone();
        todoist_item_done.id = "5678".to_string();
        todoist_item_done.checked = true;
        todoist_item_done.completed_at = Some(Utc.with_ymd_and_hms(2022, 1, 2, 0, 0, 0).unwrap());

        let creation: Box<ThirdPartyItemCreationResult> = create_resource(
            &app.client,
            &app.app.api_address,
            "third_party/task/items",
            Box::new(ThirdPartyItem {
                id: Uuid::new_v4().into(),
                source_id: todoist_item_done.id.clone(),
                created_at: Utc::now().with_nanosecond(0).unwrap(),
                updated_at: Utc::now().with_nanosecond(0).unwrap(),
                user_id: app.user.id,
                data: ThirdPartyItemData::TodoistItem(Box::new(TodoistItem {
                    project_id: "1111".to_string(), // ie. "Inbox"
                    added_at: Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
                    ..*todoist_item_done.clone()
                })),
                integration_connection_id: integration_connection.id,
            }),
        )
        .await;
        let task_done = creation.task.as_ref().unwrap().clone();
        assert_eq!(task_done.status, TaskStatus::Done);

        let tasks = list_tasks(&app.client, &app.app.api_address, TaskStatus::Active, false).await;

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0], task_active);

        let tasks = list_tasks(&app.client, &app.app.api_address, TaskStatus::Done, false).await;

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0], task_done);

        // Test listing tasks of another user
        let (client, _user) =
            authenticate_user(&app.app, "5678", "Jane", "Doe", "jane@example.com").await;

        let result = list_tasks(&client, &app.app.api_address, TaskStatus::Done, false).await;

        assert_eq!(result.len(), 0);
    }
}

mod patch_task {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_patch_task_status_without_modification(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        todoist_item: Box<TodoistItem>,
        sync_todoist_projects_response: TodoistSyncResponse,
        nango_todoist_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;
        let integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
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
            "third_party/task/items",
            Box::new(ThirdPartyItem {
                id: Uuid::new_v4().into(),
                source_id: todoist_item.id.clone(),
                created_at: Utc::now().with_nanosecond(0).unwrap(),
                updated_at: Utc::now().with_nanosecond(0).unwrap(),
                user_id: app.user.id,
                data: ThirdPartyItemData::TodoistItem(Box::new(TodoistItem {
                    project_id: "1111".to_string(), // ie. "Inbox"
                    added_at: Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
                    ..*todoist_item.clone()
                })),
                integration_connection_id: integration_connection.id,
            }),
        )
        .await;
        let task = creation.task.as_ref().unwrap().clone();

        let response = patch_resource_response(
            &app.client,
            &app.app.api_address,
            "tasks",
            task.id.into(),
            &TaskPatch {
                status: Some(task.status),
                ..Default::default()
            },
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_task_of_another_user(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        todoist_item: Box<TodoistItem>,
        sync_todoist_projects_response: TodoistSyncResponse,
        nango_todoist_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;
        let integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
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
            "third_party/task/items",
            Box::new(ThirdPartyItem {
                id: Uuid::new_v4().into(),
                source_id: todoist_item.id.clone(),
                created_at: Utc::now().with_nanosecond(0).unwrap(),
                updated_at: Utc::now().with_nanosecond(0).unwrap(),
                user_id: app.user.id,
                data: ThirdPartyItemData::TodoistItem(Box::new(TodoistItem {
                    project_id: "1111".to_string(), // ie. "Inbox"
                    added_at: Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
                    ..*todoist_item.clone()
                })),
                integration_connection_id: integration_connection.id,
            }),
        )
        .await;
        let task = creation.task.as_ref().unwrap().clone();

        let (client, _user) =
            authenticate_user(&app.app, "5678", "Jane", "Doe", "jane@example.com").await;

        let response = patch_resource_response(
            &client,
            &app.app.api_address,
            "tasks",
            task.id.into(),
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
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        todoist_item: Box<TodoistItem>,
        sync_todoist_projects_response: TodoistSyncResponse,
        nango_todoist_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;
        let integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
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
            "third_party/task/items",
            Box::new(ThirdPartyItem {
                id: Uuid::new_v4().into(),
                source_id: todoist_item.id.clone(),
                created_at: Utc::now().with_nanosecond(0).unwrap(),
                updated_at: Utc::now().with_nanosecond(0).unwrap(),
                user_id: app.user.id,
                data: ThirdPartyItemData::TodoistItem(Box::new(TodoistItem {
                    project_id: "1111".to_string(), // ie. "Inbox"
                    added_at: Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
                    ..*todoist_item.clone()
                })),
                integration_connection_id: integration_connection.id,
            }),
        )
        .await;
        let task = creation.task.as_ref().unwrap().clone();

        let response = patch_resource_response(
            &app.client,
            &app.app.api_address,
            "tasks",
            task.id.into(),
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
                        task.id
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
            &app.app.api_address,
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
        let tasks = search_tasks(&app.client, &app.app.api_address, "").await;

        assert!(tasks.is_empty());
    }

    #[rstest]
    #[tokio::test]
    async fn test_search_tasks(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        todoist_item: Box<TodoistItem>,
        sync_todoist_projects_response: TodoistSyncResponse,
        nango_todoist_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;
        let integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
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
            "third_party/task/items",
            Box::new(ThirdPartyItem {
                id: Uuid::new_v4().into(),
                source_id: todoist_item.id.clone(),
                created_at: Utc::now().with_nanosecond(0).unwrap(),
                updated_at: Utc::now().with_nanosecond(0).unwrap(),
                user_id: app.user.id,
                data: ThirdPartyItemData::TodoistItem(Box::new(TodoistItem {
                    project_id: "1111".to_string(), // ie. "Inbox"
                    added_at: Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
                    ..*todoist_item.clone()
                })),
                integration_connection_id: integration_connection.id,
            }),
        )
        .await;
        let task1 = creation.task.as_ref().unwrap().clone();
        assert_eq!(task1.title, "Task 1".to_string());

        let mut other_todoist_item = todoist_item.clone();
        other_todoist_item.id = "5678".to_string();
        other_todoist_item.content = "Other todo".to_string();
        other_todoist_item.description = "fill the form".to_string();

        let creation: Box<ThirdPartyItemCreationResult> = create_resource(
            &app.client,
            &app.app.api_address,
            "third_party/task/items",
            Box::new(ThirdPartyItem {
                id: Uuid::new_v4().into(),
                source_id: other_todoist_item.id.clone(),
                created_at: Utc::now().with_nanosecond(0).unwrap(),
                updated_at: Utc::now().with_nanosecond(0).unwrap(),
                user_id: app.user.id,
                data: ThirdPartyItemData::TodoistItem(Box::new(TodoistItem {
                    project_id: "1111".to_string(), // ie. "Inbox"
                    added_at: Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
                    ..*other_todoist_item.clone()
                })),
                integration_connection_id: integration_connection.id,
            }),
        )
        .await;
        let task2 = creation.task.as_ref().unwrap().clone();
        assert_eq!(task2.title, "Other todo".to_string());

        // Search by task title
        let tasks = search_tasks(&app.client, &app.app.api_address, "Task").await;

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, task1.id);

        let tasks = search_tasks(&app.client, &app.app.api_address, "todo").await;

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, task2.id);

        // Search by task description
        let tasks = search_tasks(&app.client, &app.app.api_address, "form").await;

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, task2.id);

        // Search by task tags
        let tasks = search_tasks(&app.client, &app.app.api_address, "Food").await;

        assert_eq!(tasks.len(), 2);
        assert!(tasks.iter().any(|t| t.id == task1.id));
        assert!(tasks.iter().any(|t| t.id == task2.id));

        // Test searching tasks of another user
        let (client, _user) =
            authenticate_user(&app.app, "5678", "Jane", "Doe", "jane@example.com").await;

        let result = search_tasks(&client, &app.app.api_address, "Task").await;

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
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
            &settings,
            nango_todoist_connection,
            None,
        )
        .await;
        let todoist_projects_mock = mock_todoist_sync_resources_service(
            &app.app.todoist_mock_server,
            "projects",
            &sync_todoist_projects_response,
            None,
        );

        let projects = search_projects(&app.client, &app.app.api_address, "in").await;

        assert_eq!(projects.len(), 1);
        assert_eq!(
            projects[0],
            ProjectSummary {
                source_id: "1111".to_string(),
                name: "Inbox".to_string()
            }
        );
        let projects = search_projects(&app.client, &app.app.api_address, "box").await;

        assert_eq!(projects.len(), 1);
        assert_eq!(
            projects[0],
            ProjectSummary {
                source_id: "1111".to_string(),
                name: "Inbox".to_string()
            }
        );

        let projects = search_projects(&app.client, &app.app.api_address, "jec").await;

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
