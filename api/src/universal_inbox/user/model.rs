use std::fmt;

use chrono::{DateTime, TimeDelta, Utc};
use email_address::EmailAddress;
use secrecy::SecretBox;
use serde::{Deserialize, Serialize};
use universal_inbox::pii::Pii;
use universal_inbox::{
    auth::{AuthIdToken, auth_token::TruncatedAuthenticationToken, oauth2::AuthorizedOAuth2Client},
    billing::UserSubscription,
    integration_connection::IntegrationConnection,
    notification::Notification,
    task::Task,
    user::{
        EmailValidationToken, PasswordHash, User, UserAuthKind, UserAuthMethod,
        UserAuthMethodDisplayInfo, UserId, UserPreferences, Username,
    },
};
use webauthn_rs::prelude::*;

use crate::universal_inbox::UniversalInboxError;

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
    pub new_email: Pii<EmailAddress>,
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

/// When a browser session last proved who its user is: at login or on
/// re-authentication. Kept in the signed session cookie and bound to the
/// user it was proven for. API bearer tokens never carry one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionAuthentication {
    pub user_id: UserId,
    pub authenticated_at: DateTime<Utc>,
}

impl SessionAuthentication {
    pub fn now(user_id: UserId) -> Self {
        Self {
            user_id,
            authenticated_at: Utc::now(),
        }
    }

    /// Sensitive account operations (ASVS 3.7.1) need `user_id` to have
    /// proven their identity in this session within `window`. A missing,
    /// stale, future or foreign proof asks the user to re-authenticate.
    pub fn ensure_recent(
        session_authentication: Option<&SessionAuthentication>,
        user_id: UserId,
        window: TimeDelta,
    ) -> Result<(), UniversalInboxError> {
        match session_authentication {
            Some(authentication)
                if authentication.user_id == user_id
                    && Self::is_recent(authentication.authenticated_at, window) =>
            {
                Ok(())
            }
            _ => Err(UniversalInboxError::ReauthenticationRequired),
        }
    }

    /// Whether an identity proof made at `authenticated_at` is at most
    /// `window` old. A proof dated in the future is refused, beyond a small
    /// tolerance for clock drift between API instances.
    pub fn is_recent(authenticated_at: DateTime<Utc>, window: TimeDelta) -> bool {
        let age = Utc::now() - authenticated_at;
        age >= -TimeDelta::seconds(CLOCK_SKEW_TOLERANCE_SECONDS) && age <= window
    }
}

/// Clock drift tolerated between API instances when checking when an
/// identity was proven.
const CLOCK_SKEW_TOLERANCE_SECONDS: i64 = 5;

#[cfg(test)]
mod session_authentication_tests {
    use uuid::Uuid;

    use super::*;

    fn window() -> TimeDelta {
        TimeDelta::minutes(15)
    }

    #[test]
    fn a_recent_authentication_of_the_same_user_is_accepted() {
        let user_id: UserId = Uuid::new_v4().into();
        let authentication = SessionAuthentication {
            user_id,
            authenticated_at: Utc::now() - TimeDelta::minutes(14),
        };

        assert!(
            SessionAuthentication::ensure_recent(Some(&authentication), user_id, window()).is_ok()
        );
    }

    #[test]
    fn a_stale_authentication_is_rejected() {
        let user_id: UserId = Uuid::new_v4().into();
        let authentication = SessionAuthentication {
            user_id,
            authenticated_at: Utc::now() - TimeDelta::minutes(16),
        };

        assert!(matches!(
            SessionAuthentication::ensure_recent(Some(&authentication), user_id, window()),
            Err(UniversalInboxError::ReauthenticationRequired)
        ));
    }

    #[test]
    fn an_authentication_of_another_user_is_rejected() {
        let authentication = SessionAuthentication::now(Uuid::new_v4().into());

        assert!(matches!(
            SessionAuthentication::ensure_recent(
                Some(&authentication),
                Uuid::new_v4().into(),
                window()
            ),
            Err(UniversalInboxError::ReauthenticationRequired)
        ));
    }

    #[test]
    fn an_authentication_dated_in_the_future_is_rejected() {
        let user_id: UserId = Uuid::new_v4().into();
        let authentication = SessionAuthentication {
            user_id,
            authenticated_at: Utc::now() + TimeDelta::minutes(1),
        };

        assert!(matches!(
            SessionAuthentication::ensure_recent(Some(&authentication), user_id, window()),
            Err(UniversalInboxError::ReauthenticationRequired)
        ));
    }

    #[test]
    fn a_small_clock_drift_is_tolerated() {
        assert!(SessionAuthentication::is_recent(
            Utc::now() + TimeDelta::seconds(2),
            window()
        ));
    }

    #[test]
    fn a_missing_authentication_is_rejected() {
        assert!(matches!(
            SessionAuthentication::ensure_recent(None, Uuid::new_v4().into(), window()),
            Err(UniversalInboxError::ReauthenticationRequired)
        ));
    }
}
