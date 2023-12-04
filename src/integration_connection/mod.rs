use std::{fmt, str::FromStr};

use chrono::{DateTime, Timelike, Utc};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use uuid::Uuid;

use crate::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        provider::{IntegrationProvider, IntegrationProviderKind},
    },
    user::UserId,
};

pub mod config;
pub mod integrations;
pub mod provider;

#[serde_as]
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
pub struct IntegrationConnection {
    pub id: IntegrationConnectionId,
    pub user_id: UserId,
    pub connection_id: ConnectionId,
    pub status: IntegrationConnectionStatus,
    pub failure_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_sync_started_at: Option<DateTime<Utc>>,
    pub last_sync_failure_message: Option<String>,
    pub provider: IntegrationProvider,
}

impl IntegrationConnection {
    pub fn new(user_id: UserId, config: IntegrationConnectionConfig) -> Self {
        Self {
            id: Uuid::new_v4().into(),
            connection_id: Uuid::new_v4().into(),
            user_id,
            status: IntegrationConnectionStatus::Created,
            failure_message: None,
            created_at: Utc::now().with_nanosecond(0).unwrap(),
            updated_at: Utc::now().with_nanosecond(0).unwrap(),
            last_sync_started_at: None,
            last_sync_failure_message: None,
            // Using unwrap as None cannot mismatch with the provided config
            provider: IntegrationProvider::new(config, None).unwrap(),
        }
    }

    pub fn is_connected(&self) -> bool {
        self.status == IntegrationConnectionStatus::Validated
    }

    pub fn is_connected_task_service(&self) -> bool {
        self.is_connected() && self.provider.is_task_service()
    }
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
pub struct IntegrationConnectionCreation {
    pub provider_kind: IntegrationProviderKind,
}

macro_attr! {
    #[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy, Eq, EnumFromStr!, EnumDisplay!, Hash)]
    pub enum IntegrationConnectionStatus {
        Created,
        Validated,
        Failing,
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Copy, Clone, Eq, Hash)]
#[serde(transparent)]
pub struct IntegrationConnectionId(pub Uuid);

impl fmt::Display for IntegrationConnectionId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for IntegrationConnectionId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl From<IntegrationConnectionId> for Uuid {
    fn from(integration_connection_id: IntegrationConnectionId) -> Self {
        integration_connection_id.0
    }
}

impl TryFrom<String> for IntegrationConnectionId {
    type Error = uuid::Error;

    fn try_from(uuid: String) -> Result<Self, Self::Error> {
        Ok(Self(Uuid::parse_str(&uuid)?))
    }
}

impl FromStr for IntegrationConnectionId {
    type Err = uuid::Error;

    fn from_str(uuid: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(uuid)?))
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Copy, Clone, Eq, Hash)]
#[serde(transparent)]
pub struct ConnectionId(pub Uuid);

impl fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for ConnectionId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl From<ConnectionId> for Uuid {
    fn from(connection_id: ConnectionId) -> Self {
        connection_id.0
    }
}

impl FromStr for ConnectionId {
    type Err = uuid::Error;

    fn from_str(uuid: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(uuid)?))
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, Hash)]
#[serde(transparent)]
pub struct NangoProviderKey(pub String);

impl fmt::Display for NangoProviderKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for NangoProviderKey {
    fn from(s: String) -> Self {
        Self(s)
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, Hash)]
#[serde(transparent)]
pub struct NangoPublicKey(pub String);

impl fmt::Display for NangoPublicKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for NangoPublicKey {
    fn from(s: String) -> Self {
        Self(s)
    }
}
