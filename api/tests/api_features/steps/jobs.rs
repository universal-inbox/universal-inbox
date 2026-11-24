use std::str::FromStr;

use apalis::prelude::{Storage, TaskId};
use chrono::{DateTime, TimeDelta, Utc};
use cucumber::{gherkin::Step, given, then, when};
use pretty_assertions::assert_eq;
use uuid::Uuid;

use universal_inbox::user::UserId;
use universal_inbox_api::jobs::{
    JobEnvelope, JobStorage, UniversalInboxJob,
    dead_letter::{
        DeadJob, DeadJobSelection, RequeueOutcome, UNDECODABLE_JOB_TYPE, delete_dead_job,
        job_result_hash, list_dead_jobs, requeue_dead_job, select_dead_jobs,
    },
    metrics::{QueueSample, sample_queue},
    sync::SyncTasksJob,
};

use crate::{
    common::{redis_storage, settings},
    world::ApiWorld,
};

const ERROR: &str = "Max retries of 3 exceeded";

const REMOVED_JOB_PAYLOAD: &[u8] = br#"{"args":{"RemovedJob":{}},"parts":{"attempt":3}}"#;

const NOT_JSON_PAYLOAD: &[u8] = b"\x00not json";

pub(super) async fn storage(world: &mut ApiWorld) -> &mut JobStorage {
    if world.jobs.storage.is_none() {
        world.jobs.storage = Some(redis_storage(settings()).await);
    }
    world.jobs.storage.as_mut().unwrap()
}

fn age_to_delta(age: &str) -> TimeDelta {
    if age == "just now" {
        return TimeDelta::zero();
    }
    let (count, unit) = age.split_once(' ').unwrap();
    let count: i64 = count.parse().unwrap();
    if unit.starts_with("hour") {
        TimeDelta::hours(count)
    } else {
        TimeDelta::days(count)
    }
}

fn sync_tasks_job(user_id: UserId) -> UniversalInboxJob {
    UniversalInboxJob::SyncTasks(SyncTasksJob {
        source: None,
        user_id: Some(user_id),
    })
}

async fn push_dead_job(
    storage: &mut JobStorage,
    job: UniversalInboxJob,
    died_at: DateTime<Utc>,
) -> String {
    let id = storage
        .push(JobEnvelope::new(job))
        .await
        .unwrap()
        .task_id
        .to_string();
    let config = storage.get_config();
    let mut conn = storage.get_connection().clone();
    let payload: Vec<u8> = redis_apalis::cmd("HGET")
        .arg(config.job_data_hash())
        .arg(&id)
        .query_async(&mut conn)
        .await
        .unwrap();
    let mut payload: serde_json::Value = serde_json::from_slice(&payload).unwrap();
    payload["parts"]["attempt"] = 3.into();
    let _: () = redis_apalis::pipe()
        .cmd("LREM")
        .arg(config.active_jobs_list())
        .arg(0)
        .arg(&id)
        .ignore()
        .cmd("HSET")
        .arg(config.job_data_hash())
        .arg(&id)
        .arg(serde_json::to_vec(&payload).unwrap())
        .ignore()
        .cmd("ZADD")
        .arg(config.dead_jobs_set())
        .arg(died_at.timestamp())
        .arg(&id)
        .ignore()
        .cmd("HSET")
        .arg(job_result_hash(storage))
        .arg(&id)
        .arg(ERROR)
        .ignore()
        .query_async(&mut conn)
        .await
        .unwrap();
    id
}

async fn put_dead_payload(storage: &JobStorage, payload: &[u8], died_at: DateTime<Utc>) -> String {
    let id = Uuid::new_v4().to_string();
    let config = storage.get_config();
    let mut conn = storage.get_connection().clone();
    let _: () = redis_apalis::pipe()
        .cmd("HSET")
        .arg(config.job_data_hash())
        .arg(&id)
        .arg(payload)
        .ignore()
        .cmd("ZADD")
        .arg(config.dead_jobs_set())
        .arg(died_at.timestamp())
        .arg(&id)
        .ignore()
        .query_async(&mut conn)
        .await
        .unwrap();
    id
}

async fn hash_has(storage: &JobStorage, hash: String, id: &str) -> bool {
    let mut conn = storage.get_connection().clone();
    redis_apalis::cmd("HEXISTS")
        .arg(hash)
        .arg(id)
        .query_async(&mut conn)
        .await
        .unwrap()
}

async fn is_dead(storage: &JobStorage, id: &str) -> bool {
    let mut conn = storage.get_connection().clone();
    let score: Option<f64> = redis_apalis::cmd("ZSCORE")
        .arg(storage.get_config().dead_jobs_set())
        .arg(id)
        .query_async(&mut conn)
        .await
        .unwrap();
    score.is_some()
}

async fn active_ids(storage: &JobStorage) -> Vec<String> {
    let mut conn = storage.get_connection().clone();
    redis_apalis::cmd("LRANGE")
        .arg(storage.get_config().active_jobs_list())
        .arg(0)
        .arg(-1)
        .query_async(&mut conn)
        .await
        .unwrap()
}

fn ids(jobs: &[DeadJob]) -> Vec<&str> {
    jobs.iter().map(|job| job.id.as_str()).collect()
}

fn listed_job<'a>(world: &'a ApiWorld, name: &str) -> &'a DeadJob {
    let id = world.jobs.id(name);
    world
        .jobs
        .listed
        .iter()
        .find(|job| job.id == id)
        .unwrap_or_else(|| panic!("The job {name:?} was not listed"))
}

fn names_in_table(step: &Step) -> Vec<String> {
    step.table
        .as_ref()
        .expect("The jobs must be given as a table")
        .rows
        .iter()
        .map(|row| row[0].clone())
        .collect()
}

#[given("an empty job queue")]
async fn an_empty_job_queue(world: &mut ApiWorld) {
    storage(world).await;
}

#[given(regex = r"^pending jobs that were enqueued (\d+) seconds ago and (\d+) seconds ago$")]
async fn pending_jobs_enqueued_ago(world: &mut ApiWorld, first: i64, second: i64) {
    let now = Utc::now();
    world.jobs.enqueued_at = Some(now);
    let storage = storage(world).await;
    for age in [first, second] {
        let mut envelope = JobEnvelope::new(UniversalInboxJob::PurgeAuditEvents);
        envelope.enqueued_at = Some(now - TimeDelta::seconds(age));
        storage.push(envelope).await.unwrap();
    }
}

#[given("a pending job that has no enqueue time")]
async fn a_pending_job_without_an_enqueue_time(world: &mut ApiWorld) {
    let mut envelope = JobEnvelope::new(UniversalInboxJob::PurgeAuditEvents);
    envelope.enqueued_at = None;
    storage(world).await.push(envelope).await.unwrap();
}

#[when("I sample the queue")]
async fn i_sample_the_queue(world: &mut ApiWorld) {
    let now = world.jobs.enqueued_at.unwrap_or_else(Utc::now);
    let sample = sample_queue(storage(world).await, now).await.unwrap();
    world.jobs.sample = Some(sample);
}

fn sample(world: &ApiWorld) -> &QueueSample {
    world
        .jobs
        .sample
        .as_ref()
        .expect("The queue was not sampled")
}

#[then("nothing is pending, running, failed or dead, and no pending job has an age")]
fn the_sample_is_empty(world: &mut ApiWorld) {
    assert_eq!(
        sample(world),
        &QueueSample {
            pending: 0,
            running: 0,
            failed: 0,
            dead: 0,
            oldest_pending_age: 0.0
        }
    );
}

#[then(expr = "the sample shows {int} pending and {int} running job(s)")]
fn the_sample_shows_pending_and_running(world: &mut ApiWorld, pending: i64, running: i64) {
    assert_eq!(sample(world).pending, pending);
    assert_eq!(sample(world).running, running);
}

#[then(expr = "the oldest pending job is {int} seconds old")]
fn the_oldest_pending_job_is_this_old(world: &mut ApiWorld, seconds: u32) {
    assert_eq!(sample(world).oldest_pending_age, f64::from(seconds));
}

#[given(
    regex = r#"^a dead "(purge|sync tasks)" job "([^"]+)" that died (just now|\d+ hours? ago|\d+ days? ago)$"#
)]
async fn a_dead_job(world: &mut ApiWorld, kind: String, name: String, age: String) {
    let died_at = world.jobs.reference_time() - age_to_delta(&age);
    let job = if kind == "purge" {
        UniversalInboxJob::PurgeAuditEvents
    } else {
        let user_id = *world
            .jobs
            .user_id
            .get_or_insert_with(|| UserId(Uuid::new_v4()));
        sync_tasks_job(user_id)
    };
    let id = push_dead_job(storage(world).await, job, died_at).await;
    world.jobs.ids.insert(name, id);
}

#[given(
    regex = r#"^a dead job "([^"]+)" whose payload is not JSON, that died (just now|\d+ hours? ago|\d+ days? ago)$"#
)]
async fn a_dead_job_with_a_payload_that_is_not_json(
    world: &mut ApiWorld,
    name: String,
    age: String,
) {
    let died_at = world.jobs.reference_time() - age_to_delta(&age);
    let id = put_dead_payload(storage(world).await, NOT_JSON_PAYLOAD, died_at).await;
    world.jobs.ids.insert(name, id);
}

#[given(
    regex = r#"^a dead job "([^"]+)" of a type that no longer exists, that died (just now|\d+ hours? ago|\d+ days? ago)$"#
)]
async fn a_dead_job_of_a_removed_type(world: &mut ApiWorld, name: String, age: String) {
    let died_at = world.jobs.reference_time() - age_to_delta(&age);
    let id = put_dead_payload(storage(world).await, REMOVED_JOB_PAYLOAD, died_at).await;
    world.jobs.ids.insert(name, id);
}

#[when("I list the dead jobs")]
async fn i_list_the_dead_jobs(world: &mut ApiWorld) {
    world.jobs.listed = list_dead_jobs(storage(world).await, None, 50)
        .await
        .unwrap();
}

#[when(expr = "I list at most {int} dead job(s) of the type {string}")]
async fn i_list_dead_jobs_of_a_type(world: &mut ApiWorld, limit: usize, job_type: String) {
    world.jobs.listed = list_dead_jobs(storage(world).await, Some(&job_type), limit)
        .await
        .unwrap();
}

#[when("I list the undecodable dead jobs")]
async fn i_list_the_undecodable_dead_jobs(world: &mut ApiWorld) {
    world.jobs.listed = list_dead_jobs(storage(world).await, Some(UNDECODABLE_JOB_TYPE), 50)
        .await
        .unwrap();
}

#[then("the listed jobs are, most recently dead first:")]
fn the_listed_jobs_are(world: &mut ApiWorld, step: &Step) {
    let expected: Vec<&str> = names_in_table(step)
        .iter()
        .map(|name| world.jobs.id(name))
        .collect();
    assert_eq!(ids(&world.jobs.listed), expected);
}

#[then(expr = "the only listed job is {string}")]
fn the_only_listed_job_is(world: &mut ApiWorld, name: String) {
    assert_eq!(ids(&world.jobs.listed), vec![world.jobs.id(&name)]);
}

#[then(
    expr = "the listed job {string} is a decodable {string} job of that user, which ran {int} attempts, died at the reference time and failed with {string}"
)]
fn the_listed_job_is_described(
    world: &mut ApiWorld,
    name: String,
    job_type: String,
    attempts: u64,
    error: String,
) {
    let died_at = world.jobs.reference_time.expect("No job died yet");
    assert_eq!(
        listed_job(world, &name),
        &DeadJob {
            id: world.jobs.id(&name).to_string(),
            job_type,
            user_id: world.jobs.user_id,
            attempts: Some(attempts),
            died_at: Some(died_at),
            last_error: Some(error),
            decodable: true,
        }
    );
}

#[then(expr = "the listed job {string} belongs to no user")]
fn the_listed_job_belongs_to_no_user(world: &mut ApiWorld, name: String) {
    assert_eq!(listed_job(world, &name).user_id, None);
}

#[then(expr = "the listed job {string} has the undecodable type and cannot be decoded")]
fn the_listed_job_is_undecodable(world: &mut ApiWorld, name: String) {
    let job = listed_job(world, &name);
    assert_eq!(job.job_type, UNDECODABLE_JOB_TYPE);
    assert!(!job.decodable);
}

#[then(expr = "the listed job {string} has the type {string} and {int} attempts")]
fn the_listed_job_has_a_type_and_attempts(
    world: &mut ApiWorld,
    name: String,
    job_type: String,
    attempts: u64,
) {
    let job = listed_job(world, &name);
    assert_eq!(job.job_type, job_type);
    assert_eq!(job.attempts, Some(attempts));
}

#[when(expr = "I requeue the dead job {string}")]
async fn i_requeue_the_dead_job(world: &mut ApiWorld, name: String) {
    let id = world.jobs.id(&name).to_string();
    let outcome = requeue_dead_job(storage(world).await, &id).await.unwrap();
    world.jobs.requeue_outcome = Some(outcome);
}

#[then("the job is requeued")]
fn the_job_is_requeued(world: &mut ApiWorld) {
    assert_eq!(world.jobs.requeue_outcome, Some(RequeueOutcome::Requeued));
}

#[then("the app says the job is not dead")]
fn the_app_says_the_job_is_not_dead(world: &mut ApiWorld) {
    assert_eq!(world.jobs.requeue_outcome, Some(RequeueOutcome::NotDead));
}

#[then(expr = "the requeue outcome is {string}")]
fn the_requeue_outcome_is(world: &mut ApiWorld, outcome: String) {
    let expected = match outcome.as_str() {
        "undecodable" => RequeueOutcome::Undecodable,
        other => panic!("Unknown requeue outcome {other:?}"),
    };
    assert_eq!(world.jobs.requeue_outcome, Some(expected));
}

#[then(expr = "{string} is no longer dead and is the only active job")]
async fn the_job_is_the_only_active_job(world: &mut ApiWorld, name: String) {
    let id = world.jobs.id(&name).to_string();
    let storage = storage(world).await;
    assert!(!is_dead(storage, &id).await);
    assert_eq!(active_ids(storage).await, vec![id]);
}

#[then(expr = "{string} is still the only active job")]
async fn the_job_is_still_the_only_active_job(world: &mut ApiWorld, name: String) {
    let id = world.jobs.id(&name).to_string();
    assert_eq!(active_ids(storage(world).await).await, vec![id]);
}

#[then("the workers were woken up once")]
async fn the_consumers_were_woken_up(world: &mut ApiWorld) {
    let storage = storage(world).await;
    let mut conn = storage.get_connection().clone();
    let signals: usize = redis_apalis::cmd("LLEN")
        .arg(storage.get_config().signal_list())
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(signals, 1);
}

#[then(expr = "a worker reads {string} as a {string} job that has not run yet")]
async fn a_worker_reads_the_requeued_job(world: &mut ApiWorld, name: String, job_type: String) {
    let id = world.jobs.id(&name).to_string();
    let request = storage(world)
        .await
        .fetch_by_id(&TaskId::from_str(&id).unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(request.parts.attempt.current(), 0);
    assert_eq!(request.args.job.name(), job_type);
}

#[then(expr = "the queue has {int} pending and {int} dead job(s)")]
async fn the_queue_has_pending_and_dead_jobs(world: &mut ApiWorld, pending: i64, dead: i64) {
    let sample = sample_queue(storage(world).await, Utc::now())
        .await
        .unwrap();
    assert_eq!((sample.pending, sample.dead), (pending, dead));
}

#[then(expr = "{string} is still dead and nothing is active")]
async fn the_job_is_still_dead_and_nothing_is_active(world: &mut ApiWorld, name: String) {
    let id = world.jobs.id(&name).to_string();
    let storage = storage(world).await;
    assert!(is_dead(storage, &id).await);
    assert!(active_ids(storage).await.is_empty());
}

#[when(expr = "I select the dead jobs with the id of {string} and an id nobody knows")]
async fn i_select_dead_jobs_by_ids(world: &mut ApiWorld, name: String) {
    let id = world.jobs.id(&name).to_string();
    world.jobs.selected = select_dead_jobs(
        storage(world).await,
        &DeadJobSelection::Ids(vec![id, "unknown".to_string()]),
        None,
    )
    .await
    .unwrap();
}

#[when(expr = "I select the undecodable dead jobs with the id of {string}")]
async fn i_select_undecodable_dead_jobs_by_ids(world: &mut ApiWorld, name: String) {
    let id = world.jobs.id(&name).to_string();
    world.jobs.selected = select_dead_jobs(
        storage(world).await,
        &DeadJobSelection::Ids(vec![id]),
        Some(UNDECODABLE_JOB_TYPE),
    )
    .await
    .unwrap();
}

#[when(expr = "I select the dead jobs that died more than {int} days ago")]
async fn i_select_dead_jobs_by_age(world: &mut ApiWorld, days: i64) {
    let limit = Utc::now() - TimeDelta::days(days);
    world.jobs.selected = select_dead_jobs(
        storage(world).await,
        &DeadJobSelection::DiedBefore(limit),
        None,
    )
    .await
    .unwrap();
}

#[then(expr = "the only selected job is {string}")]
fn the_only_selected_job_is(world: &mut ApiWorld, name: String) {
    assert_eq!(ids(&world.jobs.selected), vec![world.jobs.id(&name)]);
}

#[when("I delete the selected jobs")]
async fn i_delete_the_selected_jobs(world: &mut ApiWorld) {
    let selected: Vec<String> = ids(&world.jobs.selected)
        .into_iter()
        .map(str::to_string)
        .collect();
    let storage = storage(world).await;
    for id in selected {
        assert!(delete_dead_job(storage, &id).await.unwrap());
    }
}

#[then(expr = "{string} is no longer dead")]
async fn the_job_is_no_longer_dead(world: &mut ApiWorld, name: String) {
    let id = world.jobs.id(&name).to_string();
    assert!(!is_dead(storage(world).await, &id).await);
}

#[then(expr = "{string} is still dead")]
async fn the_job_is_still_dead(world: &mut ApiWorld, name: String) {
    let id = world.jobs.id(&name).to_string();
    assert!(is_dead(storage(world).await, &id).await);
}

#[then(expr = "neither the data nor the result of {string} is stored anymore")]
async fn neither_the_data_nor_the_result_is_stored(world: &mut ApiWorld, name: String) {
    let id = world.jobs.id(&name).to_string();
    let storage = storage(world).await;
    let data_hash = storage.get_config().job_data_hash();
    assert!(!hash_has(storage, data_hash, &id).await);
    assert!(!hash_has(storage, job_result_hash(storage), &id).await);
}

#[then(expr = "the data of {string} is not stored anymore")]
async fn the_data_is_not_stored_anymore(world: &mut ApiWorld, name: String) {
    let id = world.jobs.id(&name).to_string();
    let storage = storage(world).await;
    let data_hash = storage.get_config().job_data_hash();
    assert!(!hash_has(storage, data_hash, &id).await);
}

#[then(expr = "deleting {string} again finds nothing to delete")]
async fn deleting_again_finds_nothing(world: &mut ApiWorld, name: String) {
    let id = world.jobs.id(&name).to_string();
    assert!(!delete_dead_job(storage(world).await, &id).await.unwrap());
}
