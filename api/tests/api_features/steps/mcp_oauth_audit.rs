use cucumber::{then, when};
use pretty_assertions::assert_eq;
use reqwest::StatusCode;
use secrecy::ExposeSecret;
use serde_json::json;

use crate::{
    steps::{mcp_client, mcp_protocol::body},
    world::ApiWorld,
};

fn notification_id(world: &ApiWorld) -> uuid::Uuid {
    world
        .integrations
        .github_notification
        .as_ref()
        .expect("The account has no notification")
        .id
        .0
}

#[when(expr = "I snooze my GitHub notification until {string} through MCP, with that OAuth2 token")]
async fn i_snooze_through_mcp(world: &mut ApiWorld, until: String) {
    let id = notification_id(world);
    let token = world.mcp_oauth.token("access_token").to_string();
    let app = world.app.get().await;
    world.mcp.body = Some(
        mcp_client::tool_call(
            app,
            &token,
            "act_on_notification",
            json!({ "notification_id": id, "action": "snooze_until", "snoozed_until": until }),
        )
        .await,
    );
    world.mcp.status = Some(StatusCode::OK);
}

#[when(
    "I search the audit log through MCP, with that OAuth2 token, for the changes that MCP made to my GitHub notification"
)]
async fn i_search_the_audit_log(world: &mut ApiWorld) {
    let id = notification_id(world);
    let token = world.mcp_oauth.token("access_token").to_string();
    let app = world.app.get().await;
    world.mcp.body = Some(
        mcp_client::tool_call(
            app,
            &token,
            "search_audit_log",
            json!({ "entity_id": id, "actor_kinds": ["Mcp"] }),
        )
        .await,
    );
    world.mcp.status = Some(StatusCode::OK);
}

#[when(
    "the other user searches the audit log through MCP for the changes to my GitHub notification"
)]
async fn the_other_user_searches_the_audit_log(world: &mut ApiWorld) {
    let id = notification_id(world);
    let token = world
        .mcp
        .other_api_token
        .as_ref()
        .expect("The other user has no API token")
        .jwt_token
        .expose_secret()
        .0
        .clone();
    let app = world.app.get().await;
    world.mcp.body = Some(
        mcp_client::tool_call(app, &token, "search_audit_log", json!({ "entity_id": id })).await,
    );
    world.mcp.status = Some(StatusCode::OK);
}

#[then(expr = "the audit log has {int} change(s)")]
fn the_audit_log_has_changes(world: &mut ApiWorld, total: u64) {
    assert_eq!(body(world)["result"]["isError"], false, "{}", body(world));
    assert_eq!(
        body(world)["result"]["structuredContent"]["total"],
        total,
        "{}",
        body(world)
    );
}

#[then(expr = "that change was made by the MCP client named {string}")]
fn that_change_was_made_by_the_client(world: &mut ApiWorld, name: String) {
    let event = &body(world)["result"]["structuredContent"]["events"][0];
    assert_eq!(event["actor"]["kind"], "Mcp");
    assert!(
        event["actor"]["client_id"]
            .as_str()
            .is_some_and(|client_id| !client_id.is_empty()),
        "{event}"
    );
    assert_eq!(event["actor_name"], name);
}

#[then(expr = "that change moved the snooze date to {string}")]
fn that_change_moved_the_snooze_date(world: &mut ApiWorld, date: String) {
    let event = &body(world)["result"]["structuredContent"]["events"][0];
    assert_eq!(event["changes"]["snoozed_until"]["to"], date);
}
