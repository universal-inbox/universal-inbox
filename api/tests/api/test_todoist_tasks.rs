use chrono::{NaiveDate, TimeZone, Utc};
use rstest::*;

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
        item::{ThirdPartyItem, ThirdPartyItemData},
    },
};

use wiremock::{
    Mock, ResponseTemplate,
    matchers::{method, path},
};

use universal_inbox_api::{
    configuration::Settings,
    integrations::todoist::{
        TodoistService, TodoistSyncCommandItemMoveArgs, TodoistSyncCommandItemUpdateArgs,
        TodoistSyncResponse,
    },
};

use crate::helpers::integration_connection::OAuthCredentialFixture;
use crate::helpers::third_party::create_task_third_party_item;
use crate::helpers::{
    auth::{AuthenticatedApp, authenticated_app},
    integration_connection::{
        create_and_mock_integration_connection, github_oauth_credential, todoist_oauth_credential,
    },
    notification::{
        create_task_from_notification,
        github::{create_notification_from_github_notification, github_notification},
    },
    rest::{get_resource, patch_resource, patch_resource_response},
    settings,
    task::todoist::{
        TodoistSyncPartialCommand, mock_todoist_complete_item_service,
        mock_todoist_delete_item_service, mock_todoist_get_item_service,
        mock_todoist_item_add_service, mock_todoist_no_sync_call, mock_todoist_sync_project_add,
        mock_todoist_sync_resources_service, mock_todoist_sync_service,
        mock_todoist_sync_service_expecting_one_call, sync_todoist_projects_response, todoist_item,
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
        todoist_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;
        let _integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
            &settings,
            todoist_oauth_credential,
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

        let creation = create_task_third_party_item(
            &app.app,
            ThirdPartyItemData::TodoistItem(Box::new(TodoistItem {
                project_id: "1111".to_string(), // ie. "Inbox"
                added_at: Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
                ..*todoist_item.clone()
            })),
            app.user.id,
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

        let deleted_notification: Box<NotificationWithTask> = get_resource(
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
        todoist_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;
        let _integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
            &settings,
            todoist_oauth_credential,
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

        let creation = create_task_third_party_item(
            &app.app,
            ThirdPartyItemData::TodoistItem(Box::new(TodoistItem {
                project_id: "1111".to_string(), // ie. "Inbox"
                added_at: Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
                ..*todoist_item.clone()
            })),
            app.user.id,
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

        let deleted_notification: Box<NotificationWithTask> = get_resource(
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
        todoist_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;
        let _integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
            &settings,
            todoist_oauth_credential,
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

        let creation = create_task_third_party_item(
            &app.app,
            ThirdPartyItemData::TodoistItem(Box::new(TodoistItem {
                project_id: "1111".to_string(), // ie. "Inbox"
                added_at: Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
                ..*todoist_item.clone()
            })),
            app.user.id,
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

        let deleted_notification: Box<NotificationWithTask> = get_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            existing_todoist_notification.id.into(),
        )
        .await;
        assert_eq!(deleted_notification.status, NotificationStatus::Deleted);
    }

    /// The task manager owns the title, so an explicit rename must reach it —
    /// even when `title` is the only field in the patch.
    #[rstest]
    #[tokio::test]
    async fn test_patch_todoist_task_title_only(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        todoist_item: Box<TodoistItem>,
        sync_todoist_projects_response: TodoistSyncResponse,
        todoist_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;
        let _integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
            &settings,
            todoist_oauth_credential,
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

        let creation = create_task_third_party_item(
            &app.app,
            ThirdPartyItemData::TodoistItem(Box::new(TodoistItem {
                project_id: "1111".to_string(), // ie. "Inbox"
                added_at: Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
                ..*todoist_item.clone()
            })),
            app.user.id,
        )
        .await;
        let existing_todoist_task = creation.task.as_ref().unwrap().clone();
        let new_title = "Reply to Marc about the billing thread".to_string();
        assert_ne!(existing_todoist_task.title, new_title);

        // The rename must be sent as the `content` of an `item_update` command,
        // with no other field riding along.
        let _todoist_sync_mock = mock_todoist_sync_service_expecting_one_call(
            &app.app.todoist_mock_server,
            vec![TodoistSyncPartialCommand::ItemUpdate {
                args: TodoistSyncCommandItemUpdateArgs {
                    id: creation.third_party_item.source_id.clone(),
                    content: Some(new_title.clone()),
                    ..Default::default()
                },
            }],
            None,
        )
        .await;

        let patched_task: Box<Task> = patch_resource(
            &app.client,
            &app.app.api_address,
            "tasks",
            existing_todoist_task.id.into(),
            &TaskPatch {
                title: Some(new_title.clone()),
                ..Default::default()
            },
        )
        .await;

        assert_eq!(
            patched_task,
            Box::new(Task {
                title: new_title.clone(),
                ..existing_todoist_task.clone()
            })
        );

        let reloaded_task: Box<Task> = get_resource(
            &app.client,
            &app.app.api_address,
            "tasks",
            existing_todoist_task.id.into(),
        )
        .await;
        assert_eq!(reloaded_task.title, new_title);
    }

    /// Renaming a task to the title it already has changes nothing, so it must
    /// answer `304` and reach no provider.
    #[rstest]
    #[tokio::test]
    async fn test_patch_todoist_task_title_unchanged_is_a_no_op(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        todoist_item: Box<TodoistItem>,
        sync_todoist_projects_response: TodoistSyncResponse,
        todoist_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;
        let _integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
            &settings,
            todoist_oauth_credential,
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

        let creation = create_task_third_party_item(
            &app.app,
            ThirdPartyItemData::TodoistItem(Box::new(TodoistItem {
                project_id: "1111".to_string(), // ie. "Inbox"
                added_at: Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
                ..*todoist_item.clone()
            })),
            app.user.id,
        )
        .await;
        let existing_todoist_task = creation.task.as_ref().unwrap().clone();

        // No sync command must be sent: verified when the mock server drops.
        mock_todoist_no_sync_call(&app.app.todoist_mock_server).await;

        let response = patch_resource_response(
            &app.client,
            &app.app.api_address,
            "tasks",
            existing_todoist_task.id.into(),
            &TaskPatch {
                title: Some(existing_todoist_task.title.clone()),
                ..Default::default()
            },
        )
        .await;

        assert_eq!(response.status(), http::StatusCode::NOT_MODIFIED);
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
        todoist_oauth_credential: OAuthCredentialFixture,
        github_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;

        let github_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Github(GithubConfig::enabled()),
            &settings,
            github_oauth_credential,
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
            IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
            &settings,
            todoist_oauth_credential,
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
                time_config: None,
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

        let deleted_notification: Box<NotificationWithTask> = get_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            notification.id.into(),
        )
        .await;
        assert_eq!(deleted_notification.status, NotificationStatus::Deleted);
        assert_eq!(
            deleted_notification.task.as_ref().map(|t| t.id),
            Some(new_task_id)
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_create_todoist_task_with_defaults_from_notification(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        sync_todoist_projects_response: TodoistSyncResponse,
        todoist_item: Box<TodoistItem>,
        todoist_oauth_credential: OAuthCredentialFixture,
        github_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;

        let github_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Github(GithubConfig::enabled()),
            &settings,
            github_oauth_credential,
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
            todoist_oauth_credential,
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

        let deleted_notification: Box<NotificationWithTask> = get_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            notification.id.into(),
        )
        .await;
        assert_eq!(deleted_notification.status, NotificationStatus::Deleted);
        assert_eq!(
            deleted_notification.task.as_ref().map(|t| t.id),
            Some(new_task_id)
        );
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
        todoist_oauth_credential: OAuthCredentialFixture,
        github_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;
        let github_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Github(GithubConfig::enabled()),
            &settings,
            github_oauth_credential,
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
        let _todoist_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
            &settings,
            todoist_oauth_credential,
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

        let creation = create_task_third_party_item(
            &app.app,
            ThirdPartyItemData::TodoistItem(Box::new(TodoistItem {
                project_id: "2222".to_string(), // ie. "Project2"
                added_at: Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
                ..*todoist_item.clone()
            })),
            app.user.id,
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

        let updated_notification: Box<NotificationWithTask> = get_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            notification.id.into(),
        )
        .await;

        assert_eq!(
            Box::new(Notification::from(*updated_notification)),
            Box::new(Notification {
                task_id: Some(existing_todoist_task.id),
                ..*notification
            })
        );
    }
}

mod legacy_todoist_ids {
    use super::*;
    use pretty_assertions::assert_eq;

    use universal_inbox::task::TaskId;
    use universal_inbox_api::{
        commands::todoist::migrate_legacy_ids,
        integrations::todoist::TodoistSyncCommandItemCompleteArgs,
        repository::third_party::ThirdPartyItemRepository,
    };

    use crate::helpers::task::todoist::{
        mock_todoist_id_mappings_service, mock_todoist_sync_service_with_command_error,
    };

    const LEGACY_TASK_ID: &str = "9735649058";
    const NEW_TASK_ID: &str = "6Jf8VQXxpwv56VQ7";
    const LEGACY_PROJECT_ID: &str = "2203306141";
    const NEW_PROJECT_ID: &str = "6Jf8VQXxpwv56VQ9";

    async fn setup(
        settings: &Settings,
        app: &AuthenticatedApp,
        sync_todoist_projects_response: &TodoistSyncResponse,
        todoist_oauth_credential: OAuthCredentialFixture,
    ) {
        create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
            settings,
            todoist_oauth_credential,
            None,
            None,
        )
        .await;
        mock_todoist_sync_resources_service(
            &app.app.todoist_mock_server,
            "projects",
            sync_todoist_projects_response,
            None,
        )
        .await;
    }

    fn legacy_todoist_item(todoist_item: &TodoistItem, id: &str) -> ThirdPartyItemData {
        ThirdPartyItemData::TodoistItem(Box::new(TodoistItem {
            id: id.to_string(),
            project_id: LEGACY_PROJECT_ID.to_string(),
            ..todoist_item.clone()
        }))
    }

    async fn get_task(app: &AuthenticatedApp, task_id: TaskId) -> Box<Task> {
        get_resource(&app.client, &app.app.api_address, "tasks", task_id.into()).await
    }

    fn assert_todoist_ids(item: &ThirdPartyItem, id: &str, project_id: &str) {
        assert_eq!(item.source_id, id);
        let ThirdPartyItemData::TodoistItem(ref todoist_item) = item.data else {
            panic!("Expected a Todoist item, got {:?}", item.data);
        };
        assert_eq!(todoist_item.id, id);
        assert_eq!(todoist_item.project_id, project_id);
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_todoist_task_with_legacy_id_status_as_done(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        todoist_item: Box<TodoistItem>,
        sync_todoist_projects_response: TodoistSyncResponse,
        todoist_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;
        setup(
            &settings,
            &app,
            &sync_todoist_projects_response,
            todoist_oauth_credential,
        )
        .await;
        let creation = create_task_third_party_item(
            &app.app,
            legacy_todoist_item(&todoist_item, LEGACY_TASK_ID),
            app.user.id,
        )
        .await;
        let existing_todoist_task = creation.task.as_ref().unwrap().clone();

        mock_todoist_sync_service_with_command_error(
            &app.app.todoist_mock_server,
            vec![TodoistSyncPartialCommand::ItemComplete {
                args: TodoistSyncCommandItemCompleteArgs {
                    id: LEGACY_TASK_ID.to_string(),
                },
            }],
            557,
        )
        .await;
        mock_todoist_id_mappings_service(
            &app.app.todoist_mock_server,
            "tasks",
            vec![(LEGACY_TASK_ID, NEW_TASK_ID)],
        )
        .await;
        mock_todoist_id_mappings_service(
            &app.app.todoist_mock_server,
            "projects",
            vec![(LEGACY_PROJECT_ID, NEW_PROJECT_ID)],
        )
        .await;
        mock_todoist_complete_item_service(&app.app.todoist_mock_server, NEW_TASK_ID).await;

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
        assert_eq!(patched_task.status, TaskStatus::Done);

        let task: Box<Task> = get_resource(
            &app.client,
            &app.app.api_address,
            "tasks",
            existing_todoist_task.id.into(),
        )
        .await;
        assert_eq!(task.status, TaskStatus::Done);
        assert_todoist_ids(&task.source_item, NEW_TASK_ID, NEW_PROJECT_ID);
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_todoist_task_with_legacy_id_status_and_title(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        todoist_item: Box<TodoistItem>,
        sync_todoist_projects_response: TodoistSyncResponse,
        todoist_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;
        setup(
            &settings,
            &app,
            &sync_todoist_projects_response,
            todoist_oauth_credential,
        )
        .await;
        let creation = create_task_third_party_item(
            &app.app,
            legacy_todoist_item(&todoist_item, LEGACY_TASK_ID),
            app.user.id,
        )
        .await;
        let existing_todoist_task = creation.task.as_ref().unwrap().clone();

        mock_todoist_sync_service_with_command_error(
            &app.app.todoist_mock_server,
            vec![TodoistSyncPartialCommand::ItemComplete {
                args: TodoistSyncCommandItemCompleteArgs {
                    id: LEGACY_TASK_ID.to_string(),
                },
            }],
            557,
        )
        .await;
        mock_todoist_complete_item_service(&app.app.todoist_mock_server, NEW_TASK_ID).await;
        // The title update is sent with the (stale) legacy ID too: it is
        // remapped a second time.
        mock_todoist_sync_service_with_command_error(
            &app.app.todoist_mock_server,
            vec![TodoistSyncPartialCommand::ItemUpdate {
                args: TodoistSyncCommandItemUpdateArgs {
                    id: LEGACY_TASK_ID.to_string(),
                    content: Some("New title".to_string()),
                    ..Default::default()
                },
            }],
            557,
        )
        .await;
        mock_todoist_sync_service_expecting_one_call(
            &app.app.todoist_mock_server,
            vec![TodoistSyncPartialCommand::ItemUpdate {
                args: TodoistSyncCommandItemUpdateArgs {
                    id: NEW_TASK_ID.to_string(),
                    content: Some("New title".to_string()),
                    ..Default::default()
                },
            }],
            None,
        )
        .await;
        mock_todoist_id_mappings_service(
            &app.app.todoist_mock_server,
            "tasks",
            vec![(LEGACY_TASK_ID, NEW_TASK_ID)],
        )
        .await;
        mock_todoist_id_mappings_service(
            &app.app.todoist_mock_server,
            "projects",
            vec![(LEGACY_PROJECT_ID, NEW_PROJECT_ID)],
        )
        .await;

        let patched_task: Box<Task> = patch_resource(
            &app.client,
            &app.app.api_address,
            "tasks",
            existing_todoist_task.id.into(),
            &TaskPatch {
                status: Some(TaskStatus::Done),
                title: Some("New title".to_string()),
                ..Default::default()
            },
        )
        .await;
        assert_eq!(patched_task.status, TaskStatus::Done);

        let task = get_task(&app, existing_todoist_task.id).await;
        assert_eq!(task.status, TaskStatus::Done);
        assert_todoist_ids(&task.source_item, NEW_TASK_ID, NEW_PROJECT_ID);
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_todoist_task_with_unmapped_legacy_id_fails(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        todoist_item: Box<TodoistItem>,
        sync_todoist_projects_response: TodoistSyncResponse,
        todoist_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;
        setup(
            &settings,
            &app,
            &sync_todoist_projects_response,
            todoist_oauth_credential,
        )
        .await;
        let creation = create_task_third_party_item(
            &app.app,
            legacy_todoist_item(&todoist_item, LEGACY_TASK_ID),
            app.user.id,
        )
        .await;
        let existing_todoist_task = creation.task.as_ref().unwrap().clone();

        mock_todoist_sync_service_with_command_error(
            &app.app.todoist_mock_server,
            vec![TodoistSyncPartialCommand::ItemComplete {
                args: TodoistSyncCommandItemCompleteArgs {
                    id: LEGACY_TASK_ID.to_string(),
                },
            }],
            557,
        )
        .await;
        mock_todoist_id_mappings_service(&app.app.todoist_mock_server, "tasks", vec![]).await;

        let response = patch_resource_response(
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
        assert_eq!(response.status(), 500);

        let task: Box<Task> = get_resource(
            &app.client,
            &app.app.api_address,
            "tasks",
            existing_todoist_task.id.into(),
        )
        .await;
        assert_eq!(task.status, TaskStatus::Active);
        assert_todoist_ids(&task.source_item, LEGACY_TASK_ID, LEGACY_PROJECT_ID);
    }

    #[rstest]
    #[tokio::test]
    async fn test_migrate_legacy_todoist_ids(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        todoist_item: Box<TodoistItem>,
        sync_todoist_projects_response: TodoistSyncResponse,
        todoist_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;
        setup(
            &settings,
            &app,
            &sync_todoist_projects_response,
            todoist_oauth_credential,
        )
        .await;
        const LEGACY_DUPLICATED_TASK_ID: &str = "9735649059";
        const NEW_DUPLICATED_TASK_ID: &str = "6Jf8VQXxpwv56VQ8";
        let legacy = create_task_third_party_item(
            &app.app,
            legacy_todoist_item(&todoist_item, LEGACY_TASK_ID),
            app.user.id,
        )
        .await;
        let legacy_duplicated = create_task_third_party_item(
            &app.app,
            legacy_todoist_item(&todoist_item, LEGACY_DUPLICATED_TASK_ID),
            app.user.id,
        )
        .await;
        // Synced after the Todoist API v1 migration, duplicating `legacy_duplicated`
        let duplicate = create_task_third_party_item(
            &app.app,
            ThirdPartyItemData::TodoistItem(Box::new(TodoistItem {
                id: NEW_DUPLICATED_TASK_ID.to_string(),
                project_id: NEW_PROJECT_ID.to_string(),
                ..*todoist_item.clone()
            })),
            app.user.id,
        )
        .await;

        mock_todoist_id_mappings_service(
            &app.app.todoist_mock_server,
            "tasks",
            vec![
                (LEGACY_TASK_ID, NEW_TASK_ID),
                (LEGACY_DUPLICATED_TASK_ID, NEW_DUPLICATED_TASK_ID),
            ],
        )
        .await;
        mock_todoist_id_mappings_service(
            &app.app.todoist_mock_server,
            "projects",
            vec![(LEGACY_PROJECT_ID, NEW_PROJECT_ID)],
        )
        .await;

        migrate_legacy_ids(
            app.app.task_service.clone(),
            app.app.integration_connection_service.clone(),
            Some(app.user.id),
            false,
        )
        .await
        .unwrap();

        let migrated_task = get_task(&app, legacy.task.as_ref().unwrap().id).await;
        assert_eq!(migrated_task.status, TaskStatus::Active);
        assert_todoist_ids(&migrated_task.source_item, NEW_TASK_ID, NEW_PROJECT_ID);

        let legacy_duplicated_task =
            get_task(&app, legacy_duplicated.task.as_ref().unwrap().id).await;
        assert_eq!(legacy_duplicated_task.status, TaskStatus::Deleted);
        assert_todoist_ids(
            &legacy_duplicated_task.source_item,
            LEGACY_DUPLICATED_TASK_ID,
            LEGACY_PROJECT_ID,
        );

        let duplicate_task = get_task(&app, duplicate.task.as_ref().unwrap().id).await;
        assert_eq!(duplicate_task.status, TaskStatus::Active);

        // A second run has nothing left to migrate
        let mut transaction = app.app.repository.begin().await.unwrap();
        let legacy_items = app
            .app
            .repository
            .find_legacy_todoist_items(&mut transaction, Some(app.user.id))
            .await
            .unwrap();
        transaction.commit().await.unwrap();
        assert_eq!(legacy_items, vec![]);
    }
}
