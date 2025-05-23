use std::collections::HashMap;

use chrono::Utc;
use httpmock::{
    Method::{GET, POST},
    Mock, MockServer,
};
use rstest::*;
use serde_json::{json, Value};
use slack_blocks_render::SlackReferences;
use slack_morphism::prelude::*;

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
    fixture_path, load_json_fixture_file, notification::create_notification_from_source_item,
    TestedApp,
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

pub fn mock_slack_fetch_user<'a>(
    slack_mock_server: &'a MockServer,
    user_id: &'a str,
    fixture_response_file: &'a str,
) -> Mock<'a> {
    slack_mock_server.mock(|when, then| {
        when.method(GET)
            .path("/users.info")
            .query_param("user", user_id);
        then.status(200)
            .header("content-type", "application/json")
            .body_from_file(fixture_path(fixture_response_file));
    })
}

pub fn mock_slack_fetch_bot<'a>(
    slack_mock_server: &'a MockServer,
    bot_id: &'a str,
    fixture_response_file: &'a str,
) -> Mock<'a> {
    slack_mock_server.mock(|when, then| {
        when.method(GET)
            .path("/bots.info")
            .query_param("bot", bot_id);
        then.status(200)
            .header("content-type", "application/json")
            .body_from_file(fixture_path(fixture_response_file));
    })
}

pub fn mock_slack_fetch_reply<'a>(
    slack_mock_server: &'a MockServer,
    channel_id: &'a str,
    message_id: &'a str,
    fixture_response_file: &'a str,
) -> Mock<'a> {
    slack_mock_server.mock(|when, then| {
        when.method(GET)
            .path("/conversations.replies")
            .query_param("channel", channel_id)
            .query_param("ts", message_id)
            .query_param("latest", message_id)
            .query_param("limit", "1")
            .query_param("inclusive", "true");
        then.status(200)
            .header("content-type", "application/json")
            .body_from_file(fixture_path(fixture_response_file));
    })
}

#[allow(clippy::too_many_arguments)]
pub fn mock_slack_fetch_thread<'a>(
    slack_mock_server: &'a MockServer,
    channel_id: &'a str,
    first_message_id: &'a str,
    message_id: &'a str,
    fixture_response_file: &'a str,
    subscribed: bool,
    last_read_message_index: Option<usize>,
    access_token: &'a str,
) -> Mock<'a> {
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

    slack_mock_server.mock(|when, then| {
        when.method(GET)
            .header("authorization", format!("Bearer {access_token}"))
            .path("/conversations.replies")
            .query_param("channel", channel_id)
            .query_param("ts", first_message_id)
            .query_param("latest", message_id)
            .query_param("inclusive", "true");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(json_body);
    })
}
pub fn mock_slack_fetch_channel<'a>(
    slack_mock_server: &'a MockServer,
    channel_id: &'a str,
    fixture_response_file: &'a str,
) -> Mock<'a> {
    slack_mock_server.mock(|when, then| {
        when.method(GET)
            .path("/conversations.info")
            .query_param("channel", channel_id);
        then.status(200)
            .header("content-type", "application/json")
            .body_from_file(fixture_path(fixture_response_file));
    })
}

pub fn mock_slack_fetch_team<'a>(
    slack_mock_server: &'a MockServer,
    team_id: &'a str,
    fixture_response_file: &'a str,
) -> Mock<'a> {
    slack_mock_server.mock(|when, then| {
        when.method(GET)
            .path("/team.info")
            .query_param("team", team_id);
        then.status(200)
            .header("content-type", "application/json")
            .body_from_file(fixture_path(fixture_response_file));
    })
}

pub fn mock_slack_list_usergroups<'a>(
    slack_mock_server: &'a MockServer,
    fixture_response_file: &'a str,
) -> Mock<'a> {
    slack_mock_server.mock(|when, then| {
        when.method(GET).path("/usergroups.list");
        then.status(200)
            .header("content-type", "application/json")
            .body_from_file(fixture_path(fixture_response_file));
    })
}

pub fn mock_slack_list_users_in_usergroup<'a>(
    slack_mock_server: &'a MockServer,
    usergroup_id: &'a str,
    fixture_response_file: &'a str,
) -> Mock<'a> {
    slack_mock_server.mock(|when, then| {
        when.method(GET)
            .path("/usergroups.users.list")
            .query_param("usergroup", usergroup_id);
        then.status(200)
            .header("content-type", "application/json")
            .body_from_file(fixture_path(fixture_response_file));
    })
}

pub fn mock_slack_stars_add<'a>(
    slack_mock_server: &'a MockServer,
    channel_id: &'a str,
    message_id: &'a str,
) -> Mock<'a> {
    slack_mock_server.mock(|when, then| {
        when.method(POST)
            .path("/stars.add")
            .header("authorization", "Bearer slack_test_user_access_token")
            .json_body(json!({"channel": channel_id, "timestamp": message_id}));
        then.status(200)
            .header("content-type", "application/json")
            .json_body(json!({ "ok": true }));
    })
}

pub fn mock_slack_stars_remove<'a>(
    slack_mock_server: &'a MockServer,
    channel_id: &'a str,
    message_id: &'a str,
) -> Mock<'a> {
    slack_mock_server.mock(|when, then| {
        when.method(POST)
            .path("/stars.remove")
            .header("authorization", "Bearer slack_test_user_access_token")
            .json_body(json!({"channel": channel_id, "timestamp": message_id}));
        then.status(200)
            .header("content-type", "application/json")
            .json_body(json!({ "ok": true }));
    })
}

pub fn mock_slack_get_chat_permalink<'a>(
    slack_mock_server: &'a MockServer,
    channel_id: &'a str,
    message_id: &'a str,
    fixture_response_file: &'a str,
) -> Mock<'a> {
    slack_mock_server.mock(|when, then| {
        when.method(GET)
            .path("/chat.getPermalink")
            .query_param("channel", channel_id)
            .query_param("message_ts", message_id);
        then.status(200)
            .header("content-type", "application/json")
            .body_from_file(fixture_path(fixture_response_file));
    })
}

pub fn mock_slack_list_emojis<'a>(
    slack_mock_server: &'a MockServer,
    fixture_response_file: &'a str,
) -> Mock<'a> {
    slack_mock_server.mock(|when, then| {
        when.method(GET).path("/emoji.list");
        then.status(200)
            .header("content-type", "application/json")
            .body_from_file(fixture_path(fixture_response_file));
    })
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
