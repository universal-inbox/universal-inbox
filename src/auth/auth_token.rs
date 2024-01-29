use std::{fmt, str::FromStr};

use chrono::{DateTime, Utc};
use secrecy::{CloneableSecret, DebugSecret, Secret, SerializableSecret, Zeroize};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::user::UserId;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthenticationToken {
    pub id: AuthenticationTokenId,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub user_id: UserId,
    pub jwt_token: Secret<JWTToken>,
    pub expire_at: Option<DateTime<Utc>>,
    pub is_revoked: bool,
}

impl AuthenticationToken {
    pub fn new(
        user_id: UserId,
        jwt_token: Secret<JWTToken>,
        expire_at: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            user_id,
            jwt_token,
            expire_at,
            is_revoked: false,
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

impl Zeroize for JWTToken {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}
impl CloneableSecret for JWTToken {}
impl DebugSecret for JWTToken {}
impl SerializableSecret for JWTToken {}
