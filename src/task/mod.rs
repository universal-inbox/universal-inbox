use std::{
    fmt::{self, Display, Formatter},
    str::FromStr,
};

use chrono::{DateTime, NaiveDate, NaiveDateTime, ParseError, Utc};
use http::Uri;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use serde_with::{serde_as, DisplayFromStr};
use uuid::Uuid;

use crate::notification::{
    Notification, NotificationMetadata, NotificationStatus, NotificationWithTask,
};

use self::integrations::todoist::TodoistItem;

pub mod integrations;

#[serde_as]
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
pub struct Task {
    pub id: TaskId,
    pub source_id: String,
    pub title: String,
    pub body: String,
    pub status: TaskStatus,
    pub completed_at: Option<DateTime<Utc>>,
    pub priority: TaskPriority,
    pub due_at: Option<DueDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub source_html_url: Option<Uri>,
    pub tags: Vec<String>,
    pub parent_id: Option<TaskId>,
    pub project: String,
    pub is_recurring: bool,
    pub created_at: DateTime<Utc>,
    pub metadata: TaskMetadata,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
pub struct TaskSummary {
    pub id: TaskId,
    pub source_id: String,
    pub title: String,
    pub body: String,
    pub priority: TaskPriority,
    pub due_at: Option<DueDate>,
    pub tags: Vec<String>,
    pub project: String,
}

impl fmt::Display for TaskSummary {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.title)
    }
}

impl Task {
    pub fn is_in_inbox(&self) -> bool {
        self.project == "Inbox"
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
#[serde(tag = "type", content = "content")]
pub enum TaskMetadata {
    Todoist(TodoistItem),
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Copy, Clone, Eq, Hash)]
#[serde(transparent)]
pub struct TaskId(pub Uuid);

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for TaskId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl From<TaskId> for Uuid {
    fn from(task_id: TaskId) -> Self {
        task_id.0
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
#[serde(tag = "type", content = "content")]
pub enum DueDate {
    Date(NaiveDate),
    DateTime(NaiveDateTime),
    DateTimeWithTz(DateTime<Utc>),
}

impl FromStr for DueDate {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
            return Ok(DueDate::Date(date));
        }

        if let Ok(datetime) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
            return Ok(DueDate::DateTime(datetime));
        }

        if let Ok(datetime) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M") {
            return Ok(DueDate::DateTime(datetime));
        }

        DateTime::parse_from_rfc3339(s)
            .map(|datetime| DueDate::DateTimeWithTz(datetime.with_timezone(&Utc)))
    }
}

impl Display for DueDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            DueDate::Date(date) => date.format("%Y-%m-%d"),
            DueDate::DateTime(datetime) => datetime.format("%Y-%m-%dT%H:%M:%S"),
            DueDate::DateTimeWithTz(datetime) => datetime.format("%Y-%m-%dT%H:%M:%SZ"),
        };
        write!(f, "{}", s)
    }
}

macro_attr! {
    #[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy, Eq, EnumFromStr!, EnumDisplay!)]
    pub enum TaskStatus {
        Active,
        Done,
        Deleted
    }
}

#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Eq)]
pub struct TaskPatch {
    pub status: Option<TaskStatus>,
    pub project: Option<String>,
    pub due_at: Option<Option<DueDate>>,
    pub priority: Option<TaskPriority>,
    pub body: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Eq, Clone)]
pub struct TaskCreation {
    pub title: String,
    pub body: Option<String>,
    pub project: TaskProject,
    pub due_at: Option<DueDate>,
    pub priority: TaskPriority,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct TaskPlanning {
    pub project: TaskProject,
    pub due_at: Option<DueDate>,
    pub priority: TaskPriority,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct TaskProject(String);

impl Display for TaskProject {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for TaskProject {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() {
            Err("Task's project is required".to_string())
        } else if value == "Inbox" {
            Err("Task's project must be moved out of the inbox".to_string())
        } else {
            Ok(TaskProject(value.to_string()))
        }
    }
}

impl Default for TaskProject {
    fn default() -> Self {
        TaskProject("Inbox".to_string())
    }
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

impl FromStr for TaskPriority {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "1" => Ok(TaskPriority::P1),
            "2" => Ok(TaskPriority::P2),
            "3" => Ok(TaskPriority::P3),
            "4" => Ok(TaskPriority::P4),
            _ => Err(format!("Invalid task priority: {s}")),
        }
    }
}

impl Default for TaskPriority {
    fn default() -> Self {
        TaskPriority::P4
    }
}

impl From<Task> for Notification {
    fn from(task: Task) -> Self {
        (&task).into()
    }
}

impl From<&Task> for Notification {
    fn from(task: &Task) -> Self {
        Notification {
            id: Uuid::new_v4().into(),
            title: task.title.clone(),
            source_id: task.source_id.clone(),
            source_html_url: task.source_html_url.clone(),
            status: NotificationStatus::Unread,
            metadata: match task.metadata {
                TaskMetadata::Todoist(_) => NotificationMetadata::Todoist,
            },
            updated_at: task.created_at,
            last_read_at: None,
            snoozed_until: None,
            task_id: Some(task.id),
        }
    }
}

impl From<Task> for NotificationWithTask {
    fn from(task: Task) -> Self {
        (&task).into()
    }
}

impl From<&Task> for NotificationWithTask {
    fn from(task: &Task) -> Self {
        NotificationWithTask {
            id: Uuid::new_v4().into(),
            title: task.title.clone(),
            source_id: task.source_id.clone(),
            source_html_url: task.source_html_url.clone(),
            status: NotificationStatus::Unread,
            metadata: match task.metadata {
                TaskMetadata::Todoist(_) => NotificationMetadata::Todoist,
            },
            updated_at: task.created_at,
            last_read_at: None,
            snoozed_until: None,
            task: Some(task.clone()),
        }
    }
}

#[cfg(test)]
mod tests {

    mod due_date_parsing {
        use super::super::*;
        use pretty_assertions::assert_eq;
        use rstest::*;

        #[rstest]
        fn test_parse_due_date_for_naive_date() {
            assert_eq!(
                "2022-01-02".parse::<DueDate>().unwrap(),
                DueDate::Date(NaiveDate::from_ymd_opt(2022, 1, 2).unwrap())
            );
        }

        #[rstest]
        fn test_parse_due_date_for_naive_datetime() {
            assert_eq!(
                "2022-01-02T11:43:02".parse::<DueDate>().unwrap(),
                DueDate::DateTime(
                    NaiveDate::from_ymd_opt(2022, 1, 2)
                        .unwrap()
                        .and_hms_opt(11, 43, 2)
                        .unwrap()
                )
            );
        }

        #[rstest]
        fn test_parse_due_date_for_naive_datetime_without_seconds() {
            assert_eq!(
                "2022-01-02T11:43".parse::<DueDate>().unwrap(),
                DueDate::DateTime(
                    NaiveDate::from_ymd_opt(2022, 1, 2)
                        .unwrap()
                        .and_hms_opt(11, 43, 0)
                        .unwrap()
                )
            );
        }

        #[rstest]
        fn test_parse_due_date_for_datetime_with_timezone() {
            assert_eq!(
                "2022-01-02T11:43:02.000000Z".parse::<DueDate>().unwrap(),
                DueDate::DateTimeWithTz(DateTime::<Utc>::from_utc(
                    NaiveDate::from_ymd_opt(2022, 1, 2)
                        .unwrap()
                        .and_hms_opt(11, 43, 2)
                        .unwrap(),
                    Utc
                ))
            );
        }

        #[rstest]
        fn test_parse_due_date_for_wrong_date_format() {
            assert_eq!("2022-01-02T".parse::<DueDate>().is_err(), true);
        }
    }

    mod task_priority_parsing {
        use super::super::*;
        use pretty_assertions::assert_eq;
        use rstest::*;

        #[rstest]
        #[case("1", TaskPriority::P1)]
        #[case("2", TaskPriority::P2)]
        #[case("3", TaskPriority::P3)]
        #[case("4", TaskPriority::P4)]
        fn test_parse_task_priority(#[case] string_prio: &str, #[case] priority: TaskPriority) {
            assert_eq!(string_prio.parse::<TaskPriority>().unwrap(), priority);
        }

        #[rstest]
        fn test_parse_due_date_for_wrong_date_format() {
            assert_eq!("5".parse::<TaskPriority>().is_err(), true);
        }
    }
}
