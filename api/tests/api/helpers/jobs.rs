use std::time::{Duration, Instant};

use redis_apalis::AsyncCommands;
use tokio::time::sleep;

use crate::helpers::TestedApp;

const DEADLINE: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(100);
const CALM_POLLS: u32 = 3;

#[derive(Debug, PartialEq, Eq)]
pub struct JobCounts {
    pub pushed: usize,
    pub pending: usize,
    pub scheduled: usize,
    pub done: usize,
    pub dead: usize,
}

impl JobCounts {
    fn all_finished(&self) -> bool {
        self.pending == 0 && self.scheduled == 0 && self.pushed == self.done + self.dead
    }
}

/// Counts the jobs of the queue of the app. A running job is in none of the pending, scheduled,
/// done or dead keys: it sits in the inflight set of its worker, which has no fixed key. It is
/// the difference between the jobs pushed and the jobs that ended (done or dead).
pub async fn count_the_jobs(app: &TestedApp) -> JobCounts {
    let config = app.redis_storage.get_config();
    let mut connection = app.redis_storage.get_connection().clone();
    JobCounts {
        pushed: connection
            .hlen(config.job_data_hash())
            .await
            .expect("Failed to count the pushed jobs"),
        pending: connection
            .llen(config.active_jobs_list())
            .await
            .expect("Failed to count the pending jobs"),
        scheduled: connection
            .zcard(config.scheduled_jobs_set())
            .await
            .expect("Failed to count the scheduled jobs"),
        done: connection
            .zcard(config.done_jobs_set())
            .await
            .expect("Failed to count the done jobs"),
        dead: connection
            .zcard(config.dead_jobs_set())
            .await
            .expect("Failed to count the dead jobs"),
    }
}

/// Waits until every job pushed to the queue of the app is done, for several polls in a row so
/// that a job chained by another one is waited for too. A job that died fails the wait: a handler
/// that crashed must not look like a handler that did nothing.
pub async fn wait_until_the_jobs_are_done(app: &TestedApp) {
    let deadline = Instant::now() + DEADLINE;
    let mut calm = 0;
    while calm < CALM_POLLS {
        let counts = count_the_jobs(app).await;
        assert_eq!(counts.dead, 0, "a job died: {counts:?}");
        if counts.all_finished() {
            calm += 1;
        } else {
            calm = 0;
        }
        assert!(
            Instant::now() < deadline,
            "the jobs are not all done after {DEADLINE:?}: {counts:?}"
        );
        sleep(POLL_INTERVAL).await;
    }
}
