//! The dead letter queue: jobs that exhausted their attempts (or were aborted) and that apalis
//! moved to the `dead` set. Lists them, puts them back on the queue or deletes them.
//!
//! apalis-redis 0.7 has no API for that: `list_jobs` panics on a payload it cannot decode,
//! `vacuum` only cleans the `done` set and nothing requeues a dead job. So its Redis keys are
//! read directly (their names always come from the storage config) and each job is moved or
//! deleted atomically by a Lua script.
//!
//! A dead job is stored as:
//! - its id in the `dead` sorted set, scored by its death time (unix seconds),
//! - its payload in the job data hash: `{"args": <JobEnvelope>, "parts": {"attempt": N, ...}}`,
//! - its last error in the `<job data hash>::result` hash.

use apalis_redis::RedisError;
use chrono::{DateTime, Utc};
use redis_apalis::Script;
use serde_json::Value;

use universal_inbox::user::UserId;

use crate::jobs::{JobEnvelope, JobStorage};

/// Job type of a dead job whose payload is not JSON
pub const UNDECODABLE_JOB_TYPE: &str = "undecodable";
/// Job type of a dead job whose payload is missing from the job data hash
pub const MISSING_JOB_TYPE: &str = "missing";

/// Ids read from Redis per command
const BATCH_SIZE: usize = 100;

/// Fields of a [`JobEnvelope`] that are not the job itself
const ENVELOPE_FIELDS: [&str; 3] = ["trace_context", "audit_actor", "enqueued_at"];

#[derive(Debug, Clone, PartialEq)]
pub struct DeadJob {
    pub id: String,
    /// The job name (eg. `SyncTasks`), suffixed by `(undecodable)` when the payload is JSON but
    /// not a job, or [`UNDECODABLE_JOB_TYPE`] / [`MISSING_JOB_TYPE`]
    pub job_type: String,
    pub user_id: Option<UserId>,
    pub attempts: Option<u64>,
    pub died_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    /// Whether the payload decodes as a job: only such a job can be requeued
    pub decodable: bool,
}

impl DeadJob {
    /// Whether the job has the type `job_type`: the job name for a decodable job,
    /// [`UNDECODABLE_JOB_TYPE`] for any other job
    pub fn has_type(&self, job_type: &str) -> bool {
        if self.decodable {
            self.job_type == job_type
        } else {
            job_type == UNDECODABLE_JOB_TYPE
        }
    }
}

/// Dead jobs selected by a command
#[derive(Debug, Clone, PartialEq)]
pub enum DeadJobSelection {
    Ids(Vec<String>),
    /// The jobs that died before this time
    DiedBefore(DateTime<Utc>),
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequeueOutcome {
    Requeued,
    /// The job is not in the dead set (unknown, or already requeued or deleted)
    NotDead,
    /// The job payload does not decode as a job: it would die again on its first fetch
    Undecodable,
}

/// The hash where apalis-redis writes the result of each job: the last error of a dead job
pub fn job_result_hash(storage: &JobStorage) -> String {
    format!("{}::result", storage.get_config().job_data_hash())
}

/// Number of dead jobs, of any type
pub async fn count_dead_jobs(storage: &JobStorage) -> Result<usize, RedisError> {
    let mut conn = storage.get_connection().clone();
    redis_apalis::cmd("ZCARD")
        .arg(storage.get_config().dead_jobs_set())
        .query_async(&mut conn)
        .await
}

/// The `limit` most recently dead jobs, of type `job_type` if set
pub async fn list_dead_jobs(
    storage: &JobStorage,
    job_type: Option<&str>,
    limit: usize,
) -> Result<Vec<DeadJob>, RedisError> {
    if limit == 0 {
        return Ok(vec![]);
    }
    let mut conn = storage.get_connection().clone();
    let entries: Vec<(String, f64)> = redis_apalis::cmd("ZREVRANGE")
        .arg(storage.get_config().dead_jobs_set())
        .arg(0)
        .arg(-1)
        .arg("WITHSCORES")
        .query_async(&mut conn)
        .await?;

    let mut jobs = vec![];
    for batch in entries.chunks(BATCH_SIZE) {
        for job in read_dead_jobs(storage, batch).await? {
            if job_type.is_none_or(|job_type| job.has_type(job_type)) {
                jobs.push(job);
                if jobs.len() >= limit {
                    return Ok(jobs);
                }
            }
        }
    }
    Ok(jobs)
}

/// The dead jobs of `selection`, of type `job_type` if set, oldest death first. Selected ids
/// that are not dead are left out.
pub async fn select_dead_jobs(
    storage: &JobStorage,
    selection: &DeadJobSelection,
    job_type: Option<&str>,
) -> Result<Vec<DeadJob>, RedisError> {
    let mut conn = storage.get_connection().clone();
    let dead_jobs_set = storage.get_config().dead_jobs_set();
    let entries: Vec<(String, f64)> = match selection {
        DeadJobSelection::Ids(ids) => {
            let mut entries = vec![];
            for batch in ids.chunks(BATCH_SIZE) {
                let scores: Vec<Option<f64>> = redis_apalis::cmd("ZMSCORE")
                    .arg(&dead_jobs_set)
                    .arg(batch)
                    .query_async(&mut conn)
                    .await?;
                entries.extend(
                    batch
                        .iter()
                        .zip(scores)
                        .filter_map(|(id, score)| Some((id.clone(), score?))),
                );
            }
            entries
        }
        DeadJobSelection::DiedBefore(cutoff) => {
            redis_apalis::cmd("ZRANGEBYSCORE")
                .arg(&dead_jobs_set)
                .arg("-inf")
                // Exclusive: only the jobs that died strictly before the cutoff
                .arg(format!("({}", cutoff.timestamp()))
                .arg("WITHSCORES")
                .query_async(&mut conn)
                .await?
        }
        DeadJobSelection::All => {
            redis_apalis::cmd("ZRANGE")
                .arg(&dead_jobs_set)
                .arg(0)
                .arg(-1)
                .arg("WITHSCORES")
                .query_async(&mut conn)
                .await?
        }
    };

    let mut jobs = vec![];
    for batch in entries.chunks(BATCH_SIZE) {
        jobs.extend(
            read_dead_jobs(storage, batch)
                .await?
                .into_iter()
                .filter(|job| job_type.is_none_or(|job_type| job.has_type(job_type))),
        );
    }
    Ok(jobs)
}

/// Puts the dead job `id` back at the tail of the queue with its attempts reset to 0, keeping
/// the rest of its payload: its enqueue time, so the wait time metric (recorded on the first
/// attempt) measures from the original enqueue, its trace context and its audit actor. Its
/// last error is left in the result hash, the next execution overwrites it.
pub async fn requeue_dead_job(
    storage: &JobStorage,
    id: &str,
) -> Result<RequeueOutcome, RedisError> {
    let mut conn = storage.get_connection().clone();
    let config = storage.get_config();
    let dead_jobs_set = config.dead_jobs_set();
    let score: Option<f64> = redis_apalis::cmd("ZSCORE")
        .arg(&dead_jobs_set)
        .arg(id)
        .query_async(&mut conn)
        .await?;
    if score.is_none() {
        return Ok(RequeueOutcome::NotDead);
    }

    let payload: Option<Vec<u8>> = redis_apalis::cmd("HGET")
        .arg(config.job_data_hash())
        .arg(id)
        .query_async(&mut conn)
        .await?;
    let Some(payload) = payload.as_deref().and_then(reset_attempts) else {
        return Ok(RequeueOutcome::Undecodable);
    };

    let requeue_dead_job = Script::new(include_str!("../../scripts/lua/requeue_dead_job.lua"));
    let requeued: i32 = requeue_dead_job
        .key(&dead_jobs_set)
        .key(config.job_data_hash())
        .key(config.active_jobs_list())
        .key(config.signal_list())
        .arg(id)
        .arg(payload)
        .invoke_async(&mut conn)
        .await?;
    Ok(if requeued == 1 {
        RequeueOutcome::Requeued
    } else {
        RequeueOutcome::NotDead
    })
}

/// Deletes the dead job `id`: its id, payload and last error. Returns whether it was dead.
pub async fn delete_dead_job(storage: &JobStorage, id: &str) -> Result<bool, RedisError> {
    let mut conn = storage.get_connection().clone();
    let config = storage.get_config();
    let job_data_hash = config.job_data_hash();
    let delete_dead_job = Script::new(include_str!("../../scripts/lua/delete_dead_job.lua"));
    let deleted: i32 = delete_dead_job
        .key(config.dead_jobs_set())
        .key(&job_data_hash)
        .key(job_result_hash(storage))
        .arg(id)
        .invoke_async(&mut conn)
        .await?;
    Ok(deleted == 1)
}

/// Reads the payload and last error of the dead jobs `entries` (id, death time)
async fn read_dead_jobs(
    storage: &JobStorage,
    entries: &[(String, f64)],
) -> Result<Vec<DeadJob>, RedisError> {
    if entries.is_empty() {
        return Ok(vec![]);
    }
    let mut conn = storage.get_connection().clone();
    let job_data_hash = storage.get_config().job_data_hash();
    let ids: Vec<&str> = entries.iter().map(|(id, _)| id.as_str()).collect();
    let payloads: Vec<Option<Vec<u8>>> = redis_apalis::cmd("HMGET")
        .arg(&job_data_hash)
        .arg(&ids)
        .query_async(&mut conn)
        .await?;
    let results: Vec<Option<Vec<u8>>> = redis_apalis::cmd("HMGET")
        .arg(job_result_hash(storage))
        .arg(&ids)
        .query_async(&mut conn)
        .await?;

    Ok(entries
        .iter()
        .zip(payloads)
        .zip(results)
        .map(|(((id, died_at), payload), result)| {
            let decoded = decode_payload(payload.as_deref());
            DeadJob {
                id: id.clone(),
                job_type: decoded.job_type,
                user_id: decoded.user_id,
                attempts: decoded.attempts,
                died_at: DateTime::from_timestamp(*died_at as i64, 0),
                last_error: result.map(|result| String::from_utf8_lossy(&result).into_owned()),
                decodable: decoded.decodable,
            }
        })
        .collect())
}

#[derive(Debug, PartialEq)]
struct DecodedPayload {
    job_type: String,
    user_id: Option<UserId>,
    attempts: Option<u64>,
    decodable: bool,
}

/// Decodes what can be read from a stored job payload, never failing
fn decode_payload(payload: Option<&[u8]>) -> DecodedPayload {
    let Some(payload) = payload else {
        return DecodedPayload {
            job_type: MISSING_JOB_TYPE.to_string(),
            user_id: None,
            attempts: None,
            decodable: false,
        };
    };
    let Ok(request) = serde_json::from_slice::<Value>(payload) else {
        return DecodedPayload {
            job_type: UNDECODABLE_JOB_TYPE.to_string(),
            user_id: None,
            attempts: None,
            decodable: false,
        };
    };
    let attempts = request.pointer("/parts/attempt").and_then(Value::as_u64);
    match decode_envelope(&request) {
        Some(envelope) => DecodedPayload {
            job_type: envelope.job.name().to_string(),
            user_id: envelope.job.user_id(),
            attempts,
            decodable: true,
        },
        None => DecodedPayload {
            job_type: undecodable_job_type(&request),
            user_id: None,
            attempts,
            decodable: false,
        },
    }
}

fn decode_envelope(request: &Value) -> Option<JobEnvelope> {
    serde_json::from_value(request.get("args")?.clone()).ok()
}

/// Type of a JSON payload that is not a job: its enum tag (the first key of `args` that is
/// not an envelope field) suffixed by `(undecodable)`
fn undecodable_job_type(request: &Value) -> String {
    let tag = match request.get("args") {
        Some(Value::Object(args)) => args
            .keys()
            .find(|key| !ENVELOPE_FIELDS.contains(&key.as_str()))
            .cloned(),
        Some(Value::String(tag)) => Some(tag.clone()),
        _ => None,
    };
    match tag {
        Some(tag) => format!("{tag} (undecodable)"),
        None => UNDECODABLE_JOB_TYPE.to_string(),
    }
}

/// The payload with its attempts set to 0, `None` if it does not decode as a job
fn reset_attempts(payload: &[u8]) -> Option<Vec<u8>> {
    let mut request: Value = serde_json::from_slice(payload).ok()?;
    decode_envelope(&request)?;
    request
        .get_mut("parts")?
        .as_object_mut()?
        .insert("attempt".to_string(), Value::from(0));
    serde_json::to_vec(&request).ok()
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use uuid::Uuid;

    use super::*;

    #[test]
    fn test_decode_a_job_payload() {
        let user_id = Uuid::new_v4();
        let payload = format!(
            r#"{{"args":{{"enqueued_at":"2026-10-30T16:00:00Z","SyncTasks":{{"source":null,"user_id":"{user_id}"}}}},"parts":{{"task_id":"01JAZ","attempt":3,"context":{{"max_attempts":3}}}}}}"#
        );

        assert_eq!(
            decode_payload(Some(payload.as_bytes())),
            DecodedPayload {
                job_type: "SyncTasks".to_string(),
                user_id: Some(UserId(user_id)),
                attempts: Some(3),
                decodable: true,
            }
        );
    }

    #[test]
    fn test_decode_a_payload_that_is_not_a_job() {
        let unknown_job = br#"{"args":{"enqueued_at":"2026-10-30T16:00:00Z","RemovedJob":{}},"parts":{"attempt":2}}"#;

        assert_eq!(
            decode_payload(Some(unknown_job)),
            DecodedPayload {
                job_type: "RemovedJob (undecodable)".to_string(),
                user_id: None,
                attempts: Some(2),
                decodable: false,
            }
        );
        assert_eq!(
            decode_payload(Some(br#"{"parts":{}}"#)).job_type,
            UNDECODABLE_JOB_TYPE
        );
        assert_eq!(
            decode_payload(Some(b"not json")),
            DecodedPayload {
                job_type: UNDECODABLE_JOB_TYPE.to_string(),
                user_id: None,
                attempts: None,
                decodable: false,
            }
        );
        assert_eq!(decode_payload(None).job_type, MISSING_JOB_TYPE);
    }

    #[test]
    fn test_reset_attempts_keeps_the_rest_of_the_payload() {
        let payload = br#"{"args":{"enqueued_at":"2026-10-30T16:00:00Z","trace_context":{"traceparent":"00-1"},"PurgeAuditEvents":null},"parts":{"task_id":"01JAZ","attempt":3,"context":{"max_attempts":3}}}"#;

        let reset: Value = serde_json::from_slice(&reset_attempts(payload).unwrap()).unwrap();

        let mut expected: Value = serde_json::from_slice(payload).unwrap();
        expected["parts"]["attempt"] = Value::from(0);
        assert_eq!(reset, expected);
    }

    #[test]
    fn test_reset_attempts_of_a_payload_that_is_not_a_job() {
        assert_eq!(
            reset_attempts(br#"{"args":{"RemovedJob":{}},"parts":{"attempt":3}}"#),
            None
        );
        assert_eq!(reset_attempts(b"not json"), None);
    }

    #[test]
    fn test_undecodable_jobs_only_have_the_undecodable_type() {
        let job = |job_type: &str, decodable| DeadJob {
            id: "1".to_string(),
            job_type: job_type.to_string(),
            user_id: None,
            attempts: None,
            died_at: None,
            last_error: None,
            decodable,
        };

        assert!(job("SyncTasks", true).has_type("SyncTasks"));
        assert!(!job("SyncTasks", true).has_type(UNDECODABLE_JOB_TYPE));
        assert!(job("RemovedJob (undecodable)", false).has_type(UNDECODABLE_JOB_TYPE));
        assert!(!job("RemovedJob (undecodable)", false).has_type("RemovedJob"));
        assert!(job(MISSING_JOB_TYPE, false).has_type(UNDECODABLE_JOB_TYPE));
    }
}
