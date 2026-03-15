use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::{error, info};

use universal_inbox::integration_connection::provider::IntegrationProviderKind;

use crate::universal_inbox::{
    UniversalInboxError, integration_connection::service::IntegrationConnectionService,
};

#[tracing::instrument(
    name = "migrate-oauth-tokens-command",
    level = "info",
    skip(integration_connection_service),
    err
)]
pub async fn migrate_oauth_tokens(
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    provider_kind: Option<IntegrationProviderKind>,
    dry_run: bool,
) -> Result<(), UniversalInboxError> {
    let provider_kind_string = provider_kind
        .map(|s| s.to_string())
        .unwrap_or_else(|| "all".to_string());
    info!(
        "Migrating {provider_kind_string} Nango OAuth tokens to local management (dry_run={dry_run})"
    );
    let service = integration_connection_service.read().await;

    let result = service.migrate_nango_tokens(provider_kind, dry_run).await;

    match &result {
        Ok((migrated, failed)) => {
            info!(
                "Migration complete: {migrated} migrated, {failed} failed for {provider_kind_string} providers"
            );
        }
        Err(err) => {
            error!("Failed to migrate {provider_kind_string} OAuth tokens: {err:?}");
        }
    };

    result.map(|_| ())
}
