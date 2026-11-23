use cucumber::{then, when};
use pretty_assertions::assert_eq;
use reqwest::StatusCode;

use universal_inbox::notification::{
    NotificationStatus,
    service::{NotificationPatch, PatchNotificationsRequest},
};

use crate::{
    helpers::{
        notification::sync_notifications_response, rest::patch_resource_collection_response,
        task::sync_tasks_response,
    },
    world::{ApiWorld, Response},
};

#[when(
    expr = "I ask for {int} syncs, alternating between notifications and tasks, each answered 201"
)]
async fn i_ask_for_syncs_each_answered_201(world: &mut ApiWorld, count: usize) {
    let client = world.client();
    let api_address = world.app.get().await.api_address.clone();
    for i in 0..count {
        let response = if i % 2 == 0 {
            sync_notifications_response(&client, &api_address, None, true).await
        } else {
            sync_tasks_response(&client, &api_address, None, true).await
        };
        assert_eq!(response.status(), StatusCode::CREATED, "request {i}");
    }
}

#[when("I ask for one more task sync")]
async fn i_ask_for_one_more_task_sync(world: &mut ApiWorld) {
    let client = world.client();
    let api_address = world.app.get().await.api_address.clone();
    let response = sync_tasks_response(&client, &api_address, None, true).await;
    world.http.record(Response::read(response).await);
}

#[then("the response asks to wait at least 1 second before retrying")]
fn the_response_asks_to_wait(world: &mut ApiWorld) {
    let retry_after: u64 = world
        .http
        .last()
        .header("retry-after")
        .expect("Retry-After header")
        .parse()
        .unwrap();
    assert!(retry_after >= 1);
}

#[when("that other user asks for a notifications sync")]
async fn that_other_user_asks_for_a_notifications_sync(world: &mut ApiWorld) {
    let client = world
        .other_session
        .as_ref()
        .expect("No other user is signed in")
        .client
        .clone();
    let app = world.app.get().await;
    let response = sync_notifications_response(&client, &app.api_address, None, true).await;
    world.http.record(Response::read(response).await);
}

fn delete_all_unread_notifications() -> PatchNotificationsRequest {
    PatchNotificationsRequest {
        status: vec![NotificationStatus::Unread],
        sources: vec![],
        patch: NotificationPatch {
            status: Some(NotificationStatus::Deleted),
            ..Default::default()
        },
    }
}

#[when(expr = "I delete all my unread notifications {int} times, each answered 200")]
async fn i_delete_all_my_unread_notifications_several_times(world: &mut ApiWorld, count: usize) {
    let client = world.client();
    let api_address = world.app.get().await.api_address.clone();
    for i in 0..count {
        let response = patch_resource_collection_response(
            &client,
            &api_address,
            "notifications",
            &delete_all_unread_notifications(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK, "request {i}");
    }
}

#[when("I delete all my unread notifications once more")]
async fn i_delete_all_my_unread_notifications_once_more(world: &mut ApiWorld) {
    let client = world.client();
    let api_address = world.app.get().await.api_address.clone();
    let response = patch_resource_collection_response(
        &client,
        &api_address,
        "notifications",
        &delete_all_unread_notifications(),
    )
    .await;
    world.http.record(Response::read(response).await);
}
