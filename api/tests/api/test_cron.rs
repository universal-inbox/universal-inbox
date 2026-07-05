use apalis::prelude::{Data, Storage};
use apalis_cron::CronContext;
use apalis_redis::RedisStorage;
use chrono::{TimeZone, Timelike, Utc};
use rstest::*;
use uuid::Uuid;

use universal_inbox_api::{
    configuration::{RefreshOAuthTokensCronSettings, Settings},
    jobs::{
        UniversalInboxJob,
        cron::{handle_refresh_oauth_tokens_cron_tick, try_acquire_cron_tick_lock},
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
    let tick = Utc.with_ymd_and_hms(2026, 7, 5, 12, 0, 0).unwrap();

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
