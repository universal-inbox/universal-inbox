use std::sync::Arc;

use actix_http::body::BoxBody;
use actix_jwt_authc::{Authenticated, MaybeAuthenticated};
use actix_web::{web, HttpResponse, Scope};
use anyhow::Context;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::RwLock;
use tracing::{error, info};

use universal_inbox::{
    task::{
        service::SyncTasksParameters, service::TaskPatch, ProjectSummary, Task, TaskId, TaskStatus,
        TaskSummary,
    },
    user::UserId,
};

use crate::{
    observability::spawn_with_tracing,
    universal_inbox::{task::service::TaskService, UniversalInboxError, UpdateStatus},
    utils::jwt::Claims,
};

pub fn scope() -> Scope {
    web::scope("/tasks")
        .route("/sync", web::post().to(sync_tasks))
        .route("/search", web::get().to(search_tasks))
        .service(
            web::resource("")
                .name("tasks")
                .route(web::get().to(list_tasks))
                .route(web::post().to(create_task)),
        )
        .service(
            web::resource("/{task_id}")
                .route(web::get().to(get_task))
                .route(web::patch().to(patch_task)),
        )
        .service(web::scope("/projects").route("/search", web::get().to(search_projects)))
}

#[derive(Debug, Deserialize)]
pub struct ListTaskRequest {
    status: TaskStatus,
}

pub async fn list_tasks(
    list_task_request: web::Query<ListTaskRequest>,
    task_service: web::Data<Arc<RwLock<TaskService>>>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;

    let service = task_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while listing tasks")?;
    let tasks: Vec<Task> = service
        .list_tasks(&mut transaction, list_task_request.status, user_id)
        .await?;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(serde_json::to_string(&tasks).context("Cannot serialize tasks")?))
}

#[derive(Debug, Deserialize)]
pub struct SearchTaskRequest {
    matches: String,
}

pub async fn search_tasks(
    search_task_request: web::Query<SearchTaskRequest>,
    task_service: web::Data<Arc<RwLock<TaskService>>>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;

    let service = task_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while listing tasks")?;
    let tasks: Vec<TaskSummary> = service
        .search_tasks(&mut transaction, &search_task_request.matches, user_id)
        .await?;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(serde_json::to_string(&tasks).context("Cannot serialize tasks")?))
}

pub async fn get_task(
    path: web::Path<TaskId>,
    task_service: web::Data<Arc<RwLock<TaskService>>>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;
    let task_id = path.into_inner();
    let service = task_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while getting task")?;

    match service.get_task(&mut transaction, task_id, user_id).await? {
        Some(task) => Ok(HttpResponse::Ok()
            .content_type("application/json")
            .body(serde_json::to_string(&task).context("Cannot serialize task")?)),
        None => Ok(HttpResponse::NotFound()
            .content_type("application/json")
            .body(BoxBody::new(
                json!({ "message": format!("Cannot find task {task_id}") }).to_string(),
            ))),
    }
}

pub async fn create_task(
    task: web::Json<Box<Task>>,
    task_service: web::Data<Arc<RwLock<TaskService>>>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;
    let service = task_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while creating task")?;

    let created_task = service
        .create_task(&mut transaction, task.into_inner(), user_id)
        .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit while creating task")?;

    Ok(HttpResponse::Ok().content_type("application/json").body(
        serde_json::to_string(&created_task).context("Cannot serialize task creation result")?,
    ))
}

pub async fn sync_tasks(
    params: web::Json<SyncTasksParameters>,
    task_service: web::Data<Arc<RwLock<TaskService>>>,
    maybe_authenticated: MaybeAuthenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let source = params.source;

    if let Some(authenticated) = maybe_authenticated.into_option() {
        let user_id = authenticated
            .claims
            .sub
            .parse::<UserId>()
            .context("Wrong user ID format")?;

        if params.asynchronous.unwrap_or(true) {
            let task_service = task_service.get_ref().clone();
            spawn_with_tracing(async move {
                let source_kind_string = source
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "all types of".to_string());
                info!("Syncing {source_kind_string} tasks for user {user_id}");
                let service = task_service.read().await;

                let tasks = if let Some(source) = source {
                    service.sync_tasks_with_transaction(source, user_id).await
                } else {
                    service.sync_all_tasks(user_id).await
                };

                match tasks {
                    Ok(tasks) => info!(
                        "{} {source_kind_string} tasks successfully synced for user {user_id}",
                        tasks.len()
                    ),
                    Err(err) => {
                        error!(
                            "Failed to sync {source_kind_string} tasks for user {user_id}: {err:?}"
                        )
                    }
                };
            });
            Ok(HttpResponse::Created().finish())
        } else {
            let service = task_service.read().await;

            let tasks = if let Some(source) = source {
                service.sync_tasks_with_transaction(source, user_id).await?
            } else {
                service.sync_all_tasks(user_id).await?
            };
            Ok(HttpResponse::Ok()
                .content_type("application/json")
                .body(serde_json::to_string(&tasks).context("Cannot serialize tasks")?))
        }
    } else {
        let task_service = task_service.get_ref().clone();

        spawn_with_tracing(async move {
            let source_kind_string = source
                .map(|s| s.to_string())
                .unwrap_or_else(|| "all types of".to_string());
            info!("Syncing {source_kind_string} tasks for all users");
            let service = task_service.read().await;

            let result = service.sync_tasks_for_all_users(source).await;

            match result {
                Ok(_) => info!("{source_kind_string} tasks successfully synced"),
                Err(err) => {
                    error!("Failed to sync {source_kind_string} tasks: {err:?}")
                }
            };
        });
        Ok(HttpResponse::Created().finish())
    }
}

pub async fn patch_task(
    path: web::Path<TaskId>,
    patch: web::Json<TaskPatch>,
    task_service: web::Data<Arc<RwLock<TaskService>>>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;
    let task_id = path.into_inner();
    let task_patch = patch.into_inner();
    let service = task_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context(format!("Failed to patch task {task_id}"))?;

    let updated_task = service
        .patch_task(&mut transaction, task_id, &task_patch, user_id)
        .await?;

    transaction
        .commit()
        .await
        .context(format!("Failed to commit while patching task {task_id}"))?;

    match updated_task {
        UpdateStatus {
            updated: true,
            result: Some(task),
        } => Ok(HttpResponse::Ok()
            .content_type("application/json")
            .body(serde_json::to_string(&task).context("Cannot serialize task")?)),
        UpdateStatus {
            updated: false,
            result: Some(_),
        } => Ok(HttpResponse::NotModified().finish()),
        UpdateStatus {
            updated: _,
            result: None,
        } => Ok(HttpResponse::NotFound()
            .content_type("application/json")
            .body(BoxBody::new(
                json!({ "message": format!("Cannot update unknown task {task_id}") }).to_string(),
            ))),
    }
}

#[derive(Debug, Deserialize)]
pub struct SearchProjectRequest {
    matches: String,
}

pub async fn search_projects(
    search_project_request: web::Query<SearchProjectRequest>,
    task_service: web::Data<Arc<RwLock<TaskService>>>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;

    let service = task_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while listing tasks")?;
    let tasks: Vec<ProjectSummary> = service
        .search_projects(&mut transaction, &search_project_request.matches, user_id)
        .await?;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(serde_json::to_string(&tasks).context("Cannot serialize task projects")?))
}
