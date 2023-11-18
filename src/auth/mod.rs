use std::fmt;

use serde::{Deserialize, Serialize};
use url::Url;

pub mod openidconnect;

// Simplify the ID token type to a string. This avoid to embed all the openidconnect
// associated types
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, Hash)]
#[serde(transparent)]
pub struct AuthIdToken(pub String);

impl fmt::Display for AuthIdToken {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
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

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, Hash)]
pub struct SessionAuthValidationParameters {
    pub auth_id_token: AuthIdToken,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, Hash)]
pub struct CloseSessionResponse {
    pub logout_url: Url,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, Hash)]
pub struct AuthorizeSessionResponse {
    pub authorization_url: Url,
}
