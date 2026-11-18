//! TickTick mirrored like Todoist, one level up (qk5d.11): folders and lists as contexts, tasks
//! bound to their list and nested by `parentId`, and lists created in a folder. Every TickTick
//! call is a wiremock: the folder, list creation and nesting endpoints are undocumented upstream.

use graphql_client::Response;
use http::StatusCode;
use rstest::{fixture, rstest};
use serde_json::{Value, json};
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{body_partial_json, method, path},
};

use universal_inbox::{
    HasHtmlUrl,
    context::{Context, ContextBinding, ContextSummary},
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::{
            github::GithubConfig,
            linear::{LinearConfig, LinearSyncTaskConfig},
            ticktick::TickTickConfig,
        },
        provider::IntegrationProviderKind,
    },
    notification::Notification,
    task::{
        ProjectReference, ProjectSummary, Task, TaskCreation, TaskCreationResult, TaskDestination,
        TaskId, TaskPriority, TaskStatus, TaskSyncSourceKind,
        integrations::ticktick::TickTickProject,
    },
    third_party::{
        integrations::{
            github::GithubNotification,
            linear::LinearIssue,
            ticktick::{TickTickItem, TickTickProjectGroup},
        },
        item::ThirdPartyItemData,
    },
};

use universal_inbox_api::{
    configuration::Settings, integrations::linear::graphql::assigned_issues_query,
    repository::context::ContextRepository,
    universal_inbox::audit::actor_context::with_system_actor,
};

use crate::helpers::{
    auth::{AuthenticatedApp, authenticated_app},
    integration_connection::{
        OAuthCredentialFixture, create_and_mock_integration_connection,
        create_ticktick_integration_connection, github_oauth_credential, linear_oauth_credential,
    },
    notification::{
        create_task_from_notification_response,
        github::{create_notification_from_github_notification, github_notification},
        linear::sync_linear_tasks_response,
    },
    rest::get_resource,
    settings,
    task::{
        linear::create_linear_task_with_ticktick_sink,
        ticktick::{
            mock_ticktick_inbox_data_service, mock_ticktick_list_projects_service,
            mock_ticktick_list_tasks_service, ticktick_item,
        },
    },
};

const LIST_LIMIT_MESSAGE: &str = "Couldn’t create ‘Trips’ in TickTick, so nothing was created \
     here. Your TickTick plan may have reached its list limit: if so, remove a list in TickTick \
     first. Otherwise, please try again.";

/// Folder `Personal` (`f1`) holding the list `Errands` (`l1`), and the list `Universal Inbox`
/// (`l2`) with no folder
fn personal_lists() -> Vec<TickTickProject> {
    vec![
        list("l1", "Errands", Some("f1")),
        list("l2", "Universal Inbox", None),
    ]
}

fn personal_folders() -> Vec<TickTickProjectGroup> {
    vec![TickTickProjectGroup {
        id: "f1".to_string(),
        name: "Personal".to_string(),
    }]
}

fn list(id: &str, name: &str, group_id: Option<&str>) -> TickTickProject {
    TickTickProject {
        id: id.to_string(),
        name: name.to_string(),
        color: None,
        group_id: group_id.map(str::to_string),
        sort_order: None,
        view_mode: None,
        closed: None,
    }
}

#[fixture]
fn task_template() -> TickTickItem {
    *ticktick_item()
}

fn item(template: &TickTickItem, id: &str, project_id: &str, parent: Option<&str>) -> TickTickItem {
    TickTickItem {
        id: id.to_string(),
        project_id: project_id.to_string(),
        title: format!("Task {id}"),
        parent_id: parent.map(str::to_string),
        ..template.clone()
    }
}

async fn ticktick_user(settings: &Settings, app: AuthenticatedApp) -> AuthenticatedApp {
    create_ticktick_integration_connection(
        &app.app,
        app.user.id,
        settings,
        IntegrationConnectionConfig::TickTick(TickTickConfig::enabled()),
        None,
    )
    .await;
    app
}

/// TickTick answers `GET /project/group` with `response`
async fn mock_folders(app: &AuthenticatedApp, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path("/project/group"))
        .respond_with(response)
        .mount(&app.app.ticktick_mock_server)
        .await;
}

/// The lists, folders and tasks TickTick serves, replacing whatever it served before
async fn serve_ticktick(
    app: &AuthenticatedApp,
    lists: &[TickTickProject],
    folders: ResponseTemplate,
    inbox: &[TickTickItem],
    tasks: &[TickTickItem],
) {
    let server = &app.app.ticktick_mock_server;
    server.reset().await;
    mock_ticktick_list_projects_service(server, lists).await;
    mock_folders(app, folders).await;
    mock_ticktick_inbox_data_service(server, inbox).await;
    for list in lists {
        let list_tasks: Vec<TickTickItem> = tasks
            .iter()
            .filter(|task| task.project_id == list.id)
            .cloned()
            .collect();
        mock_ticktick_list_tasks_service(server, &list.id, &list_tasks).await;
    }
}

fn folders_ok(folders: &[TickTickProjectGroup]) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(folders)
}

/// A full TickTick sync, forced past the minimum interval so a test can sync again
async fn sync(app: &AuthenticatedApp) -> Vec<TaskCreationResult> {
    app.app
        .task_service
        .read()
        .await
        .sync_tasks_with_transaction(TaskSyncSourceKind::TickTick, app.user.id, true)
        .await
        .unwrap()
}

/// Run the TickTick context sync (folders and lists mirror, task backfill) as the sync job does
async fn sync_ticktick_contexts(app: &AuthenticatedApp) {
    let task_service = app.app.task_service.read().await;
    with_system_actor(
        "SyncTasks",
        Some(IntegrationProviderKind::TickTick),
        task_service.sync_project_contexts(app.user.id, IntegrationProviderKind::TickTick),
    )
    .await
    .unwrap();
}

async fn stored_contexts(app: &AuthenticatedApp) -> Vec<Context> {
    let mut transaction = app.app.repository.begin().await.unwrap();
    app.app
        .repository
        .list_contexts_for_user(&mut transaction, app.user.id)
        .await
        .unwrap()
}

fn bound_to<'a>(contexts: &'a [Context], source_id: &str) -> &'a Context {
    contexts
        .iter()
        .find(|context| {
            context
                .binding
                .as_ref()
                .is_some_and(|binding| binding.source_id.0 == source_id)
        })
        .unwrap_or_else(|| panic!("No context bound to {source_id}"))
}

/// The task synced from `source_id`, read back through the API
async fn task_of(app: &AuthenticatedApp, results: &[TaskCreationResult], source_id: &str) -> Task {
    let task_id = results
        .iter()
        .find(|result| result.task.source_item.source_id == source_id)
        .unwrap_or_else(|| panic!("No task synced from {source_id}"))
        .task
        .id;
    *get_resource::<Task>(&app.client, &app.app.api_address, "tasks", task_id.into()).await
}

/// The parent of the task synced from `source_id`, as the sync returned it
fn returned_parent(results: &[TaskCreationResult], source_id: &str) -> Option<TaskId> {
    results
        .iter()
        .find(|result| result.task.source_item.source_id == source_id)
        .unwrap_or_else(|| panic!("No task synced from {source_id}"))
        .task
        .parent_id
}

async fn post_context(app: &AuthenticatedApp, body: Value) -> reqwest::Response {
    app.client
        .post(format!("{}contexts", app.app.api_address))
        .json(&body)
        .send()
        .await
        .unwrap()
}

/// The status and the user-facing message of an error response
async fn error_of(response: reqwest::Response) -> (StatusCode, String) {
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    (status, body["message"].as_str().unwrap().to_string())
}

/// `POST /project` (a list creation) is never called
async fn mock_no_list_creation(app: &AuthenticatedApp) {
    Mock::given(method("POST"))
        .and(path("/project"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&app.app.ticktick_mock_server)
        .await;
}

mod mirror {
    use super::*;
    use pretty_assertions::assert_eq;

    /// A folder is a context named after it holding its lists, a list with no folder sits at
    /// the root, the inbox is the inbox context, and every task is bound to its list
    #[rstest]
    #[tokio::test]
    async fn test_folders_and_lists_are_mirrored_and_tasks_bound_to_their_list(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        task_template: TickTickItem,
    ) {
        let app = ticktick_user(&settings, authenticated_app.await).await;
        let inbox_task = item(&task_template, "t_inbox", "inbox1111", None);
        let errand = item(&task_template, "t_errand", "l1", None);
        serve_ticktick(
            &app,
            &personal_lists(),
            folders_ok(&personal_folders()),
            std::slice::from_ref(&inbox_task),
            std::slice::from_ref(&errand),
        )
        .await;

        let results = sync(&app).await;

        let contexts = stored_contexts(&app).await;
        let personal = bound_to(&contexts, "group:f1");
        let errands = bound_to(&contexts, "l1");
        let universal_inbox = bound_to(&contexts, "l2");
        let inbox = bound_to(&contexts, "inbox");
        assert_eq!(
            (personal.name.as_str(), personal.parent_id),
            ("Personal", None)
        );
        assert_eq!(
            (errands.name.as_str(), errands.parent_id),
            ("Errands", Some(personal.id))
        );
        assert_eq!(universal_inbox.parent_id, None);
        assert!(inbox.is_inbox);
        assert!(personal.is_ticktick_folder() && errands.is_ticktick_list());

        assert_eq!(
            task_of(&app, &results, "t_errand").await.context_id,
            Some(errands.id)
        );
        assert_eq!(
            task_of(&app, &results, "t_inbox").await.context_id,
            Some(inbox.id)
        );
    }

    /// Subtasks nest under their parent two levels deep whatever order TickTick lists them in;
    /// a parent that is not synced leaves the task at the top level
    #[rstest]
    #[tokio::test]
    async fn test_subtasks_nest_under_their_parent_in_any_order(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        task_template: TickTickItem,
    ) {
        let app = ticktick_user(&settings, authenticated_app.await).await;
        let tasks = vec![
            item(&task_template, "t_grandchild", "l1", Some("t_child")),
            item(&task_template, "t_child", "l1", Some("t_parent")),
            item(&task_template, "t_parent", "l1", None),
            item(&task_template, "t_orphan", "l1", Some("t_gone")),
        ];
        serve_ticktick(
            &app,
            &personal_lists(),
            folders_ok(&personal_folders()),
            &[],
            &tasks,
        )
        .await;

        let results = sync(&app).await;

        let parent = task_of(&app, &results, "t_parent").await;
        let child = task_of(&app, &results, "t_child").await;
        let grandchild = task_of(&app, &results, "t_grandchild").await;
        assert_eq!(parent.parent_id, None);
        assert_eq!(child.parent_id, Some(parent.id));
        assert_eq!(grandchild.parent_id, Some(child.id));
        assert_eq!(task_of(&app, &results, "t_orphan").await.parent_id, None);
        // The sync response carries the stored parents, not the ones before the nest pass
        assert_eq!(returned_parent(&results, "t_child"), Some(parent.id));
        assert_eq!(returned_parent(&results, "t_grandchild"), Some(child.id));

        // A subtask moved to the top level in TickTick is adopted on the next sync
        let tasks = vec![
            item(&task_template, "t_grandchild", "l1", None),
            item(&task_template, "t_child", "l1", Some("t_parent")),
            item(&task_template, "t_parent", "l1", None),
        ];
        serve_ticktick(
            &app,
            &personal_lists(),
            folders_ok(&personal_folders()),
            &[],
            &tasks,
        )
        .await;
        let results = sync(&app).await;
        assert_eq!(
            task_of(&app, &results, "t_grandchild").await.parent_id,
            None
        );
        assert_eq!(
            task_of(&app, &results, "t_child").await.parent_id,
            Some(parent.id)
        );
    }

    /// A TickTick subtask of a Linear task synced to TickTick nests under that Linear task: the
    /// parent is found by its TickTick item, which is the Linear task's sink
    #[rstest]
    #[tokio::test]
    async fn test_a_subtask_of_a_task_synced_to_ticktick_nests_under_it(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        sync_linear_tasks_response: Response<assigned_issues_query::ResponseData>,
        linear_oauth_credential: OAuthCredentialFixture,
        task_template: TickTickItem,
    ) {
        let app = authenticated_app.await;
        let ticktick_connection = create_ticktick_integration_connection(
            &app.app,
            app.user.id,
            &settings,
            IntegrationConnectionConfig::TickTick(TickTickConfig::enabled()),
            None,
        )
        .await;
        let errands = ProjectSummary {
            name: "Errands".to_string(),
            source_id: "l1".into(),
        };
        let linear_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Linear(LinearConfig {
                sync_notifications_enabled: true,
                sync_task_config: LinearSyncTaskConfig {
                    enabled: true,
                    target_project: Some(ProjectReference::Project(errands.clone())),
                    task_manager_provider_kind: Some(IntegrationProviderKind::TickTick),
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
        let linear_task = create_linear_task_with_ticktick_sink(
            &app.app,
            &linear_issues[0],
            errands,
            app.user.id,
            linear_connection.id,
            ticktick_connection.id,
            "tt_linear_sink".to_string(),
            "l1".to_string(),
        )
        .await;
        let ThirdPartyItemData::TickTickItem(sink_item) =
            linear_task.sink_item.as_ref().unwrap().data.clone()
        else {
            panic!("Expected the sink item to be a TickTick item");
        };
        let subtask = item(&task_template, "t_sub", "l1", Some("tt_linear_sink"));
        serve_ticktick(
            &app,
            &personal_lists(),
            folders_ok(&personal_folders()),
            &[],
            &[*sink_item, subtask],
        )
        .await;

        let results = sync(&app).await;

        assert_eq!(returned_parent(&results, "t_sub"), Some(linear_task.id));
        assert_eq!(
            task_of(&app, &results, "t_sub").await.parent_id,
            Some(linear_task.id)
        );
    }

    /// The folders endpoint is undocumented: when it fails, the lists keep the folder they were
    /// mirrored in and the sync still succeeds
    #[rstest]
    #[tokio::test]
    async fn test_a_folder_read_failure_keeps_the_mirrored_tree(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        task_template: TickTickItem,
    ) {
        let app = ticktick_user(&settings, authenticated_app.await).await;
        let errand = item(&task_template, "t_errand", "l1", None);
        serve_ticktick(
            &app,
            &personal_lists(),
            folders_ok(&personal_folders()),
            &[],
            std::slice::from_ref(&errand),
        )
        .await;
        sync(&app).await;
        let before = stored_contexts(&app).await;

        serve_ticktick(
            &app,
            &personal_lists(),
            ResponseTemplate::new(404),
            &[],
            std::slice::from_ref(&errand),
        )
        .await;
        let results = sync(&app).await;

        assert_eq!(results.len(), 1);
        let after = stored_contexts(&app).await;
        let parent_of =
            |contexts: &[Context], source_id: &str| bound_to(contexts, source_id).parent_id;
        assert_eq!(parent_of(&after, "l1"), parent_of(&before, "l1"));
        assert_eq!(
            parent_of(&after, "l1"),
            Some(bound_to(&after, "group:f1").id)
        );
        assert_eq!(bound_to(&after, "group:f1").name, "Personal");
    }

    /// With no folder ever mirrored, a failing folder read mirrors the lists flat
    #[rstest]
    #[tokio::test]
    async fn test_a_folder_read_failure_on_the_first_sync_mirrors_flat_lists(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = ticktick_user(&settings, authenticated_app.await).await;
        serve_ticktick(
            &app,
            &personal_lists(),
            ResponseTemplate::new(500),
            &[],
            &[],
        )
        .await;

        sync(&app).await;

        let contexts = stored_contexts(&app).await;
        assert_eq!(bound_to(&contexts, "l1").parent_id, None);
        assert_eq!(bound_to(&contexts, "l2").parent_id, None);
        assert!(!contexts.iter().any(|context| context.is_ticktick_folder()));
    }

    /// TickTick's delete is soft: a trashed task still reads by id but drops out of its list's
    /// data, which is what the sync reads. Like a completed task, it then leaves the active tasks
    /// (marked done: the list data does not tell a trashed task from a completed one).
    #[rstest]
    #[tokio::test]
    async fn test_a_task_gone_from_its_list_data_leaves_the_active_tasks(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        task_template: TickTickItem,
    ) {
        let app = ticktick_user(&settings, authenticated_app.await).await;
        let kept = item(&task_template, "t_kept", "l1", None);
        let trashed = item(&task_template, "t_trashed", "l1", None);
        serve_ticktick(
            &app,
            &personal_lists(),
            folders_ok(&personal_folders()),
            &[],
            &[kept.clone(), trashed.clone()],
        )
        .await;
        let results = sync(&app).await;
        let trashed_task = task_of(&app, &results, "t_trashed").await;

        serve_ticktick(
            &app,
            &personal_lists(),
            folders_ok(&personal_folders()),
            &[],
            std::slice::from_ref(&kept),
        )
        .await;
        sync(&app).await;

        let trashed_task: Box<Task> = get_resource(
            &app.client,
            &app.app.api_address,
            "tasks",
            trashed_task.id.into(),
        )
        .await;
        assert_eq!(trashed_task.status, TaskStatus::Done);
    }

    /// The context search lists the TickTick lists a task can go to, never a folder
    #[rstest]
    #[tokio::test]
    async fn test_the_ticktick_context_search_lists_lists_only(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = ticktick_user(&settings, authenticated_app.await).await;
        serve_ticktick(
            &app,
            &personal_lists(),
            folders_ok(&personal_folders()),
            &[],
            &[],
        )
        .await;
        sync_ticktick_contexts(&app).await;

        let search = |provider_kind: &'static str| {
            let app = &app;
            async move {
                app.client
                    .get(format!(
                        "{}contexts/search?matches=&provider_kind={provider_kind}",
                        app.app.api_address
                    ))
                    .send()
                    .await
                    .unwrap()
                    .json::<Vec<ContextSummary>>()
                    .await
                    .unwrap()
            }
        };

        let names: Vec<String> = search("TickTick")
            .await
            .into_iter()
            .map(|summary| summary.display_path())
            .collect();
        assert_eq!(
            names,
            vec![
                "Inbox".to_string(),
                "Personal / Errands".to_string(),
                "Universal Inbox".to_string()
            ]
        );
        assert!(search("Todoist").await.is_empty());
    }
}

mod creation {
    use super::*;
    use pretty_assertions::assert_eq;

    async fn mirrored_user(settings: &Settings, app: AuthenticatedApp) -> AuthenticatedApp {
        let app = ticktick_user(settings, app).await;
        serve_ticktick(
            &app,
            &personal_lists(),
            folders_ok(&personal_folders()),
            &[],
            &[],
        )
        .await;
        sync_ticktick_contexts(&app).await;
        app
    }

    /// A new context under a folder is a list created in that folder, once, then bound to it
    #[rstest]
    #[tokio::test]
    async fn test_creating_under_a_folder_creates_one_list_in_it(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = mirrored_user(&settings, authenticated_app.await).await;
        let personal = bound_to(&stored_contexts(&app).await, "group:f1").id;
        Mock::given(method("POST"))
            .and(path("/project"))
            .and(body_partial_json(
                json!({ "name": "Trips", "groupId": "f1" }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(list("l9", "Trips", Some("f1"))))
            .expect(1)
            .mount(&app.app.ticktick_mock_server)
            .await;

        let response = post_context(&app, json!({ "name": "Trips", "parent_id": personal })).await;

        assert_eq!(response.status(), StatusCode::CREATED);
        let context: Context = response.json().await.unwrap();
        assert_eq!(context.parent_id, Some(personal));
        assert_eq!(
            context.binding,
            Some(
                ContextBinding::new(IntegrationProviderKind::TickTick, "l9".into(), true).unwrap()
            )
        );

        // The next sync finds it where it was created
        let mut lists = personal_lists();
        lists.push(list("l9", "Trips", Some("f1")));
        serve_ticktick(&app, &lists, folders_ok(&personal_folders()), &[], &[]).await;
        sync_ticktick_contexts(&app).await;
        let contexts = stored_contexts(&app).await;
        let trips = bound_to(&contexts, "l9");
        assert_eq!((trips.id, trips.parent_id), (context.id, Some(personal)));
    }

    /// A list just created is found right away by a project lookup, not once the cached
    /// projects expire
    #[rstest]
    #[tokio::test]
    async fn test_a_created_list_is_found_at_once(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = mirrored_user(&settings, authenticated_app.await).await;
        let personal = bound_to(&stored_contexts(&app).await, "group:f1").id;
        Mock::given(method("POST"))
            .and(path("/project"))
            .respond_with(ResponseTemplate::new(200).set_body_json(list("l9", "Trips", Some("f1"))))
            .mount(&app.app.ticktick_mock_server)
            .await;
        let mut lists = personal_lists();
        lists.push(list("l9", "Trips", Some("f1")));
        // TickTick lists the new list from now on; the cached projects do not have it yet
        Mock::given(method("GET"))
            .and(path("/project"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&lists))
            .with_priority(1)
            .mount(&app.app.ticktick_mock_server)
            .await;

        let response = post_context(&app, json!({ "name": "Trips", "parent_id": personal })).await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let found: Vec<ProjectSummary> = app
            .client
            .get(format!(
                "{}tasks/projects/search?matches=Trips&provider_kind=TickTick",
                app.app.api_address
            ))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(
            found,
            vec![ProjectSummary {
                source_id: "l9".into(),
                name: "Trips".to_string(),
            }]
        );
    }

    /// TickTick's answer to a list creation at the plan's list limit is a bare server error: it
    /// is sent once, never retried, says the limit may be reached, and leaves no context
    #[rstest]
    #[tokio::test]
    async fn test_a_list_creation_at_the_list_limit_leaves_no_context(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = mirrored_user(&settings, authenticated_app.await).await;
        let before = stored_contexts(&app).await;
        let personal = bound_to(&before, "group:f1").id;
        Mock::given(method("POST"))
            .and(path("/project"))
            .respond_with(
                ResponseTemplate::new(500)
                    .set_body_json(json!({ "errorCode": "unknown_exception" })),
            )
            .expect(1)
            .mount(&app.app.ticktick_mock_server)
            .await;

        let response = post_context(&app, json!({ "name": "Trips", "parent_id": personal })).await;

        assert_eq!(
            error_of(response).await,
            (StatusCode::BAD_GATEWAY, LIST_LIMIT_MESSAGE.to_string())
        );
        assert_eq!(stored_contexts(&app).await, before);
    }

    /// Any other failure keeps the generic message
    #[rstest]
    #[tokio::test]
    async fn test_another_list_creation_failure_keeps_the_generic_message(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = mirrored_user(&settings, authenticated_app.await).await;
        let personal = bound_to(&stored_contexts(&app).await, "group:f1").id;
        Mock::given(method("POST"))
            .and(path("/project"))
            .respond_with(ResponseTemplate::new(400))
            .mount(&app.app.ticktick_mock_server)
            .await;

        let response = post_context(&app, json!({ "name": "Trips", "parent_id": personal })).await;

        assert_eq!(
            error_of(response).await,
            (
                StatusCode::BAD_GATEWAY,
                "Couldn’t create ‘Trips’ in TickTick, so nothing was created here. Please try \
                 again."
                    .to_string()
            )
        );
    }

    /// TickTick lists do not nest: refused before any TickTick call, pointing to the folder
    #[rstest]
    #[case::in_a_folder(
        "l1",
        "‘Errands’ is a TickTick list, and TickTick lists can’t hold other lists. Create the new \
         list under its folder ‘Personal’ instead."
    )]
    #[case::without_a_folder(
        "l2",
        "‘Universal Inbox’ is a TickTick list, and TickTick lists can’t hold other lists. Create \
         the new list under a TickTick folder, or create it in TickTick and it will show here \
         after the next sync."
    )]
    #[tokio::test]
    async fn test_a_list_takes_no_child(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        #[case] list_id: &str,
        #[case] message: &str,
    ) {
        let app = mirrored_user(&settings, authenticated_app.await).await;
        mock_no_list_creation(&app).await;
        let before = stored_contexts(&app).await;
        let parent = bound_to(&before, list_id).id;

        let response = post_context(&app, json!({ "name": "Trips", "parent_id": parent })).await;

        assert_eq!(
            error_of(response).await,
            (StatusCode::CONFLICT, message.to_string())
        );
        assert_eq!(stored_contexts(&app).await, before);
    }

    /// A blank name never reaches TickTick
    #[rstest]
    #[tokio::test]
    async fn test_a_blank_list_name_is_refused_before_any_call(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = mirrored_user(&settings, authenticated_app.await).await;
        mock_no_list_creation(&app).await;
        let personal = bound_to(&stored_contexts(&app).await, "group:f1").id;

        let response = post_context(&app, json!({ "name": "  ", "parent_id": personal })).await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}

mod destination {
    use super::*;
    use pretty_assertions::assert_eq;

    /// A TickTick user with the lists mirrored, and a GitHub notification to create a task from
    async fn user_with_notification(
        settings: &Settings,
        app: AuthenticatedApp,
        github_notification: &GithubNotification,
        github_oauth_credential: OAuthCredentialFixture,
    ) -> (AuthenticatedApp, Box<Notification>) {
        let app = ticktick_user(settings, app).await;
        let github_connection = create_and_mock_integration_connection(
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
        let notification = create_notification_from_github_notification(
            &app.app,
            github_notification,
            app.user.id,
            github_connection.id,
        )
        .await;
        (app, notification)
    }

    fn task_creation(destination: Option<TaskDestination>) -> TaskCreation {
        TaskCreation {
            title: "Pack".to_string(),
            body: None,
            destination,
            due_at: None,
            priority: TaskPriority::P4,
            task_provider_kind: Some(IntegrationProviderKind::TickTick),
            time_config: None,
            parent_id: None,
        }
    }

    /// TickTick answers `POST /task` whose body matches `expected` with `created`, exactly once
    async fn mock_task_creation(app: &AuthenticatedApp, expected: Value, created: &TickTickItem) {
        Mock::given(method("POST"))
            .and(path("/task"))
            .and(body_partial_json(expected))
            .respond_with(ResponseTemplate::new(200).set_body_json(created))
            .expect(1)
            .mount(&app.app.ticktick_mock_server)
            .await;
    }

    async fn created_task(response: reqwest::Response) -> Task {
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await.unwrap();
        serde_json::from_value(body["task"].clone()).unwrap()
    }

    /// A task created in a list context goes to that list, by its id
    #[rstest]
    #[tokio::test]
    async fn test_a_task_created_in_a_list_context_goes_to_its_list(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        github_oauth_credential: OAuthCredentialFixture,
        task_template: TickTickItem,
    ) {
        let (app, notification) = user_with_notification(
            &settings,
            authenticated_app.await,
            &github_notification,
            github_oauth_credential,
        )
        .await;
        serve_ticktick(
            &app,
            &personal_lists(),
            folders_ok(&personal_folders()),
            &[],
            &[],
        )
        .await;
        sync_ticktick_contexts(&app).await;
        let errands = bound_to(&stored_contexts(&app).await, "l1").id;
        let created = item(&task_template, "t_new", "l1", None);
        mock_task_creation(
            &app,
            json!({ "title": "Pack", "projectId": "l1" }),
            &created,
        )
        .await;

        let task = created_task(
            create_task_from_notification_response(
                &app.client,
                &app.app.api_address,
                notification.id,
                Some(task_creation(Some(TaskDestination::Context(errands)))),
            )
            .await,
        )
        .await;

        assert_eq!(task.context_id, Some(errands));
    }

    /// A folder holds lists, not tasks: refused before any TickTick call
    #[rstest]
    #[tokio::test]
    async fn test_a_folder_context_takes_no_task(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        github_oauth_credential: OAuthCredentialFixture,
    ) {
        let (app, notification) = user_with_notification(
            &settings,
            authenticated_app.await,
            &github_notification,
            github_oauth_credential,
        )
        .await;
        serve_ticktick(
            &app,
            &personal_lists(),
            folders_ok(&personal_folders()),
            &[],
            &[],
        )
        .await;
        sync_ticktick_contexts(&app).await;
        let personal = bound_to(&stored_contexts(&app).await, "group:f1").id;
        Mock::given(method("POST"))
            .and(path("/task"))
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&app.app.ticktick_mock_server)
            .await;

        let response = create_task_from_notification_response(
            &app.client,
            &app.app.api_address,
            notification.id,
            Some(task_creation(Some(TaskDestination::Context(personal)))),
        )
        .await;

        assert_eq!(
            error_of(response).await,
            (
                StatusCode::BAD_REQUEST,
                "‘Personal’ is a TickTick folder, which holds lists, not tasks. Choose one of its \
                 lists."
                    .to_string()
            )
        );
    }

    /// A task created under another one is written with its `parentId`, in its parent's list,
    /// and reads back nested
    #[rstest]
    #[tokio::test]
    async fn test_a_task_created_under_a_parent_is_nested(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        github_oauth_credential: OAuthCredentialFixture,
        task_template: TickTickItem,
    ) {
        let (app, notification) = user_with_notification(
            &settings,
            authenticated_app.await,
            &github_notification,
            github_oauth_credential,
        )
        .await;
        let parent_item = item(&task_template, "t_parent", "l1", None);
        serve_ticktick(
            &app,
            &personal_lists(),
            folders_ok(&personal_folders()),
            &[],
            std::slice::from_ref(&parent_item),
        )
        .await;
        let results = sync(&app).await;
        let parent = task_of(&app, &results, "t_parent").await;
        let created = item(&task_template, "t_sub", "l1", Some("t_parent"));
        mock_task_creation(
            &app,
            json!({ "title": "Pack", "projectId": "l1", "parentId": "t_parent" }),
            &created,
        )
        .await;

        let task = created_task(
            create_task_from_notification_response(
                &app.client,
                &app.app.api_address,
                notification.id,
                Some(TaskCreation {
                    parent_id: Some(parent.id),
                    ..task_creation(None)
                }),
            )
            .await,
        )
        .await;

        assert_eq!(task.parent_id, Some(parent.id));
        let read_back: Box<Task> =
            get_resource(&app.client, &app.app.api_address, "tasks", task.id.into()).await;
        assert_eq!(read_back.parent_id, Some(parent.id));
        assert_eq!(read_back.context_id, parent.context_id);
        assert!(read_back.get_html_url().as_str().contains("t_sub"));
    }
}
