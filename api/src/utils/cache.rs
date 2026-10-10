use std::{future::Future, marker::PhantomData, sync::Arc, time::Duration};

use anyhow::Context;
use cached::{AsyncRedisCache, ConcurrentCacheBase, ConcurrentCachedAsync, RedisCacheError};
use once_cell::sync::Lazy;
use redis::{
    Client, Script,
    aio::{ConnectionManager, ConnectionManagerConfig},
};
use serde::{Serialize, de::DeserializeOwned};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::{
    configuration::{RedisSettings, Settings},
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
    pub async fn new(redis_settings: &RedisSettings) -> Result<Self, UniversalInboxError> {
        let client = Client::open(redis_settings.connection_string()).with_context(|| {
            format!(
                "Failed to open setup Redis client for {}",
                redis_settings.safe_connection_string()
            )
        })?;
        let mut config = ConnectionManagerConfig::new();
        if let Some(timeout) = redis_settings.response_timeout_in_milliseconds {
            config = config.set_response_timeout(Some(Duration::from_millis(timeout)));
        }
        let connection_manager = client
            .get_connection_manager_with_config(config)
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

/// Suffix of every cache prefix, naming the encoding of the cached values: entries
/// written in another format (MessagePack before) live under other keys and are never
/// read, they expire with their TTL.
const VALUE_FORMAT_SUFFIX: &str = ".json";

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
    let inner = AsyncRedisCache::builder(format!("{prefix}{VALUE_FORMAT_SUFFIX}"))
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
        value_type: PhantomData,
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error(transparent)]
    Redis(#[from] RedisCacheError),
    #[error("Failed to encode the cached value as JSON: {0}")]
    Json(#[from] serde_json::Error),
}

/// An [`AsyncRedisCache`] giving each Redis command an INFO client span: the
/// `cached` crate talks to Redis on its own connection, outside any traced client.
///
/// Values are stored as JSON rather than in the `cached` crate's compact MessagePack
/// encoding, which writes structs as positional arrays without field names: types
/// relying on tagged or untagged enums, or on skipped fields (most slack-morphism
/// models), do not decode back reliably from it. Since slack-morphism 2.30, a cached
/// `SlackBlock` silently came back as `SlackBlock::Unknown`.
pub struct TracedRedisCache<V> {
    inner: AsyncRedisCache<String, String>,
    prefix: String,
    value_type: PhantomData<V>,
}

impl<V> TracedRedisCache<V>
where
    V: DeserializeOwned,
{
    async fn traced<T>(
        &self,
        operation: &'static str,
        command: impl Future<Output = Result<T, RedisCacheError>>,
    ) -> Result<T, CacheError> {
        Ok(instrument_client_call(
            redis_client_span(operation, &self.prefix),
            command,
            redis_error_type,
        )
        .await?)
    }

    /// Decode a cached value. One that cannot be decoded is a cache miss: the value is
    /// fetched again and overwrites it, rather than failing the request.
    fn decode(&self, cached_value: Option<String>) -> Option<V> {
        let cached_value = cached_value?;
        serde_json::from_str(&cached_value)
            .inspect_err(|err| {
                // Neither the key nor the serde message: both can hold user data
                warn!(
                    "Ignoring undecodable `{}` cache entry: {:?} error at line {} column {}",
                    self.prefix,
                    err.classify(),
                    err.line(),
                    err.column()
                )
            })
            .ok()
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
    type Error = CacheError;
}

impl<V> ConcurrentCachedAsync<String, V> for TracedRedisCache<V>
where
    V: Serialize + DeserializeOwned + Send + Sync,
{
    async fn async_cache_get(&self, k: &String) -> Result<Option<V>, Self::Error> {
        let cached_value = self.traced("GET", self.inner.async_cache_get(k)).await?;
        Ok(self.decode(cached_value))
    }

    async fn async_cache_set(&self, k: String, v: V) -> Result<Option<V>, Self::Error> {
        let value = serde_json::to_string(&v)?;
        let previous_value = self
            .traced("SET", self.inner.async_cache_set(k, value))
            .await?;
        Ok(self.decode(previous_value))
    }

    async fn async_cache_remove(&self, k: &String) -> Result<Option<V>, Self::Error> {
        let removed_value = self.traced("DEL", self.inner.async_cache_remove(k)).await?;
        Ok(self.decode(removed_value))
    }

    async fn async_cache_remove_entry(
        &self,
        k: &String,
    ) -> Result<Option<(String, V)>, Self::Error> {
        let removed_entry = self
            .traced("DEL", self.inner.async_cache_remove_entry(k))
            .await?;
        Ok(removed_entry.and_then(|(key, value)| {
            let value = self.decode(Some(value))?;
            Some((key, value))
        }))
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
