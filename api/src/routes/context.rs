use std::sync::Arc;

use actix_web::{
    HttpResponse, Scope,
    http::header::{self, CacheDirective},
    web,
};
use anyhow::Context;
use serde::Deserialize;
use tokio::sync::RwLock;

use universal_inbox::{context::ContextSummary, user::UserId};

use crate::{
    middlewares::jwt_auth::Authenticated,
    universal_inbox::{UniversalInboxError, task::service::TaskService},
    utils::jwt::Claims,
};

pub fn scope() -> Scope {
    web::scope("/contexts").route("/search", web::get().to(search_contexts))
}

#[derive(Debug, Deserialize)]
pub struct SearchContextsRequest {
    /// Matched as a plain, case-insensitive substring on each segment of the path. Absent or
    /// empty: every context.
    matches: Option<String>,
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
