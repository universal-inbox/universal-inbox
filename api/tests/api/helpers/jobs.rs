use std::time::{Duration, Instant};

use redis_apalis::AsyncCommands;
use tokio::time::sleep;

use crate::helpers::TestedApp;

const DEADLINE: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(100);
const CALM_POLLS: u32 = 3;

/// Waits until the job queue of the app has nothing pending, running or scheduled for several
/// polls in a row, so that a "nothing happened" or "exactly one" read cannot run before the job
/// that would have made it false.
pub async fn wait_until_the_jobs_are_done(app: &TestedApp) {
    let config = app.redis_storage.get_config();
    let (pending_list, inflight_set, scheduled_set) = (
        config.active_jobs_list(),
        config.inflight_jobs_set(),
        config.scheduled_jobs_set(),
    );
    let mut connection = app.redis_storage.get_connection().clone();
    let deadline = Instant::now() + DEADLINE;
    let mut calm = 0;
    while calm < CALM_POLLS {
        let pending: usize = connection
            .llen(&pending_list)
            .await
            .expect("Failed to count the pending jobs");
        let inflight: usize = connection
            .zcard(&inflight_set)
            .await
            .expect("Failed to count the running jobs");
        let scheduled: usize = connection
            .zcard(&scheduled_set)
            .await
            .expect("Failed to count the scheduled jobs");
        if pending + inflight + scheduled == 0 {
            calm += 1;
        } else {
            calm = 0;
        }
        assert!(
            Instant::now() < deadline,
            "{pending} pending, {inflight} running and {scheduled} scheduled job(s) after {DEADLINE:?}"
        );
        sleep(POLL_INTERVAL).await;
    }
}
