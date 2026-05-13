//! Redis-backed [`SessionStore`] for cross-pod MCP session restore.
//!
//! When the API runs as multiple replicas behind a load balancer, each pod
//! holds its own [`LocalSessionManager`] and so does not know about sessions
//! that initialised on a different pod. This store persists each session's
//! `initialize` parameters (the [`SessionState`]) to shared Redis so the
//! upstream rmcp transport can transparently replay the handshake on whichever
//! pod a follow-up request lands on.
//!
//! [`LocalSessionManager`]: rmcp::transport::streamable_http_server::session::local::LocalSessionManager

use async_trait::async_trait;
use redis::{AsyncCommands, aio::ConnectionManager};
use rmcp::transport::streamable_http_server::session::{
    SessionState, SessionStore, SessionStoreError,
};

const NAMESPACE: &str = "universal-inbox:mcp:session:";

#[derive(Clone)]
pub struct RedisSessionStore {
    conn: ConnectionManager,
    ttl_seconds: u64,
}

impl RedisSessionStore {
    pub fn new(conn: ConnectionManager, ttl_seconds: u64) -> Self {
        Self { conn, ttl_seconds }
    }

    fn key(id: &str) -> String {
        format!("{NAMESPACE}{id}")
    }
}

#[async_trait]
impl SessionStore for RedisSessionStore {
    async fn load(&self, session_id: &str) -> Result<Option<SessionState>, SessionStoreError> {
        let mut conn = self.conn.clone();
        let raw: Option<String> = conn
            .get(Self::key(session_id))
            .await
            .map_err(|e| Box::new(e) as SessionStoreError)?;
        match raw {
            None => Ok(None),
            Some(s) => serde_json::from_str::<SessionState>(&s)
                .map(Some)
                .map_err(|e| Box::new(e) as SessionStoreError),
        }
    }

    async fn store(&self, session_id: &str, state: &SessionState) -> Result<(), SessionStoreError> {
        let mut conn = self.conn.clone();
        let payload = serde_json::to_string(state).map_err(|e| Box::new(e) as SessionStoreError)?;
        conn.set_ex::<_, _, ()>(Self::key(session_id), payload, self.ttl_seconds)
            .await
            .map_err(|e| Box::new(e) as SessionStoreError)?;
        Ok(())
    }

    async fn delete(&self, session_id: &str) -> Result<(), SessionStoreError> {
        let mut conn = self.conn.clone();
        let _: () = conn
            .del(Self::key(session_id))
            .await
            .map_err(|e| Box::new(e) as SessionStoreError)?;
        Ok(())
    }
}
