use apalis::prelude::{Data, Storage};
use apalis_cron::CronContext;
use chrono::{DateTime, TimeDelta, TimeZone, Timelike, Utc};
use cucumber::{given, then, when};
use pretty_assertions::assert_eq;
use redis_apalis::{AsyncCommands, aio::ConnectionManager};

use universal_inbox_api::{
    configuration::{
        PauseIntegrationConnectionsCronSettings, PurgeAuditEventsCronSettings,
        RefreshOAuthTokensCronSettings, VacuumJobsCronSettings,
    },
    jobs::{
        JobStorage,
        cron::{
            handle_pause_integration_connections_cron_tick, handle_purge_audit_events_cron_tick,
            handle_refresh_oauth_tokens_cron_tick, handle_vacuum_jobs_cron_tick,
            try_acquire_cron_tick_lock,
        },
    },
    utils::cache::Cache,
};

use crate::{
    common::{redis_storage, settings},
    steps::jobs::storage,
    world::ApiWorld,
};

struct JobQueue {
    connection: ConnectionManager,
    done_jobs_set: String,
    job_data_hash: String,
    job_result_hash: String,
    active_jobs_list: String,
}

impl JobQueue {
    fn new(storage: &JobStorage) -> Self {
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

    async fn is_purged(&mut self, job_id: &str) -> bool {
        !self.has_job_data(job_id).await
            && !self.has_job_result(job_id).await
            && !self.is_done(job_id).await
    }

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

fn cron_tick() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 7, 5, 12, 0, 0).unwrap()
}

async fn cache() -> Cache {
    Cache::new(&settings().redis)
        .await
        .expect("Failed to create cache")
}

async fn other_storage(world: &mut ApiWorld) -> &mut JobStorage {
    if world.jobs.other_storage.is_none() {
        world.jobs.other_storage = Some(redis_storage(settings()).await);
    }
    world.jobs.other_storage.as_mut().unwrap()
}

#[given("another empty job queue")]
async fn another_empty_job_queue(world: &mut ApiWorld) {
    other_storage(world).await;
}

#[given(
    expr = "the vacuum cron keeps the jobs of the last {int} hours, in batches of {int} with at most {int} batches per tick"
)]
fn the_vacuum_cron_settings(
    world: &mut ApiWorld,
    retention_hours: i64,
    batch_size: usize,
    max_batches_per_tick: usize,
) {
    world.jobs.vacuum_settings = Some(VacuumJobsCronSettings {
        retention_hours,
        batch_size,
        max_batches_per_tick,
        ..Default::default()
    });
}

#[given(expr = "a completed job {string} that finished {int} hours ago")]
async fn a_completed_job(world: &mut ApiWorld, job_id: String, hours: i64) {
    let completed_at = Utc::now() - TimeDelta::try_hours(hours).unwrap();
    JobQueue::new(storage(world).await)
        .add_completed_job(&job_id, completed_at)
        .await;
}

#[when(expr = "a completed job {string} that finished {int} hours ago is added")]
async fn a_completed_job_is_added(world: &mut ApiWorld, job_id: String, hours: i64) {
    a_completed_job(world, job_id, hours).await;
}

#[given(expr = "a completed job {string} that finished {int} hours ago, in the other queue")]
async fn a_completed_job_in_the_other_queue(world: &mut ApiWorld, job_id: String, hours: i64) {
    let completed_at = Utc::now() - TimeDelta::try_hours(hours).unwrap();
    JobQueue::new(other_storage(world).await)
        .add_completed_job(&job_id, completed_at)
        .await;
}

#[given(expr = "{int} completed jobs that finished {int} hours ago")]
async fn several_completed_jobs(world: &mut ApiWorld, count: usize, hours: i64) {
    let completed_at = Utc::now() - TimeDelta::try_hours(hours).unwrap();
    let mut queue = JobQueue::new(storage(world).await);
    for i in 0..count {
        queue
            .add_completed_job(&format!("old-job-{i}"), completed_at)
            .await;
    }
}

#[given(expr = "an active job {string}")]
async fn an_active_job(world: &mut ApiWorld, job_id: String) {
    JobQueue::new(storage(world).await)
        .add_active_job(&job_id)
        .await;
}

enum Queue {
    Own,
    Other,
}

async fn vacuum(world: &mut ApiWorld, queue: Queue) {
    let cron_settings = world
        .jobs
        .vacuum_settings
        .clone()
        .expect("The vacuum cron settings were not given");
    let cache = cache().await;
    let target = match queue {
        Queue::Own => storage(world).await.clone(),
        Queue::Other => other_storage(world).await.clone(),
    };
    handle_vacuum_jobs_cron_tick(
        Default::default(),
        CronContext::new(cron_tick()),
        Data::new(target),
        Data::new(cache),
        Data::new(cron_settings),
    )
    .await
    .expect("Failed to handle cron tick");
}

#[when("the vacuum cron tick is handled")]
async fn the_vacuum_cron_tick_is_handled(world: &mut ApiWorld) {
    vacuum(world, Queue::Own).await;
}

#[when("the same vacuum cron tick is handled again by a second worker process")]
async fn the_vacuum_cron_tick_is_handled_again(world: &mut ApiWorld) {
    vacuum(world, Queue::Own).await;
}

#[when("the same vacuum cron tick is handled on the other queue")]
async fn the_vacuum_cron_tick_is_handled_on_the_other_queue(world: &mut ApiWorld) {
    vacuum(world, Queue::Other).await;
}

#[then(expr = "the job {string} is purged from the done set, the data and the result")]
async fn the_job_is_purged(world: &mut ApiWorld, job_id: String) {
    assert!(
        JobQueue::new(storage(world).await).is_purged(&job_id).await,
        "the job {job_id} should be purged from the done set, the data hash and the result hash"
    );
}

#[then(expr = "the job {string} is purged from the other queue too")]
async fn the_job_is_purged_from_the_other_queue(world: &mut ApiWorld, job_id: String) {
    assert!(
        JobQueue::new(other_storage(world).await)
            .is_purged(&job_id)
            .await,
        "the same tick handled on another job queue should not be deduped against the first one"
    );
}

#[then(expr = "the job {string} is kept in the done set, the data and the result")]
async fn the_job_is_kept(world: &mut ApiWorld, job_id: String) {
    assert!(
        JobQueue::new(storage(world).await).is_stored(&job_id).await,
        "the job {job_id} should be kept"
    );
}

#[then(
    expr = "the data of the active job {string} is kept and the active list still holds {int} job"
)]
async fn the_active_job_is_kept(world: &mut ApiWorld, job_id: String, count: usize) {
    let mut queue = JobQueue::new(storage(world).await);
    assert!(
        queue.has_job_data(&job_id).await,
        "the data of a job that is not completed should never be purged"
    );
    assert_eq!(queue.active_jobs_count().await, count);
}

#[then(expr = "{int} completed jobs are left")]
async fn completed_jobs_are_left(world: &mut ApiWorld, count: usize) {
    assert_eq!(
        JobQueue::new(storage(world).await).done_jobs_count().await,
        count
    );
}

async fn queued_jobs(storage: &JobStorage) -> usize {
    storage
        .clone()
        .len()
        .await
        .expect("Failed to get Redis storage length")
        .try_into()
        .unwrap()
}

#[when(
    expr = "the token refresh cron runs on 2 workers, refreshing tokens that expire within {int} minutes"
)]
async fn the_refresh_tick_is_handled_twice(world: &mut ApiWorld, minutes: i64) {
    let cache = cache().await;
    let target = storage(world).await.clone();
    let cron_settings = RefreshOAuthTokensCronSettings {
        minutes_before_expiry: minutes,
        ..Default::default()
    };
    for _ in 0..2 {
        handle_refresh_oauth_tokens_cron_tick(
            Default::default(),
            CronContext::new(cron_tick()),
            Data::new(target.clone()),
            Data::new(cache.clone()),
            Data::new(cron_settings.clone()),
        )
        .await
        .expect("Failed to handle cron tick");
    }
}

#[when(
    expr = "the pause integration connections cron tick is handled by 2 worker processes, with an inactivity threshold of {int} days"
)]
async fn the_pause_tick_is_handled_twice(world: &mut ApiWorld, days: i64) {
    let cache = cache().await;
    let target = storage(world).await.clone();
    let cron_settings = PauseIntegrationConnectionsCronSettings {
        inactivity_threshold_days: days,
        ..Default::default()
    };
    for _ in 0..2 {
        handle_pause_integration_connections_cron_tick(
            Default::default(),
            CronContext::new(cron_tick()),
            Data::new(target.clone()),
            Data::new(cache.clone()),
            Data::new(cron_settings.clone()),
        )
        .await
        .expect("Failed to handle cron tick");
    }
}

#[when("the purge audit events cron tick is handled by 2 worker processes")]
async fn the_purge_tick_is_handled_twice(world: &mut ApiWorld) {
    let cache = cache().await;
    let target = storage(world).await.clone();
    for _ in 0..2 {
        handle_purge_audit_events_cron_tick(
            Default::default(),
            CronContext::new(cron_tick()),
            Data::new(target.clone()),
            Data::new(cache.clone()),
            Data::new(PurgeAuditEventsCronSettings::default()),
        )
        .await
        .expect("Failed to handle cron tick");
    }
}

#[when("the refresh OAuth tokens cron tick is handled once on each of the two queues")]
async fn the_refresh_tick_is_handled_on_both_queues(world: &mut ApiWorld) {
    let cache = cache().await;
    let first = storage(world).await.clone();
    let second = other_storage(world).await.clone();
    for target in [first, second] {
        handle_refresh_oauth_tokens_cron_tick(
            Default::default(),
            CronContext::new(cron_tick()),
            Data::new(target),
            Data::new(cache.clone()),
            Data::new(RefreshOAuthTokensCronSettings::default()),
        )
        .await
        .expect("Failed to handle cron tick");
    }
}

#[then(expr = "exactly {int} job was enqueued, although 2 processes handled the same tick")]
async fn exactly_one_job_was_enqueued(world: &mut ApiWorld, count: usize) {
    assert_eq!(
        queued_jobs(storage(world).await).await,
        count,
        "the same tick handled by 2 processes should enqueue exactly {count} job"
    );
}

#[then(expr = "each of the two queues holds exactly {int} job")]
async fn each_queue_holds_exactly_one_job(world: &mut ApiWorld, count: usize) {
    let first = storage(world).await.clone();
    let second = other_storage(world).await.clone();
    for target in [first, second] {
        assert_eq!(
            queued_jobs(&target).await,
            count,
            "the same tick handled on 2 job queues should enqueue 1 job on each queue"
        );
    }
}

#[when("I try to take the lock of a cron tick twice, then the lock of the tick 5 minutes later")]
async fn i_try_to_take_the_cron_locks(world: &mut ApiWorld) {
    let cache = cache().await;
    let job_name = format!("test-cron-job-{}", uuid::Uuid::new_v4());
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
    world.jobs.lock_results = vec![first, second, next_tick];
}

#[then("the first try wins the lock, the second does not and the next tick gets its own lock")]
fn the_cron_locks_are_dedupe(world: &mut ApiWorld) {
    assert_eq!(world.jobs.lock_results, vec![true, false, true]);
}
