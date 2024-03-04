use httpmock::{Method::GET, Mock, MockServer};
use rstest::*;
use slack_morphism::prelude::SlackPushEvent;

use crate::helpers::{fixture_path, load_json_fixture_file};

#[fixture]
pub fn slack_push_star_added_event() -> Box<SlackPushEvent> {
    load_json_fixture_file("slack_push_star_added_event.json")
}

#[fixture]
pub fn slack_push_star_removed_event() -> Box<SlackPushEvent> {
    load_json_fixture_file("slack_push_star_removed_event.json")
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
