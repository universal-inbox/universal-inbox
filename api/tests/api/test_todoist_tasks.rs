use chrono::{NaiveDate, TimeZone, Timelike, Utc};
use rstest::*;
use uuid::Uuid;

use universal_inbox::{
    HasHtmlUrl,
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::github::GithubConfig,
        integrations::todoist::TodoistConfig,
    },
    notification::{
        Notification, NotificationStatus, NotificationWithTask, service::NotificationPatch,
    },
    task::{DueDate, Task, TaskCreation, TaskPriority, TaskStatus, service::TaskPatch},
    third_party::{
        integrations::{
            github::GithubNotification,
            todoist::{TodoistItem, TodoistItemDue, TodoistItemPriority},
        },
        item::{ThirdPartyItem, ThirdPartyItemCreationResult, ThirdPartyItemData},
    },
};

use wiremock::{
    Mock, ResponseTemplate,
    matchers::{method, path},
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
    auth::{AuthenticatedApp, authenticated_app},
    integration_connection::{
        create_and_mock_integration_connection, nango_github_connection, nango_todoist_connection,
    },
    notification::{
        create_task_from_notification,
        github::{create_notification_from_github_notification, github_notification},
    },
    rest::{create_resource, get_resource, patch_resource},
    settings,
    task::todoist::{
        TodoistSyncPartialCommand, mock_todoist_complete_item_service,
        mock_todoist_delete_item_service, mock_todoist_get_item_service,
        mock_todoist_item_add_service, mock_todoist_sync_project_add,
        mock_todoist_sync_resources_service, mock_todoist_sync_service,
        sync_todoist_projects_response, todoist_item,
    },
};

mod patch_task {
    use crate::helpers::task::todoist::mock_todoist_uncomplete_item_service;

    use super::*;
    use pretty_assertions::assert_eq;
    use universal_inbox::task::{PresetDueDate, ProjectSummary};

    #[rstest]
    #[tokio::test]
    async fn test_patch_todoist_task_status_as_deleted(
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
        let existing_todoist_task = creation.task.as_ref().unwrap().clone();
        assert_eq!(existing_todoist_task.status, TaskStatus::Active);
        let existing_todoist_notification = creation.notification.as_ref().unwrap().clone();

        let _todoist_mock = mock_todoist_delete_item_service(
            &app.app.todoist_mock_server,
            &creation.third_party_item.source_id,
        )
        .await;

        let patched_task = patch_resource(
            &app.client,
            &app.app.api_address,
            "tasks",
            existing_todoist_task.id.into(),
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
                ..existing_todoist_task
            })
        );

        let deleted_notification: Box<Notification> = get_resource(
            &app.client,
            &app.app.api_address,
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
        let existing_todoist_task = creation.task.as_ref().unwrap().clone();
        assert_eq!(existing_todoist_task.status, TaskStatus::Active);
        let existing_todoist_notification = creation.notification.as_ref().unwrap().clone();

        let _todoist_mock = mock_todoist_complete_item_service(
            &app.app.todoist_mock_server,
            &creation.third_party_item.source_id,
        )
        .await;

        let patched_task: Box<Task> = patch_resource(
            &app.client,
            &app.app.api_address,
            "tasks",
            existing_todoist_task.id.into(),
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
                ..existing_todoist_task
            })
        );

        let deleted_notification: Box<Notification> = get_resource(
            &app.client,
            &app.app.api_address,
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
        let _todoist_projects_mock = mock_todoist_sync_resources_service(
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
        let existing_todoist_task = creation.task.as_ref().unwrap().clone();
        assert_eq!(
            existing_todoist_task.due_at,
            Some(DueDate::Date(NaiveDate::from_ymd_opt(2016, 9, 1).unwrap()))
        );
        assert_eq!(existing_todoist_task.priority, TaskPriority::P4);
        assert_eq!(existing_todoist_task.project, "Inbox".to_string());
        let existing_todoist_notification = creation.notification.as_ref().unwrap().clone();

        let new_due_at = DueDate::Date(NaiveDate::from_ymd_opt(2022, 1, 1).unwrap());
        let new_priority = TodoistItemPriority::P2;
        let new_project = "Project1".to_string();
        let new_project_id = "3333".to_string();

        let _todoist_project_add_mock = mock_todoist_sync_project_add(
            &app.app.todoist_mock_server,
            &new_project,
            &new_project_id,
        )
        .await;
        let _todoist_uncomplete_item_mock =
            mock_todoist_uncomplete_item_service(&app.app.todoist_mock_server, &todoist_item.id)
                .await;
        let _todoist_sync_mock = mock_todoist_sync_service(
            &app.app.todoist_mock_server,
            vec![
                TodoistSyncPartialCommand::ItemMove {
                    args: TodoistSyncCommandItemMoveArgs {
                        id: creation.third_party_item.source_id.clone(),
                        project_id: new_project_id,
                    },
                },
                TodoistSyncPartialCommand::ItemUpdate {
                    args: TodoistSyncCommandItemUpdateArgs {
                        id: creation.third_party_item.source_id.clone(),
                        due: Some(Some(TodoistItemDue {
                            string: "".to_string(),
                            date: new_due_at.clone(),
                            is_recurring: false,
                            timezone: None,
                            lang: "en".to_string(),
                        })),
                        priority: Some(new_priority),
                        description: None,
                        content: None,
                    },
                },
            ],
            None,
        )
        .await;

        let patched_task = patch_resource(
            &app.client,
            &app.app.api_address,
            "tasks",
            existing_todoist_task.id.into(),
            &TaskPatch {
                project_name: Some(new_project.clone()),
                due_at: Some(Some(new_due_at.clone())),
                priority: Some(new_priority.into()),
                status: Some(TaskStatus::Active),
                ..Default::default()
            },
        )
        .await;

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
            &app.app.api_address,
            "notifications",
            existing_todoist_notification.id.into(),
        )
        .await;
        assert_eq!(deleted_notification.status, NotificationStatus::Deleted);
    }

    // Cannot test project creation as it will fetch projects more than once
    // and wiremock does not support mocking the same URL with different results
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
            notification.get_html_url().as_ref()
        ));
        let todoist_integration_connection = create_and_mock_integration_connection(
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

        Mock::given(method("DELETE"))
            .and(path("/notifications/threads/1"))
            .respond_with(ResponseTemplate::new(205))
            .mount(&app.app.github_mock_server)
            .await;
        let _todoist_projects_mock = mock_todoist_sync_resources_service(
            &app.app.todoist_mock_server,
            "projects",
            &sync_todoist_projects_response,
            None,
        )
        .await;
        let _todoist_item_add_mock = mock_todoist_item_add_service(
            &app.app.todoist_mock_server,
            &todoist_item.id,
            todoist_item.content.clone(),
            body.clone(),
            Some(todoist_item.project_id.clone()),
            due_at.as_ref().map(|due_at| due_at.into()),
            todoist_item.priority,
        )
        .await;
        let _todoist_get_item_mock =
            mock_todoist_get_item_service(&app.app.todoist_mock_server, todoist_item.clone()).await;

        let notification_with_task = create_task_from_notification(
            &app.client,
            &app.app.api_address,
            notification.id,
            Some(TaskCreation {
                title: todoist_item.content.clone(),
                body,
                project_name: Some("Project2".to_string()),
                due_at,
                priority: todoist_item.priority.into(),
                task_provider_kind: None,
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
                    ..(*TodoistService::build_task_with_project_name(
                        &todoist_item,
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
                            source_id: todoist_item.id.clone(),
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
                            data: ThirdPartyItemData::TodoistItem(todoist_item.clone()),
                            integration_connection_id: todoist_integration_connection.id,
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

    #[rstest]
    #[tokio::test]
    async fn test_create_todoist_task_with_defaults_from_notification(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        sync_todoist_projects_response: TodoistSyncResponse,
        todoist_item: Box<TodoistItem>,
        nango_todoist_connection: Box<NangoConnection>,
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

        // Existing project in sync_todoist_projects_response
        let project = "Project2".to_string();
        let project_id = "2222".to_string();
        let todoist_item = Box::new(TodoistItem {
            project_id: project_id.clone(),
            ..(*todoist_item).clone()
        });
        let body = Some(format!(
            "- [{}]({})",
            notification.title,
            notification.get_html_url().as_ref()
        ));
        let todoist_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Todoist(TodoistConfig {
                default_project: Some(ProjectSummary {
                    source_id: project_id.clone().into(),
                    name: project.clone(),
                }),
                default_due_at: Some(PresetDueDate::Today),
                default_priority: Some(TaskPriority::from(todoist_item.priority)),
                ..TodoistConfig::enabled()
            }),
            &settings,
            nango_todoist_connection,
            None,
            None,
        )
        .await;

        Mock::given(method("DELETE"))
            .and(path("/notifications/threads/1"))
            .respond_with(ResponseTemplate::new(205))
            .mount(&app.app.github_mock_server)
            .await;
        let _todoist_projects_mock = mock_todoist_sync_resources_service(
            &app.app.todoist_mock_server,
            "projects",
            &sync_todoist_projects_response,
            None,
        )
        .await;
        let _todoist_item_add_mock = mock_todoist_item_add_service(
            &app.app.todoist_mock_server,
            &todoist_item.id,
            notification.title.clone(),
            body.clone(),
            Some(project_id.clone()),
            Some((&Into::<DueDate>::into(PresetDueDate::Today)).into()),
            todoist_item.priority,
        )
        .await;
        let _todoist_get_item_mock =
            mock_todoist_get_item_service(&app.app.todoist_mock_server, todoist_item.clone()).await;

        let notification_with_task =
            create_task_from_notification(&app.client, &app.app.api_address, notification.id, None)
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
                    ..(*TodoistService::build_task_with_project_name(
                        &todoist_item,
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
                            source_id: todoist_item.id.clone(),
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
                            data: ThirdPartyItemData::TodoistItem(todoist_item.clone()),
                            integration_connection_id: todoist_integration_connection.id,
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
}

mod patch_notification {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_patch_notification_to_link_with_task(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        todoist_item: Box<TodoistItem>,
        sync_todoist_projects_response: TodoistSyncResponse,
        nango_todoist_connection: Box<NangoConnection>,
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
        let todoist_integration_connection = create_and_mock_integration_connection(
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
                    project_id: "2222".to_string(), // ie. "Project2"
                    added_at: Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
                    ..*todoist_item.clone()
                })),
                integration_connection_id: todoist_integration_connection.id,
                source_item: None,
            }),
        )
        .await;
        let existing_todoist_task = creation.task.as_ref().unwrap().clone();

        let _todoist_sync_mock = mock_todoist_sync_service(
            &app.app.todoist_mock_server,
            vec![TodoistSyncPartialCommand::ItemUpdate {
                args: TodoistSyncCommandItemUpdateArgs {
                    id: creation.third_party_item.source_id.clone(),
                    description: Some(format!(
                        "\n- [{}]({})",
                        notification.title,
                        notification.get_html_url().as_ref()
                    )),
                    ..Default::default()
                },
            }],
            None,
        )
        .await;

        let patched_notification = patch_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            notification.id.into(),
            &NotificationPatch {
                task_id: Some(existing_todoist_task.id),
                ..Default::default()
            },
        )
        .await;

        assert_eq!(
            patched_notification,
            Box::new(Notification {
                task_id: Some(existing_todoist_task.id),
                ..*notification.clone()
            })
        );

        let updated_notification = get_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            notification.id.into(),
        )
        .await;

        assert_eq!(
            updated_notification,
            Box::new(Notification {
                task_id: Some(existing_todoist_task.id),
                ..*notification
            })
        );
    }
}
