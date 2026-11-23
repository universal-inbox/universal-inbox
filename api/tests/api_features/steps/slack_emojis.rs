use std::collections::HashMap;

use cucumber::{given, then, when};
use pretty_assertions::assert_eq;
use slack_morphism::prelude::*;
use uuid::Uuid;

use crate::{helpers::notification::slack::mock_slack_list_emojis_for_token, world::ApiWorld};

fn slack_token(value: &str) -> SlackApiToken {
    SlackApiToken::new(SlackApiTokenValue(value.to_string()))
}

fn emoji_names(emojis: &HashMap<SlackEmojiName, SlackEmojiRef>) -> Vec<String> {
    let mut names: Vec<String> = emojis.keys().map(|name| name.0.clone()).collect();
    names.sort();
    names
}

#[given(
    expr = "workspace {string} has a Slack token without a team id, and Slack knows the emoji {string} and is asked for it once"
)]
async fn a_workspace_has_a_token_and_an_emoji(
    world: &mut ApiWorld,
    workspace: String,
    emoji: String,
) {
    let token = format!("xoxp-workspace-{workspace}-{}", Uuid::new_v4());
    let app = world.app.get().await;
    mock_slack_list_emojis_for_token(
        &app.slack_mock_server,
        &token,
        serde_json::json!({ emoji.clone(): format!("https://emoji.com/{emoji}.png") }),
        1,
    )
    .await;
    world.slack_emojis.tokens.insert(workspace, token);
}

#[when(expr = "I list the emojis of workspace {string}")]
async fn i_list_the_emojis_of_a_workspace(world: &mut ApiWorld, workspace: String) {
    let token = world.slack_emojis.tokens[&workspace].clone();
    let app = world.app.get().await;
    let emojis = app
        .slack_service
        .list_emojis(&slack_token(&token))
        .await
        .unwrap();
    world.slack_emojis.listed.push((workspace, emojis));
}

#[then(expr = "the emojis listed for workspace {string} are only {string}")]
fn the_emojis_listed_are_only(world: &mut ApiWorld, workspace: String, emoji: String) {
    let (_, emojis) = world
        .slack_emojis
        .listed
        .iter()
        .find(|(name, _)| *name == workspace)
        .expect("The emojis of that workspace were not listed");
    assert_eq!(emoji_names(emojis), vec![emoji]);
}

#[then("both lists are the same")]
fn both_lists_are_the_same(world: &mut ApiWorld) {
    let listed = &world.slack_emojis.listed;
    assert_eq!(listed[0].1, listed[1].1);
}
