use std::fmt;

use chrono::{DateTime, Utc};
use secrecy::SecretBox;
use serde::{Deserialize, Serialize};
use universal_inbox::{
    auth::AuthIdToken,
    user::{PasswordHash, UserAuthKind, UserAuthMethod, UserAuthMethodDisplayInfo, Username},
};
use webauthn_rs::prelude::*;

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

impl UserAuth {
    pub fn kind(&self) -> UserAuthKind {
        match self {
            UserAuth::Local(_) => UserAuthKind::Local,
            UserAuth::OIDCGoogleAuthorizationCode(_) => UserAuthKind::OIDCGoogleAuthorizationCode,
            UserAuth::OIDCAuthorizationCodePKCE(_) => UserAuthKind::OIDCAuthorizationCodePKCE,
            UserAuth::Passkey(_) => UserAuthKind::Passkey,
        }
    }
}

impl From<&UserAuth> for UserAuthMethod {
    fn from(user_auth: &UserAuth) -> Self {
        let kind = user_auth.kind();
        let display_info = match user_auth {
            UserAuth::Local(_) => UserAuthMethodDisplayInfo::Local,
            UserAuth::OIDCGoogleAuthorizationCode(_) => {
                UserAuthMethodDisplayInfo::OIDCGoogleAuthorizationCode
            }
            UserAuth::OIDCAuthorizationCodePKCE(_) => {
                UserAuthMethodDisplayInfo::OIDCAuthorizationCodePKCE
            }
            UserAuth::Passkey(passkey_auth) => UserAuthMethodDisplayInfo::Passkey {
                username: passkey_auth.username.to_string(),
            },
        };
        UserAuthMethod { kind, display_info }
    }
}

#[derive(Debug, Clone)]
pub struct PasskeyUserAuth {
    pub username: Username,
    pub passkey: Passkey,
}

#[derive(Debug, Clone)]
pub struct LocalUserAuth {
    pub password_hash: SecretBox<PasswordHash>,
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
