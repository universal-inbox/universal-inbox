use std::sync::Arc;

use actix_web::{
    HttpResponse, Scope,
    http::header::{self, CacheDirective},
    web,
};
use anyhow::Context;
use serde::Deserialize;
use tokio::sync::RwLock;

use universal_inbox::{
    context::{ContextId, ContextSummary},
    integration_connection::provider::IntegrationProviderKind,
    user::UserId,
};

use crate::{
    middlewares::jwt_auth::Authenticated,
    universal_inbox::{UniversalInboxError, task::service::TaskService},
    utils::jwt::Claims,
};

pub fn scope() -> Scope {
    web::scope("/contexts")
        .route("/search", web::get().to(search_contexts))
        .service(web::resource("").route(web::post().to(create_context)))
        .service(web::resource("/{context_id}").route(web::patch().to(patch_context)))
}

#[derive(Debug, Deserialize)]
pub struct SearchContextsRequest {
    /// Matched as a plain, case-insensitive substring on each segment of the path. Absent or
    /// empty: every context.
    matches: Option<String>,
    /// Only the contexts a task of this task manager can go to. Absent: those of every task
    /// manager.
    provider_kind: Option<IntegrationProviderKind>,
}

/// The contexts a task can be bound to, with their path: what the project picker lists
pub async fn search_contexts(
    search_contexts_request: web::Query<SearchContextsRequest>,
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
        .context("Failed to create new transaction while searching contexts")?;
    let contexts: Vec<ContextSummary> = service
        .search_contexts(
            &mut transaction,
            search_contexts_request
                .matches
                .as_deref()
                .unwrap_or_default(),
            search_contexts_request.provider_kind,
            user_id,
        )
        .await?;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .insert_header(header::CacheControl(vec![
            CacheDirective::Private,
            CacheDirective::NoStore,
        ]))
        .body(serde_json::to_string(&contexts).context("Cannot serialize contexts")?))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateContextRequest {
    name: String,
    /// Absent or `null`: at the root
    #[serde(default)]
    parent_id: Option<ContextId>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchContextRequest {
    name: Option<String>,
    /// Absent: unchanged; `null`: to the root
    #[serde(default, with = "::serde_with::rust::double_option")]
    parent_id: Option<Option<ContextId>>,
}

/// Create a context. Under a parent synced from a provider, the project is created in the
/// provider too.
pub async fn create_context(
    request: web::Json<CreateContextRequest>,
    task_service: web::Data<Arc<RwLock<TaskService>>>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;
    let request = request.into_inner();

    let service = task_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while creating a context")?;
    let context = service
        .create_context(&mut transaction, user_id, &request.name, request.parent_id)
        .await?;
    transaction
        .commit()
        .await
        .context("Failed to commit while creating a context")?;

    Ok(HttpResponse::Created()
        .content_type("application/json")
        .body(serde_json::to_string(&context).context("Cannot serialize context")?))
}

/// Rename or move a context Universal Inbox owns
pub async fn patch_context(
    path: web::Path<ContextId>,
    patch: web::Json<PatchContextRequest>,
    task_service: web::Data<Arc<RwLock<TaskService>>>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;
    let context_id = path.into_inner();
    let patch = patch.into_inner();

    let service = task_service.read().await;
    let mut transaction = service.begin().await.context(format!(
        "Failed to create new transaction while patching context {context_id}"
    ))?;
    let context = service
        .update_context(
            &mut transaction,
            user_id,
            context_id,
            patch.name.as_deref(),
            patch.parent_id,
        )
        .await?;
    transaction.commit().await.context(format!(
        "Failed to commit while patching context {context_id}"
    ))?;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(serde_json::to_string(&context).context("Cannot serialize context")?))
}
