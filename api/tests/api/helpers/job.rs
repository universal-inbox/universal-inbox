use apalis::{prelude::Storage, redis::RedisStorage};
use tokio::time::{sleep, Duration};

use universal_inbox_api::jobs::UniversalInboxJob;

pub async fn wait_for_jobs_completion(storage: &RedisStorage<UniversalInboxJob>) -> bool {
    let mut i = 0;
    loop {
        if storage.is_empty().await.expect("Failed to get jobs count") {
            break true;
        }

        if i == 10 {
            // Give up after 10 attempts
            break false;
        }

        sleep(Duration::from_millis(100)).await;
        i += 1;
    }
}
