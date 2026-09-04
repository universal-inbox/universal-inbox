use apalis::prelude::{Data, Storage};
use apalis_cron::CronContext;
use apalis_redis::RedisStorage;
use chrono::{DateTime, TimeDelta, TimeZone, Timelike, Utc};
use redis::{AsyncCommands, aio::ConnectionManager};
use rstest::*;
use uuid::Uuid;

use universal_inbox_api::{
    configuration::{RefreshOAuthTokensCronSettings, Settings, VacuumJobsCronSettings},
    jobs::{
        UniversalInboxJob,
        cron::{
            handle_refresh_oauth_tokens_cron_tick, handle_vacuum_jobs_cron_tick,
            try_acquire_cron_tick_lock,
        },
    },
    utils::cache::Cache,
};

use crate::common::{redis_storage, settings};

#[rstest]
#[tokio::test]
async fn test_try_acquire_cron_tick_lock_dedupes_same_tick(settings: Settings) {
    let cache = Cache::new(settings.redis.connection_string())
        .await
        .expect("Failed to create cache");
    let job_name = format!("test-cron-job-{}", Uuid::new_v4());
    let tick = Utc.with_ymd_and_hms(2026, 7, 5, 12, 0, 0).unwrap();

    let first = try_acquire_cron_tick_lock(&cache, &job_name, &tick, 60)
        .await
        .expect("Failed to acquire cron lock");
    let second = try_acquire_cron_tick_lock(&cache, &job_name, &tick, 60)
        .await
        .expect("Failed to acquire cron lock");
    let next_tick =
        try_acquire_cron_tick_lock(&cache, &job_name, &tick.with_minute(5).unwrap(), 60)
            .await
            .expect("Failed to acquire cron lock");

    assert!(first, "first process should win the tick lock");
    assert!(!second, "second process should not win the same tick lock");
    assert!(next_tick, "a different tick should get its own lock");
}

#[rstest]
#[tokio::test]
async fn test_refresh_oauth_tokens_cron_tick_enqueues_job_once(
    settings: Settings,
    #[future] redis_storage: RedisStorage<UniversalInboxJob>,
) {
    let mut redis_storage = redis_storage.await;
    let cache = Cache::new(settings.redis.connection_string())
        .await
        .expect("Failed to create cache");
    let cron_settings = RefreshOAuthTokensCronSettings {
        minutes_before_expiry: 42,
        ..Default::default()
    };
    let tick = unique_tick(5);

    // Simulate 2 worker processes handling the same cron tick
    for _ in 0..2 {
        handle_refresh_oauth_tokens_cron_tick(
            Default::default(),
            CronContext::new(tick),
            Data::new(redis_storage.clone()),
            Data::new(cache.clone()),
            Data::new(cron_settings.clone()),
        )
        .await
        .expect("Failed to handle cron tick");
    }

    let queued_jobs = redis_storage
        .len()
        .await
        .expect("Failed to get Redis storage length");
    assert_eq!(
        queued_jobs, 1,
        "the same tick handled by 2 processes should enqueue exactly 1 job"
    );
}

/// Direct access to the Redis keys backing the job queue, to seed jobs and
/// assert on what the vacuum removed.
struct JobQueue {
    connection: ConnectionManager,
    done_jobs_set: String,
    job_data_hash: String,
    job_result_hash: String,
    active_jobs_list: String,
}

impl JobQueue {
    fn new(storage: &RedisStorage<UniversalInboxJob>) -> Self {
        let config = storage.get_config();
        let job_data_hash = config.job_data_hash();
        JobQueue {
            connection: storage.get_connection().clone(),
            done_jobs_set: config.done_jobs_set(),
            job_result_hash: format!("{job_data_hash}::result"),
            job_data_hash,
            active_jobs_list: config.active_jobs_list(),
        }
    }

    async fn add_completed_job(&mut self, job_id: &str, completed_at: DateTime<Utc>) {
        let _: () = self
            .connection
            .hset(&self.job_data_hash, job_id, "job payload")
            .await
            .expect("Failed to seed job data");
        let _: () = self
            .connection
            .hset(&self.job_result_hash, job_id, "job result")
            .await
            .expect("Failed to seed job result");
        let _: () = self
            .connection
            .zadd(&self.done_jobs_set, job_id, completed_at.timestamp())
            .await
            .expect("Failed to seed done job");
    }

    async fn add_active_job(&mut self, job_id: &str) {
        let _: () = self
            .connection
            .hset(&self.job_data_hash, job_id, "job payload")
            .await
            .expect("Failed to seed job data");
        let _: () = self
            .connection
            .rpush(&self.active_jobs_list, job_id)
            .await
            .expect("Failed to seed active job");
    }

    async fn has_job_data(&mut self, job_id: &str) -> bool {
        self.connection
            .hexists(&self.job_data_hash, job_id)
            .await
            .expect("Failed to check job data")
    }

    async fn has_job_result(&mut self, job_id: &str) -> bool {
        self.connection
            .hexists(&self.job_result_hash, job_id)
            .await
            .expect("Failed to check job result")
    }

    async fn is_done(&mut self, job_id: &str) -> bool {
        let score: Option<i64> = self
            .connection
            .zscore(&self.done_jobs_set, job_id)
            .await
            .expect("Failed to check done job");
        score.is_some()
    }

    /// True when the job is gone from the 3 keys the queue stores it in.
    async fn is_purged(&mut self, job_id: &str) -> bool {
        !self.has_job_data(job_id).await
            && !self.has_job_result(job_id).await
            && !self.is_done(job_id).await
    }

    /// True when the job is still stored in the 3 keys.
    async fn is_stored(&mut self, job_id: &str) -> bool {
        self.has_job_data(job_id).await
            && self.has_job_result(job_id).await
            && self.is_done(job_id).await
    }

    async fn done_jobs_count(&mut self) -> usize {
        self.connection
            .zcard(&self.done_jobs_set)
            .await
            .expect("Failed to count done jobs")
    }

    async fn active_jobs_count(&mut self) -> usize {
        self.connection
            .llen(&self.active_jobs_list)
            .await
            .expect("Failed to count active jobs")
    }
}

/// The per-tick lock key is not namespaced per test, so each test uses a
/// distinct tick to avoid stealing another test's (or another run's) lock.
fn unique_tick(offset_in_hours: i64) -> DateTime<Utc> {
    Utc::now() + TimeDelta::try_hours(offset_in_hours).unwrap()
}

fn vacuum_jobs_settings(batch_size: usize, max_batches_per_tick: usize) -> VacuumJobsCronSettings {
    VacuumJobsCronSettings {
        retention_hours: 6,
        batch_size,
        max_batches_per_tick,
        ..Default::default()
    }
}

async fn vacuum_jobs(
    redis_storage: &RedisStorage<UniversalInboxJob>,
    cache: &Cache,
    tick: DateTime<Utc>,
    cron_settings: &VacuumJobsCronSettings,
) {
    handle_vacuum_jobs_cron_tick(
        Default::default(),
        CronContext::new(tick),
        Data::new(redis_storage.clone()),
        Data::new(cache.clone()),
        Data::new(cron_settings.clone()),
    )
    .await
    .expect("Failed to handle cron tick");
}

#[rstest]
#[tokio::test]
async fn test_vacuum_jobs_cron_tick_purges_only_jobs_completed_before_the_retention_window(
    settings: Settings,
    #[future] redis_storage: RedisStorage<UniversalInboxJob>,
) {
    let redis_storage = redis_storage.await;
    let cache = Cache::new(settings.redis.connection_string())
        .await
        .expect("Failed to create cache");
    let mut queue = JobQueue::new(&redis_storage);
    let now = Utc::now();
    queue
        .add_completed_job("old-job", now - TimeDelta::try_hours(7).unwrap())
        .await;
    queue
        .add_completed_job("recent-job", now - TimeDelta::try_hours(1).unwrap())
        .await;
    queue.add_active_job("active-job").await;

    vacuum_jobs(
        &redis_storage,
        &cache,
        unique_tick(1),
        &vacuum_jobs_settings(1000, 200),
    )
    .await;

    assert!(
        queue.is_purged("old-job").await,
        "a job completed before the retention window should be purged from the done set, \
         the data hash and the result hash"
    );
    assert!(
        queue.is_stored("recent-job").await,
        "a job completed within the retention window should be kept"
    );
    assert!(
        queue.has_job_data("active-job").await,
        "the data of a job that is not completed should never be purged"
    );
    assert_eq!(
        queue.active_jobs_count().await,
        1,
        "the active jobs list should not be touched"
    );
}

#[rstest]
#[tokio::test]
async fn test_vacuum_jobs_cron_tick_purges_every_batch(
    settings: Settings,
    #[future] redis_storage: RedisStorage<UniversalInboxJob>,
) {
    let redis_storage = redis_storage.await;
    let cache = Cache::new(settings.redis.connection_string())
        .await
        .expect("Failed to create cache");
    let mut queue = JobQueue::new(&redis_storage);
    let completed_at = Utc::now() - TimeDelta::try_hours(7).unwrap();
    for i in 0..5 {
        queue
            .add_completed_job(&format!("old-job-{i}"), completed_at)
            .await;
    }

    // 5 jobs to purge, 2 per batch: the loop must run until the queue is empty
    vacuum_jobs(
        &redis_storage,
        &cache,
        unique_tick(2),
        &vacuum_jobs_settings(2, 10),
    )
    .await;

    assert_eq!(
        queue.done_jobs_count().await,
        0,
        "all completed jobs should be purged, whatever the batch size"
    );
}

#[rstest]
#[tokio::test]
async fn test_vacuum_jobs_cron_tick_stops_at_max_batches_per_tick(
    settings: Settings,
    #[future] redis_storage: RedisStorage<UniversalInboxJob>,
) {
    let redis_storage = redis_storage.await;
    let cache = Cache::new(settings.redis.connection_string())
        .await
        .expect("Failed to create cache");
    let mut queue = JobQueue::new(&redis_storage);
    let completed_at = Utc::now() - TimeDelta::try_hours(7).unwrap();
    for i in 0..5 {
        queue
            .add_completed_job(&format!("old-job-{i}"), completed_at)
            .await;
    }

    vacuum_jobs(
        &redis_storage,
        &cache,
        unique_tick(3),
        &vacuum_jobs_settings(2, 1),
    )
    .await;

    assert_eq!(
        queue.done_jobs_count().await,
        3,
        "a single batch of 2 jobs should have been purged"
    );
}

#[rstest]
#[tokio::test]
async fn test_vacuum_jobs_cron_tick_dedupes_same_tick(
    settings: Settings,
    #[future] redis_storage: RedisStorage<UniversalInboxJob>,
) {
    let redis_storage = redis_storage.await;
    let cache = Cache::new(settings.redis.connection_string())
        .await
        .expect("Failed to create cache");
    let mut queue = JobQueue::new(&redis_storage);
    let completed_at = Utc::now() - TimeDelta::try_hours(7).unwrap();
    let tick = unique_tick(4);
    let cron_settings = vacuum_jobs_settings(1000, 200);
    queue.add_completed_job("old-job", completed_at).await;

    vacuum_jobs(&redis_storage, &cache, tick, &cron_settings).await;
    queue
        .add_completed_job("another-old-job", completed_at)
        .await;
    // Simulate a second worker process handling the same cron tick
    vacuum_jobs(&redis_storage, &cache, tick, &cron_settings).await;

    assert!(queue.is_purged("old-job").await);
    assert!(
        queue.is_stored("another-old-job").await,
        "the same tick handled by 2 processes should vacuum exactly once"
    );
}

#[rstest]
fn test_vacuum_jobs_cron_settings(settings: Settings) {
    let cron_settings = settings.application.cron.vacuum_jobs;
    // Disabled in config/test.toml
    assert!(!cron_settings.is_enabled);
    // Other fields fall back to the values from config/default.toml
    assert_eq!(cron_settings.schedule, "0 */10 * * * *");
    assert_eq!(cron_settings.retention_hours, 6);
    assert_eq!(cron_settings.batch_size, 1000);
    assert_eq!(cron_settings.max_batches_per_tick, 200);
    assert_eq!(cron_settings.lock_ttl_seconds, 300);
}

#[rstest]
fn test_refresh_oauth_tokens_cron_settings(settings: Settings) {
    let cron_settings = settings.application.cron.refresh_oauth_tokens;
    // Disabled in config/test.toml
    assert!(!cron_settings.is_enabled);
    // Other fields fall back to their defaults
    assert_eq!(cron_settings.schedule, "0 */5 * * * *");
    assert_eq!(cron_settings.minutes_before_expiry, 10);
    assert_eq!(cron_settings.lock_ttl_seconds, 60);
}
