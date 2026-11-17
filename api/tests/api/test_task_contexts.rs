//! Tasks bound to contexts by id (qk5d.2.4): the inbox told by its flag, a rename that keeps
//! every binding, the context search, task creation by destination id only, and the stored
//! default projects migrated to contexts.

use std::collections::HashMap;

use http::StatusCode;
use rstest::*;
use uuid::Uuid;
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{body_string_contains, method, path},
};

use universal_inbox::{
    audit::{AuditActor, AuditUserChannel},
    context::{ContextId, ContextStatusChange, ContextSummary},
    integration_connection::{
        IntegrationConnection,
        config::IntegrationConnectionConfig,
        integrations::{
            github::GithubConfig,
            linear::{LinearConfig, LinearSyncTaskConfig},
            todoist::TodoistConfig,
        },
        provider::IntegrationProviderKind,
    },
    notification::Notification,
    task::{
        DefaultProjectIssue, ProjectReference, ProjectSummary, Task, TaskCreation,
        TaskCreationResult, TaskDestination, TaskPriority, TaskSourceKind,
    },
    third_party::integrations::github::GithubNotification,
};

use universal_inbox_api::{
    configuration::Settings, integrations::todoist::TodoistSyncResponse,
    repository::context::ContextRepository,
    universal_inbox::audit::actor_context::with_system_actor,
};

use crate::helpers::{
    auth::{AuthenticatedApp, authenticate_user, authenticated_app},
    integration_connection::{
        OAuthCredentialFixture, create_and_mock_integration_connection, github_oauth_credential,
        linear_oauth_credential, list_integration_connections, todoist_oauth_credential,
    },
    notification::{
        create_task_from_notification_response,
        github::{create_notification_from_github_notification, github_notification},
    },
    rest::get_resource,
    set_audit_actor, settings,
    task::{
        sync_tasks,
        todoist::{
            mirror_fixture_todoist_projects, mirror_todoist_projects, mock_todoist_no_project_add,
            mock_todoist_sync_resources_service, sync_todoist_items_response,
            sync_todoist_projects_response, todoist_project, todoist_projects_with,
        },
    },
};

/// Fail the test if a Todoist task is ever created
async fn mock_todoist_no_item_add(app: &AuthenticatedApp) {
    Mock::given(method("POST"))
        .and(path("/sync"))
        .and(body_string_contains("item_add"))
        .respond_with(ResponseTemplate::new(500))
        .with_priority(1)
        .expect(0)
        .mount(&app.app.todoist_mock_server)
        .await;
}

/// Run the Todoist context sync (projects mirror, task backfill, default migration) as the sync
/// job does
async fn sync_todoist_contexts(app: &AuthenticatedApp) {
    let task_service = app.app.task_service.read().await;
    with_system_actor(
        "SyncTasks",
        Some(IntegrationProviderKind::Todoist),
        task_service.sync_project_contexts(app.user.id, IntegrationProviderKind::Todoist),
    )
    .await
    .unwrap();
}

async fn search_contexts(app: &AuthenticatedApp, matches: &str) -> Vec<ContextSummary> {
    app.client
        .get(format!(
            "{}contexts/search?matches={matches}",
            app.app.api_address
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

fn connection_of(
    connections: &[IntegrationConnection],
    kind: IntegrationProviderKind,
) -> &IntegrationConnection {
    connections
        .iter()
        .find(|connection| connection.provider.kind() == kind)
        .unwrap()
}

/// The stored default project of a connection, as saved
fn default_project_of(connection: &IntegrationConnection) -> Option<ProjectReference> {
    match connection.provider.config() {
        IntegrationConnectionConfig::Todoist(config) => config.default_project,
        IntegrationConnectionConfig::Linear(config) => config.sync_task_config.target_project,
        config => panic!("No default project in {config:?}"),
    }
}

/// The user's mirrored Todoist inbox context
async fn todoist_inbox(app: &AuthenticatedApp) -> Option<ContextId> {
    let mut transaction = app.app.repository.begin().await.unwrap();
    app.app
        .repository
        .get_inbox_context_id(
            &mut transaction,
            app.user.id,
            IntegrationProviderKind::Todoist,
        )
        .await
        .unwrap()
}

/// A GitHub notification to create a task from
async fn notification_for_task(
    app: &AuthenticatedApp,
    settings: &Settings,
    github_notification: &GithubNotification,
    github_oauth_credential: OAuthCredentialFixture,
) -> Box<Notification> {
    let github_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        settings,
        github_oauth_credential,
        None,
        None,
    )
    .await;
    Mock::given(method("DELETE"))
        .and(path("/notifications/threads/1"))
        .respond_with(ResponseTemplate::new(205))
        .mount(&app.app.github_mock_server)
        .await;
    create_notification_from_github_notification(
        &app.app,
        github_notification,
        app.user.id,
        github_integration_connection.id,
    )
    .await
}

fn task_creation(destination: Option<TaskDestination>) -> TaskCreation {
    TaskCreation {
        title: "Plan the launch".to_string(),
        body: None,
        destination,
        due_at: None,
        priority: TaskPriority::P4,
        task_provider_kind: Some(IntegrationProviderKind::Todoist),
        time_config: None,
        parent_id: None,
    }
}

/// The task synced from `source_id`, read back through the API: `context` is filled on reads
async fn task_of(app: &AuthenticatedApp, results: &[TaskCreationResult], source_id: &str) -> Task {
    let task_id = results
        .iter()
        .find(|result| result.task.source_item.source_id == source_id)
        .unwrap()
        .task
        .id;
    *get_resource::<Task>(&app.client, &app.app.api_address, "tasks", task_id.into()).await
}

mod inbox {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_the_first_sync_binds_tasks_and_tells_the_inbox_by_its_flag(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        sync_todoist_items_response: TodoistSyncResponse,
        sync_todoist_projects_response: TodoistSyncResponse,
        todoist_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;
        create_and_mock_integration_connection(
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
        mock_todoist_sync_resources_service(
            &app.app.todoist_mock_server,
            "items",
            &sync_todoist_items_response,
            None,
        )
        .await;

        let results = sync_tasks(
            &app.client,
            &app.app.api_address,
            Some(TaskSourceKind::Todoist),
            false,
        )
        .await;

        // 1123 is in project 1111, flagged `inbox_project` in the fixture
        let inbox_task = task_of(&app, &results, "1123").await;
        let inbox_context = inbox_task.context.clone().unwrap();
        assert!(inbox_context.is_inbox);
        assert_eq!(inbox_task.context_id, Some(inbox_context.id));
        let inbox = todoist_inbox(&app).await;
        assert_eq!(inbox, Some(inbox_context.id));
        assert!(inbox_task.is_in_inbox(inbox));
        // Created on the very first sync: the contexts are mirrored before the tasks
        let inbox_result = results
            .iter()
            .find(|result| result.task.id == inbox_task.id)
            .unwrap();
        assert_eq!(inbox_result.notifications.len(), 1);

        let project_task = task_of(&app, &results, "1456").await;
        let project_context = project_task.context.clone().unwrap();
        assert_eq!(project_context.name, "Project2");
        assert_eq!(project_context.path, vec!["Project2".to_string()]);
        assert!(!project_context.is_inbox);
        assert!(!project_task.is_in_inbox(inbox));
    }

    #[rstest]
    #[tokio::test]
    async fn test_a_project_named_inbox_is_not_the_inbox(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        sync_todoist_items_response: TodoistSyncResponse,
        todoist_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;
        create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
            &settings,
            todoist_oauth_credential,
            None,
            None,
        )
        .await;
        // "Inbox" (1111) is a user project here, the inbox is another project
        let mut projects = todoist_projects_with(vec![{
            let mut inbox = todoist_project("9999", "Boîte de réception", None);
            inbox.inbox_project = true;
            inbox
        }]);
        for project in projects.projects.as_mut().unwrap() {
            if project.id == "1111" {
                project.inbox_project = false;
            }
        }
        mock_todoist_sync_resources_service(
            &app.app.todoist_mock_server,
            "projects",
            &projects,
            None,
        )
        .await;
        mock_todoist_sync_resources_service(
            &app.app.todoist_mock_server,
            "items",
            &sync_todoist_items_response,
            None,
        )
        .await;

        let results = sync_tasks(
            &app.client,
            &app.app.api_address,
            Some(TaskSourceKind::Todoist),
            false,
        )
        .await;

        let task = task_of(&app, &results, "1123").await;
        assert_eq!(task.context.as_ref().unwrap().name, "Inbox");
        assert!(!task.is_in_inbox(todoist_inbox(&app).await));
        let result = results
            .iter()
            .find(|result| result.task.id == task.id)
            .unwrap();
        assert!(result.notifications.is_empty());
    }
}

mod rename {
    use super::*;
    use pretty_assertions::assert_eq;

    /// AC1: renaming a project in Todoist renames the context, and every task stays bound
    #[rstest]
    #[tokio::test]
    async fn test_renaming_a_project_keeps_every_binding(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        sync_todoist_items_response: TodoistSyncResponse,
        sync_todoist_projects_response: TodoistSyncResponse,
        todoist_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;
        create_and_mock_integration_connection(
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
        mock_todoist_sync_resources_service(
            &app.app.todoist_mock_server,
            "items",
            &sync_todoist_items_response,
            None,
        )
        .await;
        let results = sync_tasks(
            &app.client,
            &app.app.api_address,
            Some(TaskSourceKind::Todoist),
            false,
        )
        .await;
        let bound: HashMap<String, Option<ContextId>> = results
            .iter()
            .map(|result| {
                (
                    result.task.source_item.source_id.clone(),
                    result.task.context_id,
                )
            })
            .collect();
        assert!(bound.values().all(Option::is_some));

        // Todoist now returns project 2222 renamed
        app.app.todoist_mock_server.reset().await;
        let mut renamed = sync_todoist_projects_response.clone();
        for project in renamed.projects.as_mut().unwrap() {
            if project.id == "2222" {
                project.name = "Launch".to_string();
            }
        }
        mock_todoist_sync_resources_service(
            &app.app.todoist_mock_server,
            "projects",
            &renamed,
            None,
        )
        .await;
        // No cache bump here: the context sync must see the rename on its own
        sync_todoist_contexts(&app).await;

        for result in &results {
            let task: Box<Task> = get_resource(
                &app.client,
                &app.app.api_address,
                "tasks",
                result.task.id.into(),
            )
            .await;
            assert_eq!(task.context_id, bound[&task.source_item.source_id]);
            if task.source_item.source_id == "1456" {
                assert_eq!(task.context.as_ref().unwrap().name, "Launch");
            }
        }
    }
}

mod search {
    use super::*;
    use pretty_assertions::assert_eq;

    /// AC3: a context is found by any segment of its path, and shown with its whole path
    #[rstest]
    #[tokio::test]
    async fn test_search_contexts_anywhere_in_the_tree(
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let archived = {
            let mut project = todoist_project("13", "Old clients", None);
            project.is_archived = true;
            project
        };
        let contexts = mirror_todoist_projects(
            &app.app,
            app.user.id,
            &todoist_projects_with(vec![
                todoist_project("10", "Work", None),
                todoist_project("11", "Clients", Some("10")),
                todoist_project("12", "Acme", Some("11")),
                archived,
                todoist_project("14", "Gone clients", None),
            ])
            .projects
            .unwrap(),
        )
        .await;
        // Project 14 is gone upstream
        let mut transaction = app.app.repository.begin().await.unwrap();
        set_audit_actor(
            &mut transaction,
            &AuditActor::User {
                via: AuditUserChannel::Web,
            },
        )
        .await;
        app.app
            .repository
            .update_context_status(
                &mut transaction,
                contexts["14"],
                ContextStatusChange::Tombstone,
            )
            .await
            .unwrap();
        transaction.commit().await.unwrap();

        let found = search_contexts(&app, "CLIENTS").await;

        // The context named after the query before the one matching through its parent
        assert_eq!(
            found
                .iter()
                .map(|context| (context.id, context.path.clone()))
                .collect::<Vec<_>>(),
            vec![
                (
                    contexts["11"],
                    vec!["Work".to_string(), "Clients".to_string()]
                ),
                (
                    contexts["12"],
                    vec![
                        "Work".to_string(),
                        "Clients".to_string(),
                        "Acme".to_string()
                    ]
                ),
            ]
        );

        let all = search_contexts(&app, "").await;
        assert!(all[0].is_inbox);
        assert_eq!(all[0].id, contexts["1111"]);
        assert!(
            !all.iter()
                .any(|context| context.id == contexts["13"] || context.id == contexts["14"])
        );
        assert_eq!(all.len(), 5); // Inbox, Project2, Work, Clients, Acme

        // A search pattern is plain text, never a regular expression
        assert!(search_contexts(&app, ".*").await.is_empty());
    }
}

mod destination {
    use super::*;
    use pretty_assertions::assert_eq;

    /// AC2: a destination that does not resolve is refused, nothing is created in Todoist
    #[rstest]
    #[case::unknown_context(None)]
    #[case::archived_context(Some("archived"))]
    #[case::another_users_context(Some("another user"))]
    #[tokio::test]
    async fn test_an_unresolved_destination_creates_nothing(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        todoist_oauth_credential: OAuthCredentialFixture,
        github_oauth_credential: OAuthCredentialFixture,
        #[case] context: Option<&str>,
    ) {
        let app = authenticated_app.await;
        let notification = notification_for_task(
            &app,
            &settings,
            &github_notification,
            github_oauth_credential,
        )
        .await;
        create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
            &settings,
            todoist_oauth_credential,
            None,
            None,
        )
        .await;
        mock_todoist_no_project_add(&app.app.todoist_mock_server).await;
        mock_todoist_no_item_add(&app).await;
        let context_id = match context {
            None => ContextId(Uuid::new_v4()),
            Some("archived") => {
                let mut project = todoist_project("13", "Old", None);
                project.is_archived = true;
                mirror_todoist_projects(&app.app, app.user.id, &[project]).await["13"]
            }
            Some(_) => {
                let (_, other_user) =
                    authenticate_user(&app.app, "5678", "Jane", "Doe", "jane@example.com").await;
                mirror_fixture_todoist_projects(&app.app, other_user.id).await["2222"]
            }
        };

        let response = create_task_from_notification_response(
            &app.client,
            &app.app.api_address,
            notification.id,
            Some(task_creation(Some(TaskDestination::Context(context_id)))),
        )
        .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    /// AC2: a Todoist task is never sent to a TickTick project
    #[rstest]
    #[tokio::test]
    async fn test_a_ticktick_destination_is_refused_for_todoist(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        todoist_oauth_credential: OAuthCredentialFixture,
        github_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;
        let notification = notification_for_task(
            &app,
            &settings,
            &github_notification,
            github_oauth_credential,
        )
        .await;
        create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
            &settings,
            todoist_oauth_credential,
            None,
            None,
        )
        .await;
        mock_todoist_no_project_add(&app.app.todoist_mock_server).await;
        mock_todoist_no_item_add(&app).await;

        let response = create_task_from_notification_response(
            &app.client,
            &app.app.api_address,
            notification.id,
            Some(task_creation(Some(TaskDestination::TickTickProject(
                "tt_proj_2222".into(),
            )))),
        )
        .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}

mod default_project {
    use super::*;
    use pretty_assertions::assert_eq;

    /// A default project saved from the settings is one the user can pick: their own active
    /// context. Another user's, an archived or an unknown context is refused, nothing is saved.
    #[rstest]
    #[case::unknown_context(None)]
    #[case::archived_context(Some("archived"))]
    #[case::another_users_context(Some("other user"))]
    #[tokio::test]
    async fn test_saving_a_default_project_checks_the_context(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        todoist_oauth_credential: OAuthCredentialFixture,
        #[case] context: Option<&str>,
    ) {
        let app = authenticated_app.await;
        let connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
            &settings,
            todoist_oauth_credential,
            None,
            None,
        )
        .await;
        let context_id = match context {
            None => ContextId(Uuid::new_v4()),
            Some("archived") => {
                let mut project = todoist_project("13", "Old", None);
                project.is_archived = true;
                mirror_todoist_projects(&app.app, app.user.id, &[project]).await["13"]
            }
            Some(_) => {
                let (_, other_user) =
                    authenticate_user(&app.app, "5678", "Jane", "Doe", "jane@example.com").await;
                mirror_fixture_todoist_projects(&app.app, other_user.id).await["2222"]
            }
        };

        let response = app
            .client
            .put(format!(
                "{}integration-connections/{}/config",
                app.app.api_address, connection.id
            ))
            .json(&IntegrationConnectionConfig::Todoist(TodoistConfig {
                default_project: Some(ProjectReference::Context { context_id }),
                ..TodoistConfig::enabled()
            }))
            .send()
            .await
            .expect("Failed to execute request");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let connections = list_integration_connections(&app.client, &app.app.api_address).await;
        assert_eq!(
            default_project_of(connection_of(
                &connections,
                IntegrationProviderKind::Todoist
            )),
            None
        );
    }

    /// AC4: the stored defaults move to contexts after a Todoist sync, by project id first, then
    /// by a unique name; a rerun changes nothing
    #[rstest]
    #[tokio::test]
    async fn test_the_context_sync_migrates_the_stored_defaults(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        sync_todoist_projects_response: TodoistSyncResponse,
        todoist_oauth_credential: OAuthCredentialFixture,
        linear_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;
        create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Todoist(TodoistConfig {
                default_project: Some(ProjectReference::Project(ProjectSummary {
                    source_id: "2222".into(),
                    name: "Former name of Project2".to_string(),
                })),
                ..TodoistConfig::enabled()
            }),
            &settings,
            todoist_oauth_credential,
            None,
            None,
        )
        .await;
        // No such Todoist project id, but a unique project of that name
        create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Linear(LinearConfig {
                sync_notifications_enabled: true,
                sync_task_config: LinearSyncTaskConfig {
                    enabled: true,
                    target_project: Some(ProjectReference::Project(ProjectSummary {
                        source_id: "unknown-id".into(),
                        name: "Project2".to_string(),
                    })),
                    ..Default::default()
                },
            }),
            &settings,
            linear_oauth_credential,
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
        mock_todoist_no_project_add(&app.app.todoist_mock_server).await;

        sync_todoist_contexts(&app).await;

        let contexts = mirror_fixture_todoist_projects(&app.app, app.user.id).await;
        let expected = Some(ProjectReference::Context {
            context_id: contexts["2222"],
        });
        let connections = list_integration_connections(&app.client, &app.app.api_address).await;
        for kind in [
            IntegrationProviderKind::Todoist,
            IntegrationProviderKind::Linear,
        ] {
            let connection = connection_of(&connections, kind);
            assert_eq!(default_project_of(connection), expected, "{kind}");
            assert_eq!(connection.default_project_issue, None, "{kind}");
        }

        sync_todoist_contexts(&app).await;

        assert_eq!(
            list_integration_connections(&app.client, &app.app.api_address)
                .await
                .iter()
                .map(|connection| (connection.id, connection.provider.config()))
                .collect::<Vec<_>>(),
            connections
                .iter()
                .map(|connection| (connection.id, connection.provider.config()))
                .collect::<Vec<_>>()
        );
    }

    /// AC4: a stored default that resolves to no project becomes a settings error, and a task
    /// created with it fails: it is not put in the inbox, no project is created
    #[rstest]
    #[tokio::test]
    async fn test_an_unresolvable_default_is_a_settings_error(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        sync_todoist_projects_response: TodoistSyncResponse,
        todoist_oauth_credential: OAuthCredentialFixture,
        github_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;
        let notification = notification_for_task(
            &app,
            &settings,
            &github_notification,
            github_oauth_credential,
        )
        .await;
        let gone = ProjectSummary {
            source_id: "gone-id".into(),
            name: "Gone project".to_string(),
        };
        create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Todoist(TodoistConfig {
                default_project: Some(ProjectReference::Project(gone.clone())),
                ..TodoistConfig::enabled()
            }),
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
        mock_todoist_no_project_add(&app.app.todoist_mock_server).await;
        mock_todoist_no_item_add(&app).await;

        sync_todoist_contexts(&app).await;

        let connections = list_integration_connections(&app.client, &app.app.api_address).await;
        let todoist = connection_of(&connections, IntegrationProviderKind::Todoist);
        assert_eq!(
            default_project_of(todoist),
            Some(ProjectReference::Unresolved { unresolved: gone })
        );
        assert_eq!(
            todoist.default_project_issue,
            Some(DefaultProjectIssue::Missing {
                provider: IntegrationProviderKind::Todoist,
                name: Some("Gone project".to_string()),
            })
        );

        let response = create_task_from_notification_response(
            &app.client,
            &app.app.api_address,
            notification.id,
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    /// AC4 (lead decision): a default bound to an archived project fails the same way, with
    /// its own settings error
    #[rstest]
    #[tokio::test]
    async fn test_an_archived_default_is_a_settings_error(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        sync_todoist_projects_response: TodoistSyncResponse,
        todoist_oauth_credential: OAuthCredentialFixture,
        github_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;
        let notification = notification_for_task(
            &app,
            &settings,
            &github_notification,
            github_oauth_credential,
        )
        .await;
        let contexts = mirror_fixture_todoist_projects(&app.app, app.user.id).await;
        create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Todoist(TodoistConfig {
                default_project: Some(ProjectReference::Context {
                    context_id: contexts["2222"],
                }),
                ..TodoistConfig::enabled()
            }),
            &settings,
            todoist_oauth_credential,
            None,
            None,
        )
        .await;
        // Project2 archived in Todoist
        let mut projects = sync_todoist_projects_response.projects.clone().unwrap();
        for project in projects.iter_mut() {
            if project.id == "2222" {
                project.is_archived = true;
            }
        }
        mirror_todoist_projects(&app.app, app.user.id, &projects).await;
        mock_todoist_no_project_add(&app.app.todoist_mock_server).await;
        mock_todoist_no_item_add(&app).await;

        let connections = list_integration_connections(&app.client, &app.app.api_address).await;
        assert_eq!(
            connection_of(&connections, IntegrationProviderKind::Todoist).default_project_issue,
            Some(DefaultProjectIssue::Archived {
                provider: IntegrationProviderKind::Todoist,
                name: "Project2".to_string(),
            })
        );

        let response = create_task_from_notification_response(
            &app.client,
            &app.app.api_address,
            notification.id,
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}

mod backfill {
    use super::*;
    use pretty_assertions::assert_eq;

    use graphql_client::Response;
    use universal_inbox::third_party::integrations::linear::LinearIssue;
    use universal_inbox_api::integrations::linear::graphql::assigned_issues_query;

    use crate::helpers::{
        notification::linear::sync_linear_tasks_response, task::linear::create_linear_task,
    };

    /// A Linear task synced to Todoist before tasks were bound to contexts gets bound by the
    /// context sync, through its Todoist sink item: an incremental sync never rewrites an
    /// unchanged sink, so the backfill is its only repair
    #[rstest]
    #[tokio::test]
    async fn test_the_context_sync_binds_a_task_living_in_todoist_through_its_sink(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        sync_todoist_projects_response: TodoistSyncResponse,
        sync_linear_tasks_response: Response<assigned_issues_query::ResponseData>,
        todoist_oauth_credential: OAuthCredentialFixture,
        linear_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;
        let todoist_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
            &settings,
            todoist_oauth_credential,
            None,
            None,
        )
        .await;
        let linear_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Linear(LinearConfig {
                sync_notifications_enabled: true,
                sync_task_config: LinearSyncTaskConfig {
                    enabled: true,
                    ..Default::default()
                },
            }),
            &settings,
            linear_oauth_credential,
            None,
            None,
        )
        .await;
        let linear_issues: Vec<LinearIssue> = sync_linear_tasks_response
            .data
            .clone()
            .unwrap()
            .try_into()
            .unwrap();
        let task = create_linear_task(
            &app.app,
            &linear_issues[0],
            ProjectSummary {
                source_id: "2222".into(),
                name: "Project2".to_string(),
            },
            app.user.id,
            linear_connection.id,
            todoist_connection.id,
            "todoist_source_id".to_string(),
        )
        .await;
        assert_eq!(task.kind, TaskSourceKind::Linear);
        assert_eq!(task.context_id, None);
        mock_todoist_sync_resources_service(
            &app.app.todoist_mock_server,
            "projects",
            &sync_todoist_projects_response,
            None,
        )
        .await;
        mock_todoist_no_project_add(&app.app.todoist_mock_server).await;

        sync_todoist_contexts(&app).await;

        let contexts = mirror_fixture_todoist_projects(&app.app, app.user.id).await;
        let bound: Box<Task> =
            get_resource(&app.client, &app.app.api_address, "tasks", task.id.into()).await;
        assert_eq!(bound.context_id, Some(contexts["2222"]));

        // A rerun changes nothing
        sync_todoist_contexts(&app).await;
        let rerun: Box<Task> =
            get_resource(&app.client, &app.app.api_address, "tasks", task.id.into()).await;
        assert_eq!(rerun, bound);
    }
}
