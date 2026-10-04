use std::{fmt, str::FromStr};

use anyhow::anyhow;
use chrono::{DateTime, Timelike, Utc};
use email_address::EmailAddress;
use secrecy::{CloneableSecret, ExposeSecret, SecretBox, SerializableSecret, zeroize::Zeroize};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use uuid::Uuid;
use validator::Validate;

use crate::{integration_connection::provider::IntegrationProviderKind, pii::Pii};

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct User {
    pub id: UserId,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub email: Option<Pii<EmailAddress>>,
    pub email_validated_at: Option<DateTime<Utc>>,
    pub email_validation_sent_at: Option<DateTime<Utc>>,
    pub chat_support_email_signature: Option<String>,
    pub is_testing: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl User {
    pub fn new(
        first_name: Option<String>,
        last_name: Option<String>,
        email: Pii<EmailAddress>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().into(),
            first_name,
            last_name,
            email: Some(email),
            email_validated_at: None,
            email_validation_sent_at: None,
            chat_support_email_signature: None,
            is_testing: false,
            created_at: Utc::now().with_nanosecond(0).unwrap(),
            updated_at: Utc::now().with_nanosecond(0).unwrap(),
        }
    }

    pub fn new_with_passkey(user_id: UserId) -> Self {
        Self {
            id: user_id,
            first_name: None,
            last_name: None,
            email: None,
            email_validated_at: None,
            email_validation_sent_at: None,
            chat_support_email_signature: None,
            is_testing: false,
            created_at: Utc::now().with_nanosecond(0).unwrap(),
            updated_at: Utc::now().with_nanosecond(0).unwrap(),
        }
    }

    pub fn is_email_validated(&self) -> bool {
        self.is_testing
            || self.email_validation_sent_at.is_none()
            || self.email_validated_at.is_some()
    }

    pub fn full_name(&self) -> Option<String> {
        match (&self.first_name, &self.last_name) {
            (Some(first_name), Some(last_name)) => Some(format!("{} {}", first_name, last_name)),
            (Some(first_name), None) => Some(first_name.clone()),
            (None, Some(last_name)) => Some(last_name.clone()),
            (None, None) => None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(transparent)]
pub struct PasswordHash(pub String);

impl Zeroize for PasswordHash {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}
impl CloneableSecret for PasswordHash {}

#[derive(Deserialize, Serialize, Validate)]
pub struct RegisterUserParameters {
    pub credentials: Credentials,
}

impl RegisterUserParameters {
    pub fn try_new(credentials: Credentials) -> Result<Self, anyhow::Error> {
        // Registration sets a new password: apply the full policy, unlike
        // `Credentials` parsing, which also serves logins.
        Password::check_policy(&credentials.password.expose_secret().0)?;
        let params = Self { credentials };

        params.validate()?;

        Ok(params)
    }
}

/// Keyword a user without an email address types to confirm the deletion of
/// their account (users with an email type their email address instead).
pub const ACCOUNT_DELETION_CONFIRMATION_KEYWORD: &str = "DELETE";

/// Body of `DELETE /api/users/me`: the user re-types their email address (or
/// [`ACCOUNT_DELETION_CONFIRMATION_KEYWORD`] when they have none) to confirm
/// that they want their account and all its data deleted.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct DeleteAccountParameters {
    pub confirmation: String,
}

impl User {
    /// The text the user must type to confirm the deletion of their account.
    pub fn account_deletion_confirmation(&self) -> String {
        self.email
            .as_ref()
            .map(|email| email.expose().to_string())
            .unwrap_or_else(|| ACCOUNT_DELETION_CONFIRMATION_KEYWORD.to_string())
    }

    /// Whether `confirmation` confirms the deletion of this user's account
    /// (surrounding whitespace and case are ignored).
    pub fn is_account_deletion_confirmed(&self, confirmation: &str) -> bool {
        confirmation
            .trim()
            .eq_ignore_ascii_case(&self.account_deletion_confirmation())
    }
}

/// Maximum length, in characters, of a user's first or last name.
pub const USER_NAME_MAX_LENGTH: u64 = 100;

/// Body of `PATCH /api/users/me`. Callers validate it (`Validate`) at the
/// request boundary; internal flows (e.g. an OIDC profile) build it unchecked.
#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq, Validate)]
#[serde(deny_unknown_fields)]
pub struct UserPatch {
    #[validate(length(max = USER_NAME_MAX_LENGTH))]
    pub first_name: Option<String>,
    #[validate(length(max = USER_NAME_MAX_LENGTH))]
    pub last_name: Option<String>,
    pub email: Option<Pii<EmailAddress>>,
}

#[derive(Deserialize, Serialize)]
pub struct Credentials {
    pub email: Pii<EmailAddress>,
    pub password: SecretBox<Password>,
}

/// Body of a password change request from an authenticated user: the
/// current password is re-checked before `new_password` replaces it.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PasswordChange {
    pub current_password: SecretBox<Password>,
    pub new_password: SecretBox<Password>,
}

/// Body of a password reset request: the one-time token from the reset email
/// travels in the body (not the URL) so it stays out of access logs.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PasswordReset {
    pub password_reset_token: PasswordResetToken,
    pub new_password: SecretBox<Password>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(transparent)]
pub struct Password(pub String);

impl Zeroize for Password {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}
impl CloneableSecret for Password {}
impl SerializableSecret for Password {}

/// Bounds of the password policy applied to every new password, counted in
/// characters (ASVS 2.1.1 / 2.1.2).
pub const PASSWORD_MIN_LENGTH: usize = 12;
/// Upper bound keeping Argon2 hashing cost bounded for attacker-sized input.
/// Longer passwords are rejected, never truncated. Also applied to login
/// attempts.
pub const PASSWORD_MAX_LENGTH: usize = 128;

impl Password {
    /// Check a new password against the password policy. `Password` is
    /// deserialized straight from JSON (`serde(transparent)`), so the API
    /// must call this explicitly before hashing a new password; it is not
    /// applied when checking a login attempt against an existing password,
    /// which may have been set under an older, shorter minimum.
    pub fn check_policy(password: &str) -> Result<(), anyhow::Error> {
        if password.chars().count() < PASSWORD_MIN_LENGTH {
            return Err(anyhow!(
                "Password must be at least {PASSWORD_MIN_LENGTH} characters long"
            ));
        }
        Self::check_max_length(password)
    }

    /// Check the upper bound only: the part of the policy that also applies
    /// to a password typed to log in or to confirm an existing password.
    pub fn check_max_length(password: &str) -> Result<(), anyhow::Error> {
        if password.chars().count() > PASSWORD_MAX_LENGTH {
            return Err(anyhow!(
                "Password must be at most {PASSWORD_MAX_LENGTH} characters long"
            ));
        }
        Ok(())
    }
}

/// Parses an existing password (login, current password): only the upper
/// bound is checked. Use [`NewPassword`] to parse a password being set.
impl FromStr for Password {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(anyhow!("Password is required"));
        }
        Self::check_max_length(s)?;

        Ok(Self(s.to_string()))
    }
}

/// A password being set (registration, reset, new local auth method or
/// password change): parsing applies the full password policy.
#[derive(Debug, Clone, PartialEq)]
pub struct NewPassword(pub Password);

impl FromStr for NewPassword {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Password::check_policy(s)?;

        Ok(Self(Password(s.to_string())))
    }
}

impl From<NewPassword> for Password {
    fn from(new_password: NewPassword) -> Self {
        new_password.0
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, Hash)]
#[serde(transparent)]
pub struct Username(pub String);

impl fmt::Display for Username {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for Username {
    fn from(string: String) -> Self {
        Self(string)
    }
}

impl From<Username> for String {
    fn from(username: Username) -> Self {
        username.0
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Copy, Clone, Eq, Hash, schemars::JsonSchema)]
#[serde(transparent)]
pub struct UserId(pub Uuid);

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for UserId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl From<UserId> for Uuid {
    fn from(user_id: UserId) -> Self {
        user_id.0
    }
}

impl TryFrom<String> for UserId {
    type Error = uuid::Error;

    fn try_from(uuid: String) -> Result<Self, Self::Error> {
        Ok(Self(Uuid::parse_str(&uuid)?))
    }
}

impl FromStr for UserId {
    type Err = uuid::Error;

    fn from_str(uuid: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(uuid)?))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(transparent)]
pub struct EmailValidationToken(pub Uuid);

impl fmt::Display for EmailValidationToken {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for EmailValidationToken {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl From<EmailValidationToken> for Uuid {
    fn from(email_validation_token: EmailValidationToken) -> Self {
        email_validation_token.0
    }
}

impl TryFrom<String> for EmailValidationToken {
    type Error = uuid::Error;

    fn try_from(uuid: String) -> Result<Self, Self::Error> {
        Ok(Self(Uuid::parse_str(&uuid)?))
    }
}

impl FromStr for EmailValidationToken {
    type Err = uuid::Error;

    fn from_str(uuid: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(uuid)?))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(transparent)]
pub struct PasswordResetToken(pub Uuid);

impl fmt::Display for PasswordResetToken {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for PasswordResetToken {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl From<PasswordResetToken> for Uuid {
    fn from(password_reset_token: PasswordResetToken) -> Self {
        password_reset_token.0
    }
}

impl TryFrom<String> for PasswordResetToken {
    type Error = uuid::Error;

    fn try_from(uuid: String) -> Result<Self, Self::Error> {
        Ok(Self(Uuid::parse_str(&uuid)?))
    }
}

impl FromStr for PasswordResetToken {
    type Err = uuid::Error;

    fn from_str(uuid: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(uuid)?))
    }
}

macro_attr! {
    #[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, EnumFromStr!, EnumDisplay!)]
    pub enum UserAuthKind {
        Local,
        OIDCGoogleAuthorizationCode,
        OIDCAuthorizationCodePKCE,
        Passkey,
    }
}

/// Frontend-safe representation of a user's authentication method (no secrets exposed).
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct UserAuthMethod {
    pub kind: UserAuthKind,
    pub display_info: UserAuthMethodDisplayInfo,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum UserAuthMethodDisplayInfo {
    Local,
    Passkey { username: String },
    OIDCGoogleAuthorizationCode,
    OIDCAuthorizationCodePKCE,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct UserPreferences {
    pub user_id: UserId,
    pub default_task_manager_provider_kind: Option<IntegrationProviderKind>,
    /// When `true`, opening a notification's source (e.g. via the `Enter`
    /// shortcut) opens it in a background tab so focus stays on Universal
    /// Inbox. Defaults to `false` (foreground).
    pub open_links_in_background: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct UserPreferencesPatch {
    pub default_task_manager_provider_kind: Option<Option<IntegrationProviderKind>>,
    pub open_links_in_background: Option<bool>,
}

#[cfg(test)]
mod password_policy_tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::too_short(11, false)]
    #[case::min_length(12, true)]
    #[case::max_length(128, true)]
    #[case::too_long(129, false)]
    fn a_new_password_must_fit_the_length_bounds(#[case] length: usize, #[case] is_valid: bool) {
        assert_eq!("a".repeat(length).parse::<NewPassword>().is_ok(), is_valid);
    }

    #[test]
    fn length_is_counted_in_characters_not_bytes() {
        // 12 characters, 36 bytes
        assert!("€".repeat(12).parse::<NewPassword>().is_ok());
        // 128 characters, 384 bytes
        assert!("€".repeat(128).parse::<NewPassword>().is_ok());
        assert!("€".repeat(129).parse::<NewPassword>().is_err());
    }

    #[rstest]
    #[case::short_legacy_password(6, true)]
    #[case::max_length(128, true)]
    #[case::too_long(129, false)]
    fn an_existing_password_is_only_checked_against_the_max_length(
        #[case] length: usize,
        #[case] is_valid: bool,
    ) {
        assert_eq!("a".repeat(length).parse::<Password>().is_ok(), is_valid);
    }

    #[test]
    fn an_existing_password_cannot_be_empty() {
        assert!("".parse::<Password>().is_err());
    }
}

#[cfg(test)]
mod account_deletion_confirmation_tests {
    use super::*;

    #[test]
    fn a_user_with_an_email_confirms_with_their_email() {
        let user = User::new(None, None, "John.Doe@example.com".parse().unwrap());

        assert!(user.is_account_deletion_confirmed("John.Doe@example.com"));
        assert!(user.is_account_deletion_confirmed("  john.doe@EXAMPLE.com "));
        assert!(!user.is_account_deletion_confirmed("DELETE"));
        assert!(!user.is_account_deletion_confirmed("other@example.com"));
        assert!(!user.is_account_deletion_confirmed(""));
    }

    #[test]
    fn a_user_without_an_email_confirms_with_the_keyword() {
        let mut user = User::new(None, None, "john@example.com".parse().unwrap());
        user.email = None;

        assert!(user.is_account_deletion_confirmed("DELETE"));
        assert!(user.is_account_deletion_confirmed("delete"));
        assert!(!user.is_account_deletion_confirmed(""));
    }
}

#[cfg(test)]
mod user_patch_validation_tests {
    use super::*;

    fn patch(first_name: Option<String>, last_name: Option<String>) -> UserPatch {
        UserPatch {
            first_name,
            last_name,
            email: None,
        }
    }

    #[test]
    fn names_up_to_the_limit_are_valid() {
        let name = "é".repeat(USER_NAME_MAX_LENGTH as usize);
        assert!(patch(Some(name.clone()), Some(name)).validate().is_ok());
        assert!(patch(None, None).validate().is_ok());
    }

    #[test]
    fn names_over_the_limit_are_invalid() {
        let name = "é".repeat(USER_NAME_MAX_LENGTH as usize + 1);
        assert!(patch(Some(name.clone()), None).validate().is_err());
        assert!(patch(None, Some(name)).validate().is_err());
    }

    #[test]
    fn unknown_fields_are_rejected() {
        assert!(serde_json::from_str::<UserPatch>(r#"{"first_name": "John", "id": "x"}"#).is_err());
        assert!(serde_json::from_str::<UserPatch>(r#"{"first_name": "John"}"#).is_ok());
    }
}
