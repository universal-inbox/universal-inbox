//! Context writes under their ownership (qk5d.2.5): Todoist owns the name and the parent of a
//! mirrored context, Universal Inbox those of the others, and a context created under a mirrored
//! parent exists in Todoist and here, or in neither.

use std::collections::HashMap;

use http::StatusCode;
use rstest::*;
use serde_json::{Value, json};
use uuid::Uuid;
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{body_partial_json, method, path},
};

use universal_inbox::{
    context::{Context, ContextBinding, ContextId, ContextSummary},
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::todoist::{SyncToken, TodoistConfig},
        provider::IntegrationProviderKind,
    },
    task::integrations::todoist::TodoistProject,
};

use universal_inbox_api::{
    configuration::Settings,
    integrations::todoist::{TodoistCommandStatus, TodoistSyncResponse, TodoistSyncStatusResponse},
    repository::context::ContextRepository,
    universal_inbox::audit::actor_context::with_system_actor,
};

use crate::helpers::{
    auth::{AuthenticatedApp, authenticated_app},
    integration_connection::{
        OAuthCredentialFixture, create_and_mock_integration_connection, todoist_oauth_credential,
    },
    settings,
    task::todoist::{
        mirror_todoist_projects, mock_todoist_no_project_add, mock_todoist_sync_resources_service,
        todoist_project, todoist_projects_with,
    },
};

const TODOIST_CREATE_FAILED: &str =
    "Couldn’t create ‘Launch’ in Todoist, so nothing was created here. Please try again.";

/// `Work` (`W`) at the root, `Archive` (`A`) archived, as Todoist returns them
fn work_projects() -> Vec<TodoistProject> {
    let mut archive = todoist_project("A", "Archive", None);
    archive.is_archived = true;
    vec![todoist_project("W", "Work", None), archive]
}

/// The user with a Todoist connection and `projects` mirrored as contexts, by `source_id`
async fn todoist_user(
    settings: &Settings,
    authenticated_app: AuthenticatedApp,
    todoist_oauth_credential: OAuthCredentialFixture,
    projects: &[TodoistProject],
) -> (AuthenticatedApp, HashMap<String, ContextId>) {
    create_and_mock_integration_connection(
        &authenticated_app.app,
        authenticated_app.user.id,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        settings,
        todoist_oauth_credential,
        None,
        None,
    )
    .await;
    let contexts =
        mirror_todoist_projects(&authenticated_app.app, authenticated_app.user.id, projects).await;
    (authenticated_app, contexts)
}

/// Todoist answers the `project_add` of `name` under `parent_id` with `response`, exactly once
async fn mock_todoist_project_add(
    app: &AuthenticatedApp,
    name: &str,
    parent_id: &str,
    response: ResponseTemplate,
) {
    Mock::given(method("POST"))
        .and(path("/sync"))
        .and(body_partial_json(json!({
            "commands": [{
                "type": "project_add",
                "args": { "name": name, "parent_id": parent_id }
            }]
        })))
        .respond_with(response)
        .with_priority(1)
        .expect(1)
        .mount(&app.app.todoist_mock_server)
        .await;
}

fn todoist_sync_status(status: TodoistCommandStatus, new_id: Option<&str>) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(TodoistSyncStatusResponse {
        sync_status: HashMap::from([(Uuid::new_v4(), status)]),
        full_sync: false,
        temp_id_mapping: new_id
            .map(|id| HashMap::from([(Uuid::new_v4().to_string(), id.to_string())]))
            .unwrap_or_default(),
        sync_token: SyncToken("sync token".to_string()),
    })
}

fn todoist_created(new_id: &str) -> ResponseTemplate {
    todoist_sync_status(TodoistCommandStatus::Ok("ok".to_string()), Some(new_id))
}

async fn post_context(app: &AuthenticatedApp, body: Value) -> reqwest::Response {
    app.client
        .post(format!("{}contexts", app.app.api_address))
        .json(&body)
        .send()
        .await
        .unwrap()
}

async fn patch_context(app: &AuthenticatedApp, id: ContextId, body: Value) -> reqwest::Response {
    app.client
        .patch(format!("{}contexts/{id}", app.app.api_address))
        .json(&body)
        .send()
        .await
        .unwrap()
}

async fn created(response: reqwest::Response) -> Context {
    assert_eq!(response.status(), StatusCode::CREATED);
    response.json().await.unwrap()
}

/// The status and the user-facing message of an error response
async fn error_of(response: reqwest::Response) -> (StatusCode, String) {
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    (status, body["message"].as_str().unwrap().to_string())
}

async fn stored_contexts(app: &AuthenticatedApp) -> Vec<Context> {
    let mut transaction = app.app.repository.begin().await.unwrap();
    app.app
        .repository
        .list_contexts_for_user(&mut transaction, app.user.id)
        .await
        .unwrap()
}

async fn sync_todoist_contexts(app: &AuthenticatedApp, projects: &TodoistSyncResponse) {
    app.app.todoist_mock_server.reset().await;
    mock_todoist_sync_resources_service(&app.app.todoist_mock_server, "projects", projects, None)
        .await;
    let task_service = app.app.task_service.read().await;
    with_system_actor(
        "SyncTasks",
        Some(IntegrationProviderKind::Todoist),
        task_service.sync_todoist_project_contexts(app.user.id),
    )
    .await
    .unwrap();
}

fn todoist_binding(source_id: &str) -> Option<ContextBinding> {
    Some(ContextBinding::new(IntegrationProviderKind::Todoist, source_id.into(), true).unwrap())
}

mod creation {
    use super::*;
    use pretty_assertions::assert_eq;

    /// AC2: the project is created in Todoist under the parent's project, then bound to the id
    /// Todoist returns; a later upstream rename reaches the same context
    #[rstest]
    #[tokio::test]
    async fn test_creating_under_a_mirrored_parent_creates_and_binds_the_todoist_project(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        todoist_oauth_credential: OAuthCredentialFixture,
    ) {
        let (app, contexts) = todoist_user(
            &settings,
            authenticated_app.await,
            todoist_oauth_credential,
            &work_projects(),
        )
        .await;
        mock_todoist_project_add(&app, "Launch", "W", todoist_created("NEW1")).await;

        let launch = created(
            post_context(
                &app,
                json!({ "name": " Launch ", "parent_id": contexts["W"] }),
            )
            .await,
        )
        .await;

        assert_eq!(launch.name, "Launch");
        assert_eq!(launch.parent_id, Some(contexts["W"]));
        assert_eq!(launch.binding, todoist_binding("NEW1"));
        app.app.todoist_mock_server.verify().await;

        let renamed = todoist_project("NEW1", "Launch v2", Some("W"));
        sync_todoist_contexts(
            &app,
            &todoist_projects_with([work_projects(), vec![renamed]].concat()),
        )
        .await;
        let stored = stored_contexts(&app).await;
        let launches: Vec<&Context> = stored
            .iter()
            .filter(|context| context.binding == todoist_binding("NEW1"))
            .collect();
        assert_eq!(launches.len(), 1);
        assert_eq!(launches[0].id, launch.id);
        assert_eq!(launches[0].name, "Launch v2");
        assert_eq!(launches[0].parent_id, Some(contexts["W"]));
    }

    /// The sync mirrored the new project before the context was stored: that context is the one
    /// returned, never a second
    #[rstest]
    #[tokio::test]
    async fn test_creating_returns_the_context_the_sync_already_mirrored(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        todoist_oauth_credential: OAuthCredentialFixture,
    ) {
        let projects = [
            work_projects(),
            vec![todoist_project("NEW1", "Launch", Some("W"))],
        ]
        .concat();
        let (app, contexts) = todoist_user(
            &settings,
            authenticated_app.await,
            todoist_oauth_credential,
            &projects,
        )
        .await;
        mock_todoist_project_add(&app, "Launch", "W", todoist_created("NEW1")).await;
        let before = stored_contexts(&app).await.len();

        let launch = created(
            post_context(
                &app,
                json!({ "name": "Launch", "parent_id": contexts["W"] }),
            )
            .await,
        )
        .await;

        assert_eq!(launch.id, contexts["NEW1"]);
        assert_eq!(stored_contexts(&app).await.len(), before);
    }

    /// AC3: at the root or under a context Universal Inbox owns, nothing is sent to Todoist
    #[rstest]
    #[tokio::test]
    async fn test_creating_at_the_root_or_under_a_local_parent_makes_no_provider_call(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        todoist_oauth_credential: OAuthCredentialFixture,
    ) {
        let (app, _) = todoist_user(
            &settings,
            authenticated_app.await,
            todoist_oauth_credential,
            &work_projects(),
        )
        .await;
        mock_todoist_no_project_add(&app.app.todoist_mock_server).await;

        let home = created(post_context(&app, json!({ "name": "Home" })).await).await;
        let garden =
            created(post_context(&app, json!({ "name": "Garden", "parent_id": home.id })).await)
                .await;

        assert_eq!(home.binding, None);
        assert_eq!(home.parent_id, None);
        assert_eq!(garden.binding, None);
        assert_eq!(garden.parent_id, Some(home.id));
        app.app.todoist_mock_server.verify().await;
    }

    /// AC4: Todoist refusing or failing the creation stores nothing, and says so
    #[rstest]
    #[case::http_error(ResponseTemplate::new(400))]
    #[case::command_error(todoist_sync_status(
        TodoistCommandStatus::Error { error_code: 42, error: "Invalid parent".to_string() },
        None,
    ))]
    #[tokio::test]
    async fn test_a_failed_todoist_creation_leaves_no_context(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        todoist_oauth_credential: OAuthCredentialFixture,
        #[case] todoist_response: ResponseTemplate,
    ) {
        let (app, contexts) = todoist_user(
            &settings,
            authenticated_app.await,
            todoist_oauth_credential,
            &work_projects(),
        )
        .await;
        Mock::given(method("POST"))
            .and(path("/sync"))
            .and(body_partial_json(
                json!({ "commands": [{ "type": "project_add" }] }),
            ))
            .respond_with(todoist_response)
            .with_priority(1)
            .mount(&app.app.todoist_mock_server)
            .await;
        let before = stored_contexts(&app).await;

        let response = post_context(
            &app,
            json!({ "name": "Launch", "parent_id": contexts["W"] }),
        )
        .await;

        assert_eq!(
            error_of(response).await,
            (StatusCode::BAD_GATEWAY, TODOIST_CREATE_FAILED.to_string())
        );
        assert_eq!(stored_contexts(&app).await, before);
    }

    #[rstest]
    #[tokio::test]
    async fn test_an_archived_parent_takes_no_child(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        todoist_oauth_credential: OAuthCredentialFixture,
    ) {
        let (app, contexts) = todoist_user(
            &settings,
            authenticated_app.await,
            todoist_oauth_credential,
            &work_projects(),
        )
        .await;
        mock_todoist_no_project_add(&app.app.todoist_mock_server).await;

        let response = post_context(
            &app,
            json!({ "name": "Launch", "parent_id": contexts["A"] }),
        )
        .await;

        assert_eq!(
            error_of(response).await,
            (
                StatusCode::CONFLICT,
                "‘Archive’ is archived in Todoist, so nothing can be added under it. Unarchive it \
                 in Todoist first, or choose another parent."
                    .to_string()
            )
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_another_users_parent_is_not_found(#[future] authenticated_app: AuthenticatedApp) {
        let app = authenticated_app.await;

        let response = post_context(
            &app,
            json!({ "name": "Launch", "parent_id": ContextId(Uuid::new_v4()) }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(stored_contexts(&app).await.is_empty());
    }
}

mod ownership {
    use super::*;
    use pretty_assertions::assert_eq;

    /// AC1 + AC5: Todoist owns the name and the parent of a mirrored context, wherever it would
    /// go, and the refusal names Todoist
    #[rstest]
    #[tokio::test]
    async fn test_the_name_and_parent_of_a_mirrored_context_are_read_only(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        todoist_oauth_credential: OAuthCredentialFixture,
    ) {
        let (app, contexts) = todoist_user(
            &settings,
            authenticated_app.await,
            todoist_oauth_credential,
            &work_projects(),
        )
        .await;
        let home = created(post_context(&app, json!({ "name": "Home" })).await).await;
        let work = contexts["W"];
        let before = stored_contexts(&app).await;

        assert_eq!(
            error_of(patch_context(&app, work, json!({ "name": "Job" })).await).await,
            (
                StatusCode::CONFLICT,
                "‘Work’ is synced from Todoist, which owns its name. Rename it in Todoist and the \
                 new name will show here after the next sync."
                    .to_string()
            )
        );
        let moved = "‘Work’ is synced from Todoist, which owns where it sits. Move it in Todoist \
                     and the change will show here after the next sync.";
        for parent_id in [Some(home.id), Some(contexts["A"])] {
            assert_eq!(
                error_of(patch_context(&app, work, json!({ "parent_id": parent_id })).await).await,
                (StatusCode::CONFLICT, moved.to_string())
            );
        }
        assert_eq!(stored_contexts(&app).await, before);

        // Sending the current values back writes nothing and is no conflict
        let response =
            patch_context(&app, work, json!({ "name": "Work", "parent_id": null })).await;
        assert_eq!(response.status(), StatusCode::OK);

        let summaries: Vec<ContextSummary> = app
            .client
            .get(format!("{}contexts/search", app.app.api_address))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let work_summary = summaries.iter().find(|summary| summary.id == work).unwrap();
        assert_eq!(
            work_summary.mirrored_from,
            Some(IntegrationProviderKind::Todoist)
        );
    }

    /// AC5: a context Universal Inbox owns cannot cross the mirror line either
    #[rstest]
    #[tokio::test]
    async fn test_a_local_context_cannot_move_under_a_mirrored_parent(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        todoist_oauth_credential: OAuthCredentialFixture,
    ) {
        let (app, contexts) = todoist_user(
            &settings,
            authenticated_app.await,
            todoist_oauth_credential,
            &work_projects(),
        )
        .await;
        let home = created(post_context(&app, json!({ "name": "Home" })).await).await;

        let response = patch_context(&app, home.id, json!({ "parent_id": contexts["W"] })).await;

        assert_eq!(
            error_of(response).await,
            (
                StatusCode::CONFLICT,
                "‘Home’ exists only in Universal Inbox and can’t be moved under ‘Work’, which is \
                 synced from Todoist. Create a new project under ‘Work’ instead and it will be \
                 added to Todoist too."
                    .to_string()
            )
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_a_local_context_is_renamed_and_moved(
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let home = created(post_context(&app, json!({ "name": "Home" })).await).await;
        let garden =
            created(post_context(&app, json!({ "name": "Garden", "parent_id": home.id })).await)
                .await;

        let response = patch_context(
            &app,
            garden.id,
            json!({ "name": "Yard", "parent_id": null }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let yard: Context = response.json().await.unwrap();
        assert_eq!(yard.name, "Yard");
        assert_eq!(yard.parent_id, None);

        let response = patch_context(&app, home.id, json!({ "parent_id": yard.id })).await;
        assert_eq!(response.status(), StatusCode::OK);
        let home: Context = response.json().await.unwrap();
        assert_eq!(home.name, "Home");
        assert_eq!(home.parent_id, Some(yard.id));

        // Yard is now under nothing and Home under Yard: Yard under Home closes a cycle
        let response = patch_context(&app, yard.id, json!({ "parent_id": home.id })).await;
        assert_eq!(
            error_of(response).await,
            (
                StatusCode::BAD_REQUEST,
                "‘Yard’ can’t be moved under ‘Home’, because ‘Home’ is inside it. Choose another \
                 parent."
                    .to_string()
            )
        );
        let response = patch_context(&app, yard.id, json!({ "parent_id": yard.id })).await;
        assert_eq!(
            error_of(response).await,
            (
                StatusCode::BAD_REQUEST,
                "‘Yard’ can’t be moved under itself. Choose another parent.".to_string()
            )
        );
        let stored = stored_contexts(&app).await;
        let yard_stored = stored.iter().find(|context| context.id == yard.id).unwrap();
        assert_eq!(yard_stored.parent_id, None);
    }

    #[rstest]
    #[tokio::test]
    async fn test_a_blank_name_is_rejected(#[future] authenticated_app: AuthenticatedApp) {
        let app = authenticated_app.await;
        let home = created(post_context(&app, json!({ "name": "Home" })).await).await;

        assert_eq!(
            post_context(&app, json!({ "name": "  " })).await.status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            patch_context(&app, home.id, json!({ "name": "" }))
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
}
