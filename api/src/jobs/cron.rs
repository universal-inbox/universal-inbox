use anyhow::Context;
use apalis::prelude::*;
use apalis_cron::CronContext;
use apalis_redis::RedisStorage;
use chrono::{DateTime, TimeDelta, Utc};
use redis::{AsyncCommands, ExistenceCheck, Script, SetExpiry, SetOptions};
use tracing::{info, warn};

use crate::{
    configuration::{RefreshOAuthTokensCronSettings, VacuumJobsCronSettings},
    jobs::UniversalInboxJob,
    universal_inbox::UniversalInboxError,
    utils::cache::Cache,
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

/// Cron tick request for the `vacuum-jobs` job. Carries no data; the scheduled
/// tick timestamp is injected via [`CronContext`].
#[derive(Debug, Clone, Default)]
pub struct VacuumJobsCronTick;

/// Handles a cron tick by electing a single winner across all worker processes
/// (per-tick Redis lock) and purging completed jobs older than the retention
/// window from the Redis job queue.
///
/// The purge runs inline instead of being enqueued as a [`UniversalInboxJob`]:
/// such a job would store its own payload in the very hash it is purging, and
/// would depend on the worker pool being healthy exactly when the queue is
/// degraded. This is storage maintenance, not user work.
///
/// [`apalis_redis::RedisStorage::vacuum`] is deliberately not used: it ignores
/// the `::result` hash (18.4 MB leaked during the 2026-08-12 outage) and reads
/// the whole done jobs set in a single Lua invocation, blocking the Redis event
/// loop for seconds once the backlog grows.
#[tracing::instrument(
    name = "vacuum-jobs-cron-tick",
    level = "info",
    skip_all,
    fields(cron.tick = %ctx.get_timestamp()),
    err
)]
pub async fn handle_vacuum_jobs_cron_tick(
    _tick: VacuumJobsCronTick,
    ctx: CronContext<Utc>,
    storage: Data<RedisStorage<UniversalInboxJob>>,
    cache: Data<Cache>,
    settings: Data<VacuumJobsCronSettings>,
) -> Result<(), UniversalInboxError> {
    if !try_acquire_cron_tick_lock(
        &cache,
        "vacuum-jobs",
        ctx.get_timestamp(),
        settings.lock_ttl_seconds,
    )
    .await?
    {
        info!("Tick already handled by another worker process, skipping");
        return Ok(());
    }

    // Key names are derived from the storage configuration, never hardcoded, so
    // they follow the queue's namespace. The `::result` suffix is hardcoded in
    // apalis-redis' `done_job.lua`.
    let config = storage.get_config();
    let done_jobs_set = config.done_jobs_set();
    let job_data_hash = config.job_data_hash();
    let job_result_hash = format!("{job_data_hash}::result");
    let mut connection = storage.get_connection().clone();

    let cutoff = (Utc::now()
        - TimeDelta::try_hours(settings.retention_hours).unwrap_or_else(|| {
            panic!(
                "Invalid `retention_hours` value: {}",
                settings.retention_hours
            )
        }))
    .timestamp();
    let vacuum_jobs = Script::new(include_str!("../../scripts/lua/vacuum_jobs.lua"));

    let mut purged_jobs_count = 0;
    let mut batches = 0;
    while batches < settings.max_batches_per_tick {
        let purged_jobs: usize = vacuum_jobs
            .key(&done_jobs_set)
            .key(&job_data_hash)
            .key(&job_result_hash)
            .arg(cutoff)
            .arg(settings.batch_size)
            .invoke_async(&mut connection)
            .await
            .context("Failed to vacuum completed jobs")?;

        purged_jobs_count += purged_jobs;
        batches += 1;
        if purged_jobs < settings.batch_size {
            info!("Vacuumed {purged_jobs_count} completed jobs in {batches} batch(es)");
            return Ok(());
        }
    }

    warn!(
        "Vacuumed {purged_jobs_count} completed jobs but reached the {} batches per tick limit: \
         jobs older than {} hours remain, the `vacuum-jobs` schedule or batch size is too small",
        settings.max_batches_per_tick, settings.retention_hours
    );
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
