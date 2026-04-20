use chrono::{TimeZone, Timelike, Utc};
use http::StatusCode;
use rstest::*;
use serde_json::json;
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::todoist::TodoistConfig,
    },
    task::{ProjectSummary, TaskStatus, service::TaskPatch},
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
    auth::{AuthenticatedApp, authenticate_user, authenticated_app},
    integration_connection::{create_and_mock_integration_connection, nango_todoist_connection},
    rest::{create_resource, get_resource_response, patch_resource_response},
    settings,
    task::{
        list_tasks, search_projects,
        ticktick::{ticktick_item, ticktick_projects_response},
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
            None,
        )
        .await;
        mock_todoist_sync_resources_service(
            &app.app.todoist_mock_server,
            "projects",
            &sync_todoist_projects_response,
            None,
        )
        .await;

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
                source_item: None,
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
            None,
        )
        .await;
        mock_todoist_sync_resources_service(
            &app.app.todoist_mock_server,
            "projects",
            &sync_todoist_projects_response,
            None,
        )
        .await;

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
                source_item: None,
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
                source_item: None,
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
            None,
        )
        .await;
        mock_todoist_sync_resources_service(
            &app.app.todoist_mock_server,
            "projects",
            &sync_todoist_projects_response,
            None,
        )
        .await;

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
                source_item: None,
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
            None,
        )
        .await;
        mock_todoist_sync_resources_service(
            &app.app.todoist_mock_server,
            "projects",
            &sync_todoist_projects_response,
            None,
        )
        .await;

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
                source_item: None,
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
            None,
        )
        .await;
        mock_todoist_sync_resources_service(
            &app.app.todoist_mock_server,
            "projects",
            &sync_todoist_projects_response,
            None,
        )
        .await;

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
                source_item: None,
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
        let tasks = search_tasks(&app.client, &app.app.api_address, "", None).await;

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
            None,
        )
        .await;
        mock_todoist_sync_resources_service(
            &app.app.todoist_mock_server,
            "projects",
            &sync_todoist_projects_response,
            None,
        )
        .await;

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
                source_item: None,
            }),
        )
        .await;
        let task1 = creation.task.as_ref().unwrap().clone();
        assert_eq!(
            task1.title,
            "Release new version of Universal Inbox".to_string()
        );

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
                source_item: None,
            }),
        )
        .await;
        let task2 = creation.task.as_ref().unwrap().clone();
        assert_eq!(task2.title, "Other todo".to_string());

        // Search by task title
        let tasks = search_tasks(
            &app.client,
            &app.app.api_address,
            "Release new version",
            None,
        )
        .await;

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, task1.id);

        let tasks = search_tasks(&app.client, &app.app.api_address, "todo", None).await;

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, task2.id);

        // Search by task description
        let tasks = search_tasks(&app.client, &app.app.api_address, "form", None).await;

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, task2.id);

        // Search by task tags
        let tasks = search_tasks(&app.client, &app.app.api_address, "Food", None).await;

        assert_eq!(tasks.len(), 2);
        assert!(tasks.iter().any(|t| t.id == task1.id));
        assert!(tasks.iter().any(|t| t.id == task2.id));

        // Test searching tasks of another user
        let (client, _user) =
            authenticate_user(&app.app, "5678", "Jane", "Doe", "jane@example.com").await;

        let result = search_tasks(&client, &app.app.api_address, "Task", None).await;

        assert_eq!(result.len(), 0);
    }

    #[rstest]
    #[tokio::test]
    async fn test_search_tasks_filtered_by_provider_kind(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        todoist_item: Box<TodoistItem>,
        ticktick_item: Box<universal_inbox::third_party::integrations::ticktick::TickTickItem>,
        ticktick_projects_response: Vec<
            universal_inbox::task::integrations::ticktick::TickTickProject,
        >,
        sync_todoist_projects_response: TodoistSyncResponse,
        nango_todoist_connection: Box<NangoConnection>,
    ) {
        use universal_inbox::integration_connection::{
            integrations::ticktick::TickTickConfig, provider::IntegrationProviderKind,
        };
        use universal_inbox::third_party::integrations::ticktick::TickTickItem;

        use crate::helpers::task::ticktick::mock_ticktick_list_projects_service;

        let app = authenticated_app.await;

        // Todoist connection (via Nango) — task seeded with the fixture title "Release new version..."
        let todoist_ic = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
            &settings,
            nango_todoist_connection,
            None,
            None,
        )
        .await;
        mock_todoist_sync_resources_service(
            &app.app.todoist_mock_server,
            "projects",
            &sync_todoist_projects_response,
            None,
        )
        .await;

        // TickTick connection (internal OAuth helper) — task sharing the word "Universal" in its title
        let ticktick_ic =
            crate::helpers::integration_connection::create_ticktick_integration_connection(
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

        let _todoist_created: Box<ThirdPartyItemCreationResult> = create_resource(
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
                    project_id: "1111".to_string(),
                    added_at: Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
                    ..*todoist_item.clone()
                })),
                integration_connection_id: todoist_ic.id,
                source_item: None,
            }),
        )
        .await;

        // TickTick task has a distinct title so we can assert which provider each match came from,
        // even though the word "Universal" appears in both.
        let _ticktick_created: Box<ThirdPartyItemCreationResult> = create_resource(
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
                    title: "Universal ticktick task".to_string(),
                    project_id: "tt_proj_1111".to_string(),
                    ..*ticktick_item.clone()
                })),
                integration_connection_id: ticktick_ic.id,
                source_item: None,
            }),
        )
        .await;

        // No filter → both providers are searchable.
        let tasks = search_tasks(&app.client, &app.app.api_address, "Universal", None).await;
        let titles: Vec<String> = tasks.iter().map(|t| t.title.clone()).collect();
        assert_eq!(
            tasks.len(),
            2,
            "expected both Todoist + TickTick tasks, got: {titles:?}"
        );

        // Filter to Todoist only.
        let tasks = search_tasks(
            &app.client,
            &app.app.api_address,
            "Universal",
            Some(IntegrationProviderKind::Todoist),
        )
        .await;
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Release new version of Universal Inbox");

        // Filter to TickTick only.
        let tasks = search_tasks(
            &app.client,
            &app.app.api_address,
            "Universal",
            Some(IntegrationProviderKind::TickTick),
        )
        .await;
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Universal ticktick task");
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
            None,
        )
        .await;
        let _todoist_projects_mock = mock_todoist_sync_resources_service(
            &app.app.todoist_mock_server,
            "projects",
            &sync_todoist_projects_response,
            None,
        )
        .await;

        let projects = search_projects(&app.client, &app.app.api_address, "in", None).await;

        assert_eq!(projects.len(), 1);
        assert_eq!(
            projects[0],
            ProjectSummary {
                source_id: "1111".into(),
                name: "Inbox".to_string()
            }
        );
        let projects = search_projects(&app.client, &app.app.api_address, "box", None).await;

        assert_eq!(projects.len(), 1);
        assert_eq!(
            projects[0],
            ProjectSummary {
                source_id: "1111".into(),
                name: "Inbox".to_string()
            }
        );

        let projects = search_projects(&app.client, &app.app.api_address, "jec", None).await;

        assert_eq!(projects.len(), 1);
        assert_eq!(
            projects[0],
            ProjectSummary {
                source_id: "2222".into(),
                name: "Project2".to_string()
            }
        );

        // Todoist API calls should have been cached
    }
}
