use std::{fmt, str::FromStr};

use anyhow::anyhow;
use chrono::{DateTime, Timelike, Utc};
use email_address::EmailAddress;
use secrecy::{CloneableSecret, SecretBox, SerializableSecret, zeroize::Zeroize};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use uuid::Uuid;
use validator::Validate;

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct User {
    pub id: UserId,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub email: Option<EmailAddress>,
    pub email_validated_at: Option<DateTime<Utc>>,
    pub email_validation_sent_at: Option<DateTime<Utc>>,
    pub chat_support_email_signature: Option<String>,
    pub is_testing: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl User {
    pub fn new(first_name: Option<String>, last_name: Option<String>, email: EmailAddress) -> Self {
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
        let params = Self { credentials };

        params.validate()?;

        Ok(params)
    }
}

#[derive(Serialize, Deserialize, Debug, Default, PartialEq)]
pub struct UserPatch {
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub email: Option<EmailAddress>,
}

#[derive(Deserialize, Serialize)]
pub struct Credentials {
    pub email: EmailAddress,
    pub password: SecretBox<Password>,
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

impl FromStr for Password {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() < 6 {
            return Err(anyhow!("Password must be at least 6 characters long"));
        }

        Ok(Self(s.to_string()))
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
