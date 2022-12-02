use chrono::{DateTime, Utc};
use http::Uri;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use serde_with::{serde_as, DisplayFromStr};
use uuid::Uuid;

use self::integrations::todoist::TodoistTask;

pub mod integrations;

#[serde_as]
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
pub struct Task {
    pub id: Uuid,
    pub source_id: String,
    pub title: String,
    pub body: String,
    pub status: TaskStatus,
    pub completed_at: Option<DateTime<Utc>>,
    pub priority: TaskPriority,
    pub due_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub source_html_url: Option<Uri>,
    pub tags: Vec<String>,
    pub parent_id: Option<Uuid>,
    pub project: String,
    pub is_recurring: bool,
    pub created_at: DateTime<Utc>,
    pub metadata: TaskMetadata,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
#[serde(tag = "type", content = "content")]
pub enum TaskMetadata {
    Todoist(TodoistTask),
}

macro_attr! {
    #[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy, Eq, EnumFromStr!, EnumDisplay!)]
    pub enum TaskStatus {
        Active,
        Done
    }
}

#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Eq)]
pub struct TaskPatch {
    pub status: Option<TaskStatus>,
}

#[derive(
    Serialize_repr,
    Deserialize_repr,
    PartialEq,
    Debug,
    Clone,
    Eq,
    Copy,
    TryFromPrimitive,
    IntoPrimitive,
)]
#[repr(u8)]
pub enum TaskPriority {
    P1 = 1,
    P2 = 2,
    P3 = 3,
    P4 = 4,
}
