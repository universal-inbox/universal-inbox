//! Audit log of notification and task changes.
//!
//! Rows are written by Postgres triggers on the `notification` and `task` tables. The actor
//! comes from the transaction-local `app.audit_actor` setting, set by the API from the
//! request, MCP call, job or CLI command that opened the transaction.

use std::{collections::BTreeMap, fmt, str::FromStr};

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{integration_connection::provider::IntegrationProviderKind, user::UserId};

#[derive(Debug, Serialize, Deserialize, PartialEq, Copy, Clone, Eq, Hash, JsonSchema)]
#[serde(transparent)]
pub struct AuditEventId(pub Uuid);

impl fmt::Display for AuditEventId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for AuditEventId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl From<AuditEventId> for Uuid {
    fn from(id: AuditEventId) -> Self {
        id.0
    }
}

impl FromStr for AuditEventId {
    type Err = uuid::Error;

    fn from_str(uuid: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(uuid)?))
    }
}

macro_attr! {
    #[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy, Eq, Hash, EnumFromStr!, EnumDisplay!, JsonSchema)]
    pub enum AuditEntityKind {
        Notification,
        Task,
        Context,
    }
}

macro_attr! {
    #[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy, Eq, Hash, EnumFromStr!, EnumDisplay!, JsonSchema)]
    pub enum AuditAction {
        Created,
        Updated,
        Removed,
    }
}

macro_attr! {
    /// The kind of change an event carries, used to filter the audit log.
    #[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy, Eq, Hash, EnumFromStr!, EnumDisplay!, JsonSchema)]
    pub enum AuditChangeKind {
        Created,
        Status,
        Snoozed,
        SetAside,
        TaskLink,
        Removed,
    }
}

impl AuditChangeKind {
    /// Key of this change in `AuditEvent::changes`, `None` for `Created` and `Removed` which are
    /// actions rather than field changes.
    pub fn change_key(&self) -> Option<&'static str> {
        match self {
            AuditChangeKind::Status => Some("status"),
            AuditChangeKind::Snoozed => Some("snoozed_until"),
            AuditChangeKind::SetAside => Some("set_aside_at"),
            AuditChangeKind::TaskLink => Some("task_id"),
            AuditChangeKind::Created | AuditChangeKind::Removed => None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy, Eq, Hash, JsonSchema)]
pub enum AuditUserChannel {
    Web,
}

/// Who made a change. Stored as JSON, so new kinds can be added without a migration.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, JsonSchema)]
#[serde(tag = "kind")]
pub enum AuditActor {
    /// The user, from the web application (session cookie).
    User { via: AuditUserChannel },
    /// A personal API key (bearer token without audience).
    ApiKey,
    /// The browser extension, identified by the `X-Universal-Inbox-Client` header.
    Extension { version: String },
    /// An MCP client, identified by its OAuth client id.
    Mcp { client_id: String },
    /// A background job (synchronization, side effects, maintenance).
    System {
        job: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<IntegrationProviderKind>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        triggered_by: Option<Box<AuditActor>>,
    },
    /// An incoming webhook from a provider.
    Webhook { provider: IntegrationProviderKind },
    /// An automatic agent, with the reason of its decision.
    Agent {
        agent_id: String,
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<serde_json::Value>,
    },
    /// A command line invocation of the API binary.
    Cli { command: String },
    /// A change made without an actor in context: a bug to fix.
    Unknown,
}

macro_attr! {
    #[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy, Eq, Hash, EnumFromStr!, EnumDisplay!, JsonSchema)]
    pub enum AuditActorKind {
        User,
        ApiKey,
        Extension,
        Mcp,
        System,
        Webhook,
        Agent,
        Cli,
        Unknown,
    }
}

impl AuditActor {
    pub fn kind(&self) -> AuditActorKind {
        match self {
            AuditActor::User { .. } => AuditActorKind::User,
            AuditActor::ApiKey => AuditActorKind::ApiKey,
            AuditActor::Extension { .. } => AuditActorKind::Extension,
            AuditActor::Mcp { .. } => AuditActorKind::Mcp,
            AuditActor::System { .. } => AuditActorKind::System,
            AuditActor::Webhook { .. } => AuditActorKind::Webhook,
            AuditActor::Agent { .. } => AuditActorKind::Agent,
            AuditActor::Cli { .. } => AuditActorKind::Cli,
            AuditActor::Unknown => AuditActorKind::Unknown,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, JsonSchema)]
pub struct AuditChange {
    pub from: Option<serde_json::Value>,
    pub to: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, JsonSchema)]
pub struct AuditEvent {
    pub id: AuditEventId,
    pub user_id: UserId,
    pub entity_kind: AuditEntityKind,
    pub entity_id: Uuid,
    /// For a notification event, the task it is linked to. For an attachment or a note event,
    /// the context or task it belongs to (`entity_id` is then the attachment or the note).
    pub related_entity_id: Option<Uuid>,
    pub action: AuditAction,
    /// Title of the notification or task, or name of the context, when the change happened.
    pub title: String,
    /// `NotificationSourceKind` or `TaskSourceKind` of the entity, or the binding provider of a
    /// context (`Local` when it is not bound).
    pub source_kind: Option<String>,
    /// Changed fields, keyed by column name (`status`, `snoozed_until`, `set_aside_at`, `task_id`).
    pub changes: BTreeMap<String, AuditChange>,
    /// Task or context creation parameters. A task or note body is never copied, only whether one
    /// is set (`body_present`).
    pub params: Option<serde_json::Value>,
    pub actor: AuditActor,
    /// Display name of the actor when it is resolved at read time (MCP client name).
    pub actor_name: Option<String>,
    /// Postgres transaction id: events sharing it come from one action (eg. a bulk update).
    pub transaction_id: i64,
    pub created_at: DateTime<Utc>,
}

/// One row of the audit log: a single event, or the events of one bulk action. `Updated` events
/// sharing their transaction, entity kind, actor and changes form one group.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, JsonSchema)]
pub struct AuditEventGroup {
    /// All events of the group, newest first. Never empty.
    pub events: Vec<AuditEvent>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, Default, JsonSchema)]
pub struct AuditEventFilter {
    pub entity_kind: Option<AuditEntityKind>,
    /// Events of this entity. For a task, also the events of notifications linked to it.
    pub entity_id: Option<Uuid>,
    #[serde(default)]
    pub actor_kinds: Vec<AuditActorKind>,
    #[serde(default)]
    pub source_kinds: Vec<String>,
    pub change: Option<AuditChangeKind>,
    /// Case-insensitive search in titles.
    pub query: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    #[test]
    fn test_actor_serialization_is_tagged_by_kind() {
        let actor = AuditActor::System {
            job: "sync-notifications".to_string(),
            source: Some(IntegrationProviderKind::Github),
            triggered_by: Some(Box::new(AuditActor::User {
                via: AuditUserChannel::Web,
            })),
        };

        let value = serde_json::to_value(&actor).unwrap();

        assert_eq!(
            value,
            json!({
                "kind": "System",
                "job": "sync-notifications",
                "source": "Github",
                "triggered_by": { "kind": "User", "via": "Web" }
            })
        );
        assert_eq!(serde_json::from_value::<AuditActor>(value).unwrap(), actor);
        assert_eq!(
            serde_json::to_value(AuditActor::Unknown).unwrap(),
            json!({ "kind": "Unknown" })
        );
    }

    #[test]
    fn test_actor_kind_matches_serialized_tag() {
        for actor in [
            AuditActor::ApiKey,
            AuditActor::Extension {
                version: "1.4.2".to_string(),
            },
            AuditActor::Mcp {
                client_id: "client".to_string(),
            },
            AuditActor::Cli {
                command: "sync".to_string(),
            },
            AuditActor::Unknown,
        ] {
            let tag = serde_json::to_value(&actor).unwrap()["kind"]
                .as_str()
                .unwrap()
                .to_string();
            assert_eq!(actor.kind().to_string(), tag);
        }
    }
}
