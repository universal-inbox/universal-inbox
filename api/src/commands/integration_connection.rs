use std::sync::Arc;

use chrono::{DateTime, TimeDelta, Utc};
use tokio::sync::RwLock;
use tracing::info;

use crate::observability::RecordSpanError;
use crate::observability::attr;
use crate::universal_inbox::{
    UniversalInboxError,
    integration_connection::service::{IntegrationConnectionService, PauseWithoutEmailReport},
};

/// One-off pause, without any email, of the connections of users inactive
/// since `inactive_before` and of the connections failing for more than
/// `failing_threshold_days`, to run before enabling the
/// `pause-integration-connections` cron.
#[tracing::instrument(
    name = "integration-connection-pause-without-email-command",
    level = "info",
    skip_all,
    fields(
        { attr::INTEGRATION_CONNECTION_INACTIVE_BEFORE } = inactive_before.to_rfc3339(),
        { attr::INTEGRATION_CONNECTION_FAILING_THRESHOLD_DAYS } = failing_threshold_days,
        { attr::COMMAND_DRY_RUN } = dry_run,
        { attr::ERROR_TYPE } = tracing::field::Empty
    )
)]
pub async fn pause_without_email(
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    inactive_before: DateTime<Utc>,
    failing_threshold_days: i64,
    dry_run: bool,
) -> Result<PauseWithoutEmailReport, UniversalInboxError> {
    let result: Result<PauseWithoutEmailReport, UniversalInboxError> = async move {
        let now = Utc::now();
        if inactive_before > now {
            return Err(UniversalInboxError::InvalidInputData {
                source: None,
                user_error: format!("--inactive-before {inactive_before} is in the future"),
            });
        }
        let failing_before = TimeDelta::try_days(failing_threshold_days)
            .filter(|_| failing_threshold_days >= 0)
            .map(|delta| now - delta)
            .ok_or_else(|| UniversalInboxError::InvalidInputData {
                source: None,
                user_error: format!(
                    "Invalid --failing-threshold-days {failing_threshold_days}"
                ),
            })?;

        let report = integration_connection_service
            .read()
            .await
            .pause_existing_integration_connections_without_email(
                inactive_before,
                failing_before,
                dry_run,
            )
            .await?;
        let prefix = if dry_run { "[dry-run] Would have paused" } else { "Paused" };
        info!(
            "{prefix} {} integration connection(s) of users inactive since {inactive_before} ({} failed) and {} failing integration connection(s) ({} failed), without email",
            report.paused_inactive,
            report.failed_inactive,
            report.paused_failing,
            report.failed_failing
        );
        Ok(report)
    }
    .await;
    result.record_span_error()
}
