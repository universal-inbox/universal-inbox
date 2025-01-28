use std::fmt;

use chrono::{DateTime, Utc};
use secrecy::Secret;
use serde::{Deserialize, Serialize};
use universal_inbox::{auth::AuthIdToken, user::Username};
use webauthn_rs::prelude::*;

use universal_inbox::user::PasswordHash;

#[derive(Debug, Clone)]
pub enum UserAuth {
    Local(Box<LocalUserAuth>),
    OIDCGoogleAuthorizationCode(Box<OpenIdConnectUserAuth>),
    OIDCAuthorizationCodePKCE(Box<OpenIdConnectUserAuth>),
    Passkey(Box<PasskeyUserAuth>),
}

impl fmt::Display for UserAuth {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                UserAuth::Local(_) => "Local",
                UserAuth::OIDCGoogleAuthorizationCode(_) => "OIDCGoogleAuthorizationCode",
                UserAuth::OIDCAuthorizationCodePKCE(_) => "OIDCAuthorizationCodePKCE",
                UserAuth::Passkey(_) => "Passkey",
            }
        )
    }
}

macro_attr! {
    #[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, EnumFromStr!, EnumDisplay!)]
    pub enum UserAuthKind {
        Local,
        OIDCGoogleAuthorizationCode,
        OIDCAuthorizationCodePKCE,
        Passkey,
    }
}

#[derive(Debug, Clone)]
pub struct PasskeyUserAuth {
    pub username: Username,
    pub passkey: Passkey,
}

#[derive(Debug, Clone)]
pub struct LocalUserAuth {
    pub password_hash: Secret<PasswordHash>,
    pub password_reset_at: Option<DateTime<Utc>>,
    pub password_reset_sent_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
pub struct OpenIdConnectUserAuth {
    pub auth_user_id: AuthUserId,
    pub auth_id_token: AuthIdToken,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, Hash)]
#[serde(transparent)]
pub struct AuthUserId(pub String);

impl fmt::Display for AuthUserId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for AuthUserId {
    fn from(string: String) -> Self {
        Self(string)
    }
}

impl From<AuthUserId> for String {
    fn from(auth_user_id: AuthUserId) -> Self {
        auth_user_id.0
    }
}
