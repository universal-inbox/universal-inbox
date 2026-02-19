use chrono::{TimeZone, Utc};
use http::StatusCode;
use pretty_assertions::assert_eq;
use rstest::*;
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        IntegrationConnectionStatus,
        config::IntegrationConnectionConfig,
        integrations::ticktick::TickTickConfig,
        provider::{IntegrationProvider, IntegrationProviderKind},
    },
    notification::{Notification, NotificationStatus},
    task::{Task, TaskCreationResult, TaskSourceKind},
    third_party::{
        integrations::ticktick::TickTickItem,
        item::{ThirdPartyItem, ThirdPartyItemCreationResult, ThirdPartyItemData},
    },
};
use universal_inbox_api::{configuration::Settings, integrations::oauth2::NangoConnection};

use universal_inbox::task::integrations::ticktick::TickTickProject;

use crate::helpers::{
    auth::{AuthenticatedApp, authenticated_app},
    integration_connection::{
        create_and_mock_integration_connection, create_integration_connection,
        get_integration_connection, get_integration_connection_per_provider,
        nango_ticktick_connection,
    },
    notification::list_notifications_with_tasks,
    rest::{create_resource, get_resource},
    settings,
    task::{
        sync_tasks, sync_tasks_response,
        ticktick::{
            assert_sync_ticktick_items, mock_ticktick_list_projects_service,
            mock_ticktick_list_tasks_service, ticktick_projects_response, ticktick_tasks_response,
        },
    },
};
use wiremock::{Mock, ResponseTemplate};

#[rstest]
#[tokio::test]
async fn test_sync_ticktick_tasks_should_add_new_task_and_update_existing_one(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    ticktick_tasks_response: Vec<TickTickItem>,
    ticktick_projects_response: Vec<TickTickProject>,
    nango_ticktick_connection: Box<NangoConnection>,
) {
    // existing task will be updated
    // a new task will be created
    // Inbox tasks get a notification; non-Inbox tasks get a deleted notification
    let app = authenticated_app.await;
    let integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::TickTick(TickTickConfig::enabled()),
        &settings,
        nango_ticktick_connection,
        None,
        None,
    )
    .await;

    // Mock TickTick list projects (called during sync AND during third_party_item_into_task)
    mock_ticktick_list_projects_service(&app.app.ticktick_mock_server, &ticktick_projects_response)
        .await;

    let ticktick_items = ticktick_tasks_response.clone();
    // Create an existing third-party item for the second task (tt_task_1456)
    // so that sync will UPDATE it instead of creating it fresh
    let existing_ticktick_third_party_item_creation: Box<ThirdPartyItemCreationResult> =
        create_resource(
            &app.client,
            &app.app.api_address,
            "third_party/task/items",
            Box::new(ThirdPartyItem {
                id: Uuid::new_v4().into(),
                source_id: ticktick_items[1].id.clone(),
                created_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
                updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
                user_id: app.user.id,
                data: ThirdPartyItemData::TickTickItem(Box::new(TickTickItem {
                    title: "old task 1".to_string(),
                    content: Some("more details".to_string()),
                    priority: universal_inbox::third_party::integrations::ticktick::TickTickItemPriority::High,
                    tags: Some(vec!["tag1".to_string()]),
                    project_id: "tt_proj_1111".to_string(), // ie. "Inbox"
                    ..ticktick_items[1].clone()
                })),
                integration_connection_id: integration_connection.id,
                source_item: None,
            }),
        )
        .await;
    let existing_ticktick_task = existing_ticktick_third_party_item_creation
        .task
        .as_ref()
        .unwrap();
    let existing_ticktick_notification = existing_ticktick_third_party_item_creation
        .notification
        .as_ref()
        .unwrap();

    // Mock TickTick list tasks for each project
    // tt_proj_1111 (Inbox) has task tt_task_1123
    let inbox_tasks: Vec<TickTickItem> = ticktick_items
        .iter()
        .filter(|t| t.project_id == "tt_proj_1111")
        .cloned()
        .collect();
    mock_ticktick_list_tasks_service(&app.app.ticktick_mock_server, "tt_proj_1111", &inbox_tasks)
        .await;
    // tt_proj_2222 (Project2) has task tt_task_1456
    let project2_tasks: Vec<TickTickItem> = ticktick_items
        .iter()
        .filter(|t| t.project_id == "tt_proj_2222")
        .cloned()
        .collect();
    mock_ticktick_list_tasks_service(
        &app.app.ticktick_mock_server,
        "tt_proj_2222",
        &project2_tasks,
    )
    .await;

    let task_creations: Vec<TaskCreationResult> = sync_tasks(
        &app.client,
        &app.app.api_address,
        Some(TaskSourceKind::TickTick),
        false,
    )
    .await;

    assert_eq!(task_creations.len(), ticktick_items.len());
    assert_sync_ticktick_items(&task_creations, &ticktick_items, app.user.id);

    let updated_ticktick_task: Box<Task> = get_resource(
        &app.client,
        &app.app.api_address,
        "tasks",
        existing_ticktick_task.id.into(),
    )
    .await;
    assert_eq!(updated_ticktick_task.id, existing_ticktick_task.id);
    assert_eq!(
        updated_ticktick_task.source_item.source_id,
        existing_ticktick_task.source_item.source_id
    );
    // Updated fields
    assert_eq!(updated_ticktick_task.title, "Task 2");
    assert_eq!(updated_ticktick_task.body, "");
    assert_eq!(updated_ticktick_task.tags.is_empty(), true);
    assert_eq!(updated_ticktick_task.project, "Project2".to_string());
    assert_eq!(
        updated_ticktick_task.source_item.data,
        ThirdPartyItemData::TickTickItem(Box::new(ticktick_items[1].clone()))
    );

    let updated_ticktick_notification: Box<Notification> = get_resource(
        &app.client,
        &app.app.api_address,
        "notifications",
        existing_ticktick_notification.id.into(),
    )
    .await;
    assert_eq!(
        updated_ticktick_notification.id,
        existing_ticktick_notification.id
    );
    assert_eq!(
        updated_ticktick_notification.source_item.source_id,
        existing_ticktick_notification.source_item.source_id
    );
    assert_eq!(
        updated_ticktick_notification.task_id,
        Some(updated_ticktick_task.id)
    );
    // The existing notification will be marked as deleted because the task moved out of Inbox
    assert_eq!(
        updated_ticktick_notification.status,
        NotificationStatus::Deleted
    );
    assert_eq!(updated_ticktick_notification.title, "old task 1");

    // Newly created task (tt_task_1123) is in the Inbox, thus a notification should be created
    let new_ticktick_task_creation = task_creations
        .iter()
        .find(|task_creation| task_creation.task.source_item.source_id == ticktick_items[0].id)
        .unwrap();
    let new_task = &new_ticktick_task_creation.task;
    let notifications = list_notifications_with_tasks(
        &app.client,
        &app.app.api_address,
        vec![NotificationStatus::Unread],
        false,
        Some(new_task.id),
        None,
        false,
    )
    .await;

    assert_eq!(notifications.len(), 1);
    assert_eq!(
        notifications[0].source_item.source_id,
        new_task.source_item.source_id
    );
    assert_eq!(notifications[0].task, Some(new_task.clone()));
    assert_eq!(
        Into::<Notification>::into(notifications[0].clone()),
        new_ticktick_task_creation.notifications[0]
    );

    // Verify integration connection context was updated with last_sync_at
    let updated_integration_connection =
        get_integration_connection(&app, integration_connection.id)
            .await
            .unwrap();
    match &updated_integration_connection.provider {
        IntegrationProvider::TickTick { context, config } => {
            assert!(context.is_some());
            assert!(context.as_ref().unwrap().last_sync_at.is_some());
            assert_eq!(*config, TickTickConfig::enabled());
        }
        _ => panic!("Expected TickTick provider"),
    }

    let integration_connection = get_integration_connection_per_provider(
        &app,
        app.user.id,
        IntegrationProviderKind::TickTick,
        None,
        None,
    )
    .await
    .unwrap();
    assert!(integration_connection.last_tasks_sync_started_at.is_some());
    assert!(
        integration_connection
            .last_tasks_sync_completed_at
            .is_some()
    );
    assert!(integration_connection.last_tasks_sync_failed_at.is_none());
    assert!(
        integration_connection
            .last_tasks_sync_failure_message
            .is_none()
    );
    assert_eq!(integration_connection.tasks_sync_failures, 0);
    assert_eq!(
        integration_connection.status,
        IntegrationConnectionStatus::Validated
    );
    assert!(integration_connection.failure_message.is_none());
}

#[rstest]
#[tokio::test]
async fn test_sync_ticktick_tasks_with_no_validated_integration_connections(
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    create_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::TickTick(TickTickConfig::enabled()),
        IntegrationConnectionStatus::Created,
        None,
        None,
        None,
        None,
        None,
    )
    .await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.app.ticktick_mock_server)
        .await;

    let response = sync_tasks_response(
        &app.client,
        &app.app.api_address,
        Some(TaskSourceKind::TickTick),
        false,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
}

#[rstest]
#[tokio::test]
async fn test_sync_ticktick_tasks_with_synchronization_disabled(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    nango_ticktick_connection: Box<NangoConnection>,
) {
    let app = authenticated_app.await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::TickTick(TickTickConfig::disabled()),
        &settings,
        nango_ticktick_connection,
        None,
        None,
    )
    .await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.app.ticktick_mock_server)
        .await;

    let response = sync_tasks_response(
        &app.client,
        &app.app.api_address,
        Some(TaskSourceKind::TickTick),
        false,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
}
