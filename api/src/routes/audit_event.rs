use std::sync::Arc;

use actix_web::{HttpResponse, Scope, web};
use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_with::{StringWithSeparator, formats::CommaSeparator, serde_as};
use uuid::Uuid;

use universal_inbox::{
    PageToken,
    audit::{AuditActorKind, AuditChangeKind, AuditEntityKind, AuditEventFilter},
};

use crate::{
    middlewares::jwt_auth::Authenticated,
    universal_inbox::{UniversalInboxError, audit::service::AuditService},
    utils::jwt::Claims,
};

pub fn scope() -> Scope {
    web::scope("/audit-events")
        .service(
            web::resource("")
                .name("audit-events")
                .route(web::get().to(list_audit_events)),
        )
        .service(
            web::resource("/groups")
                .name("audit-event-groups")
                .route(web::get().to(list_audit_event_groups)),
        )
}

#[serde_as]
#[derive(Debug, Deserialize)]
pub struct ListAuditEventsRequest {
    entity_kind: Option<AuditEntityKind>,
    entity_id: Option<Uuid>,
    #[serde_as(as = "Option<StringWithSeparator::<CommaSeparator, AuditActorKind>>")]
    actor_kinds: Option<Vec<AuditActorKind>>,
    #[serde_as(as = "Option<StringWithSeparator::<CommaSeparator, String>>")]
    source_kinds: Option<Vec<String>>,
    change: Option<AuditChangeKind>,
    query: Option<String>,
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
    page_token: Option<String>,
    per_page: Option<usize>,
}

impl ListAuditEventsRequest {
    fn parse(
        self,
    ) -> Result<(AuditEventFilter, Option<PageToken>, Option<usize>), UniversalInboxError> {
        let page_token = self
            .page_token
            .as_deref()
            .map(PageToken::from_url_parameter)
            .transpose()
            .map_err(|_| UniversalInboxError::InvalidInputData {
                source: None,
                user_error: "Invalid page token".to_string(),
            })?;
        let filter = AuditEventFilter {
            entity_kind: self.entity_kind,
            entity_id: self.entity_id,
            actor_kinds: self.actor_kinds.unwrap_or_default(),
            source_kinds: self.source_kinds.unwrap_or_default(),
            change: self.change,
            query: self.query,
            since: self.since,
            until: self.until,
        };
        Ok((filter, page_token, self.per_page))
    }
}

pub async fn list_audit_events(
    request: web::Query<ListAuditEventsRequest>,
    audit_service: web::Data<Arc<AuditService>>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated.user_id()?;
    let (filter, page_token, per_page) = request.into_inner().parse()?;

    let mut transaction = audit_service
        .begin()
        .await
        .context("Failed to create new transaction while listing audit events")?;
    let page = audit_service
        .fetch_audit_events(&mut transaction, user_id, &filter, page_token, per_page)
        .await?;
    transaction
        .commit()
        .await
        .context("Failed to commit while listing audit events")?;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(serde_json::to_string(&page).context("Cannot serialize audit events")?))
}

/// Audit log rows: a bulk action is one row, so a page never splits it.
pub async fn list_audit_event_groups(
    request: web::Query<ListAuditEventsRequest>,
    audit_service: web::Data<Arc<AuditService>>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated.user_id()?;
    let (filter, page_token, per_page) = request.into_inner().parse()?;

    let mut transaction = audit_service
        .begin()
        .await
        .context("Failed to create new transaction while listing audit event groups")?;
    let page = audit_service
        .fetch_audit_event_groups(&mut transaction, user_id, &filter, page_token, per_page)
        .await?;
    transaction
        .commit()
        .await
        .context("Failed to commit while listing audit event groups")?;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(serde_json::to_string(&page).context("Cannot serialize audit event groups")?))
}
