use std::fmt;

use secrecy::{CloneableSecret, SerializableSecret, zeroize::Zeroize};
use serde::{Deserialize, Serialize};

pub mod provider;

#[derive(Serialize, Deserialize, PartialEq, Clone, Eq, Hash, Default)]
#[serde(transparent)]
pub struct AccessToken(pub String);

impl AccessToken {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Zeroize for AccessToken {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

impl CloneableSecret for AccessToken {}

impl fmt::Debug for AccessToken {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("AccessToken([redacted])")
    }
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Eq, Hash)]
#[serde(transparent)]
pub struct RefreshToken(pub String);

impl RefreshToken {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Zeroize for RefreshToken {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

impl CloneableSecret for RefreshToken {}

impl fmt::Debug for RefreshToken {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("RefreshToken([redacted])")
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ClientSecret(pub String);

impl ClientSecret {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Zeroize for ClientSecret {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

impl CloneableSecret for ClientSecret {}

impl fmt::Debug for ClientSecret {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("ClientSecret([redacted])")
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AuthorizationCode(pub String);

impl AuthorizationCode {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Zeroize for AuthorizationCode {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

impl CloneableSecret for AuthorizationCode {}

impl fmt::Debug for AuthorizationCode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("AuthorizationCode([redacted])")
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PkceVerifier(pub String);

impl PkceVerifier {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Zeroize for PkceVerifier {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

impl CloneableSecret for PkceVerifier {}
impl SerializableSecret for PkceVerifier {}

impl fmt::Debug for PkceVerifier {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("PkceVerifier([redacted])")
    }
}

// These types deliberately implement neither `Display` nor a raw `Debug`: a
// `{}`/`{:?}` in a log line or a `tracing::instrument` field would otherwise
// export the secret. Use `as_str()` where the raw value is really needed.
#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "s3cr3t-value-that-must-not-leak";

    fn assert_redacted(debug: String, pretty_debug: String) {
        for output in [debug, pretty_debug] {
            assert!(!output.contains(SECRET), "secret leaked in {output}");
            assert!(output.contains("[redacted]"), "missing marker in {output}");
        }
    }

    #[test]
    fn credential_types_redact_debug() {
        let access_token = AccessToken(SECRET.to_string());
        assert_redacted(format!("{access_token:?}"), format!("{access_token:#?}"));
        let refresh_token = RefreshToken(SECRET.to_string());
        assert_redacted(format!("{refresh_token:?}"), format!("{refresh_token:#?}"));
        let client_secret = ClientSecret(SECRET.to_string());
        assert_redacted(format!("{client_secret:?}"), format!("{client_secret:#?}"));
        let code = AuthorizationCode(SECRET.to_string());
        assert_redacted(format!("{code:?}"), format!("{code:#?}"));
        let verifier = PkceVerifier(SECRET.to_string());
        assert_redacted(format!("{verifier:?}"), format!("{verifier:#?}"));
    }

    #[test]
    fn credential_types_redact_debug_when_nested() {
        let tuple = (
            AccessToken(SECRET.to_string()),
            Some(RefreshToken(SECRET.to_string())),
        );
        assert_redacted(format!("{tuple:?}"), format!("{tuple:#?}"));
    }
}
