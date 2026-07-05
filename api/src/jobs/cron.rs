use anyhow::Context;
use apalis::prelude::*;
use apalis_cron::CronContext;
use apalis_redis::RedisStorage;
use chrono::{DateTime, Utc};
use redis::{AsyncCommands, ExistenceCheck, SetExpiry, SetOptions};
use tracing::info;

use crate::{
    configuration::RefreshOAuthTokensCronSettings, jobs::UniversalInboxJob,
    universal_inbox::UniversalInboxError, utils::cache::Cache,
};

/// Cron tick request for the `refresh-oauth-tokens` job. Carries no data; the
/// scheduled tick timestamp is injected via [`CronContext`].
#[derive(Debug, Clone, Default)]
pub struct RefreshOAuthTokensCronTick;

/// Handles a cron tick by electing a single winner across all worker processes
/// (per-tick Redis lock) and enqueuing a durable `RefreshOAuthTokens` job on
/// the shared Redis-backed queue, executed once by the regular worker pool.
#[tracing::instrument(
    name = "refresh-oauth-tokens-cron-tick",
    level = "info",
    skip_all,
    fields(cron.tick = %ctx.get_timestamp()),
    err
)]
pub async fn handle_refresh_oauth_tokens_cron_tick(
    _tick: RefreshOAuthTokensCronTick,
    ctx: CronContext<Utc>,
    storage: Data<RedisStorage<UniversalInboxJob>>,
    cache: Data<Cache>,
    settings: Data<RefreshOAuthTokensCronSettings>,
) -> Result<(), UniversalInboxError> {
    if !try_acquire_cron_tick_lock(
        &cache,
        "refresh-oauth-tokens",
        ctx.get_timestamp(),
        settings.lock_ttl_seconds,
    )
    .await?
    {
        info!("Tick already handled by another worker process, skipping");
        return Ok(());
    }

    let mut storage = (*storage).clone();
    storage
        .push(UniversalInboxJob::RefreshOAuthTokens {
            minutes_before_expiry: settings.minutes_before_expiry,
        })
        .await
        .context("Failed to enqueue RefreshOAuthTokens job")?;
    info!("Enqueued RefreshOAuthTokens job");
    Ok(())
}

/// Acquires a distributed lock for the given cron job and tick using Redis
/// `SET NX EX`. The key is derived from the scheduled tick timestamp, which is
/// identical across processes, so exactly one process wins per tick. The TTL
/// only bounds the key's lifetime; deduplication correctness comes from the
/// per-tick key.
pub async fn try_acquire_cron_tick_lock(
    cache: &Cache,
    job_name: &str,
    tick: &DateTime<Utc>,
    lock_ttl_seconds: u64,
) -> Result<bool, UniversalInboxError> {
    let mut connection = cache.connection_manager.clone();
    let key = format!("universal-inbox:cron:{job_name}:{}", tick.timestamp());
    let options = SetOptions::default()
        .conditional_set(ExistenceCheck::NX)
        .with_expiration(SetExpiry::EX(lock_ttl_seconds));
    let acquired: Option<String> = connection
        .set_options(&key, "locked", options)
        .await
        .context(format!("Failed to acquire cron lock for {job_name}"))?;
    Ok(acquired.is_some())
}
