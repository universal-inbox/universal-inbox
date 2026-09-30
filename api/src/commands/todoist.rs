use std::{collections::HashMap, sync::Arc};

use anyhow::Context;
use tokio::sync::RwLock;
use tracing::{info, warn};

use universal_inbox::{
    integration_connection::IntegrationConnectionId, third_party::item::ThirdPartyItem,
    user::UserId,
};

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
    skip(task_service, integration_connection_service),
    err
)]
pub async fn migrate_legacy_ids(
    task_service: Arc<RwLock<TaskService>>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    user_id: Option<UserId>,
    dry_run: bool,
) -> Result<(), UniversalInboxError> {
    let service = task_service.read().await;
    let todoist_service = service.todoist_service.clone();

    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while loading legacy Todoist items")?;
    let legacy_items = todoist_service
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

    for ((user_id, integration_connection_id), items) in items_per_connection {
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
            continue;
        };

        let report = todoist_service
            .migrate_legacy_items(&mut transaction, &items, &access_token)
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
    }

    Ok(())
}
