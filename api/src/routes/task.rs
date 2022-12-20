use std::sync::Arc;

use actix_http::body::BoxBody;
use actix_web::{web, HttpResponse, Scope};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use universal_inbox::task::{Task, TaskPatch, TaskStatus};

use crate::{
    integrations::task::TaskSyncSourceKind,
    universal_inbox::{
        task::{service::TaskService, TaskCreationResult},
        UniversalInboxError, UpdateStatus,
    },
};

use super::option_wildcard;

pub fn scope() -> Scope {
    web::scope("/tasks")
        .route("/sync", web::post().to(sync_tasks))
        .service(
            web::resource("")
                .name("tasks")
                .route(web::get().to(list_tasks))
                .route(web::post().to(create_task))
                .route(web::method(http::Method::OPTIONS).to(option_wildcard)),
        )
        .service(
            web::resource("/{task_id}")
                .route(web::get().to(get_task))
                .route(web::patch().to(patch_task))
                .route(web::method(http::Method::OPTIONS).to(option_wildcard)),
        )
}

#[derive(Debug, Deserialize)]
pub struct ListTaskRequest {
    status: TaskStatus,
}

#[tracing::instrument(level = "debug", skip(service))]
pub async fn list_tasks(
    list_task_request: web::Query<ListTaskRequest>,
    service: web::Data<Arc<TaskService>>,
) -> Result<HttpResponse, UniversalInboxError> {
    let tasks: Vec<Task> = service
        .connect()
        .await?
        .list_tasks(list_task_request.status)
        .await?;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(serde_json::to_string(&tasks).context("Cannot serialize tasks")?))
}

#[tracing::instrument(level = "debug", skip(service))]
pub async fn get_task(
    path: web::Path<Uuid>,
    service: web::Data<Arc<TaskService>>,
) -> Result<HttpResponse, UniversalInboxError> {
    let task_id = path.into_inner();

    match service.connect().await?.get_task(task_id).await? {
        Some(task) => Ok(HttpResponse::Ok()
            .content_type("application/json")
            .body(serde_json::to_string(&task).context("Cannot serialize task")?)),
        None => Ok(HttpResponse::NotFound()
            .content_type("application/json")
            .body(BoxBody::new(
                json!({ "message": format!("Cannot find task {}", task_id) }).to_string(),
            ))),
    }
}

#[tracing::instrument(level = "debug", skip(service))]
pub async fn create_task(
    task: web::Json<Box<Task>>,
    service: web::Data<Arc<TaskService>>,
) -> Result<HttpResponse, UniversalInboxError> {
    let transactional_service = service
        .begin()
        .await
        .context("Failed to create new transaction while creating task")?;

    let created_task = transactional_service.create_task(task.into_inner()).await?;

    transactional_service
        .commit()
        .await
        .context("Failed to commit while creating task")?;

    Ok(HttpResponse::Ok().content_type("application/json").body(
        serde_json::to_string(&created_task).context("Cannot serialize task creation result")?,
    ))
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SyncTasksParameters {
    source: Option<TaskSyncSourceKind>,
}

#[tracing::instrument(level = "debug", skip(service))]
pub async fn sync_tasks(
    params: web::Json<SyncTasksParameters>,
    service: web::Data<Arc<TaskService>>,
) -> Result<HttpResponse, UniversalInboxError> {
    let transactional_service = service.begin().await.context(format!(
        "Failed to create new transaction while syncing {:?}",
        &params.source
    ))?;

    let tasks: Vec<TaskCreationResult> = transactional_service.sync_tasks(&params.source).await?;

    transactional_service.commit().await.context(format!(
        "Failed to commit while syncing {:?}",
        &params.source
    ))?;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(serde_json::to_string(&tasks).context("Cannot serialize task creation results")?))
}

#[tracing::instrument(level = "debug", skip(service))]
pub async fn patch_task(
    path: web::Path<Uuid>,
    patch: web::Json<TaskPatch>,
    service: web::Data<Arc<TaskService>>,
) -> Result<HttpResponse, UniversalInboxError> {
    let task_id = path.into_inner();
    let transactional_service = service
        .begin()
        .await
        .context(format!("Failed to patch task {task_id}"))?;

    let updated_task = transactional_service
        .patch_task(task_id, &patch.into_inner())
        .await?;

    transactional_service
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
                json!({ "message": format!("Cannot update unknown task {}", task_id) }).to_string(),
            ))),
    }
}
