use std::fmt;

use secrecy::{CloneableSecret, SerializableSecret, zeroize::Zeroize};
use serde::{Deserialize, Serialize};

pub mod provider;

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, Hash, Default)]
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

impl fmt::Display for AccessToken {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, Hash)]
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

impl fmt::Display for RefreshToken {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
