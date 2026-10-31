use std::str::FromStr;

use apalis::prelude::{Storage, TaskId};
use chrono::{DateTime, TimeDelta, Utc};
use pretty_assertions::assert_eq;
use rstest::*;
use uuid::Uuid;

use universal_inbox::user::UserId;
use universal_inbox_api::jobs::{
    JobEnvelope, JobStorage, UniversalInboxJob,
    dead_letter::{
        DeadJob, DeadJobSelection, RequeueOutcome, UNDECODABLE_JOB_TYPE, delete_dead_job,
        job_result_hash, list_dead_jobs, requeue_dead_job, select_dead_jobs,
    },
    metrics::sample_queue,
    sync::SyncTasksJob,
};

use crate::common::redis_storage;

const ERROR: &str = "Max retries of 3 exceeded";

fn sync_tasks_job(user_id: UserId) -> UniversalInboxJob {
    UniversalInboxJob::SyncTasks(SyncTasksJob {
        source: None,
        user_id: Some(user_id),
    })
}

/// Pushes `job` and makes it dead the way apalis does: its id moves from the active list to the
/// dead set, its error is written to the result hash and its attempts are those it ran.
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

/// Makes a dead job whose payload is `payload`, as left by a job type that no longer exists
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

#[rstest]
#[tokio::test]
async fn test_list_dead_jobs(#[future] redis_storage: JobStorage) {
    let mut storage = redis_storage.await;
    let user_id = UserId(Uuid::new_v4());
    let died_at = DateTime::from_timestamp(Utc::now().timestamp(), 0).unwrap();
    let older_purge_id = push_dead_job(
        &mut storage,
        UniversalInboxJob::PurgeAuditEvents,
        died_at - TimeDelta::hours(2),
    )
    .await;
    let purge_id = push_dead_job(
        &mut storage,
        UniversalInboxJob::PurgeAuditEvents,
        died_at - TimeDelta::hours(1),
    )
    .await;
    let sync_id = push_dead_job(&mut storage, sync_tasks_job(user_id), died_at).await;

    let jobs = list_dead_jobs(&storage, None, 50).await.unwrap();

    assert_eq!(ids(&jobs), vec![&sync_id, &purge_id, &older_purge_id]);
    assert_eq!(
        jobs[0],
        DeadJob {
            id: sync_id.clone(),
            job_type: "SyncTasks".to_string(),
            user_id: Some(user_id),
            attempts: Some(3),
            died_at: Some(died_at),
            last_error: Some(ERROR.to_string()),
            decodable: true,
        }
    );
    assert_eq!(jobs[1].user_id, None);

    let purge_jobs = list_dead_jobs(&storage, Some("PurgeAuditEvents"), 1)
        .await
        .unwrap();
    assert_eq!(ids(&purge_jobs), vec![&purge_id]);
}

#[rstest]
#[tokio::test]
async fn test_list_undecodable_dead_jobs(#[future] redis_storage: JobStorage) {
    let mut storage = redis_storage.await;
    let now = Utc::now();
    let sync_id = push_dead_job(
        &mut storage,
        sync_tasks_job(UserId(Uuid::new_v4())),
        now - TimeDelta::hours(1),
    )
    .await;
    let raw_id = put_dead_payload(&storage, b"\x00not json", now).await;
    let removed_job_id = put_dead_payload(
        &storage,
        br#"{"args":{"RemovedJob":{}},"parts":{"attempt":3}}"#,
        now - TimeDelta::hours(2),
    )
    .await;

    let jobs = list_dead_jobs(&storage, None, 50).await.unwrap();

    assert_eq!(ids(&jobs), vec![&raw_id, &sync_id, &removed_job_id]);
    assert_eq!(jobs[0].job_type, UNDECODABLE_JOB_TYPE);
    assert!(!jobs[0].decodable);
    assert_eq!(jobs[2].job_type, "RemovedJob (undecodable)");
    assert_eq!(jobs[2].attempts, Some(3));

    let undecodable_jobs = list_dead_jobs(&storage, Some(UNDECODABLE_JOB_TYPE), 50)
        .await
        .unwrap();
    assert_eq!(ids(&undecodable_jobs), vec![&raw_id, &removed_job_id]);
}

#[rstest]
#[tokio::test]
async fn test_requeue_dead_job(#[future] redis_storage: JobStorage) {
    let mut storage = redis_storage.await;
    let id = push_dead_job(
        &mut storage,
        sync_tasks_job(UserId(Uuid::new_v4())),
        Utc::now(),
    )
    .await;

    assert_eq!(
        requeue_dead_job(&storage, &id).await.unwrap(),
        RequeueOutcome::Requeued
    );

    assert!(!is_dead(&storage, &id).await);
    assert_eq!(active_ids(&storage).await, vec![id.clone()]);
    // The consumers are woken up
    let mut conn = storage.get_connection().clone();
    let signals: usize = redis_apalis::cmd("LLEN")
        .arg(storage.get_config().signal_list())
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(signals, 1);
    // The worker decodes the requeued payload as apalis does
    let request = storage
        .fetch_by_id(&TaskId::from_str(&id).unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(request.parts.attempt.current(), 0);
    assert_eq!(request.args.job.name(), "SyncTasks");
    let sample = sample_queue(&storage, Utc::now()).await.unwrap();
    assert_eq!((sample.pending, sample.dead), (1, 0));

    assert_eq!(
        requeue_dead_job(&storage, &id).await.unwrap(),
        RequeueOutcome::NotDead
    );
    assert_eq!(active_ids(&storage).await, vec![id]);
}

#[rstest]
#[tokio::test]
async fn test_requeue_undecodable_dead_job(#[future] redis_storage: JobStorage) {
    let storage = redis_storage.await;
    let id = put_dead_payload(
        &storage,
        br#"{"args":{"RemovedJob":{}},"parts":{"attempt":3}}"#,
        Utc::now(),
    )
    .await;

    assert_eq!(
        requeue_dead_job(&storage, &id).await.unwrap(),
        RequeueOutcome::Undecodable
    );

    assert!(is_dead(&storage, &id).await);
    assert!(active_ids(&storage).await.is_empty());
}

#[rstest]
#[tokio::test]
async fn test_cleanup_dead_job_by_id(#[future] redis_storage: JobStorage) {
    let mut storage = redis_storage.await;
    let id = push_dead_job(
        &mut storage,
        UniversalInboxJob::PurgeAuditEvents,
        Utc::now(),
    )
    .await;
    let kept_id = push_dead_job(
        &mut storage,
        UniversalInboxJob::PurgeAuditEvents,
        Utc::now(),
    )
    .await;
    let job_data_hash = storage.get_config().job_data_hash();

    let jobs = select_dead_jobs(
        &storage,
        &DeadJobSelection::Ids(vec![id.clone(), "unknown".to_string()]),
        None,
    )
    .await
    .unwrap();
    assert_eq!(ids(&jobs), vec![&id]);
    assert!(delete_dead_job(&storage, &id).await.unwrap());

    assert!(!is_dead(&storage, &id).await);
    assert!(!hash_has(&storage, job_data_hash.clone(), &id).await);
    assert!(!hash_has(&storage, job_result_hash(&storage), &id).await);
    assert!(is_dead(&storage, &kept_id).await);
    assert!(!delete_dead_job(&storage, &id).await.unwrap());
}

#[rstest]
#[tokio::test]
async fn test_cleanup_dead_jobs_by_age(#[future] redis_storage: JobStorage) {
    let mut storage = redis_storage.await;
    let now = Utc::now();
    let old_id = push_dead_job(
        &mut storage,
        UniversalInboxJob::PurgeAuditEvents,
        now - TimeDelta::days(8),
    )
    .await;
    let recent_id = push_dead_job(
        &mut storage,
        UniversalInboxJob::PurgeAuditEvents,
        now - TimeDelta::days(6),
    )
    .await;

    let jobs = select_dead_jobs(
        &storage,
        &DeadJobSelection::DiedBefore(now - TimeDelta::days(7)),
        None,
    )
    .await
    .unwrap();
    assert_eq!(ids(&jobs), vec![&old_id]);
    for job in &jobs {
        assert!(delete_dead_job(&storage, &job.id).await.unwrap());
    }

    assert!(!is_dead(&storage, &old_id).await);
    assert!(is_dead(&storage, &recent_id).await);
}

#[rstest]
#[tokio::test]
async fn test_cleanup_undecodable_dead_job_by_id(#[future] redis_storage: JobStorage) {
    let storage = redis_storage.await;
    let id = put_dead_payload(&storage, b"\x00not json", Utc::now()).await;
    let job_data_hash = storage.get_config().job_data_hash();

    let jobs = select_dead_jobs(
        &storage,
        &DeadJobSelection::Ids(vec![id.clone()]),
        Some(UNDECODABLE_JOB_TYPE),
    )
    .await
    .unwrap();
    assert_eq!(ids(&jobs), vec![&id]);
    assert!(delete_dead_job(&storage, &id).await.unwrap());

    assert!(!is_dead(&storage, &id).await);
    assert!(!hash_has(&storage, job_data_hash, &id).await);
}
