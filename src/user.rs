use std::{fmt, str::FromStr};

use anyhow::anyhow;
use chrono::{DateTime, Timelike, Utc};
use email_address::EmailAddress;
use secrecy::{CloneableSecret, DebugSecret, Secret, SerializableSecret, Zeroize};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use uuid::Uuid;
use validator::Validate;

use crate::auth::AuthIdToken;

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct User {
    pub id: UserId,
    pub first_name: String,
    pub last_name: String,
    pub email: EmailAddress,
    pub email_validated_at: Option<DateTime<Utc>>,
    pub email_validation_sent_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub auth: UserAuth,
}

impl User {
    pub fn new(first_name: String, last_name: String, email: EmailAddress, auth: UserAuth) -> Self {
        Self {
            id: Uuid::new_v4().into(),
            first_name,
            last_name,
            email,
            email_validated_at: None,
            email_validation_sent_at: None,
            created_at: Utc::now().with_nanosecond(0).unwrap(),
            updated_at: Utc::now().with_nanosecond(0).unwrap(),
            auth,
        }
    }

    pub fn is_email_validated(&self) -> bool {
        match self.auth {
            UserAuth::Local(_) => self.email_validated_at.is_some(),
            UserAuth::OpenIdConnect(_) => true,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", content = "content")]
pub enum UserAuth {
    Local(LocalUserAuth),
    OpenIdConnect(OpenIdConnectUserAuth),
}

impl fmt::Display for UserAuth {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                UserAuth::Local(_) => "Local",
                UserAuth::OpenIdConnect(_) => "OpenIdConnect",
            }
        )
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LocalUserAuth {
    pub password_hash: Secret<PasswordHash>,
    pub password_reset_at: Option<DateTime<Utc>>,
    pub password_reset_sent_at: Option<DateTime<Utc>>,
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
impl DebugSecret for PasswordHash {}
impl SerializableSecret for PasswordHash {}

#[derive(Deserialize, Serialize, Validate)]
pub struct RegisterUserParameters {
    #[validate(length(min = 1))]
    pub first_name: String,
    #[validate(length(min = 1))]
    pub last_name: String,
    pub credentials: Credentials,
}

impl RegisterUserParameters {
    pub fn try_new(
        first_name: String,
        last_name: String,
        credentials: Credentials,
    ) -> Result<Self, anyhow::Error> {
        let params = Self {
            first_name,
            last_name,
            credentials,
        };

        params.validate()?;

        Ok(params)
    }
}

#[derive(Deserialize, Serialize)]
pub struct Credentials {
    pub email: EmailAddress,
    pub password: Secret<Password>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(transparent)]
pub struct Password(pub String);

impl Zeroize for Password {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}
impl CloneableSecret for Password {}
impl DebugSecret for Password {}
impl SerializableSecret for Password {}

impl FromStr for Password {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() < 6 {
            return Err(anyhow!("Password must be at least 6 characters long"));
        }

        Ok(Self(s.to_string()))
    }
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

#[derive(Debug, Serialize, Deserialize, PartialEq, Copy, Clone, Eq, Hash)]
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
