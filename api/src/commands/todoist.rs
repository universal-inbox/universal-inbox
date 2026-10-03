use std::{collections::HashMap, sync::Arc};

use anyhow::Context;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use universal_inbox::{
    integration_connection::IntegrationConnectionId, third_party::item::ThirdPartyItem,
    user::UserId,
};

use crate::observability::RecordSpanError;
use crate::observability::attr;
use crate::{
    repository::third_party::ThirdPartyItemRepository,
    universal_inbox::{
        UniversalInboxError, integration_connection::service::IntegrationConnectionService,
        task::service::TaskService,
    },
};

/// One-off backfill of Todoist items synced before the migration to Todoist
/// API v1: their stored legacy (all-digit) IDs are rejected by the API (error
/// 557) and incremental syncs never re-emit unchanged items.
#[tracing::instrument(
    name = "todoist-migrate-legacy-ids-command",
    level = "info",
    skip_all,
    fields(
        { attr::USER_ID } = ?user_id,
        { attr::COMMAND_DRY_RUN } = dry_run,
        { attr::ERROR_TYPE } = tracing::field::Empty
    )
)]
pub async fn migrate_legacy_ids(
    task_service: Arc<RwLock<TaskService>>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    user_id: Option<UserId>,
    dry_run: bool,
) -> Result<(), UniversalInboxError> {
    let result: Result<(), UniversalInboxError> = async move {
    let service = task_service.read().await;

    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while loading legacy Todoist items")?;
    let legacy_items = service
        .todoist_service
        .repository
        .find_legacy_todoist_items(&mut transaction, user_id)
        .await?;
    transaction
        .commit()
        .await
        .context("Failed to commit transaction while loading legacy Todoist items")?;
    info!(
        "Found {} Todoist items with a legacy ID",
        legacy_items.len()
    );

    let mut items_per_connection: HashMap<(UserId, IntegrationConnectionId), Vec<ThirdPartyItem>> =
        HashMap::new();
    for item in legacy_items {
        items_per_connection
            .entry((item.user_id, item.integration_connection_id))
            .or_default()
            .push(item);
    }

    // Each connection is migrated in its own transaction: a failure (e.g. a
    // revoked access token) only rolls back that user's changes and the
    // command carries on with the next one.
    let mut failed_user_ids = Vec::new();
    for ((user_id, integration_connection_id), items) in items_per_connection {
        if let Err(err) = migrate_connection_legacy_ids(
            &service,
            &integration_connection_service,
            user_id,
            integration_connection_id,
            &items,
            dry_run,
        )
        .await
        {
            error!(
                "Failed to migrate {} legacy Todoist items of user {user_id} (integration connection {integration_connection_id}), skipping: {err:?}",
                items.len()
            );
            failed_user_ids.push(user_id);
        }
    }

    if !failed_user_ids.is_empty() {
        warn!(
            "Legacy Todoist IDs migration failed for {} users: {failed_user_ids:?}",
            failed_user_ids.len()
        );
    }

    Ok(())
}.await;
    result.record_span_error()
}

async fn migrate_connection_legacy_ids(
    service: &TaskService,
    integration_connection_service: &RwLock<IntegrationConnectionService>,
    user_id: UserId,
    integration_connection_id: IntegrationConnectionId,
    items: &[ThirdPartyItem],
    dry_run: bool,
) -> Result<(), UniversalInboxError> {
    let mut transaction = service.begin().await.context(format!(
        "Failed to create new transaction while migrating legacy Todoist IDs for user {user_id}"
    ))?;
    let Some((access_token, _)) = integration_connection_service
        .read()
        .await
        .find_access_token_for_connection(&mut transaction, integration_connection_id, user_id)
        .await?
    else {
        warn!(
            "Skipping {} legacy Todoist items of user {user_id}: no access token for integration connection {integration_connection_id}",
            items.len()
        );
        transaction
            .rollback()
            .await
            .context("Failed to rollback transaction while migrating legacy Todoist IDs")?;
        return Ok(());
    };

    // On error, `transaction` is dropped and thus rolled back
    let report = service
        .todoist_service
        .migrate_legacy_items(&mut transaction, items, &access_token)
        .await?;
    info!(
        "User {user_id}: {} Todoist items migrated, {} duplicates marked as deleted, {} unmapped{}",
        report.migrated,
        report.duplicates,
        report.unmapped,
        if dry_run { " (dry-run)" } else { "" }
    );

    if dry_run {
        transaction.rollback().await.context(
            "Failed to rollback (dry-run) transaction while migrating legacy Todoist IDs",
        )?;
    } else {
        transaction
            .commit()
            .await
            .context("Failed to commit transaction while migrating legacy Todoist IDs")?;
    }

    Ok(())
}
