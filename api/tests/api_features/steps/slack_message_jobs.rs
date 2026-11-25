use std::str::FromStr;

use cucumber::{Parameter, given, then, when};
use pretty_assertions::assert_eq;
use slack_morphism::prelude::*;
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{method, path, query_param},
};

use universal_inbox::{
    audit::AuditActor,
    integration_connection::{
        IntegrationConnectionStatus, config::IntegrationConnectionConfig,
        integrations::slack::SlackConfig, provider::IntegrationProviderKind,
    },
    notification::{NotificationSourceKind, NotificationStatus},
    third_party::{integrations::slack::SlackThread, item::ThirdPartyItemData},
};
use universal_inbox_api::{
    integrations::slack::edited_slack_message_event,
    jobs::slack::slack_message::{SlackMessageDecision, handle_slack_message_push_event},
};

use crate::{
    common::settings,
    helpers::{
        fixture_path,
        integration_connection::{
            create_and_mock_integration_connection, create_integration_connection, slack_context,
            slack_oauth_credential,
        },
        notification::{
            list_notifications,
            slack::{
                create_notification_from_slack_thread, mock_slack_fetch_bot,
                mock_slack_fetch_channel, mock_slack_fetch_team, mock_slack_fetch_thread,
                mock_slack_fetch_user, mock_slack_get_chat_permalink, mock_slack_list_emojis,
                mock_slack_list_usergroups, mock_slack_list_users_in_usergroup, slack_thread,
            },
        },
        set_audit_actor,
    },
    state::SlackEvent,
    steps::{slack_webhook_events::Subscription, status::UserStatus},
    world::ApiWorld,
};

const CHANNEL: &str = "C05XXX";
const TEAM: &str = "T01";

#[derive(Clone, Copy, Debug)]
pub enum Who {
    Me,
    TheOtherUser,
}

impl Parameter for Who {
    const REGEX: &'static str = "I|the other user";
    const NAME: &'static str = "who";
}

impl FromStr for Who {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "I" => Ok(Self::Me),
            "the other user" => Ok(Self::TheOtherUser),
            other => Err(format!("Unknown person {other:?}")),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ThreadKind {
    Ordinary,
    EmbeddedProfiles,
    WithABot,
    UnknownUsergroup,
    PlanBlock,
    SubscribedReadToFirst,
    UnsubscribedReadToFirst,
    SubscribedReadToSecond,
}

impl Parameter for ThreadKind {
    const REGEX: &'static str = "an ordinary thread|a thread with embedded user profiles|a thread with a bot|a thread that references an unknown usergroup|a thread with a plan block and an unknown block|a subscribed thread, read up to its first message|an unsubscribed thread, read up to its first message|a subscribed thread, read up to its second message";
    const NAME: &'static str = "thread_kind";
}

impl FromStr for ThreadKind {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Ok(match text {
            "an ordinary thread" => Self::Ordinary,
            "a thread with embedded user profiles" => Self::EmbeddedProfiles,
            "a thread with a bot" => Self::WithABot,
            "a thread that references an unknown usergroup" => Self::UnknownUsergroup,
            "a thread with a plan block and an unknown block" => Self::PlanBlock,
            "a subscribed thread, read up to its first message" => Self::SubscribedReadToFirst,
            "an unsubscribed thread, read up to its first message" => Self::UnsubscribedReadToFirst,
            "a subscribed thread, read up to its second message" => Self::SubscribedReadToSecond,
            other => return Err(format!("Unknown thread kind {other:?}")),
        })
    }
}

impl ThreadKind {
    fn fixture_subscribed_read(self) -> (&'static str, bool, Option<usize>) {
        match self {
            Self::Ordinary => ("slack_fetch_thread_response.json", true, None),
            Self::EmbeddedProfiles => (
                "slack_embedded_user_profiles_fetch_thread_response.json",
                true,
                None,
            ),
            Self::WithABot => ("slack_fetch_thread_with_bot_response.json", true, None),
            Self::UnknownUsergroup => (
                "slack_fetch_thread_with_unknown_usergroup_response.json",
                true,
                None,
            ),
            Self::PlanBlock => (
                "slack_fetch_thread_with_plan_block_response.json",
                true,
                None,
            ),
            Self::SubscribedReadToFirst => ("slack_fetch_thread_response.json", true, Some(0)),
            Self::UnsubscribedReadToFirst => ("slack_fetch_thread_response.json", false, Some(0)),
            Self::SubscribedReadToSecond => ("slack_fetch_thread_response.json", true, Some(1)),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum MessageRef {
    ThatMessage,
    FirstOfTheThread,
    LastOfTheThread,
}

impl Parameter for MessageRef {
    const REGEX: &'static str =
        "that message|the first message of the thread|the last message of the thread";
    const NAME: &'static str = "message_ref";
}

impl FromStr for MessageRef {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "that message" => Ok(Self::ThatMessage),
            "the first message of the thread" => Ok(Self::FirstOfTheThread),
            "the last message of the thread" => Ok(Self::LastOfTheThread),
            other => Err(format!("Unknown message reference {other:?}")),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum SyncWays {
    OneWay,
    TwoWay,
}

impl Parameter for SyncWays {
    const REGEX: &'static str = "one-way|two-way";
    const NAME: &'static str = "sync";
}

impl FromStr for SyncWays {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "one-way" => Ok(Self::OneWay),
            "two-way" => Ok(Self::TwoWay),
            other => Err(format!("Unknown sync {other:?}")),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum ReadTo {
    Nothing,
    TheLastMessage,
}

const DECISIONS: [&str; 7] = [
    "created",
    "updated",
    "unchanged",
    "unsubscribed",
    "deleted_last_from_user",
    "sync_disabled",
    "thread_not_accessible",
];

#[derive(Debug)]
pub struct Decisions(Vec<&'static str>);

impl Parameter for Decisions {
    const REGEX: &'static str = "[a-z_]+(?:, [a-z_]+)*";
    const NAME: &'static str = "decisions";
}

impl FromStr for Decisions {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        text.split(", ")
            .map(|decision| {
                DECISIONS
                    .into_iter()
                    .find(|known| *known == decision)
                    .ok_or_else(|| format!("Unknown decision {decision:?}"))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Self)
    }
}
fn callback_of(world: &ApiWorld) -> SlackPushEventCallback {
    match world.slack_webhook.event.as_ref().expect("No Slack event") {
        SlackEvent::Push(event) => match &**event {
            SlackPushEvent::EventCallback(callback) => callback.clone(),
            _ => panic!("Unexpected event type"),
        },
        SlackEvent::Edit(raw) => {
            let callback: SlackPushEventCallback = serde_json::from_value(raw.clone()).unwrap();
            let SlackEventCallbackBody::Message(changed) = &callback.event else {
                panic!("Unexpected event type");
            };
            let edited = edited_slack_message_event(changed, raw)
                .expect("message_changed event should carry an edited message");
            let origin = &edited.origin;
            assert_eq!(origin.ts.to_string(), "1732535291.911209");
            assert_eq!(
                origin.thread_ts.as_ref().map(|ts| ts.to_string()),
                Some("1732535291.911209".to_string())
            );
            SlackPushEventCallback {
                event: SlackEventCallbackBody::Message(edited),
                ..callback
            }
        }
    }
}

fn ids_of_the_event(world: &ApiWorld) -> (String, String) {
    let SlackEventCallbackBody::Message(message) = callback_of(world).event else {
        panic!("Unexpected event type");
    };
    let ts = message.origin.ts.to_string();
    let root = message
        .origin
        .thread_ts
        .map_or_else(|| ts.clone(), |thread_ts| thread_ts.to_string());
    (root, ts)
}

fn client_of(world: &ApiWorld, who: Who) -> reqwest::Client {
    match who {
        Who::Me => world.client(),
        Who::TheOtherUser => world.other_session.as_ref().unwrap().client.clone(),
    }
}

async fn connect(
    world: &mut ApiWorld,
    who: Who,
    slack_user: Option<&str>,
    access_token: Option<&str>,
    sync: Option<SyncWays>,
) {
    let user_id = match who {
        Who::Me => world.user().id,
        Who::TheOtherUser => world.other_session.as_ref().unwrap().user.id,
    };
    let mut credential = slack_oauth_credential();
    if let Some(slack_user) = slack_user {
        credential.provider_user_id = Some(slack_user.to_string());
    }
    if let Some(access_token) = access_token {
        credential.access_token =
            universal_inbox_api::integrations::oauth2::AccessToken(access_token.to_string());
    }
    let mut config = SlackConfig::enabled_as_notifications();
    if let Some(sync) = sync {
        config.message_config.is_2way_sync = matches!(sync, SyncWays::TwoWay);
    }
    let app = world.app.get().await;
    let connection = create_and_mock_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::Slack(config),
        &settings(),
        credential,
        None,
        Some(slack_context(TEAM)),
    )
    .await;
    match who {
        Who::Me => world.integrations.slack_connection = Some(connection),
        Who::TheOtherUser => world.slack_webhook.other_connection = Some(connection),
    }
}

#[given(expr = "{who} am/is connected to Slack as the Slack user {string}")]
async fn connected_as(world: &mut ApiWorld, who: Who, slack_user: String) {
    connect(world, who, Some(&slack_user), None, None).await;
}

#[given(
    expr = "{who} am/is connected to Slack as the Slack user {string}, with the access token {string}"
)]
async fn connected_with_a_token(world: &mut ApiWorld, who: Who, slack_user: String, token: String) {
    connect(world, who, Some(&slack_user), Some(&token), None).await;
}

#[given(expr = "I am connected to Slack as the Slack user {string}, with {sync} sync")]
async fn connected_as_with_sync(world: &mut ApiWorld, slack_user: String, sync: SyncWays) {
    connect(world, Who::Me, Some(&slack_user), None, Some(sync)).await;
}

#[given(expr = "I am connected to Slack, with {sync} sync")]
async fn connected_with_sync(world: &mut ApiWorld, sync: SyncWays) {
    connect(world, Who::Me, None, None, Some(sync)).await;
}

#[given(expr = "I am connected to Slack as the Slack user {string}, with its sync disabled")]
async fn connected_with_the_sync_disabled(world: &mut ApiWorld, slack_user: String) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    create_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::Slack(SlackConfig::disabled()),
        IntegrationConnectionStatus::Validated,
        None,
        Some(slack_user),
        None,
        None,
        None,
    )
    .await;
}

async fn know_a_thread(
    world: &mut ApiWorld,
    who: Who,
    subscribed: Subscription,
    read_to: ReadTo,
    expected: NotificationStatus,
) {
    let (user_id, connection_id) = match who {
        Who::Me => (
            world.user().id,
            world.integrations.slack_connection.as_ref().unwrap().id,
        ),
        Who::TheOtherUser => (
            world.other_session.as_ref().unwrap().user.id,
            world.slack_webhook.other_connection.as_ref().unwrap().id,
        ),
    };
    let mut thread = slack_thread();
    thread.subscribed = subscribed.is_subscribed();
    thread.last_read = match read_to {
        ReadTo::Nothing => None,
        ReadTo::TheLastMessage => Some(thread.messages.last().origin.ts.clone()),
    };
    let app = world.app.get().await;
    let notification =
        create_notification_from_slack_thread(app, &thread, user_id, connection_id).await;
    assert_eq!(notification.status, expected);
    if matches!(who, Who::Me) {
        world.slack_webhook.known_thread = Some(thread);
    }
}

#[given(expr = "{who} know(s) a Slack thread that is {subscription}, read up to its last message")]
async fn know_a_read_thread(world: &mut ApiWorld, who: Who, subscription: Subscription) {
    let expected = if subscription.is_subscribed() {
        NotificationStatus::Deleted
    } else {
        NotificationStatus::Unsubscribed
    };
    know_a_thread(world, who, subscription, ReadTo::TheLastMessage, expected).await;
}

#[given(expr = "{who} know(s) a Slack thread that is subscribed and not read yet")]
async fn know_an_unread_thread(world: &mut ApiWorld, who: Who) {
    know_a_thread(
        world,
        who,
        Subscription::Subscribed,
        ReadTo::Nothing,
        NotificationStatus::Unread,
    )
    .await;
}

#[given(expr = "that message mentions the Slack usergroup {string}")]
fn that_message_mentions_a_usergroup(world: &mut ApiWorld, usergroup_id: String) {
    let Some(SlackEvent::Push(event)) = world.slack_webhook.event.as_mut() else {
        panic!("The scenario has no message");
    };
    let SlackPushEvent::EventCallback(SlackPushEventCallback {
        event: SlackEventCallbackBody::Message(message),
        ..
    }) = &mut **event
    else {
        panic!("Unexpected event type");
    };
    message.content.as_mut().expect("No content").blocks = Some(vec![SlackBlock::RichText(
        serde_json::from_value(serde_json::json!({
            "block_id": "12345",
            "elements": [{
                "type": "rich_text_section",
                "elements": [{ "type": "usergroup", "usergroup_id": usergroup_id }]
            }]
        }))
        .unwrap(),
    )]);
}

async fn body_of(fixture: &str) -> String {
    std::fs::read_to_string(fixture_path(fixture)).unwrap()
}

#[given(expr = "Slack serves the Slack user {string}")]
async fn slack_serves_a_user(world: &mut ApiWorld, user_id: String) {
    let app = world.app.get().await;
    mock_slack_fetch_user(
        &app.slack_mock_server,
        &user_id,
        "slack_fetch_user_response.json",
    )
    .await;
}

#[given(expr = "Slack serves the channel {string}")]
async fn slack_serves_a_channel(world: &mut ApiWorld, channel_id: String) {
    let app = world.app.get().await;
    mock_slack_fetch_channel(
        &app.slack_mock_server,
        &channel_id,
        "slack_fetch_channel_response.json",
    )
    .await;
}

#[given(expr = "Slack serves the team {string}")]
async fn slack_serves_a_team(world: &mut ApiWorld, team_id: String) {
    let app = world.app.get().await;
    mock_slack_fetch_team(
        &app.slack_mock_server,
        &team_id,
        "slack_fetch_team_response.json",
    )
    .await;
}

#[given(expr = "Slack does not know the team {string}")]
async fn slack_does_not_know_the_team(world: &mut ApiWorld, team_id: String) {
    let app = world.app.get().await;
    mock_slack_fetch_team(
        &app.slack_mock_server,
        &team_id,
        "slack_fetch_team_not_found_response.json",
    )
    .await;
}

#[given("Slack serves its emojis")]
async fn slack_serves_its_emojis(world: &mut ApiWorld) {
    let app = world.app.get().await;
    mock_slack_list_emojis(&app.slack_mock_server, "slack_emoji_list_response.json").await;
}

#[given(expr = "Slack serves the permalink of {message_ref}")]
async fn slack_serves_a_permalink(world: &mut ApiWorld, message: MessageRef) {
    let ts = match message {
        MessageRef::ThatMessage => ids_of_the_event(world).1,
        MessageRef::FirstOfTheThread => slack_thread().messages.first().origin.ts.to_string(),
        MessageRef::LastOfTheThread => slack_thread().messages.last().origin.ts.to_string(),
    };
    serve_permalink(world, &ts).await;
}

#[given(expr = "Slack serves the permalink of the message {string}")]
async fn slack_serves_the_permalink_of_a_ts(world: &mut ApiWorld, ts: String) {
    serve_permalink(world, &ts).await;
}

async fn serve_permalink(world: &mut ApiWorld, ts: &str) {
    let app = world.app.get().await;
    mock_slack_get_chat_permalink(
        &app.slack_mock_server,
        CHANNEL,
        ts,
        "slack_get_chat_permalink_response.json",
    )
    .await;
}

#[given(expr = "Slack serves {thread_kind} of that message to the token {string}")]
async fn slack_serves_a_thread(world: &mut ApiWorld, kind: ThreadKind, token: String) {
    let (root, ts) = ids_of_the_event(world);
    let (fixture, subscribed, read) = kind.fixture_subscribed_read();
    let app = world.app.get().await;
    mock_slack_fetch_thread(
        &app.slack_mock_server,
        CHANNEL,
        &root,
        &ts,
        fixture,
        subscribed,
        read,
        &token,
    )
    .await;
}

#[given(
    expr = "Slack does not let the token {string} see the thread of that message, and is asked about it once"
)]
async fn slack_hides_the_thread(world: &mut ApiWorld, _token: String) {
    let (_, ts) = ids_of_the_event(world);
    let body = body_of("slack_fetch_thread_channel_not_found_response.json").await;
    let app = world.app.get().await;
    Mock::given(method("GET"))
        .and(path("/conversations.replies"))
        .and(query_param("ts", ts))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/json"))
        .expect(1)
        .mount(&app.slack_mock_server)
        .await;
}

#[given(expr = "Slack does not know the bot {string}")]
async fn slack_does_not_know_the_bot(world: &mut ApiWorld, bot_id: String) {
    let app = world.app.get().await;
    mock_slack_fetch_bot(
        &app.slack_mock_server,
        &bot_id,
        "slack_fetch_bot_not_found_response.json",
    )
    .await;
}

#[given(expr = "Slack is rate limiting the requests about the bot {string}")]
async fn slack_rate_limits_the_bot(world: &mut ApiWorld, bot_id: String) {
    let app = world.app.get().await;
    mock_slack_fetch_bot(
        &app.slack_mock_server,
        &bot_id,
        "slack_fetch_bot_ratelimited_response.json",
    )
    .await;
}

#[given(expr = "Slack serves the members of the usergroup {string}")]
async fn slack_serves_a_usergroup(world: &mut ApiWorld, usergroup_id: String) {
    let app = world.app.get().await;
    mock_slack_list_users_in_usergroup(
        &app.slack_mock_server,
        &usergroup_id,
        "slack_list_users_in_usergroup_response.json",
    )
    .await;
}

#[given(expr = "Slack does not know the usergroup {string}")]
async fn slack_does_not_know_the_usergroup(world: &mut ApiWorld, usergroup_id: String) {
    let app = world.app.get().await;
    mock_slack_list_users_in_usergroup(
        &app.slack_mock_server,
        &usergroup_id,
        "slack_list_users_in_usergroup_no_such_subteam_response.json",
    )
    .await;
}

#[given("Slack lists its usergroups")]
async fn slack_lists_its_usergroups(world: &mut ApiWorld) {
    let app = world.app.get().await;
    mock_slack_list_usergroups(
        &app.slack_mock_server,
        "slack_list_usergroups_response.json",
    )
    .await;
}

#[given(
    expr = "Slack does not know the bot {string} nor the user {string}, and is asked once about each"
)]
async fn slack_does_not_know_a_bot_nor_a_user(world: &mut ApiWorld, bot: String, user: String) {
    let bot_body = body_of("slack_fetch_bot_not_found_response.json").await;
    let user_body = body_of("slack_fetch_user_not_found_response.json").await;
    let app = world.app.get().await;
    Mock::given(method("GET"))
        .and(path("/bots.info"))
        .and(query_param("bot", bot))
        .respond_with(ResponseTemplate::new(200).set_body_raw(bot_body, "application/json"))
        .expect(1)
        .mount(&app.slack_mock_server)
        .await;
    Mock::given(method("GET"))
        .and(path("/users.info"))
        .and(query_param("user", user))
        .respond_with(ResponseTemplate::new(200).set_body_raw(user_body, "application/json"))
        .expect(1)
        .mount(&app.slack_mock_server)
        .await;
}

fn outcomes_of(decisions: &[(impl Sized, SlackMessageDecision)]) -> Vec<&'static str> {
    let mut outcomes: Vec<&'static str> = decisions
        .iter()
        .map(|(_, decision)| decision.as_str())
        .collect();
    outcomes.sort_unstable();
    outcomes
}

async fn run_the_handler(world: &mut ApiWorld, times: usize) {
    let callback = callback_of(world);
    let app = world.app.get().await;
    let service = app.notification_service.read().await;
    let mut transaction = service.begin().await.unwrap();
    set_audit_actor(
        &mut transaction,
        &AuditActor::Webhook {
            provider: IntegrationProviderKind::Slack,
        },
    )
    .await;
    let mut runs = Vec::new();
    let mut failure = None;
    for _ in 0..times {
        match handle_slack_message_push_event(
            &mut transaction,
            &callback,
            app.notification_service.clone(),
            app.integration_connection_service.clone(),
            app.third_party_item_service.clone(),
            app.slack_service.clone(),
        )
        .await
        {
            Ok(decisions) => runs.push(outcomes_of(&decisions)),
            Err(err) => {
                failure = Some(format!("{err:?}"));
                break;
            }
        }
    }
    if failure.is_none() {
        transaction
            .commit()
            .await
            .expect("Failed to commit transaction");
    } else {
        drop(transaction);
    }
    world.slack_webhook.runs = runs;
    world.slack_webhook.failure = failure;
}

#[when("the Slack message job handles that event")]
async fn the_job_handles_the_event(world: &mut ApiWorld) {
    run_the_handler(world, 1).await;
}

#[when(
    "the Slack message job handles that event, then handles it again as Slack retries events it did not see acknowledged"
)]
async fn the_job_handles_the_event_twice(world: &mut ApiWorld) {
    run_the_handler(world, 2).await;
}

#[when(
    expr = "the app asks Slack twice who the bot {string} and the user {string} are, with the token {string}"
)]
async fn the_app_asks_twice(world: &mut ApiWorld, bot: String, user: String, token: String) {
    let user_id = world.user().id;
    let api_token = SlackApiToken::new(SlackApiTokenValue(token));
    let app = world.app.get().await;
    let mut unresolved = Vec::new();
    for _ in 0..2 {
        let bot = app
            .slack_service
            .fetch_bot(&SlackBotId(bot.clone()), &api_token)
            .await
            .unwrap();
        let user = app
            .slack_service
            .fetch_user(&SlackUserId(user.clone()), user_id, &api_token)
            .await
            .unwrap();
        unresolved.push(bot.is_none());
        unresolved.push(user.is_none());
    }
    world.slack_webhook.unresolved = unresolved;
}

#[when(expr = "{who} look(s) at my/their notifications")]
async fn look_at_notifications(world: &mut ApiWorld, who: Who) {
    let client = client_of(world, who);
    let app = world.app.get().await;
    let listing =
        list_notifications(&client, &app.api_address, vec![], false, None, None, false).await;
    world.slack_webhook.listing = listing;
}

#[then(expr = "the job decides {decisions}")]
fn the_job_decides(world: &mut ApiWorld, expected: Decisions) {
    assert_eq!(world.slack_webhook.runs.first(), Some(&expected.0));
}

#[then("the job makes no decision")]
fn the_job_decides_nothing(world: &mut ApiWorld) {
    assert_eq!(world.slack_webhook.runs.first(), Some(&Vec::<&str>::new()));
}

#[then(expr = "the second time, the job decides {decisions}")]
fn the_job_decides_the_second_time(world: &mut ApiWorld, expected: Decisions) {
    assert_eq!(world.slack_webhook.runs.get(1), Some(&expected.0));
}

#[then(expr = "the job fails, with an error mentioning {string}")]
fn the_job_fails(world: &mut ApiWorld, text: String) {
    let failure = world
        .slack_webhook
        .failure
        .as_ref()
        .expect("The job did not fail");
    assert!(failure.contains(&text), "The failure is {failure}");
}

#[then("nobody has any notification")]
fn nobody_has_notifications(world: &mut ApiWorld) {
    assert!(world.slack_webhook.listing.is_empty());
}

fn only_notification(world: &ApiWorld) -> &universal_inbox::notification::Notification {
    assert_eq!(
        world.slack_webhook.listing.len(),
        1,
        "{:?}",
        world.slack_webhook.listing
    );
    &world.slack_webhook.listing[0]
}

fn thread_of(world: &ApiWorld) -> &SlackThread {
    let ThirdPartyItemData::SlackThread(thread) = &only_notification(world).source_item.data else {
        panic!("Unexpected item data");
    };
    thread
}

#[then(
    expr = "that is the only notification: the Slack thread {string}, titled {string}, {status}"
)]
fn the_only_notification(
    world: &mut ApiWorld,
    source_id: String,
    title: String,
    status: UserStatus,
) {
    let notification = only_notification(world);
    assert_eq!(notification.source_item.source_id, source_id);
    assert_eq!(notification.title, title);
    assert_eq!(notification.status, status.0);
    assert_eq!(notification.kind, NotificationSourceKind::Slack);
}

#[then(expr = "that is the only notification: the Slack thread {string}, {status}")]
fn the_only_notification_untitled(world: &mut ApiWorld, source_id: String, status: UserStatus) {
    let notification = only_notification(world);
    assert_eq!(notification.source_item.source_id, source_id);
    assert_eq!(notification.status, status.0);
    assert_eq!(notification.kind, NotificationSourceKind::Slack);
}

#[then("that notification is unread, with no read date, no task and no snooze")]
fn that_notification_is_untouched(world: &mut ApiWorld) {
    let notification = only_notification(world);
    assert_eq!(notification.status, NotificationStatus::Unread);
    assert!(notification.last_read_at.is_none());
    assert!(notification.task_id.is_none());
    assert!(notification.snoozed_until.is_none());
}

#[then(expr = "that Slack thread is in the channel {string}")]
fn that_thread_is_in_the_channel(world: &mut ApiWorld, channel: String) {
    assert_eq!(thread_of(world).channel.id.to_string(), channel);
}

#[then(expr = "that Slack thread has the sender profiles of {string} and {string}")]
fn that_thread_has_the_profiles(world: &mut ApiWorld, first: String, second: String) {
    let thread = thread_of(world);
    assert!(thread.sender_profiles.contains_key(&first));
    assert!(thread.sender_profiles.contains_key(&second));
}

#[then(expr = "that Slack thread has the sender profile of {string}, but not the one of {string}")]
fn that_thread_has_one_profile(world: &mut ApiWorld, present: String, absent: String) {
    let thread = thread_of(world);
    assert!(thread.sender_profiles.contains_key(&present));
    assert!(!thread.sender_profiles.contains_key(&absent));
}

#[then(expr = "that Slack thread is {subscription}")]
fn that_thread_is(world: &mut ApiWorld, subscription: Subscription) {
    assert_eq!(thread_of(world).subscribed, subscription.is_subscribed());
}

#[then(expr = "that Slack thread names the emoji {string} as an alias of {string}")]
fn that_thread_names_an_emoji(world: &mut ApiWorld, emoji: String, alias: String) {
    let references = thread_of(world).references.as_ref().expect("No references");
    assert_eq!(
        references.emojis.get(&SlackEmojiName(emoji)),
        Some(&Some(SlackEmojiRef::Alias(SlackEmojiName(alias))))
    );
}

#[then(expr = "that Slack thread only knows the team {string}")]
fn that_thread_knows_only_the_team(world: &mut ApiWorld, team_id: String) {
    assert_eq!(
        thread_of(world).team,
        SlackTeamInfo::new(SlackTeamId(team_id))
    );
}

#[then(expr = "that Slack thread names the usergroup {string} {string}")]
fn that_thread_names_a_usergroup(world: &mut ApiWorld, id: String, name: String) {
    let references = thread_of(world)
        .references
        .as_ref()
        .expect("Slack references must be resolved");
    assert_eq!(
        references.usergroups.get(&SlackUserGroupId(id)),
        Some(&Some(name))
    );
}

#[then(expr = "that Slack thread could not name the usergroup {string}")]
fn that_thread_cannot_name_a_usergroup(world: &mut ApiWorld, id: String) {
    let references = thread_of(world)
        .references
        .as_ref()
        .expect("Slack references must be resolved");
    assert_eq!(
        references.usergroups.get(&SlackUserGroupId(id)),
        Some(&None)
    );
}

#[then(expr = "that Slack thread highlights only the usergroup {string}")]
fn that_thread_highlights_a_usergroup(world: &mut ApiWorld, id: String) {
    let references = thread_of(world).references.as_ref().expect("No references");
    assert_eq!(
        references.usergroup_ids_to_highlight,
        Some(vec![SlackUserGroupId(id)])
    );
}

#[then("the last message of that Slack thread keeps its plan block and its unknown block")]
fn that_thread_keeps_its_blocks(world: &mut ApiWorld) {
    let blocks = thread_of(world)
        .messages
        .last()
        .content
        .blocks
        .as_ref()
        .unwrap();
    assert_eq!(blocks.len(), 3);
    assert!(matches!(&blocks[1], SlackBlock::Plan(plan) if plan.title == "Release plan"));
    assert!(
        matches!(&blocks[2], SlackBlock::Unknown(value) if value["type"] == "some_future_block")
    );
}

#[then("Slack does not know either of them, both times")]
fn slack_does_not_know_either(world: &mut ApiWorld) {
    assert_eq!(world.slack_webhook.unresolved, vec![true; 4]);
}
