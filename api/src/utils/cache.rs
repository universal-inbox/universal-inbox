use std::{sync::Arc, time::Duration};

use anyhow::Context;
use cached::AsyncRedisCache;
use once_cell::sync::Lazy;
use redis::{Client, Script, aio::ConnectionManager};
use serde::{Serialize, de::DeserializeOwned};
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::{configuration::Settings, universal_inbox::UniversalInboxError};

pub struct Config {
    settings: Settings,
    namespace: Arc<RwLock<String>>,
}

impl Config {
    fn load() -> Self {
        Self {
            settings: Settings::new().unwrap(),
            namespace: Arc::new(RwLock::new("universal-inbox:cache:".to_string())),
        }
    }

    async fn namespace(&self) -> String {
        self.namespace.read().await.clone()
    }

    async fn set_namespace(&self, namespace: String) {
        *(self.namespace.write().await) = namespace;
    }
}

static CACHE_CONFIG: Lazy<Config> = Lazy::new(Config::load);

#[derive(Clone)]
pub struct Cache {
    pub connection_manager: ConnectionManager,
}

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
        let namespace = CACHE_CONFIG.namespace().await;
        let full_prefix = prefix
            .as_ref()
            .map(|p| format!("{namespace}{p}"))
            .unwrap_or(namespace.to_string());
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

    pub async fn set_namespace(namespace: String) {
        CACHE_CONFIG.set_namespace(namespace).await;
    }
}

pub async fn build_redis_cache<T>(
    prefix: &str,
    ttl_in_seconds: Duration,
    refresh: bool,
) -> AsyncRedisCache<String, T>
where
    T: Serialize + DeserializeOwned + Send + Sync,
{
    let settings = &CACHE_CONFIG.settings;
    let namespace = CACHE_CONFIG.namespace().await;
    info!(
        "Connecting to Redis server for caching on {} with namespace: {}:{}",
        &settings.redis.safe_connection_string(),
        &namespace,
        &prefix
    );
    AsyncRedisCache::new(prefix, ttl_in_seconds)
        .set_refresh(refresh)
        .set_namespace(&namespace)
        .set_connection_string(&settings.redis.connection_string())
        .build()
        .await
        .expect("error building Redis cache")
}
