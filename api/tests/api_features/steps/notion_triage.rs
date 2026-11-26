use std::collections::BTreeSet;

use cucumber::{given, then};
use pretty_assertions::assert_eq;
use uuid::Uuid;

use universal_inbox::integration_connection::integrations::notion::NotionConfig;

use crate::{
    common::settings,
    helpers::{
        auth::authenticate_user,
        notion::{ME, MY_TOKEN, create_notion_integration_connection, mock_notion_not_accessible},
    },
    world::ApiWorld,
};

fn colleague_token(index: usize) -> String {
    format!("notion-token-colleague-{index}")
}

#[given(
    expr = "I and {int} other users are connected to the same Notion workspace, each with a token of their own, and Notion shares the page with none of us"
)]
async fn we_are_connected_and_see_nothing(world: &mut ApiWorld, others: usize) {
    let user_id = world.user().id;
    let workspace_id = Uuid::new_v4().to_string();
    let app = world.app.get().await;
    let connection = create_notion_integration_connection(
        app,
        user_id,
        &settings(),
        ME,
        MY_TOKEN,
        &workspace_id,
        NotionConfig::enabled(),
    )
    .await;
    mock_notion_not_accessible(&app.notion_mock_server, MY_TOKEN).await;
    for index in 1..=others {
        let (_, colleague) = authenticate_user(
            app,
            &format!("notion-colleague-{index}"),
            "Colleague",
            &index.to_string(),
            &format!("colleague-{index}@example.com"),
        )
        .await;
        let token = colleague_token(index);
        create_notion_integration_connection(
            app,
            colleague.id,
            &settings(),
            &format!("notion-user-colleague-{index}"),
            &token,
            &workspace_id,
            NotionConfig::enabled(),
        )
        .await;
        mock_notion_not_accessible(&app.notion_mock_server, &token).await;
    }
    world.notion_webhook.workspace_id = Some(workspace_id);
    world.notion_webhook.connection = Some(connection);
    world.notion_triage_connections = others + 1;
}

#[then(
    expr = "Notion was asked with {int} of our tokens, each a different one, and with no other token"
)]
async fn notion_was_asked_with_some_tokens(world: &mut ApiWorld, asked: usize) {
    let connections = world.notion_triage_connections;
    let ours: BTreeSet<String> = std::iter::once(MY_TOKEN.to_string())
        .chain((1..connections).map(colleague_token))
        .map(|token| format!("Bearer {token}"))
        .collect();
    let app = world.app.get().await;
    let used: BTreeSet<String> = app
        .notion_mock_server
        .received_requests()
        .await
        .expect("The Notion mock records its requests")
        .iter()
        .map(|request| {
            request
                .headers
                .get("authorization")
                .expect("A Notion request carries a token")
                .to_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert!(used.is_subset(&ours), "{used:?} is not within {ours:?}");
    assert_eq!(used.len(), asked);
}
