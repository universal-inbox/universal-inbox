use redis::AsyncCommands;
use rmcp::{
    model::{ClientCapabilities, Implementation, InitializeRequestParams},
    transport::streamable_http_server::session::{SessionState, SessionStore},
};
use rstest::*;
use uuid::Uuid;

use universal_inbox_api::mcp::RedisSessionStore;

use crate::helpers::{TestedApp, tested_app};

fn fresh_session_id() -> String {
    format!("test-session-{}", Uuid::new_v4())
}

fn sample_state() -> SessionState {
    SessionState::new(InitializeRequestParams::new(
        ClientCapabilities::default(),
        Implementation::new("test-client", "1.0.0"),
    ))
}

#[rstest]
#[tokio::test]
async fn store_then_load_round_trips(#[future] tested_app: TestedApp) {
    let app = tested_app.await;
    let store = RedisSessionStore::new(app.cache.connection_manager.clone(), 60);
    let id = fresh_session_id();
    let state = sample_state();

    store.store(&id, &state).await.expect("store failed");

    let loaded = store.load(&id).await.expect("load failed");
    let loaded = loaded.expect("expected Some(SessionState)");
    assert_eq!(
        loaded.initialize_params.client_info.name,
        state.initialize_params.client_info.name
    );
    assert_eq!(
        loaded.initialize_params.client_info.version,
        state.initialize_params.client_info.version
    );
}

#[rstest]
#[tokio::test]
async fn load_returns_none_for_unknown_session(#[future] tested_app: TestedApp) {
    let app = tested_app.await;
    let store = RedisSessionStore::new(app.cache.connection_manager.clone(), 60);

    let loaded = store.load(&fresh_session_id()).await.expect("load failed");
    assert!(loaded.is_none());
}

#[rstest]
#[tokio::test]
async fn delete_removes_persisted_session(#[future] tested_app: TestedApp) {
    let app = tested_app.await;
    let store = RedisSessionStore::new(app.cache.connection_manager.clone(), 60);
    let id = fresh_session_id();

    store
        .store(&id, &sample_state())
        .await
        .expect("store failed");
    store.delete(&id).await.expect("delete failed");

    let loaded = store.load(&id).await.expect("load failed");
    assert!(loaded.is_none());
}

#[rstest]
#[tokio::test]
async fn store_applies_configured_ttl(#[future] tested_app: TestedApp) {
    let app = tested_app.await;
    let configured_ttl: u64 = 600;
    let store = RedisSessionStore::new(app.cache.connection_manager.clone(), configured_ttl);
    let id = fresh_session_id();
    let key = format!("universal-inbox:mcp:session:{id}");

    store
        .store(&id, &sample_state())
        .await
        .expect("store failed");

    let mut conn = app.cache.connection_manager.clone();
    let ttl: i64 = conn.ttl(&key).await.expect("TTL query failed");
    assert!(
        ttl > 0 && ttl <= configured_ttl as i64,
        "expected TTL within (0, {configured_ttl}], got {ttl}"
    );

    let _: () = conn.del(&key).await.unwrap_or(());
}
