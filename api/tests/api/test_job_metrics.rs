use apalis::prelude::Storage;
use chrono::{TimeDelta, Utc};
use rstest::*;

use universal_inbox_api::jobs::{
    JobEnvelope, JobStorage, UniversalInboxJob,
    metrics::{QueueSample, sample_queue},
};

use crate::common::redis_storage;

fn purge_job() -> UniversalInboxJob {
    UniversalInboxJob::PurgeAuditEvents
}

#[rstest]
#[tokio::test]
async fn test_sample_of_an_empty_queue(#[future] redis_storage: JobStorage) {
    let redis_storage = redis_storage.await;

    let sample = sample_queue(&redis_storage, Utc::now()).await.unwrap();

    assert_eq!(
        sample,
        QueueSample {
            pending: 0,
            running: 0,
            failed: 0,
            dead: 0,
            oldest_pending_age: 0.0
        }
    );
}

#[rstest]
#[tokio::test]
async fn test_sample_counts_pending_jobs_and_ages_the_oldest(#[future] redis_storage: JobStorage) {
    let mut redis_storage = redis_storage.await;
    let now = Utc::now();
    for age in [60, 10] {
        let mut envelope = JobEnvelope::new(purge_job());
        envelope.enqueued_at = Some(now - TimeDelta::seconds(age));
        redis_storage.push(envelope).await.unwrap();
    }

    let sample = sample_queue(&redis_storage, now).await.unwrap();

    assert_eq!(sample.pending, 2);
    assert_eq!(sample.running, 0);
    assert_eq!(sample.oldest_pending_age, 60.0);
}

#[rstest]
#[tokio::test]
async fn test_sample_age_is_zero_when_the_oldest_job_has_no_enqueue_time(
    #[future] redis_storage: JobStorage,
) {
    let mut redis_storage = redis_storage.await;
    let mut envelope = JobEnvelope::new(purge_job());
    envelope.enqueued_at = None;
    redis_storage.push(envelope).await.unwrap();

    let sample = sample_queue(&redis_storage, Utc::now()).await.unwrap();

    assert_eq!(sample.pending, 1);
    assert_eq!(sample.oldest_pending_age, 0.0);
}
