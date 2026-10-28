use anyhow::Context;
use base64::prelude::*;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey};
use ring::signature::KeyPair;
use ring::{rand::SystemRandom, signature::Ed25519KeyPair};
use serde::{Deserialize, Serialize};

use universal_inbox::auth::oauth2::{OAUTH2_SCOPE_READ, OAUTH2_SCOPE_WRITE};

use crate::universal_inbox::UniversalInboxError;

pub const JWT_SIGNING_ALGO: Algorithm = Algorithm::EdDSA;
pub const JWT_SESSION_KEY: &str = "jwt-session";

pub struct JWTSigningKeys {
    pub encoding_key: EncodingKey,
    pub decoding_key: DecodingKey,
}

pub struct JWTBase64EncodedSigningKeys {
    pub secret_key: String,
    pub public_key: String,
}

impl JWTBase64EncodedSigningKeys {
    pub fn generate() -> Result<Self, UniversalInboxError> {
        let doc = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
            .context("Failed to generate JWT keys")?;
        let keypair = Ed25519KeyPair::from_pkcs8(doc.as_ref())
            .context("Failed to generate JWT deriving keys")?;
        let secret_key = BASE64_STANDARD.encode(doc.as_ref());
        let public_key = BASE64_STANDARD.encode(keypair.public_key().as_ref());
        Ok(JWTBase64EncodedSigningKeys {
            secret_key,
            public_key,
        })
    }
}

impl JWTSigningKeys {
    pub fn load_from_base64_encoded_keys(
        base64_encoded_keys: JWTBase64EncodedSigningKeys,
    ) -> Result<Self, UniversalInboxError> {
        let encoding_key = EncodingKey::from_ed_der(
            BASE64_STANDARD
                .decode(base64_encoded_keys.secret_key)
                .context("Failed to decode JWT secret key")?
                .as_ref(),
        );
        let decoding_key = DecodingKey::from_ed_der(
            BASE64_STANDARD
                .decode(base64_encoded_keys.public_key)
                .context("Failed to decode JWT public key")?
                .as_ref(),
        );
        Ok(JWTSigningKeys {
            encoding_key,
            decoding_key,
        })
    }
}

/// What a JWT is for. Signed into the token so that the revocation policy is chosen
/// from the token itself and not from the transport (cookie or `Authorization` header)
/// it arrives on.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TokenPurpose {
    /// Web session, stored in the session cookie. Revoked by logout and password change.
    Session,
    /// API key, stored by digest. Revoked by deleting or revoking its row.
    Api,
    /// OAuth2 access token. Revoked by deleting the refresh tokens.
    OAuth,
}

/// JWT claims shared by session, API and OAuth2 tokens.
///
/// Tokens minted before the `purpose` claim existed carry none. They are classified by
/// [`Claims::effective_purpose`]: an audience means OAuth2, otherwise the purpose is
/// unknown (legacy API key or session token) and the transport decides which
/// revocation check applies. See the `jwt_auth` middleware for the resulting rules.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub exp: usize,
    pub iat: usize,
    pub sub: String,
    pub jti: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<TokenPurpose>,
    // OAuth2 fields (None for legacy API key tokens)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aud: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
}

impl Claims {
    /// The purpose of this token: the signed claim when present, `OAuth` for a legacy
    /// token with an audience, `None` for a legacy API key or session token.
    pub fn effective_purpose(&self) -> Option<TokenPurpose> {
        self.purpose
            .or_else(|| self.aud.as_ref().map(|_| TokenPurpose::OAuth))
    }

    /// Whether this token grants `required_scope`.
    ///
    /// - Tokens without an audience are first-party credentials (API keys
    ///   minted by the user for themselves, session JWTs): they carry the
    ///   user's full authority.
    /// - Audienced tokens are OAuth2 access tokens issued to a third-party
    ///   client: only the scopes the user consented to are granted. An absent
    ///   or empty `scope` claim is read-only. `write` implies `read`.
    pub fn grants_scope(&self, required_scope: &str) -> bool {
        if self.aud.is_none() {
            return true;
        }
        let mut granted: Vec<&str> = self
            .scope
            .as_deref()
            .unwrap_or("")
            .split_whitespace()
            .collect();
        if granted.is_empty() {
            // Grants recorded before scopes were enforced may carry no scope:
            // keep them readable, never writable.
            granted.push(OAUTH2_SCOPE_READ);
        }
        granted.contains(&required_scope)
            || (required_scope == OAUTH2_SCOPE_READ && granted.contains(&OAUTH2_SCOPE_WRITE))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::*;

    fn claims(aud: Option<&str>, scope: Option<&str>) -> Claims {
        Claims {
            exp: 0,
            iat: 0,
            sub: "user".to_string(),
            jti: "jti".to_string(),
            purpose: None,
            aud: aud.map(str::to_string),
            scope: scope.map(str::to_string),
            client_id: None,
        }
    }

    #[rstest]
    #[case::session(Some(TokenPurpose::Session), None, Some(TokenPurpose::Session))]
    #[case::api(Some(TokenPurpose::Api), None, Some(TokenPurpose::Api))]
    #[case::oauth(Some(TokenPurpose::OAuth), Some("mcp"), Some(TokenPurpose::OAuth))]
    #[case::claim_wins_over_audience(Some(TokenPurpose::Api), Some("mcp"), Some(TokenPurpose::Api))]
    #[case::legacy_oauth_by_audience(None, Some("mcp"), Some(TokenPurpose::OAuth))]
    #[case::legacy_api_or_session(None, None, None)]
    fn test_effective_purpose(
        #[case] purpose: Option<TokenPurpose>,
        #[case] aud: Option<&str>,
        #[case] expected: Option<TokenPurpose>,
    ) {
        let mut claims = claims(aud, None);
        claims.purpose = purpose;
        assert_eq!(claims.effective_purpose(), expected);
    }

    #[test]
    fn test_legacy_claims_without_purpose_deserialize() {
        let claims: Claims =
            serde_json::from_str(r#"{"exp":1,"iat":0,"sub":"u","jti":"j"}"#).unwrap();
        assert_eq!(claims.purpose, None);
    }

    #[rstest]
    #[case::first_party_read(None, None, "read", true)]
    #[case::first_party_write(None, None, "write", true)]
    #[case::oauth2_read_only_reads(Some("mcp"), Some("read"), "read", true)]
    #[case::oauth2_read_only_cannot_write(Some("mcp"), Some("read"), "write", false)]
    #[case::oauth2_read_write_writes(Some("mcp"), Some("read write"), "write", true)]
    #[case::oauth2_write_implies_read(Some("mcp"), Some("write"), "read", true)]
    #[case::oauth2_empty_scope_is_read_only(Some("mcp"), Some(""), "read", true)]
    #[case::oauth2_empty_scope_cannot_write(Some("mcp"), Some(""), "write", false)]
    #[case::oauth2_absent_scope_cannot_write(Some("mcp"), None, "write", false)]
    #[case::oauth2_unknown_scope(Some("mcp"), Some("admin"), "write", false)]
    fn test_grants_scope(
        #[case] aud: Option<&str>,
        #[case] scope: Option<&str>,
        #[case] required: &str,
        #[case] expected: bool,
    ) {
        assert_eq!(claims(aud, scope).grants_scope(required), expected);
    }
}
