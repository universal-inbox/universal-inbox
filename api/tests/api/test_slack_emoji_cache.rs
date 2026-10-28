use pretty_assertions::assert_eq;
use rstest::*;
use slack_morphism::prelude::*;
use uuid::Uuid;

use crate::helpers::{
    TestedApp, notification::slack::mock_slack_list_emojis_for_token, tested_app,
};

fn slack_token(value: &str) -> SlackApiToken {
    SlackApiToken::new(SlackApiTokenValue(value.to_string()))
}

fn emoji_names(emojis: &std::collections::HashMap<SlackEmojiName, SlackEmojiRef>) -> Vec<String> {
    let mut names: Vec<String> = emojis.keys().map(|name| name.0.clone()).collect();
    names.sort();
    names
}

#[rstest]
#[tokio::test]
async fn test_list_emojis_cache_is_not_shared_between_credentials_without_team_context(
    #[future] tested_app: TestedApp,
) {
    let app = tested_app.await;
    // Unique tokens so cached entries of other tests never match
    let token_a = format!("xoxp-workspace-a-{}", Uuid::new_v4());
    let token_b = format!("xoxp-workspace-b-{}", Uuid::new_v4());
    mock_slack_list_emojis_for_token(
        &app.slack_mock_server,
        &token_a,
        serde_json::json!({ "secret_a": "https://emoji.com/secret_a.png" }),
        1,
    )
    .await;
    mock_slack_list_emojis_for_token(
        &app.slack_mock_server,
        &token_b,
        serde_json::json!({ "secret_b": "https://emoji.com/secret_b.png" }),
        1,
    )
    .await;

    // Neither token carries a team id: both used to share the "no-team" cache key
    let emojis_a = app
        .slack_service
        .list_emojis(&slack_token(&token_a))
        .await
        .unwrap();
    let emojis_b = app
        .slack_service
        .list_emojis(&slack_token(&token_b))
        .await
        .unwrap();

    assert_eq!(emoji_names(&emojis_a), vec!["secret_a".to_string()]);
    assert_eq!(emoji_names(&emojis_b), vec!["secret_b".to_string()]);
}

#[rstest]
#[tokio::test]
async fn test_list_emojis_cache_hit_for_repeated_lookups_by_same_credential(
    #[future] tested_app: TestedApp,
) {
    let app = tested_app.await;
    let token = format!("xoxp-workspace-a-{}", Uuid::new_v4());
    // A single upstream call: the second lookup must be served from the cache
    mock_slack_list_emojis_for_token(
        &app.slack_mock_server,
        &token,
        serde_json::json!({ "party": "https://emoji.com/party.png" }),
        1,
    )
    .await;

    let first = app
        .slack_service
        .list_emojis(&slack_token(&token))
        .await
        .unwrap();
    let second = app
        .slack_service
        .list_emojis(&slack_token(&token))
        .await
        .unwrap();

    assert_eq!(emoji_names(&first), vec!["party".to_string()]);
    assert_eq!(first, second);
}
