use std::sync::Arc;

use anyhow::Context;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use universal_inbox::user::UserId;

use crate::{
    integrations::slack::SlackService,
    repository::integration_connection::SlackIntegrationConnectionWithoutContext,
    universal_inbox::{
        UniversalInboxError, integration_connection::service::IntegrationConnectionService,
    },
};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct BackfillTeamIdReport {
    pub updated: usize,
    /// Connections without a stored OAuth credential (never completed the OAuth flow)
    pub skipped_without_credential: usize,
    /// Connections with a credential but no usable access token (not `Validated`)
    pub skipped_without_access_token: usize,
    /// Connections whose access token was rejected (expired, revoked, ...)
    pub failed: usize,
}

/// One-off backfill of the `SlackContext` (hence `team_id`) of Slack
/// integration connections created before it was captured from the OAuth
/// response: they are invisible to `team_id` lookups (Slack webhook events).
#[tracing::instrument(
    name = "slack-backfill-team-id-command",
    level = "info",
    skip(integration_connection_service, slack_service),
    err
)]
pub async fn backfill_team_id(
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    slack_service: Arc<SlackService>,
    user_id: Option<UserId>,
    dry_run: bool,
) -> Result<BackfillTeamIdReport, UniversalInboxError> {
    let service = integration_connection_service.read().await;

    let mut transaction = service.begin().await.context(
        "Failed to create new transaction while loading Slack connections without context",
    )?;
    let connections = service
        .find_slack_integration_connections_without_context(&mut transaction, user_id)
        .await?;
    transaction
        .commit()
        .await
        .context("Failed to commit transaction while loading Slack connections without context")?;
    info!(
        "Found {} Slack integration connections without context",
        connections.len()
    );

    let mut report = BackfillTeamIdReport::default();
    for connection in connections {
        if !connection.has_credential {
            info!(
                "Skipping Slack integration connection {} of user {} ({}): no stored credential",
                connection.id, connection.user_id, connection.status
            );
            report.skipped_without_credential += 1;
            continue;
        }

        match backfill_connection_team_id(&service, &slack_service, &connection, dry_run).await {
            Ok(true) => report.updated += 1,
            Ok(false) => report.skipped_without_access_token += 1,
            Err(err) => {
                error!(
                    "Failed to backfill Slack team context of integration connection {} of user {}, skipping: {err:?}",
                    connection.id, connection.user_id
                );
                report.failed += 1;
            }
        }
    }

    info!(
        "Slack team context backfill{}: {} updated, {} skipped without credential, {} skipped without access token, {} failed",
        if dry_run { " (dry-run)" } else { "" },
        report.updated,
        report.skipped_without_credential,
        report.skipped_without_access_token,
        report.failed
    );

    Ok(report)
}

/// Returns `false` when the connection has no usable access token.
async fn backfill_connection_team_id(
    service: &IntegrationConnectionService,
    slack_service: &SlackService,
    connection: &SlackIntegrationConnectionWithoutContext,
    dry_run: bool,
) -> Result<bool, UniversalInboxError> {
    let mut transaction = service.begin().await.context(format!(
        "Failed to create new transaction while backfilling Slack team context of integration connection {}",
        connection.id
    ))?;
    let Some((access_token, integration_connection)) = service
        .find_access_token_for_connection(&mut transaction, connection.id, connection.user_id)
        .await?
    else {
        warn!(
            "Skipping Slack integration connection {} of user {} ({}): no access token",
            connection.id, connection.user_id, connection.status
        );
        transaction
            .rollback()
            .await
            .context("Failed to rollback transaction while backfilling Slack team context")?;
        return Ok(false);
    };

    let team_id = slack_service
        .ensure_team_context(&mut transaction, &access_token, &integration_connection)
        .await?;
    info!(
        "Slack integration connection {} of user {}: team context set to {team_id:?}{}",
        connection.id,
        connection.user_id,
        if dry_run { " (dry-run)" } else { "" }
    );

    if dry_run {
        transaction.rollback().await.context(
            "Failed to rollback (dry-run) transaction while backfilling Slack team context",
        )?;
    } else {
        transaction
            .commit()
            .await
            .context("Failed to commit transaction while backfilling Slack team context")?;
    }

    Ok(true)
}
