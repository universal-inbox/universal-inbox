use anyhow::Context;
use cached::AsyncRedisCache;
use once_cell::sync::Lazy;
use redis::{aio::ConnectionManager, Client, Script};
use serde::{de::DeserializeOwned, Serialize};
use tracing::{debug, info};

use crate::{configuration::Settings, universal_inbox::UniversalInboxError};

struct Config {
    settings: Settings,
}

impl Config {
    fn load() -> Self {
        Self {
            settings: Settings::new().unwrap(),
        }
    }
}

static CONFIG: Lazy<Config> = Lazy::new(Config::load);

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

pub async fn build_redis_cache<T>(namespace: &str, ttl: u64) -> AsyncRedisCache<String, T>
where
    T: Serialize + DeserializeOwned + Send + Sync,
{
    let settings = &CONFIG.settings;
    info!(
        "Connecting to Redis server for caching on {}",
        &settings.redis.safe_connection_string()
    );
    AsyncRedisCache::new(namespace, ttl)
        .set_refresh(true)
        .set_namespace(CACHE_NAMESPACE)
        .set_connection_string(&settings.redis.connection_string())
        .build()
        .await
        .expect("error building Redis cache")
}
