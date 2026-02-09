use std::collections::HashMap;

use chrono::Utc;
use rstest::*;
use serde_json::{Value, json};
use slack_blocks_render::SlackReferences;
use slack_morphism::prelude::*;
use wiremock::matchers::{body_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use universal_inbox::{
    integration_connection::IntegrationConnectionId,
    notification::Notification,
    third_party::{
        integrations::slack::{
            SlackMessageDetails, SlackMessageSenderDetails, SlackStar, SlackStarItem,
            SlackStarState, SlackThread,
        },
        item::ThirdPartyItemData,
    },
    user::UserId,
};
use universal_inbox_api::integrations::slack::SlackService;

use crate::helpers::{
    TestedApp, fixture_path, load_json_fixture_file,
    notification::create_notification_from_source_item,
};

#[fixture]
pub fn slack_push_star_added_event() -> Box<SlackPushEvent> {
    load_json_fixture_file("slack_push_star_added_event.json")
}

#[fixture]
pub fn slack_push_reaction_added_event() -> Box<SlackPushEvent> {
    load_json_fixture_file("slack_push_reaction_added_event.json")
}

#[fixture]
pub fn slack_push_bot_star_added_event() -> Box<SlackPushEvent> {
    load_json_fixture_file("slack_push_bot_star_added_event.json")
}

#[fixture]
pub fn slack_push_star_removed_event() -> Box<SlackPushEvent> {
    load_json_fixture_file("slack_push_star_removed_event.json")
}

#[fixture]
pub fn slack_push_reaction_removed_event() -> Box<SlackPushEvent> {
    load_json_fixture_file("slack_push_reaction_removed_event.json")
}

#[fixture]
pub fn slack_push_message_event() -> Box<SlackPushEvent> {
    load_json_fixture_file("slack_push_message_event.json")
}

#[fixture]
pub fn slack_push_message_in_thread_event() -> Box<SlackPushEvent> {
    load_json_fixture_file("slack_push_message_in_thread_event.json")
}

#[fixture]
pub fn slack_star_added() -> Box<SlackStar> {
    let message_response: SlackApiConversationsHistoryResponse =
        load_json_fixture_file("slack_fetch_message_response.json");
    let channel_response: SlackApiConversationsInfoResponse =
        load_json_fixture_file("slack_fetch_channel_response.json");
    let user_response: SlackApiUsersInfoResponse =
        load_json_fixture_file("slack_fetch_user_response.json");
    let sender = SlackMessageSenderDetails::User(Box::new(user_response.user.profile.unwrap()));
    let team_response: SlackApiTeamInfoResponse =
        load_json_fixture_file("slack_fetch_team_response.json");

    Box::new(SlackStar {
        state: SlackStarState::StarAdded,
        created_at: Utc::now(),
        item: SlackStarItem::SlackMessage(Box::new(SlackMessageDetails {
            url: "https://example.com".parse().unwrap(),
            message: message_response.messages[0].clone(),
            channel: channel_response.channel,
            sender,
            team: team_response.team,
            references: None,
        })),
    })
}

pub async fn create_notification_from_slack_star(
    app: &TestedApp,
    slack_star: &SlackStar,
    user_id: UserId,
    slack_integration_connection_id: IntegrationConnectionId,
) -> Box<Notification> {
    create_notification_from_source_item::<SlackStar, SlackService>(
        app,
        slack_star.item.id(),
        ThirdPartyItemData::SlackStar(Box::new(slack_star.clone())),
        app.notification_service.read().await.slack_service.clone(),
        user_id,
        slack_integration_connection_id,
    )
    .await
}

#[fixture]
pub fn slack_thread() -> Box<SlackThread> {
    let message_response: SlackApiConversationsHistoryResponse =
        load_json_fixture_file("slack_fetch_thread_response.json");
    let channel_response: SlackApiConversationsInfoResponse =
        load_json_fixture_file("slack_fetch_channel_response.json");
    let team_response: SlackApiTeamInfoResponse =
        load_json_fixture_file("slack_fetch_team_response.json");

    Box::new(SlackThread {
        url: "https://example.com".parse().unwrap(),
        messages: message_response.messages.try_into().unwrap(),
        subscribed: true,
        last_read: None,
        channel: channel_response.channel.clone(),
        team: team_response.team.clone(),
        references: None,
        sender_profiles: Default::default(),
        user_slack_id: None,
    })
}

pub async fn create_notification_from_slack_thread(
    app: &TestedApp,
    slack_thread: &SlackThread,
    user_id: UserId,
    slack_integration_connection_id: IntegrationConnectionId,
) -> Box<Notification> {
    create_notification_from_source_item::<SlackThread, SlackService>(
        app,
        slack_thread.messages.first().origin.ts.to_string(),
        ThirdPartyItemData::SlackThread(Box::new(slack_thread.clone())),
        app.notification_service.read().await.slack_service.clone(),
        user_id,
        slack_integration_connection_id,
    )
    .await
}

pub async fn mock_slack_fetch_user(
    slack_mock_server: &MockServer,
    user_id: &str,
    fixture_response_file: &str,
) {
    Mock::given(method("GET"))
        .and(path("/users.info"))
        .and(query_param("user", user_id))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            std::fs::read_to_string(fixture_path(fixture_response_file)).unwrap(),
            "application/json",
        ))
        .mount(slack_mock_server)
        .await;
}

pub async fn mock_slack_fetch_bot(
    slack_mock_server: &MockServer,
    bot_id: &str,
    fixture_response_file: &str,
) {
    Mock::given(method("GET"))
        .and(path("/bots.info"))
        .and(query_param("bot", bot_id))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            std::fs::read_to_string(fixture_path(fixture_response_file)).unwrap(),
            "application/json",
        ))
        .mount(slack_mock_server)
        .await;
}

pub async fn mock_slack_fetch_reply(
    slack_mock_server: &MockServer,
    channel_id: &str,
    message_id: &str,
    fixture_response_file: &str,
) {
    Mock::given(method("GET"))
        .and(path("/conversations.replies"))
        .and(query_param("channel", channel_id))
        .and(query_param("ts", message_id))
        .and(query_param("latest", message_id))
        .and(query_param("limit", "1"))
        .and(query_param("inclusive", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            std::fs::read_to_string(fixture_path(fixture_response_file)).unwrap(),
            "application/json",
        ))
        .mount(slack_mock_server)
        .await;
}

#[allow(clippy::too_many_arguments)]
pub async fn mock_slack_fetch_thread(
    slack_mock_server: &MockServer,
    channel_id: &str,
    first_message_id: &str,
    message_id: &str,
    fixture_response_file: &str,
    subscribed: bool,
    last_read_message_index: Option<usize>,
    access_token: &str,
) {
    let mut json_body: Value = load_json_fixture_file(fixture_response_file);
    json_body["messages"][0]["subscribed"] = Value::Bool(subscribed);
    json_body["messages"][0]["last_read"] = match last_read_message_index {
        Some(index) => Value::String(
            json_body["messages"][index]["ts"]
                .as_str()
                .unwrap()
                .to_string(),
        ),
        None => Value::Null,
    };

    Mock::given(method("GET"))
        .and(header(
            "authorization",
            format!("Bearer {access_token}").as_str(),
        ))
        .and(path("/conversations.replies"))
        .and(query_param("channel", channel_id))
        .and(query_param("ts", first_message_id))
        .and(query_param("latest", message_id))
        .and(query_param("inclusive", "true"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(json_body),
        )
        .mount(slack_mock_server)
        .await;
}

pub async fn mock_slack_fetch_channel(
    slack_mock_server: &MockServer,
    channel_id: &str,
    fixture_response_file: &str,
) {
    Mock::given(method("GET"))
        .and(path("/conversations.info"))
        .and(query_param("channel", channel_id))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            std::fs::read_to_string(fixture_path(fixture_response_file)).unwrap(),
            "application/json",
        ))
        .mount(slack_mock_server)
        .await;
}

pub async fn mock_slack_fetch_team(
    slack_mock_server: &MockServer,
    team_id: &str,
    fixture_response_file: &str,
) {
    Mock::given(method("GET"))
        .and(path("/team.info"))
        .and(query_param("team", team_id))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            std::fs::read_to_string(fixture_path(fixture_response_file)).unwrap(),
            "application/json",
        ))
        .mount(slack_mock_server)
        .await;
}

pub async fn mock_slack_list_usergroups(
    slack_mock_server: &MockServer,
    fixture_response_file: &str,
) {
    Mock::given(method("GET"))
        .and(path("/usergroups.list"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            std::fs::read_to_string(fixture_path(fixture_response_file)).unwrap(),
            "application/json",
        ))
        .mount(slack_mock_server)
        .await;
}

pub async fn mock_slack_list_users_in_usergroup(
    slack_mock_server: &MockServer,
    usergroup_id: &str,
    fixture_response_file: &str,
) {
    Mock::given(method("GET"))
        .and(path("/usergroups.users.list"))
        .and(query_param("usergroup", usergroup_id))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            std::fs::read_to_string(fixture_path(fixture_response_file)).unwrap(),
            "application/json",
        ))
        .mount(slack_mock_server)
        .await;
}

pub async fn mock_slack_stars_add(
    slack_mock_server: &MockServer,
    channel_id: &str,
    message_id: &str,
) {
    Mock::given(method("POST"))
        .and(path("/stars.add"))
        .and(header(
            "authorization",
            "Bearer slack_test_user_access_token",
        ))
        .and(body_json(
            json!({"channel": channel_id, "timestamp": message_id}),
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(json!({ "ok": true })),
        )
        .mount(slack_mock_server)
        .await;
}

pub async fn mock_slack_stars_remove(
    slack_mock_server: &MockServer,
    channel_id: &str,
    message_id: &str,
) {
    Mock::given(method("POST"))
        .and(path("/stars.remove"))
        .and(header(
            "authorization",
            "Bearer slack_test_user_access_token",
        ))
        .and(body_json(
            json!({"channel": channel_id, "timestamp": message_id}),
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(json!({ "ok": true })),
        )
        .mount(slack_mock_server)
        .await;
}

pub async fn mock_slack_get_chat_permalink(
    slack_mock_server: &MockServer,
    channel_id: &str,
    message_id: &str,
    fixture_response_file: &str,
) {
    Mock::given(method("GET"))
        .and(path("/chat.getPermalink"))
        .and(query_param("channel", channel_id))
        .and(query_param("message_ts", message_id))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            std::fs::read_to_string(fixture_path(fixture_response_file)).unwrap(),
            "application/json",
        ))
        .mount(slack_mock_server)
        .await;
}

pub async fn mock_slack_list_emojis(slack_mock_server: &MockServer, fixture_response_file: &str) {
    Mock::given(method("GET"))
        .and(path("/emoji.list"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            std::fs::read_to_string(fixture_path(fixture_response_file)).unwrap(),
            "application/json",
        ))
        .mount(slack_mock_server)
        .await;
}

#[fixture]
pub fn slack_starred_message() -> Box<SlackStarItem> {
    let message_response: SlackApiConversationsHistoryResponse =
        load_json_fixture_file("slack_fetch_message_response.json");
    let channel_response: SlackApiConversationsInfoResponse =
        load_json_fixture_file("slack_fetch_channel_response.json");
    let user_response: SlackApiUsersInfoResponse =
        load_json_fixture_file("slack_fetch_user_response.json");
    let sender = SlackMessageSenderDetails::User(Box::new(user_response.user.profile.unwrap()));
    let team_response: SlackApiTeamInfoResponse =
        load_json_fixture_file("slack_fetch_team_response.json");

    Box::new(SlackStarItem::SlackMessage(Box::new(SlackMessageDetails {
        url: "https://example.com".parse().unwrap(),
        message: message_response.messages[0].clone(),
        channel: channel_response.channel,
        sender,
        team: team_response.team,
        references: Some(SlackReferences {
            users: HashMap::from([(
                SlackUserId("U05YYY".to_string()),
                Some("john.doe".to_string()),
            )]),
            channels: HashMap::from([(
                SlackChannelId("C05XXX".to_string()),
                Some("universal-inbox".to_string()),
            )]),
            usergroups: HashMap::from([(
                SlackUserGroupId("S05ZZZ".to_string()),
                Some("admins".to_string()),
            )]),
            emojis: HashMap::from([
                (
                    SlackEmojiName("unknown1".to_string()),
                    Some(SlackEmojiRef::Alias(SlackEmojiName("wave".to_string()))),
                ),
                (
                    SlackEmojiName("unknown2".to_string()),
                    Some(SlackEmojiRef::Url(
                        "https://emoji.com/unknown2.png".parse().unwrap(),
                    )),
                ),
            ]),
        }),
    })))
}
