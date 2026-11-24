use std::str::FromStr;

use chrono::{DateTime, TimeDelta, Utc};
use cucumber::{Parameter, given, then, when};
use pretty_assertions::assert_eq;
use tokio::time::{Duration, sleep};
use wiremock::{Mock, ResponseTemplate};

use universal_inbox::{
    integration_connection::{
        IntegrationConnectionStatus, config::IntegrationConnectionConfig,
        integrations::github::GithubConfig, provider::IntegrationProviderKind,
    },
    notification::{
        NotificationSourceKind, NotificationStatus, NotificationSyncSourceKind,
        NotificationWithTask, service::NotificationPatch,
    },
    third_party::{
        integrations::github::{GithubNotification, GithubNotificationItem},
        item::ThirdPartyItemData,
    },
};
use universal_inbox_api::repository::integration_connection::{
    IntegrationConnectionRepository, TOO_MANY_SYNC_FAILURES_ERROR_MESSAGE,
};

use crate::{
    common::settings,
    helpers::{
        integration_connection::{
            create_and_mock_integration_connection,
            create_and_mock_integration_connection_with_backoff, create_integration_connection,
            github_oauth_credential,
        },
        notification::{
            github::{
                assert_sync_notifications, create_notification_from_github_notification,
                github_pull_request_123_no_commits_response, github_pull_request_123_response,
                mock_github_notifications_service, mock_github_pull_request_query,
                sync_github_notifications,
            },
            list_notifications, set_notification_updated_at, sync_notifications,
            sync_notifications_response,
        },
        rest::get_resource,
        third_party::overwrite_third_party_item_data,
    },
    state::{KeptTriage, PullRequest},
    steps::notifications::{parse_time, update},
    world::{ApiWorld, Response},
};

const THREAD_456: &str = "456";
const UPDATED_AT_BEFORE_THE_RESYNC: &str = "2026-10-21T12:57:31Z";

#[derive(Clone, Copy, Debug)]
pub struct UserStatus(pub NotificationStatus);

impl Parameter for UserStatus {
    const REGEX: &'static str = "read|deleted|unsubscribed";
    const NAME: &'static str = "status";
}

impl FromStr for UserStatus {
    type Err = String;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Ok(Self(match name {
            "read" => NotificationStatus::Read,
            "deleted" => NotificationStatus::Deleted,
            "unsubscribed" => NotificationStatus::Unsubscribed,
            other => return Err(format!("Unknown status {other:?}")),
        }))
    }
}

fn thread_456(world: &mut ApiWorld) -> &mut GithubNotification {
    world
        .github_sync
        .reported
        .iter_mut()
        .find(|notification| notification.id == THREAD_456)
        .expect("GitHub does not report the thread 456")
}

#[given("GitHub reports the notifications 123 and 456")]
fn github_reports_the_notifications(world: &mut ApiWorld) {
    world.github_sync.reported = sync_github_notifications();
}

#[given("GitHub reports every thread as unread")]
fn github_reports_every_thread_as_unread(world: &mut ApiWorld) {
    for notification in world.github_sync.reported.iter_mut() {
        notification.unread = true;
    }
}

#[given("GitHub knows pull request 123")]
fn github_knows_pull_request_123(world: &mut ApiWorld) {
    world.github_sync.pull_request = Some(PullRequest::WithCommits);
}

#[given("GitHub knows pull request 123, which has no commit")]
fn github_knows_pull_request_123_without_commits(world: &mut ApiWorld) {
    world.github_sync.pull_request = Some(PullRequest::WithoutCommits);
}

#[given(expr = "I already have the notification {string} of the GitHub thread 456")]
async fn i_already_have_the_notification_of_thread_456(world: &mut ApiWorld, name: String) {
    let user_id = world.user().id;
    let connection_id = world
        .integrations
        .github_connection
        .as_ref()
        .expect("The account is not connected to GitHub")
        .id;
    let reported = thread_456(world).clone();
    let app = world.app.get().await;
    let notification =
        create_notification_from_github_notification(app, &reported, user_id, connection_id).await;
    world.notifications.by_name.insert(name, *notification);
}

#[given(expr = "the stored GitHub data of {string} no longer decodes")]
async fn the_stored_data_no_longer_decodes(world: &mut ApiWorld, name: String) {
    let source_item_id = world.notifications.named(&name).source_item.id.0;
    let app = world.app.get().await;
    overwrite_third_party_item_data(
        app,
        source_item_id,
        r#"{"type": "GithubNotification", "content": {"unexpected": true}}"#,
    )
    .await;
}

#[given(expr = "I deleted {string} in Universal Inbox")]
async fn i_deleted_the_notification(world: &mut ApiWorld, name: String) {
    let before = world.notifications.named(&name).last_read_at.unwrap();
    let deleted = update(
        world,
        &name,
        NotificationPatch {
            status: Some(NotificationStatus::Deleted),
            ..Default::default()
        },
    )
    .await;
    let deleted_at = deleted
        .last_read_at
        .expect("deleting a notification must set its last_read_at");
    assert!(deleted_at > before);
    world.github_sync.deleted_at = Some(deleted_at);
}

fn report_thread_456_unread_again(
    world: &mut ApiWorld,
    read_marker_after_delete: Option<TimeDelta>,
) {
    let deleted_at = world
        .github_sync
        .deleted_at
        .expect("No notification was deleted in this scenario");
    let notification = thread_456(world);
    notification.unread = true;
    notification.updated_at = deleted_at + TimeDelta::hours(2);
    if let Some(after) = read_marker_after_delete {
        notification.last_read_at = Some(deleted_at + after);
    }
}

#[given(
    "GitHub reports the thread 456 as unread again, with new activity and its former read marker"
)]
fn github_reports_thread_456_with_its_former_marker(world: &mut ApiWorld) {
    report_thread_456_unread_again(world, None);
}

#[given(
    "GitHub reports the thread 456 as unread again, with new activity and a read marker one hour after I deleted it"
)]
fn github_reports_thread_456_with_a_newer_marker(world: &mut ApiWorld) {
    report_thread_456_unread_again(world, Some(TimeDelta::hours(1)));
}

fn pull_request_response(
    pull_request: PullRequest,
) -> graphql_client::Response<
    universal_inbox_api::integrations::github::graphql::pull_request_query::ResponseData,
> {
    match pull_request {
        PullRequest::WithCommits => github_pull_request_123_response(),
        PullRequest::WithoutCommits => github_pull_request_123_no_commits_response(),
    }
}

async fn mount_github(world: &mut ApiWorld) {
    let reported = world.github_sync.reported.clone();
    let pull_request = world.github_sync.pull_request.map(pull_request_response);
    let app = world.app.get().await;
    mock_github_notifications_service(&app.github_mock_server, "1", &reported).await;
    mock_github_notifications_service(&app.github_mock_server, "2", &Vec::new()).await;
    if let Some(pull_request) = pull_request {
        mock_github_pull_request_query(
            &app.github_mock_server,
            "octokit".to_string(),
            "octokit.rb".to_string(),
            123,
            &pull_request,
        )
        .await;
    }
}

#[when("I sync my GitHub notifications")]
async fn i_sync_my_github_notifications(world: &mut ApiWorld) {
    mount_github(world).await;
    let client = world.client();
    let app = world.app.get().await;
    let synced = sync_notifications(
        &client,
        &app.api_address,
        Some(NotificationSourceKind::Github),
        false,
    )
    .await;
    world.github_sync.synced = synced;
}

#[given(expr = "I take note of the synced notification of the GitHub thread 456 as {string}")]
#[when(expr = "I take note of the synced notification of the GitHub thread 456 as {string}")]
fn i_take_note_of_the_synced_thread_456(world: &mut ApiWorld, name: String) {
    let notification = world
        .github_sync
        .synced
        .iter()
        .find(|notification| notification.source_item.source_id == THREAD_456)
        .expect("the GitHub notification 456 must be synced")
        .clone();
    assert_eq!(notification.status, NotificationStatus::Unread);
    world.notifications.by_name.insert(name, notification);
}

#[when(expr = "I change {string} to {status} in Universal Inbox only")]
async fn i_change_the_status_in_universal_inbox_only(
    world: &mut ApiWorld,
    name: String,
    status: UserStatus,
) {
    let task_id = world.notifications.linked_task.as_ref().map(|task| task.id);
    let patch = NotificationPatch {
        status: Some(status.0),
        task_id,
        ..Default::default()
    };
    let notification = update(world, &name, patch).await;
    let updated_at = parse_time(UPDATED_AT_BEFORE_THE_RESYNC);
    let app = world.app.get().await;
    set_notification_updated_at(app, notification.id, updated_at).await;
    world.github_sync.kept = Some(KeptTriage {
        status: status.0,
        task_id,
        updated_at,
    });
}

#[when("I force a new sync of my GitHub notifications, which reports the same threads")]
async fn i_force_a_new_sync(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    app.notification_service
        .read()
        .await
        .sync_notifications_with_transaction(NotificationSyncSourceKind::Github, user_id, true)
        .await
        .unwrap();
}

#[then("my notifications are the threads GitHub reported")]
fn my_notifications_are_the_reported_threads(world: &mut ApiWorld) {
    assert_eq!(
        world.github_sync.synced.len(),
        world.github_sync.reported.len()
    );
}

#[then("they carry what GitHub reported, with the details of pull request 123")]
fn they_carry_what_github_reported(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let pull_request = world
        .github_sync
        .pull_request
        .map(pull_request_response)
        .expect("GitHub knows no pull request in this scenario")
        .data
        .unwrap()
        .try_into()
        .unwrap();
    assert_sync_notifications(
        &world.github_sync.synced,
        &world.github_sync.reported,
        user_id,
        Some(GithubNotificationItem::GithubPullRequest(pull_request)),
    );
}

async fn read_notification(world: &mut ApiWorld, name: &str) -> NotificationWithTask {
    let id = world.notifications.named(name).id;
    let client = world.client();
    let app = world.app.get().await;
    let notification: Box<NotificationWithTask> =
        get_resource(&client, &app.api_address, "notifications", id.into()).await;
    *notification
}

#[then(expr = "{string} is read since {string}")]
async fn a_notification_is_read_since(world: &mut ApiWorld, name: String, since: String) {
    let original = world.notifications.named(&name).clone();
    let notification = read_notification(world, &name).await;
    assert_eq!(notification.id, original.id);
    assert_eq!(notification.kind, NotificationSourceKind::Github);
    assert_eq!(
        notification.source_item.source_id,
        original.source_item.source_id
    );
    assert_eq!(notification.status, NotificationStatus::Read);
    assert_eq!(notification.last_read_at, Some(parse_time(&since)));
}

#[then(expr = "{string} is still snoozed until {string}")]
async fn a_notification_is_still_snoozed(world: &mut ApiWorld, name: String, until: String) {
    let notification = read_notification(world, &name).await;
    assert_eq!(notification.snoozed_until, Some(parse_time(&until)));
}

#[then(expr = "{string} is still linked to my Todoist task")]
async fn a_notification_is_still_linked(world: &mut ApiWorld, name: String) {
    let linked = world.notifications.linked_task.clone().unwrap().id;
    let notification = read_notification(world, &name).await;
    assert_eq!(notification.task.map(|task| task.id), Some(linked));
}

#[then("my GitHub connection recorded a completed sync, without failure")]
async fn my_connection_recorded_a_completed_sync(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let connection = app
        .repository
        .get_integration_connection_per_provider(
            &mut transaction,
            user_id,
            IntegrationProviderKind::Github,
            None,
            None,
        )
        .await
        .unwrap()
        .unwrap();
    transaction.commit().await.unwrap();
    assert!(connection.last_notifications_sync_started_at.is_some());
    assert!(connection.last_notifications_sync_completed_at.is_some());
    assert!(connection.last_notifications_sync_failed_at.is_none());
    assert!(connection.last_notifications_sync_failure_message.is_none());
    assert_eq!(connection.notifications_sync_failures, 0);
    assert_eq!(connection.status, IntegrationConnectionStatus::Validated);
    assert!(connection.failure_message.is_none());
}

#[then(expr = "{string} is unread, its read marker being the moment I deleted it")]
async fn unread_with_the_deletion_marker(world: &mut ApiWorld, name: String) {
    let expected = world.github_sync.deleted_at;
    assert_unread_with_marker(world, &name, expected).await;
}

#[then(expr = "{string} is unread, its read marker being one hour after I deleted it")]
async fn unread_with_the_newer_marker(world: &mut ApiWorld, name: String) {
    let expected = world
        .github_sync
        .deleted_at
        .map(|deleted_at| deleted_at + TimeDelta::hours(1));
    assert_unread_with_marker(world, &name, expected).await;
}

async fn assert_unread_with_marker(
    world: &mut ApiWorld,
    name: &str,
    expected: Option<DateTime<Utc>>,
) {
    let notification = read_notification(world, name).await;
    assert_eq!(notification.status, NotificationStatus::Unread);
    assert_eq!(notification.last_read_at, expected);
}

#[then(expr = "{string} keeps the status I gave it, its update time and its task")]
async fn a_notification_keeps_its_triage(world: &mut ApiWorld, name: String) {
    let notification = read_notification(world, &name).await;
    let kept = world
        .github_sync
        .kept
        .as_ref()
        .expect("No status was changed in this scenario");
    assert_eq!(notification.status, kept.status);
    assert_eq!(notification.updated_at, kept.updated_at);
    assert_eq!(notification.task.map(|task| task.id), kept.task_id);
}

#[then(
    expr = "{string} is healed in place: its stored data is what GitHub reported, and it is read since {string}"
)]
async fn a_notification_is_healed(world: &mut ApiWorld, name: String, since: String) {
    let original = world.notifications.named(&name).clone();
    let reported = thread_456(world).clone();
    let notification = read_notification(world, &name).await;
    assert_eq!(notification.source_item.id, original.source_item.id);
    assert_eq!(
        notification.source_item.data,
        ThirdPartyItemData::GithubNotification(Box::new(reported))
    );
    assert_eq!(notification.status, NotificationStatus::Read);
    assert_eq!(notification.last_read_at, Some(parse_time(&since)));
}

#[derive(Clone, Copy)]
enum SyncRequest {
    Asked,
    NotAsked,
}

async fn list_read_notifications(world: &mut ApiWorld, request: SyncRequest) -> usize {
    let client = world.client();
    let app = world.app.get().await;
    list_notifications(
        &client,
        &app.api_address,
        vec![NotificationStatus::Read],
        false,
        None,
        None,
        matches!(request, SyncRequest::Asked),
    )
    .await
    .len()
}

#[given("GitHub would answer 200 to anything")]
#[when("GitHub would answer 200 to anything")]
async fn github_would_answer_200(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let anything = Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(200))
        .mount_as_scoped(&app.github_mock_server)
        .await;
    world.github_sync.anything = Some(anything);
}

#[then("GitHub was not asked anything")]
async fn github_was_not_asked_anything(world: &mut ApiWorld) {
    let anything = world
        .github_sync
        .anything
        .as_ref()
        .expect("GitHub was not prepared to answer anything");
    assert_eq!(anything.received_requests().await.len(), 0);
}

#[given("GitHub answers 400 to anything")]
async fn github_answers_400(world: &mut ApiWorld) {
    let app = world.app.get().await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(400))
        .mount(&app.github_mock_server)
        .await;
}

#[given("I have a GitHub connection that was never validated")]
async fn i_have_a_never_validated_connection(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    create_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        IntegrationConnectionStatus::Created,
        None,
        None,
        None,
        None,
        None,
    )
    .await;
}

#[given("I have a GitHub connection whose synchronisation is disabled")]
async fn i_have_a_connection_with_the_sync_disabled(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    create_and_mock_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::Github(GithubConfig::disabled()),
        &settings(),
        github_oauth_credential(),
        None,
        None,
    )
    .await;
}

#[given("my GitHub connection already failed 5 times, the first failure being 2 hours old")]
async fn my_connection_already_failed(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let connection = create_and_mock_integration_connection_with_backoff(
        app,
        user_id,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        &settings(),
        github_oauth_credential(),
        Some(5),
        Some(Utc::now() - TimeDelta::hours(2)),
        None,
    )
    .await;
    world.integrations.github_connection = Some(connection);
}

#[when("I list my read notifications, asking for a sync")]
async fn i_list_my_read_notifications_asking_for_a_sync(world: &mut ApiWorld) {
    let count = list_read_notifications(world, SyncRequest::Asked).await;
    world.github_sync.read_count = Some(count);
}

#[when("I ask for an asynchronous sync of my GitHub notifications")]
async fn i_ask_for_an_asynchronous_sync(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    let response = sync_notifications_response(
        &client,
        &app.api_address,
        Some(NotificationSourceKind::Github),
        true,
    )
    .await;
    world.http.record(Response::read(response).await);
}

#[when("I ask for a synchronous sync of my GitHub notifications")]
async fn i_ask_for_a_synchronous_sync(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    let response = sync_notifications_response(
        &client,
        &app.api_address,
        Some(NotificationSourceKind::Github),
        false,
    )
    .await;
    world.http.record(Response::read(response).await);
}

#[when("one second passes")]
async fn one_second_passes(_world: &mut ApiWorld) {
    sleep(Duration::from_secs(1)).await;
}

async fn wait_for_a_read_notification(world: &mut ApiWorld, request: SyncRequest) {
    for _ in 0..=20 {
        if list_read_notifications(world, request).await == 1 {
            return;
        }
        sleep(Duration::from_millis(100)).await;
    }
    panic!("None of the notifications became read within two seconds");
}

#[then("none of my notifications is read yet")]
fn none_of_my_notifications_is_read_yet(world: &mut ApiWorld) {
    assert_eq!(world.github_sync.read_count, Some(0));
}

#[then(
    "within two seconds, one of my notifications becomes read, listing them asking for a sync each time"
)]
async fn a_notification_becomes_read_asking_for_syncs(world: &mut ApiWorld) {
    wait_for_a_read_notification(world, SyncRequest::Asked).await;
}

#[then(
    "within two seconds, one of my notifications becomes read, listing them without asking for a sync"
)]
async fn a_notification_becomes_read_without_asking(world: &mut ApiWorld) {
    wait_for_a_read_notification(world, SyncRequest::NotAsked).await;
}

#[then("I still have exactly one read notification")]
async fn i_still_have_one_read_notification(world: &mut ApiWorld) {
    assert_eq!(
        list_read_notifications(world, SyncRequest::NotAsked).await,
        1
    );
}

#[then("I have no read notification")]
async fn i_have_no_read_notification(world: &mut ApiWorld) {
    assert_eq!(
        list_read_notifications(world, SyncRequest::NotAsked).await,
        0
    );
}

#[then(
    "my GitHub connection recorded a failed sync, the sixth, and is failing for too many sync failures"
)]
async fn my_connection_recorded_a_failed_sync(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let connection = app
        .repository
        .get_integration_connection_per_provider(
            &mut transaction,
            user_id,
            IntegrationProviderKind::Github,
            None,
            None,
        )
        .await
        .unwrap()
        .unwrap();
    transaction.commit().await.unwrap();
    assert!(connection.last_notifications_sync_started_at.is_some());
    assert!(connection.last_notifications_sync_completed_at.is_none());
    assert!(connection.last_notifications_sync_failed_at.is_some());
    assert_eq!(
        connection
            .last_notifications_sync_failure_message
            .unwrap()
            .as_str(),
        "Failed to fetch notifications from Github"
    );
    assert_eq!(connection.notifications_sync_failures, 6);
    assert!(connection.first_notifications_sync_failed_at.is_some());
    assert_eq!(connection.status, IntegrationConnectionStatus::Failing);
    assert_eq!(
        connection.failure_message,
        Some(TOO_MANY_SYNC_FAILURES_ERROR_MESSAGE.to_string())
    );
}

#[given(expr = "{string} is unread in Universal Inbox")]
async fn a_notification_is_unread_in_universal_inbox(world: &mut ApiWorld, name: String) {
    update(
        world,
        &name,
        NotificationPatch {
            status: Some(NotificationStatus::Unread),
            ..Default::default()
        },
    )
    .await;
}

#[given("GitHub serves what it reported")]
async fn github_serves_what_it_reported(world: &mut ApiWorld) {
    mount_github(world).await;
}
