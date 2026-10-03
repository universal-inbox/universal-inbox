use std::fmt;

use ::openidconnect::AccessToken;
use serde::{Deserialize, Serialize};
use url::Url;

pub mod auth_token;
pub mod oauth2;
pub mod openidconnect;

// Simplify the ID token type to a string. This avoid to embed all the openidconnect
// associated types
// The token is a JWT: it implements neither `Display` nor a raw `Debug` so it
// never ends up in a log line or a span attribute. Use `as_str()` instead.
#[derive(Serialize, Deserialize, PartialEq, Clone, Eq, Hash)]
#[serde(transparent)]
pub struct AuthIdToken(pub String);

impl AuthIdToken {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for AuthIdToken {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("AuthIdToken([redacted])")
    }
}

impl From<AuthIdToken> for String {
    fn from(auth_id_token: AuthIdToken) -> Self {
        auth_id_token.0
    }
}

impl From<String> for AuthIdToken {
    fn from(auth_id_token: String) -> Self {
        Self(auth_id_token)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SessionAuthValidationParameters {
    pub auth_id_token: AuthIdToken,
    pub access_token: AccessToken,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, Hash)]
pub struct CloseSessionResponse {
    pub logout_url: Url,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, Hash)]
pub struct AuthorizeSessionResponse {
    pub authorization_url: Url,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_id_token_redacts_debug() {
        let secret = "eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiJ1c2VyIn0.c2lnbmF0dXJl";
        let token = AuthIdToken(secret.to_string());
        let parameters = SessionAuthValidationParameters {
            auth_id_token: token.clone(),
            access_token: AccessToken::new("access-token-secret".to_string()),
        };

        for output in [
            format!("{token:?}"),
            format!("{token:#?}"),
            format!("{parameters:?}"),
        ] {
            assert!(!output.contains(secret), "secret leaked in {output}");
            assert!(
                !output.contains("access-token-secret"),
                "secret leaked in {output}"
            );
            assert!(output.contains("[redacted]"), "missing marker in {output}");
        }
    }
}
