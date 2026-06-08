use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::user::UserId;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OAuth2Client {
    pub id: Uuid,
    pub client_id: String,
    pub client_name: Option<String>,
    pub redirect_uris: Vec<String>,
    pub grant_types: Vec<String>,
    pub response_types: Vec<String>,
    pub token_endpoint_auth_method: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct OAuth2AuthorizationCode {
    pub code: String,
    pub client_id: String,
    pub user_id: UserId,
    pub redirect_uri: String,
    pub scope: Option<String>,
    pub code_challenge: String,
    pub code_challenge_method: String,
    pub resource: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct OAuth2RefreshToken {
    pub id: Uuid,
    pub token_hash: String,
    pub client_id: String,
    pub user_id: UserId,
    pub scope: Option<String>,
    pub resource: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub refresh_token: String,
    pub scope: String,
}

/// Summary of an authorized OAuth2 client for display in the user profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizedOAuth2Client {
    pub client_id: String,
    pub client_name: Option<String>,
    pub scope: Option<String>,
    pub first_authorized_at: DateTime<Utc>,
    pub last_used_at: DateTime<Utc>,
    /// True when this client holds an active grant whose *effective* resource
    /// targets the MCP server — i.e. an explicit RFC 8707 resource indicator
    /// ending in `/mcp`, or no indicator at all (clients such as Claude Code
    /// don't send one, and the token endpoint then defaults the audience to
    /// the MCP `resource_url`). Drives membership in the "AI Agents (MCP)"
    /// settings card. Computed server-side so the frontend stays free of
    /// base-URL knowledge. Clients that request a different resource are excluded.
    #[serde(default)]
    pub is_mcp: bool,
    /// `software_id` from the client's CIMD metadata document, when available
    /// (NULL for clients registered via plain RFC 7591 DCR). Used together with
    /// `client_name` to pick a per-agent icon.
    #[serde(default)]
    pub software_id: Option<String>,
}

/// Recorded user consent for an OAuth2 client (granted scope is the union of
/// scopes the user has approved for that client).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuth2UserConsent {
    pub user_id: UserId,
    pub client_id: String,
    pub scope: String,
    pub granted_at: DateTime<Utc>,
}

/// Payload returned by `GET /oauth2/authorize/consent` so the frontend can
/// render the consent screen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2ConsentRequest {
    pub request_id: String,
    pub csrf_token: String,
    pub client_id: String,
    pub client_name: Option<String>,
    pub redirect_uri: String,
    pub scope: Option<String>,
}

/// Body of `POST /oauth2/authorize/consent`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2ConsentSubmission {
    pub request_id: String,
    pub csrf_token: String,
    pub decision: OAuth2ConsentDecision,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OAuth2ConsentDecision {
    Allow,
    Deny,
}

/// Response from `POST /oauth2/authorize/consent` — the frontend uses
/// `redirect_url` to navigate the browser back to the third-party client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2ConsentResponse {
    pub redirect_url: String,
}
