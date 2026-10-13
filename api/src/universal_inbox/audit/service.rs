use std::sync::Arc;

use chrono::{TimeDelta, Utc};
use sqlx::{Postgres, Transaction};
use tracing::info;

use universal_inbox::{
    Page, PageToken,
    audit::{AuditEvent, AuditEventFilter, AuditEventGroup},
    user::UserId,
};

use crate::observability::attr;
use crate::{
    repository::{Repository, audit_event::AuditEventRepository},
    universal_inbox::UniversalInboxError,
};

pub const DEFAULT_AUDIT_EVENTS_PAGE_SIZE: usize = 50;
pub const MAX_AUDIT_EVENTS_PAGE_SIZE: usize = 200;
const PURGE_BATCH_SIZE: i64 = 10_000;

pub struct AuditService {
    repository: Arc<Repository>,
    retention_days: u32,
}

impl AuditService {
    pub fn new(repository: Arc<Repository>, retention_days: u32) -> Self {
        Self {
            repository,
            retention_days,
        }
    }

    pub fn retention_days(&self) -> u32 {
        self.retention_days
    }

    pub async fn begin(&self) -> Result<Transaction<'_, Postgres>, UniversalInboxError> {
        self.repository.begin().await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::USER_ID } = %user_id),
        err
    )]
    pub async fn fetch_audit_events(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        filter: &AuditEventFilter,
        page_token: Option<PageToken>,
        per_page: Option<usize>,
    ) -> Result<Page<AuditEvent>, UniversalInboxError> {
        let per_page = per_page
            .unwrap_or(DEFAULT_AUDIT_EVENTS_PAGE_SIZE)
            .clamp(1, MAX_AUDIT_EVENTS_PAGE_SIZE);
        self.repository
            .fetch_audit_events(executor, user_id, filter, page_token, per_page)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::USER_ID } = %user_id),
        err
    )]
    pub async fn fetch_audit_event_groups(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        filter: &AuditEventFilter,
        page_token: Option<PageToken>,
        per_page: Option<usize>,
    ) -> Result<Page<AuditEventGroup>, UniversalInboxError> {
        let per_page = per_page
            .unwrap_or(DEFAULT_AUDIT_EVENTS_PAGE_SIZE)
            .clamp(1, MAX_AUDIT_EVENTS_PAGE_SIZE);
        self.repository
            .fetch_audit_event_groups(executor, user_id, filter, page_token, per_page)
            .await
    }

    /// Delete the events older than the retention period, in batches each in its own transaction
    /// so a large backlog does not hold one long transaction. Returns the number deleted.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub async fn purge_expired_audit_events(&self) -> Result<u64, UniversalInboxError> {
        let cutoff = Utc::now() - TimeDelta::days(self.retention_days.into());
        let mut deleted = 0;
        loop {
            let mut transaction = self.begin().await?;
            let batch_deleted = self
                .repository
                .delete_audit_events_older_than(&mut transaction, cutoff, PURGE_BATCH_SIZE)
                .await?;
            transaction
                .commit()
                .await
                .map_err(|err| UniversalInboxError::DatabaseError {
                    message: format!("Failed to commit audit events purge: {err}"),
                    source: err,
                })?;
            deleted += batch_deleted;
            if batch_deleted < PURGE_BATCH_SIZE as u64 {
                break;
            }
        }
        info!("Purged {deleted} audit events created before {cutoff}");
        Ok(deleted)
    }
}
