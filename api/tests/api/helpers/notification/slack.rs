use std::collections::HashMap;

use httpmock::{
    Method::{GET, POST},
    Mock, MockServer,
};
use rstest::*;
use serde_json::json;
use slack_blocks_render::SlackReferences;
use slack_morphism::prelude::*;

use universal_inbox::{
    notification::NotificationDetails,
    third_party::integrations::slack::SlackStarItem,
    third_party::integrations::slack::{SlackMessageDetails, SlackMessageSenderDetails},
};

use crate::helpers::{fixture_path, load_json_fixture_file};

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
pub fn slack_notification_details() -> Box<NotificationDetails> {
    let message_response: SlackApiConversationsHistoryResponse =
        load_json_fixture_file("slack_fetch_message_response.json");
    let channel_response: SlackApiConversationsInfoResponse =
        load_json_fixture_file("slack_fetch_channel_response.json");
    let user_response: SlackApiUsersInfoResponse =
        load_json_fixture_file("slack_fetch_user_response.json");
    let sender = SlackMessageSenderDetails::User(Box::new(user_response.user));
    let team_response: SlackApiTeamInfoResponse =
        load_json_fixture_file("slack_fetch_team_response.json");

    Box::new(NotificationDetails::SlackMessage(SlackMessageDetails {
        url: "https://example.com".parse().unwrap(),
        message: message_response.messages[0].clone(),
        channel: channel_response.channel,
        sender,
        team: team_response.team,
        references: None,
    }))
}

pub fn mock_slack_fetch_user<'a>(
    slack_mock_server: &'a MockServer,
    user_id: &'a str,
    fixture_response_file: &'a str,
) -> Mock<'a> {
    slack_mock_server.mock(|when, then| {
        when.method(GET)
            .path("/users.info")
            .header("authorization", "Bearer slack_test_user_access_token")
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
            .header("authorization", "Bearer slack_test_user_access_token")
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
            .header("authorization", "Bearer slack_test_user_access_token")
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

pub fn mock_slack_fetch_channel<'a>(
    slack_mock_server: &'a MockServer,
    channel_id: &'a str,
    fixture_response_file: &'a str,
) -> Mock<'a> {
    slack_mock_server.mock(|when, then| {
        when.method(GET)
            .path("/conversations.info")
            .header("authorization", "Bearer slack_test_user_access_token")
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
            .header("authorization", "Bearer slack_test_user_access_token")
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
        when.method(GET)
            .path("/usergroups.list")
            .header("authorization", "Bearer slack_test_user_access_token");
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
            .header("authorization", "Bearer slack_test_user_access_token")
            .query_param("channel", channel_id)
            .query_param("message_ts", message_id);
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
    let sender = SlackMessageSenderDetails::User(Box::new(user_response.user));
    let team_response: SlackApiTeamInfoResponse =
        load_json_fixture_file("slack_fetch_team_response.json");

    Box::new(SlackStarItem::SlackMessage(SlackMessageDetails {
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
                Some("test".to_string()),
            )]),
            usergroups: HashMap::from([(
                SlackUserGroupId("S05ZZZ".to_string()),
                Some("admins".to_string()),
            )]),
        }),
    }))
}
