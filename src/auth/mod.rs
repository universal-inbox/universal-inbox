use std::hash::{Hash, Hasher};

use ::openidconnect::AccessToken;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use url::Url;

pub mod auth_token;
pub mod oauth2;
pub mod openidconnect;

// Simplify the ID token type to a string. This avoid to embed all the openidconnect
// associated types
// The token is a JWT: it is held in a `SecretString`, so it implements neither
// `Display` nor a raw `Debug` (it never ends up in a log line or a span
// attribute) and is zeroized on drop. Read it with `expose_secret()`.
#[derive(Debug, Clone)]
pub struct AuthIdToken(SecretString);

impl AuthIdToken {
    pub fn expose_secret(&self) -> &str {
        self.0.expose_secret()
    }
}

impl PartialEq for AuthIdToken {
    fn eq(&self, other: &Self) -> bool {
        self.expose_secret() == other.expose_secret()
    }
}

impl Eq for AuthIdToken {}

impl Hash for AuthIdToken {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.expose_secret().hash(state);
    }
}

// The token travels from the web app to the API and is stored in the database:
// it is (de)serialized as a plain string.
impl Serialize for AuthIdToken {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.expose_secret())
    }
}

impl<'de> Deserialize<'de> for AuthIdToken {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Self::from)
    }
}

impl From<String> for AuthIdToken {
    fn from(auth_id_token: String) -> Self {
        Self(auth_id_token.into())
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
        let token = AuthIdToken::from(secret.to_string());
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
            assert!(
                output.to_lowercase().contains("[redacted]"),
                "missing marker in {output}"
            );
        }
    }

    #[test]
    fn auth_id_token_serializes_as_a_plain_string() {
        let token = AuthIdToken::from("id-token".to_string());

        let json = serde_json::to_string(&token).unwrap();

        assert_eq!(json, r#""id-token""#);
        assert_eq!(serde_json::from_str::<AuthIdToken>(&json).unwrap(), token);
    }
}
