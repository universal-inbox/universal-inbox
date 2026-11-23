use cucumber::{given, then, when};
use pretty_assertions::assert_eq;
use redis::AsyncCommands;
use rmcp::{
    model::{ClientCapabilities, Implementation, InitializeRequestParams},
    transport::streamable_http_server::session::{SessionState, SessionStore},
};
use uuid::Uuid;

use universal_inbox_api::mcp::RedisSessionStore;

use crate::{helpers::TestedApp, world::ApiWorld};

fn fresh_session_id() -> String {
    format!("test-session-{}", Uuid::new_v4())
}

fn session_key(id: &str) -> String {
    format!("universal-inbox:mcp:session:{id}")
}

fn store(app: &TestedApp, ttl_seconds: u64) -> RedisSessionStore {
    RedisSessionStore::new(app.cache.connection_manager.clone(), ttl_seconds)
}

#[given(expr = "the MCP session store keeps a session for {int} seconds")]
fn the_store_keeps_a_session_for(world: &mut ApiWorld, seconds: u64) {
    world.mcp_sessions.ttl_seconds = seconds;
}

#[when(expr = "a session for the client {string} version {string} is stored")]
async fn a_session_is_stored(world: &mut ApiWorld, name: String, version: String) {
    let ttl = world.mcp_sessions.ttl_seconds;
    let app = world.app.get().await;
    let client = Implementation::new(name, version);
    let state = SessionState::new(InitializeRequestParams::new(
        ClientCapabilities::default(),
        client.clone(),
    ));
    let id = fresh_session_id();
    store(app, ttl)
        .store(&id, &state)
        .await
        .expect("store failed");
    world.mcp_sessions.id = Some(id);
    world.mcp_sessions.stored_client = Some(client);
}

#[when("that session is deleted")]
async fn that_session_is_deleted(world: &mut ApiWorld) {
    let ttl = world.mcp_sessions.ttl_seconds;
    let id = world
        .mcp_sessions
        .id
        .clone()
        .expect("No session was stored");
    let app = world.app.get().await;
    store(app, ttl).delete(&id).await.expect("delete failed");
}

async fn load(world: &mut ApiWorld, id: &str) {
    let ttl = world.mcp_sessions.ttl_seconds;
    let app = world.app.get().await;
    world.mcp_sessions.loaded = Some(store(app, ttl).load(id).await.expect("load failed"));
}

#[when("that session is loaded")]
async fn that_session_is_loaded(world: &mut ApiWorld) {
    let id = world
        .mcp_sessions
        .id
        .clone()
        .expect("No session was stored");
    load(world, &id).await;
}

#[when("a session that nobody stored is loaded")]
async fn an_unknown_session_is_loaded(world: &mut ApiWorld) {
    load(world, &fresh_session_id()).await;
}

#[then(expr = "the loaded session is for the client {string} version {string}")]
fn the_loaded_session_is_for_the_client(world: &mut ApiWorld, name: String, version: String) {
    let loaded = world
        .mcp_sessions
        .loaded
        .as_ref()
        .unwrap()
        .as_ref()
        .expect("expected Some(SessionState)");
    let stored = world.mcp_sessions.stored_client.as_ref().unwrap();
    assert_eq!(loaded.initialize_params.client_info.name, stored.name);
    assert_eq!(loaded.initialize_params.client_info.version, stored.version);
    assert_eq!(loaded.initialize_params.client_info.name, name);
    assert_eq!(loaded.initialize_params.client_info.version, version);
}

#[then("no session is found")]
fn no_session_is_found(world: &mut ApiWorld) {
    assert!(world.mcp_sessions.loaded.as_ref().unwrap().is_none());
}

#[then(expr = "the stored session expires within the {int} seconds it is kept")]
async fn the_stored_session_expires_within(world: &mut ApiWorld, seconds: i64) {
    let key = session_key(
        world
            .mcp_sessions
            .id
            .as_ref()
            .expect("No session was stored"),
    );
    let app = world.app.get().await;
    let mut conn = app.cache.connection_manager.clone();
    let ttl: i64 = conn.ttl(&key).await.expect("TTL query failed");
    assert!(
        ttl > 0 && ttl <= seconds,
        "expected TTL within (0, {seconds}], got {ttl}"
    );
    let _: () = conn.del(&key).await.unwrap_or(());
}
