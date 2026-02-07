use std::{fmt, str::FromStr};

use chrono::{DateTime, Utc};
use secrecy::{CloneableSecret, ExposeSecret, SecretBox, SerializableSecret, zeroize::Zeroize};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::user::UserId;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthenticationToken {
    pub id: AuthenticationTokenId,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub user_id: UserId,
    pub jwt_token: SecretBox<JWTToken>,
    pub expire_at: Option<DateTime<Utc>>,
    pub is_revoked: bool,
    pub is_session_token: bool,
}

impl AuthenticationToken {
    pub fn new(
        user_id: UserId,
        jwt_token: SecretBox<JWTToken>,
        expire_at: Option<DateTime<Utc>>,
        is_session_token: bool,
    ) -> Self {
        Self {
            id: Uuid::new_v4().into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            user_id,
            jwt_token,
            expire_at,
            is_revoked: false,
            is_session_token,
        }
    }

    pub fn is_expired(&self) -> bool {
        self.expire_at
            .map(|expire_at| expire_at < Utc::now())
            .unwrap_or(false)
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, Hash)]
#[serde(transparent)]
pub struct AuthenticationTokenId(pub Uuid);

impl fmt::Display for AuthenticationTokenId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for AuthenticationTokenId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl From<AuthenticationTokenId> for Uuid {
    fn from(auth_token_id: AuthenticationTokenId) -> Self {
        auth_token_id.0
    }
}

impl TryFrom<String> for AuthenticationTokenId {
    type Error = uuid::Error;

    fn try_from(uuid: String) -> Result<Self, Self::Error> {
        Ok(Self(Uuid::parse_str(&uuid)?))
    }
}

impl FromStr for AuthenticationTokenId {
    type Err = uuid::Error;

    fn from_str(uuid: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(uuid)?))
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, Hash)]
#[serde(transparent)]
pub struct JWTToken(pub String);

impl fmt::Display for JWTToken {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Zeroize for JWTToken {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

impl CloneableSecret for JWTToken {}
impl SerializableSecret for JWTToken {}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TruncatedAuthenticationToken {
    pub id: AuthenticationTokenId,
    pub user_id: UserId,
    pub truncated_jwt_token: String,
    pub expire_at: Option<DateTime<Utc>>,
    pub is_revoked: bool,
    pub is_session_token: bool,
}

impl TruncatedAuthenticationToken {
    pub fn new(authentication_token: AuthenticationToken) -> Self {
        let mut truncated_jwt_token = authentication_token.jwt_token.expose_secret().to_string();
        let keep = truncated_jwt_token.len() - 5;
        truncated_jwt_token.drain(..keep);

        Self {
            id: authentication_token.id,
            user_id: authentication_token.user_id,
            truncated_jwt_token,
            expire_at: authentication_token.expire_at,
            is_revoked: authentication_token.is_revoked,
            is_session_token: authentication_token.is_session_token,
        }
    }
}

#[cfg(test)]
mod tests {

    mod truncated_jwt_token {
        use super::super::*;
        use pretty_assertions::assert_eq;
        use rstest::*;

        #[rstest]
        fn test_truncated_jwt_token_keep_secret() {
            assert_eq!(
                TruncatedAuthenticationToken::new(AuthenticationToken::new(
                    UserId(Uuid::new_v4()),
                    SecretBox::new(Box::new(JWTToken("long_value".to_string()))),
                    Some(Utc::now()),
                    false,
                ))
                .truncated_jwt_token,
                "value".to_string()
            );
        }
    }
}
