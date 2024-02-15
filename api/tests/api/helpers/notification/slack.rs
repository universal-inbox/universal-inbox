use rstest::*;
use slack_morphism::prelude::SlackPushEvent;

use crate::helpers::load_json_fixture_file;

#[fixture]
pub fn slack_push_star_added_event() -> Box<SlackPushEvent> {
    load_json_fixture_file("/tests/api/fixtures/slack_push_star_added_event.json")
}

#[fixture]
pub fn slack_push_star_removed_event() -> Box<SlackPushEvent> {
    load_json_fixture_file("/tests/api/fixtures/slack_push_star_removed_event.json")
}
