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
use redis::{AsyncCommands, RedisError, aio::ConnectionManager};
use rmcp::transport::streamable_http_server::session::{
    SessionState, SessionStore, SessionStoreError,
};

use universal_inbox::user::UserId;

const NAMESPACE: &str = "universal-inbox:mcp:session:";
const OWNER_NAMESPACE: &str = "universal-inbox:mcp:session-owner:";

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

/// Records which user owns each MCP session, in shared Redis so every pod
/// agrees.
///
/// The `Mcp-Session-Id` header is client-supplied and rmcp's streamable HTTP
/// transport, which restores unknown sessions from the shared session store,
/// would otherwise serve any session to any authenticated caller (replaying its
/// SSE events, closing it, or injecting responses into it). The MCP
/// middleware records the owner when `initialize` mints a session and
/// rejects every later request whose authenticated user is not that owner.
#[derive(Clone)]
pub struct McpSessionOwnerStore {
    conn: ConnectionManager,
    ttl_seconds: u64,
}

impl McpSessionOwnerStore {
    pub fn new(conn: ConnectionManager, ttl_seconds: u64) -> Self {
        Self { conn, ttl_seconds }
    }

    fn key(id: &str) -> String {
        format!("{OWNER_NAMESPACE}{id}")
    }

    pub async fn record_owner(&self, session_id: &str, user_id: UserId) -> Result<(), RedisError> {
        let mut conn = self.conn.clone();
        conn.set_ex::<_, _, ()>(Self::key(session_id), user_id.to_string(), self.ttl_seconds)
            .await
    }

    /// Whether `user_id` owns `session_id`. A session with no recorded
    /// owner (unknown, expired, or created before owners were recorded) is
    /// owned by nobody. On success the owner record's TTL is refreshed so an
    /// active session keeps its binding.
    pub async fn is_owned_by(&self, session_id: &str, user_id: UserId) -> Result<bool, RedisError> {
        let mut conn = self.conn.clone();
        let key = Self::key(session_id);
        let owner: Option<String> = conn.get(&key).await?;
        let is_owner = owner.is_some_and(|owner| owner == user_id.to_string());
        if is_owner {
            conn.expire::<_, ()>(&key, self.ttl_seconds as i64).await?;
        }
        Ok(is_owner)
    }
}
