use anyhow::Context;
use redis::{aio::ConnectionManager, Client, Script};
use tracing::debug;

use crate::universal_inbox::UniversalInboxError;

pub struct Cache {
    pub connection_manager: ConnectionManager,
}

pub const CACHE_NAMESPACE: &str = "universal-inbox:cache:";

impl Cache {
    pub async fn new(redis_address: String) -> Result<Self, UniversalInboxError> {
        let client = Client::open(redis_address)
            .context("Failed to open setup Redis client for {redis_address}")?;
        let connection_manager = client
            .get_connection_manager()
            .await
            .context("Failed to get connection manager for Redis client")?;
        Ok(Cache { connection_manager })
    }

    pub async fn clear(&self, prefix: &Option<String>) -> Result<(), UniversalInboxError> {
        let mut connection = self.connection_manager.clone();
        let full_prefix = prefix
            .as_ref()
            .map(|p| format!("{CACHE_NAMESPACE}{p}"))
            .unwrap_or(CACHE_NAMESPACE.to_string());
        let pattern = format!("{full_prefix}*");

        let deleted_keys_count: usize =
            Script::new(include_str!("../../scripts/lua/clear_cache.lua"))
                .arg(pattern.clone())
                .invoke_async(&mut connection)
                .await
                .context("Failed to clear cache")?;

        debug!("Cleared Redis {deleted_keys_count} cache entries with pattern: `{pattern}`");
        Ok(())
    }
}
