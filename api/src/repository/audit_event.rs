use std::collections::BTreeMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{FromRow, Postgres, QueryBuilder, Transaction, types::Json};
use uuid::Uuid;

use universal_inbox::{
    Page, PageToken,
    audit::{
        AuditAction, AuditActor, AuditChange, AuditChangeKind, AuditEntityKind, AuditEvent,
        AuditEventFilter,
    },
    user::UserId,
};

use crate::observability::attr;
use crate::universal_inbox::UniversalInboxError;

use super::{Repository, decode_rows_skipping_invalid};

#[async_trait]
pub trait AuditEventRepository {
    async fn fetch_audit_events(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        filter: &AuditEventFilter,
        page_token: Option<PageToken>,
        per_page: usize,
    ) -> Result<Page<AuditEvent>, UniversalInboxError>;

    /// Delete at most `batch_size` events created before `cutoff`, returning how many were deleted
    async fn delete_audit_events_older_than(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        cutoff: DateTime<Utc>,
        batch_size: i64,
    ) -> Result<u64, UniversalInboxError>;
}

#[async_trait]
impl AuditEventRepository for Repository {
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            { attr::USER_ID } = user_id.to_string()
        ),
        err
    )]
    async fn fetch_audit_events(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        filter: &AuditEventFilter,
        page_token: Option<PageToken>,
        per_page: usize,
    ) -> Result<Page<AuditEvent>, UniversalInboxError> {
        fn add_filters(
            query_builder: &mut QueryBuilder<Postgres>,
            user_id: UserId,
            filter: &AuditEventFilter,
        ) {
            query_builder
                .push(" WHERE audit_event.user_id = ")
                .push_bind(user_id.0);

            match filter.entity_id {
                // A task's history includes the events of the notifications linked to it.
                Some(entity_id) => {
                    query_builder
                        .push(" AND (audit_event.entity_id = ")
                        .push_bind(entity_id)
                        .push(" OR audit_event.related_entity_id = ")
                        .push_bind(entity_id)
                        .push(")");
                }
                None => {
                    if let Some(entity_kind) = filter.entity_kind {
                        query_builder
                            .push(" AND audit_event.entity_kind::TEXT = ")
                            .push_bind(entity_kind.to_string());
                    }
                }
            }

            if !filter.actor_kinds.is_empty() {
                let actor_kinds: Vec<String> =
                    filter.actor_kinds.iter().map(|k| k.to_string()).collect();
                query_builder
                    .push(" AND audit_event.actor_kind = ANY(")
                    .push_bind(actor_kinds)
                    .push(")");
            }

            if !filter.source_kinds.is_empty() {
                query_builder
                    .push(" AND audit_event.source_kind = ANY(")
                    .push_bind(filter.source_kinds.clone())
                    .push(")");
            }

            match filter.change {
                Some(AuditChangeKind::Created) => {
                    query_builder
                        .push(" AND audit_event.action = ")
                        .push_bind(AuditAction::Created.to_string());
                }
                Some(AuditChangeKind::Removed) => {
                    query_builder
                        .push(" AND audit_event.action = ")
                        .push_bind(AuditAction::Removed.to_string());
                }
                Some(change) => {
                    if let Some(key) = change.change_key() {
                        query_builder
                            .push(" AND audit_event.action = ")
                            .push_bind(AuditAction::Updated.to_string())
                            .push(" AND audit_event.changes ? ")
                            .push_bind(key);
                    }
                }
                None => {}
            }

            if let Some(query) = filter.query.as_deref().map(str::trim)
                && !query.is_empty()
            {
                let escaped = query
                    .replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_");
                query_builder
                    .push(" AND audit_event.title ILIKE ")
                    .push_bind(format!("%{escaped}%"));
            }

            if let Some(since) = filter.since {
                query_builder
                    .push(" AND audit_event.created_at >= ")
                    .push_bind(since);
            }
            if let Some(until) = filter.until {
                query_builder
                    .push(" AND audit_event.created_at < ")
                    .push_bind(until);
            }
        }

        let mut count_query_builder = QueryBuilder::new("SELECT count(*) FROM audit_event");
        add_filters(&mut count_query_builder, user_id, filter);
        let count = count_query_builder
            .build_query_scalar::<i64>()
            .fetch_one(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!("Failed to count audit events from storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        // Events are listed newest first and paged by offset: a bulk action writes many events
        // with nearly identical timestamps, which a timestamp cursor could skip.
        let offset = match page_token {
            Some(PageToken::Offset(offset)) => offset,
            _ => 0,
        };

        let mut query_builder = QueryBuilder::new(
            r#"
                SELECT
                  audit_event.id,
                  audit_event.user_id,
                  audit_event.entity_kind::TEXT as entity_kind,
                  audit_event.entity_id,
                  audit_event.related_entity_id,
                  audit_event.action,
                  audit_event.title,
                  audit_event.source_kind,
                  audit_event.changes,
                  audit_event.params,
                  audit_event.actor,
                  oauth2_client.client_name as actor_name,
                  audit_event.transaction_id,
                  audit_event.created_at
                FROM audit_event
                LEFT JOIN oauth2_client
                  ON audit_event.actor_kind = 'Mcp'
                  AND oauth2_client.client_id = audit_event.actor->>'client_id'
            "#,
        );
        add_filters(&mut query_builder, user_id, filter);
        query_builder
            .push(" ORDER BY audit_event.created_at DESC, audit_event.id DESC LIMIT ")
            .push_bind(per_page as i64)
            .push(" OFFSET ")
            .push_bind(offset as i64);

        let rows = query_builder
            .build()
            .fetch_all(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!("Failed to fetch audit events from storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        let content = decode_rows_skipping_invalid::<AuditEventRow>(&rows, "id", "audit_event")
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<AuditEvent>, UniversalInboxError>>()?;

        let total: usize = count.try_into().unwrap(); // count(*) cannot be negative
        let next_offset = offset + content.len();
        Ok(Page {
            per_page,
            pages_count: total.div_ceil(per_page.max(1)),
            total,
            previous_page_token: (offset > 0)
                .then(|| PageToken::Offset(offset.saturating_sub(per_page))),
            next_page_token: (next_offset < total).then_some(PageToken::Offset(next_offset)),
            content,
        })
    }

    #[tracing::instrument(level = "debug", skip_all, err)]
    async fn delete_audit_events_older_than(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        cutoff: DateTime<Utc>,
        batch_size: i64,
    ) -> Result<u64, UniversalInboxError> {
        let result = sqlx::query(
            r#"
                DELETE FROM audit_event
                WHERE id IN (
                  SELECT id FROM audit_event WHERE created_at < $1 LIMIT $2
                )
            "#,
        )
        .bind(cutoff)
        .bind(batch_size)
        .execute(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!("Failed to delete old audit events from storage: {err}");
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        Ok(result.rows_affected())
    }
}

#[derive(Debug, FromRow)]
struct AuditEventRow {
    id: Uuid,
    user_id: Uuid,
    entity_kind: String,
    entity_id: Uuid,
    related_entity_id: Option<Uuid>,
    action: String,
    title: String,
    source_kind: Option<String>,
    changes: Json<BTreeMap<String, AuditChange>>,
    params: Option<Json<serde_json::Value>>,
    actor: Json<AuditActor>,
    actor_name: Option<String>,
    transaction_id: i64,
    created_at: DateTime<Utc>,
}

impl TryFrom<AuditEventRow> for AuditEvent {
    type Error = UniversalInboxError;

    fn try_from(row: AuditEventRow) -> Result<Self, Self::Error> {
        let entity_kind = row.entity_kind.parse::<AuditEntityKind>().map_err(|err| {
            UniversalInboxError::InvalidEnumData {
                source: err,
                output: row.entity_kind.clone(),
            }
        })?;
        let action = row.action.parse::<AuditAction>().map_err(|err| {
            UniversalInboxError::InvalidEnumData {
                source: err,
                output: row.action.clone(),
            }
        })?;

        Ok(AuditEvent {
            id: row.id.into(),
            user_id: row.user_id.into(),
            entity_kind,
            entity_id: row.entity_id,
            related_entity_id: row.related_entity_id,
            action,
            title: row.title,
            source_kind: row.source_kind,
            changes: row.changes.0,
            params: row.params.map(|params| params.0),
            actor: row.actor.0,
            actor_name: row.actor_name,
            transaction_id: row.transaction_id,
            created_at: row.created_at,
        })
    }
}
