use std::sync::Arc;

use anyhow::{Context, anyhow};
use chrono::{TimeDelta, Utc};
use tokio::sync::RwLock;
use tracing::{error, info};

use universal_inbox::integration_connection::provider::IntegrationProviderKind;

use crate::universal_inbox::{
    UniversalInboxError,
    integration_connection::service::{GrantRevocationRetryPolicy, IntegrationConnectionService},
};

#[tracing::instrument(
    name = "refresh-oauth-tokens",
    level = "info",
    skip(integration_connection_service),
    err
)]
pub async fn refresh_oauth_tokens(
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    provider_kind: Option<IntegrationProviderKind>,
    minutes_before_expiry: i64,
) -> Result<(), UniversalInboxError> {
    let provider_kind_string = provider_kind
        .map(|s| s.to_string())
        .unwrap_or_else(|| "all providers".to_string());
    info!(
        "Refreshing OAuth tokens expiring within {minutes_before_expiry} minutes for {provider_kind_string}"
    );

    let service = integration_connection_service.read().await;
    let mut transaction = service.begin().await.context(format!(
        "Failed to create new transaction while refreshing OAuth tokens for {provider_kind_string}"
    ))?;
    let result = service
        .refresh_expiring_tokens(&mut transaction, minutes_before_expiry, provider_kind)
        .await;

    match result {
        Ok((refreshed, failed)) => {
            info!(
                "OAuth token refresh complete for {provider_kind_string}: {refreshed} refreshed, {failed} failed"
            );
            if failed > 0 {
                error!("{failed} token refresh(es) failed for {provider_kind_string}");
            }
            transaction
                .commit()
                .await
                .context(format!(
                    "Failed to commit transaction while refreshing OAuth tokens for {provider_kind_string}"
                ))?;
            Ok(())
        }
        Err(err) => {
            error!("Failed to refresh OAuth tokens for {provider_kind_string}: {err:?}");
            Err(err)
        }
    }
}

#[tracing::instrument(
    name = "retry-oauth-grant-revocations",
    level = "info",
    skip(integration_connection_service),
    err
)]
pub async fn retry_oauth_grant_revocations(
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    max_revocations: usize,
    retry_policy: GrantRevocationRetryPolicy,
) -> Result<(), UniversalInboxError> {
    let service = integration_connection_service.read().await;
    let (completed, failed) = service
        .retry_due_grant_revocations(max_revocations, &retry_policy)
        .await?;
    info!("OAuth grant revocation retries complete: {completed} completed, {failed} failed");
    Ok(())
}

#[tracing::instrument(
    name = "pause-slack-connections",
    level = "info",
    skip(integration_connection_service),
    err
)]
pub async fn pause_slack_connections(
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    inactivity_threshold_days: i64,
    inactivity_warning_days: i64,
    failing_threshold_days: i64,
) -> Result<(), UniversalInboxError> {
    let days_ago = |days: i64| {
        TimeDelta::try_days(days)
            .map(|delta| Utc::now() - delta)
            .ok_or_else(|| {
                UniversalInboxError::Unexpected(anyhow!("Invalid number of days: {days}"))
            })
    };
    let inactive_before = days_ago(inactivity_threshold_days)?;
    let failing_before = days_ago(failing_threshold_days)?;
    let service = integration_connection_service.read().await;

    // Users are warned `inactivity_warning_days` before the pause, and a
    // connection is paused at the earliest `inactivity_warning_days` after its
    // warning, even when its user was already inactive for longer.
    let (failed_warnings, warned_before) = if inactivity_warning_days > 0 {
        info!(
            "Warning users inactive for more than {} days that their Slack connections will be paused",
            inactivity_threshold_days - inactivity_warning_days
        );
        let (_, failed_warnings) = service
            .warn_integration_connections_of_inactive_users(
                IntegrationProviderKind::Slack,
                days_ago(inactivity_threshold_days - inactivity_warning_days)?,
                Utc::now() + TimeDelta::days(inactivity_warning_days),
            )
            .await?;
        (failed_warnings, Some(days_ago(inactivity_warning_days)?))
    } else {
        (0, None)
    };

    info!(
        "Pausing Slack connections of users inactive for more than {inactivity_threshold_days} days"
    );
    let (paused_inactive, failed_inactive) = service
        .pause_integration_connections_of_inactive_users(
            IntegrationProviderKind::Slack,
            inactive_before,
            warned_before,
        )
        .await?;

    info!("Pausing Slack connections failing for more than {failing_threshold_days} days");
    let (paused_failing, failed_failing) = service
        .pause_long_failing_integration_connections(IntegrationProviderKind::Slack, failing_before)
        .await?;

    if failed_warnings > 0 {
        error!(
            "{failed_warnings} user(s) could not be warned that their Slack connection will be paused"
        );
    }
    let failed = failed_inactive + failed_failing;
    if failed > 0 {
        error!("{failed} Slack connection(s) could not be paused");
    }
    info!(
        "Paused {paused_inactive} Slack connection(s) of inactive users and {paused_failing} long failing Slack connection(s)"
    );
    Ok(())
}
