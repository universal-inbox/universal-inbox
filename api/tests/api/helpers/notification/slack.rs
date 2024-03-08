use httpmock::{
    Method::{GET, POST},
    Mock, MockServer,
};
use rstest::*;
use serde_json::json;
use slack_morphism::prelude::{
    SlackApiConversationsHistoryResponse, SlackApiConversationsInfoResponse,
    SlackApiTeamInfoResponse, SlackApiUsersInfoResponse, SlackPushEvent,
};

use universal_inbox::notification::{
    integrations::slack::{SlackMessageDetails, SlackMessageSenderDetails},
    NotificationDetails,
};

use crate::helpers::{fixture_path, load_json_fixture_file};

#[fixture]
pub fn slack_push_star_added_event() -> Box<SlackPushEvent> {
    load_json_fixture_file("slack_push_star_added_event.json")
}

#[fixture]
pub fn slack_push_star_removed_event() -> Box<SlackPushEvent> {
    load_json_fixture_file("slack_push_star_removed_event.json")
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

pub fn mock_slack_fetch_message<'a>(
    slack_mock_server: &'a MockServer,
    channel_id: &'a str,
    message_id: &'a str,
    fixture_response_file: &'a str,
) -> Mock<'a> {
    slack_mock_server.mock(|when, then| {
        when.method(GET)
            .path("/conversations.history")
            .header("authorization", "Bearer slack_test_user_access_token")
            .query_param("channel", channel_id)
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
