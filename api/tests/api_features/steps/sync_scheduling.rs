use cucumber::{then, when};
use pretty_assertions::assert_eq;

use universal_inbox::notification::{NotificationSourceKind, NotificationStatus};

use crate::{
    helpers::{
        notification::{list_notifications_response, sync_notifications_response},
        task::sync_tasks_response,
    },
    world::{ApiWorld, Response},
};

#[when(
    expr = "I list my unread and read notifications {int} at a time, {int} times in a row, each time asking for a sync"
)]
async fn i_list_notifications_concurrently(world: &mut ApiWorld, width: usize, rounds: usize) {
    let client = world.client();
    let app = world.app.get().await;
    for _ in 0..rounds {
        let requests = (0..width).map(|_| {
            list_notifications_response(
                &client,
                &app.api_address,
                vec![NotificationStatus::Unread, NotificationStatus::Read],
                false,
                None,
                None,
                true,
            )
        });
        for response in futures::future::join_all(requests).await {
            world.http.record(Response::read(response).await);
        }
    }
}

#[when(
    "a visitor who is not signed in asks for a GitHub notification sync, a notification sync and a task sync"
)]
async fn a_visitor_asks_for_syncs(world: &mut ApiWorld) {
    let visitor = reqwest::Client::new();
    let app = world.app.get().await;
    let answers = [
        sync_notifications_response(
            &visitor,
            &app.api_address,
            Some(NotificationSourceKind::Github),
            true,
        )
        .await,
        sync_notifications_response(&visitor, &app.api_address, None, true).await,
        sync_tasks_response(&visitor, &app.api_address, None, true).await,
    ];
    for answer in answers {
        world.http.record(Response::read(answer).await);
    }
}

#[then(expr = "the app answered {int}, {int} times")]
fn the_app_answered_n_times(world: &mut ApiWorld, status: u16, count: usize) {
    let answers = world.http.all();
    assert_eq!(answers.len(), count);
    for answer in answers {
        assert_eq!(answer.status.as_u16(), status);
    }
}
