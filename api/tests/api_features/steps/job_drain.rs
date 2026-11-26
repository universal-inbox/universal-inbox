use cucumber::{given, then, when};
use pretty_assertions::assert_eq;

use crate::{
    helpers::jobs::{JobCounts, wait_until_the_jobs_are_done, wait_until_the_jobs_have_ended},
    world::ApiWorld,
};

#[given("the background jobs are all done")]
#[when("the background jobs are all done")]
async fn the_background_jobs_are_done(world: &mut ApiWorld) {
    let app = world.app.get().await;
    wait_until_the_jobs_are_done(app).await;
}

#[when("the background jobs have all ended")]
async fn the_background_jobs_have_ended(world: &mut ApiWorld) {
    let app = world.app.get().await;
    world.job_drain = Some(wait_until_the_jobs_have_ended(app).await);
}

#[then(
    expr = "of the {int} job(s) queued, {int} is/are done, {int} is/are dead and {int} wait(s) for a retry"
)]
fn the_jobs_ended_as(
    world: &mut ApiWorld,
    pushed: usize,
    done: usize,
    dead: usize,
    scheduled: usize,
) {
    assert_eq!(
        world.job_drain.take(),
        Some(JobCounts {
            pushed,
            pending: 0,
            scheduled,
            done,
            dead,
        })
    );
}
