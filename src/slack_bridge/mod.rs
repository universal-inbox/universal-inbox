use std::{fmt, str::FromStr};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use slack_morphism::prelude::*;
use uuid::Uuid;

use crate::{notification::NotificationId, user::UserId};

#[derive(Debug, Serialize, Deserialize, PartialEq, Copy, Clone, Eq, Hash)]
#[serde(transparent)]
pub struct SlackBridgePendingActionId(pub Uuid);

impl fmt::Display for SlackBridgePendingActionId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for SlackBridgePendingActionId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}

impl From<SlackBridgePendingActionId> for Uuid {
    fn from(id: SlackBridgePendingActionId) -> Self {
        id.0
    }
}

impl FromStr for SlackBridgePendingActionId {
    type Err = uuid::Error;

    fn from_str(uuid: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(uuid)?))
    }
}

macro_attr! {
    #[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, EnumFromStr!, EnumDisplay!)]
    pub enum SlackBridgeActionType {
        MarkAsRead,
        Unsubscribe,
    }
}

macro_attr! {
    #[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, EnumFromStr!, EnumDisplay!)]
    pub enum SlackBridgeActionStatus {
        Pending,
        Completed,
        Failed,
        PermanentlyFailed,
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SlackBridgePendingAction {
    pub id: SlackBridgePendingActionId,
    pub user_id: UserId,
    pub notification_id: Option<NotificationId>,
    pub action_type: SlackBridgeActionType,
    pub slack_team_id: SlackTeamId,
    pub slack_channel_id: SlackChannelId,
    pub slack_thread_ts: SlackTs,
    pub slack_last_message_ts: SlackTs,
    pub status: SlackBridgeActionStatus,
    pub failure_message: Option<String>,
    pub retry_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SlackBridgeStatus {
    pub extension_connected: bool,
    pub team_id_match: bool,
    pub user_id_match: bool,
    pub pending_actions_count: i64,
    pub failed_actions_count: i64,
    pub last_completed_at: Option<DateTime<Utc>>,
}
