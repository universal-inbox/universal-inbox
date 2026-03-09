use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::{error, info};

use universal_inbox::integration_connection::provider::IntegrationProviderKind;

use crate::universal_inbox::{
    UniversalInboxError, integration_connection::service::IntegrationConnectionService,
};

#[tracing::instrument(
    name = "refresh-oauth-tokens-command",
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
    let result = service
        .refresh_expiring_tokens(minutes_before_expiry, provider_kind)
        .await;

    match &result {
        Ok((refreshed, failed)) => {
            info!(
                "OAuth token refresh complete for {provider_kind_string}: {refreshed} refreshed, {failed} failed"
            );
            if *failed > 0 {
                error!("{failed} token refresh(es) failed for {provider_kind_string}");
            }
        }
        Err(err) => {
            error!("Failed to refresh OAuth tokens for {provider_kind_string}: {err:?}");
        }
    };

    result.map(|_| ())
}
