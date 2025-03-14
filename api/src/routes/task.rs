use std::sync::Arc;

use actix_http::body::BoxBody;
use actix_jwt_authc::{Authenticated, MaybeAuthenticated};
use actix_web::{
    http::header::{self, CacheDirective},
    web, HttpResponse, Scope,
};
use anyhow::Context;
use apalis_redis::RedisStorage;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::RwLock;

use universal_inbox::{
    task::{
        service::SyncTasksParameters, service::TaskPatch, ProjectSummary, Task, TaskId, TaskStatus,
        TaskSummary,
    },
    user::UserId,
    Page,
};

use crate::{
    jobs::UniversalInboxJob,
    universal_inbox::{
        integration_connection::service::IntegrationConnectionService, task::service::TaskService,
        UniversalInboxError, UpdateStatus,
    },
    utils::jwt::Claims,
};

pub fn scope() -> Scope {
    web::scope("/tasks")
        .route("/sync", web::post().to(sync_tasks))
        .route("/search", web::get().to(search_tasks))
        .service(
            web::resource("")
                .name("tasks")
                .route(web::get().to(list_tasks)),
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
    only_synced_tasks: Option<bool>,
    trigger_sync: Option<bool>,
}

pub async fn list_tasks(
    list_task_request: web::Query<ListTaskRequest>,
    task_service: web::Data<Arc<RwLock<TaskService>>>,
    authenticated: Authenticated<Claims>,
    job_storage: web::Data<RedisStorage<UniversalInboxJob>>,
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
    let tasks_page: Page<Task> = service
        .list_tasks(
            &mut transaction,
            list_task_request.status,
            list_task_request.only_synced_tasks.unwrap_or_default(),
            user_id,
            list_task_request
                .trigger_sync
                .unwrap_or(true)
                .then(|| job_storage.as_ref().clone()),
        )
        .await?;
    transaction
        .commit()
        .await
        .context("Failed to commit while listing tasks")?;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(serde_json::to_string(&tasks_page).context("Cannot serialize tasks")?))
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

pub async fn sync_tasks(
    params: web::Json<SyncTasksParameters>,
    task_service: web::Data<Arc<RwLock<TaskService>>>,
    integration_connection_service: web::Data<Arc<RwLock<IntegrationConnectionService>>>,
    maybe_authenticated: MaybeAuthenticated<Claims>,
    storage: web::Data<RedisStorage<UniversalInboxJob>>,
) -> Result<HttpResponse, UniversalInboxError> {
    let source = params.source;
    let mut storage = storage.as_ref().clone();

    if let Some(authenticated) = maybe_authenticated.into_option() {
        let user_id = authenticated
            .claims
            .sub
            .parse::<UserId>()
            .context("Wrong user ID format")?;

        if params.asynchronous.unwrap_or(true) {
            let service = integration_connection_service.read().await;
            let mut transaction = service
                .begin()
                .await
                .context("Failed to create new transaction while triggering tasks sync")?;
            service
                .trigger_sync_tasks(&mut transaction, source, Some(user_id), &mut storage)
                .await?;
            transaction
                .commit()
                .await
                .context("Failed to commit while triggering tasks sync")?;
            Ok(HttpResponse::Created().finish())
        } else {
            let service = task_service.read().await;

            let tasks = if let Some(source) = source {
                service
                    .sync_tasks_with_transaction(source, user_id, false)
                    .await?
            } else {
                service.sync_all_tasks(user_id, false).await?
            };
            Ok(HttpResponse::Ok()
                .content_type("application/json")
                .body(serde_json::to_string(&tasks).context("Cannot serialize tasks")?))
        }
    } else {
        let service = integration_connection_service.read().await;
        let mut transaction = service
            .begin()
            .await
            .context("Failed to create new transaction while triggering tasks sync")?;
        service
            .trigger_sync_tasks(&mut transaction, source, None, &mut storage)
            .await?;
        transaction
            .commit()
            .await
            .context("Failed to commit while triggering tasks sync")?;
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
        .insert_header(header::CacheControl(vec![
            CacheDirective::Private,
            CacheDirective::MaxAge(600u32),
        ]))
        .body(serde_json::to_string(&tasks).context("Cannot serialize task projects")?))
}
