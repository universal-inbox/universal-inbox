use std::fmt;

use chrono::{DateTime, Utc};
use email_address::EmailAddress;
use secrecy::SecretBox;
use serde::{Deserialize, Serialize};
use universal_inbox::{
    auth::{AuthIdToken, auth_token::TruncatedAuthenticationToken, oauth2::AuthorizedOAuth2Client},
    billing::UserSubscription,
    integration_connection::IntegrationConnection,
    notification::Notification,
    task::Task,
    user::{
        EmailValidationToken, PasswordHash, User, UserAuthKind, UserAuthMethod,
        UserAuthMethodDisplayInfo, UserPreferences, Username,
    },
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

/// An email change awaiting verification of the new address (see
/// `UserService::patch_user`).
#[derive(Debug, Clone)]
pub struct PendingEmailChange {
    pub new_email: EmailAddress,
    pub validation_token: EmailValidationToken,
    pub requested_at: DateTime<Utc>,
}

/// Everything Universal Inbox stores about a user, as handed out by
/// `GET /users/me/export`. Only types without secrets go in here: auth methods
/// instead of `UserAuth`, truncated API tokens, and connections without their
/// OAuth credentials (stored apart, in `oauth_credential`).
#[derive(Debug, Serialize)]
pub struct UserDataExport {
    pub exported_at: DateTime<Utc>,
    pub user: User,
    pub preferences: Option<UserPreferences>,
    pub auth_methods: Vec<UserAuthMethod>,
    pub authentication_tokens: Vec<TruncatedAuthenticationToken>,
    pub oauth2_authorized_clients: Vec<AuthorizedOAuth2Client>,
    pub integration_connections: Vec<IntegrationConnection>,
    pub subscription: Option<UserSubscription>,
    pub notifications: Vec<Notification>,
    pub tasks: Vec<Task>,
}
