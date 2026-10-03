use std::{future::Future, sync::Arc, time::Duration};

use anyhow::Context;
use cached::{AsyncRedisCache, ConcurrentCacheBase, ConcurrentCachedAsync, RedisCacheError};
use once_cell::sync::Lazy;
use redis::{Client, Script, aio::ConnectionManager};
use serde::{Serialize, de::DeserializeOwned};
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::{
    configuration::Settings,
    observability::{instrument_client_call, redis_client_span, spans::OTHER_ERROR_TYPE},
    universal_inbox::UniversalInboxError,
};

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
) -> TracedRedisCache<T>
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
    let inner = AsyncRedisCache::builder(prefix)
        .ttl(ttl_in_seconds)
        .refresh_on_hit(refresh)
        .namespace(&namespace)
        .connection_string(&settings.redis.connection_string())
        .connection_manager(true)
        .build()
        .await
        .expect("error building Redis cache");
    TracedRedisCache {
        inner,
        prefix: prefix.to_string(),
    }
}

/// An [`AsyncRedisCache`] giving each Redis command an INFO client span: the
/// `cached` crate talks to Redis on its own connection, outside any traced client.
pub struct TracedRedisCache<V> {
    inner: AsyncRedisCache<String, V>,
    prefix: String,
}

impl<V> TracedRedisCache<V> {
    async fn traced<T>(
        &self,
        operation: &'static str,
        command: impl Future<Output = Result<T, RedisCacheError>>,
    ) -> Result<T, RedisCacheError> {
        instrument_client_call(
            redis_client_span(operation, &self.prefix),
            command,
            redis_error_type,
        )
        .await
    }
}

fn redis_error_type(error: &RedisCacheError) -> String {
    match error {
        RedisCacheError::Redis { .. } => "redis",
        RedisCacheError::Pool { .. } => "pool",
        RedisCacheError::CacheDeserialization { .. } => "deserialization",
        _ => OTHER_ERROR_TYPE,
    }
    .to_string()
}

impl<V> ConcurrentCacheBase for TracedRedisCache<V> {
    type Error = RedisCacheError;
}

impl<V> ConcurrentCachedAsync<String, V> for TracedRedisCache<V>
where
    V: Serialize + DeserializeOwned + Send + Sync,
{
    async fn async_cache_get(&self, k: &String) -> Result<Option<V>, Self::Error> {
        self.traced("GET", self.inner.async_cache_get(k)).await
    }

    async fn async_cache_set(&self, k: String, v: V) -> Result<Option<V>, Self::Error> {
        self.traced("SET", self.inner.async_cache_set(k, v)).await
    }

    async fn async_cache_remove(&self, k: &String) -> Result<Option<V>, Self::Error> {
        self.traced("DEL", self.inner.async_cache_remove(k)).await
    }

    async fn async_cache_remove_entry(
        &self,
        k: &String,
    ) -> Result<Option<(String, V)>, Self::Error> {
        self.traced("DEL", self.inner.async_cache_remove_entry(k))
            .await
    }

    async fn async_cache_contains(&self, k: &String) -> Result<bool, Self::Error>
    where
        Self: Sync,
    {
        self.traced("EXISTS", self.inner.async_cache_contains(k))
            .await
    }

    async fn async_cache_clear(&self) -> Result<(), Self::Error>
    where
        Self: Sync,
    {
        self.traced("SCAN", self.inner.async_cache_clear()).await
    }

    async fn async_cache_reset(&self) -> Result<(), Self::Error>
    where
        Self: Sync,
    {
        self.traced("SCAN", self.inner.async_cache_reset()).await
    }
}
