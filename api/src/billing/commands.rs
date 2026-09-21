//! Operator-facing billing CLI subcommand.
//!
//! Mounted from `crate::commands::Commands::Billing { action }`. The
//! action enum and runner live here rather than in `api/src/commands/` so
//! the billing module stays the single source of truth for billing code —
//! deleting it leaves `commands/` untouched apart from the one delegating
//! arm.

use std::sync::Arc;

use clap::Subcommand;
use tracing::info;

use crate::{billing::service::BillingService, universal_inbox::UniversalInboxError};

#[derive(Subcommand, Debug, Clone)]
pub enum BillingAction {
    /// Reconcile local subscription state with Stripe. Intended for daily
    /// cron / systemd-timer / Kubernetes CronJob invocation. Refreshes every
    /// non-terminal subscription from Stripe and triggers
    /// `enforce_free_plan_compliance` for any user whose
    /// `over_limit_grace_deadline` has passed.
    Reconcile {
        /// Compute everything that would change but do not write back.
        #[arg(long)]
        dry_run: bool,
    },
}

/// Run the billing subcommand. When billing is not configured on the
/// instance, log + exit with success — the operator's cron job is still
/// safe to run on a self-hosted box.
pub async fn run(
    action: &BillingAction,
    billing_service: Option<Arc<BillingService>>,
) -> Result<(), UniversalInboxError> {
    let billing = match billing_service {
        Some(b) => b,
        None => {
            info!("[billing] is not configured on this instance — `billing` subcommand is a no-op");
            return Ok(());
        }
    };

    match action {
        BillingAction::Reconcile { dry_run } => {
            // The reconcile owns its own per-row transactions, so nothing is
            // held open while Stripe is answering.
            let report = billing.reconcile_subscriptions(*dry_run).await?;
            let mut tx = billing.begin().await?;
            // Prune the idempotency table in the same pass so it can't grow
            // unbounded. No-op under --dry-run.
            let pruned_stripe_events = billing.prune_stripe_events(&mut tx, *dry_run).await?;
            tx.commit().await.map_err(|err| {
                UniversalInboxError::Unexpected(anyhow::anyhow!(
                    "Failed to commit reconcile transaction: {err}"
                ))
            })?;
            info!(
                "Stripe reconciliation complete: refreshed={} adopted={} grace_expired={} pruned_stripe_events={pruned_stripe_events} dry_run={dry_run}",
                report.refreshed, report.adopted, report.grace_expired
            );
            Ok(())
        }
    }
}
