use std::{
    fmt::{self, Display},
    str::FromStr,
};

use chrono::{DateTime, NaiveDate, NaiveDateTime, ParseError, Utc};
use clap::ValueEnum;
use http::Uri;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use serde_with::{serde_as, DisplayFromStr};
use uuid::Uuid;

use crate::{
    integration_connection::{IntegrationProvider, IntegrationProviderKind},
    notification::{
        IntoNotification, Notification, NotificationMetadata, NotificationStatus,
        NotificationWithTask,
    },
    task::integrations::todoist::{TodoistItem, DEFAULT_TODOIST_HTML_URL},
    user::UserId,
    HasHtmlUrl,
};

pub mod integrations;
pub mod service;

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
    pub user_id: UserId,
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

#[serde_as]
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
pub struct ProjectSummary {
    pub source_id: String,
    pub name: String,
}

impl fmt::Display for ProjectSummary {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl Task {
    pub fn is_in_inbox(&self) -> bool {
        self.project == "Inbox"
    }

    pub fn get_task_source_kind(&self) -> TaskSourceKind {
        match self.metadata {
            TaskMetadata::Todoist(_) => TaskSourceKind::Todoist,
        }
    }
}

impl HasHtmlUrl for Task {
    fn get_html_url(&self) -> Uri {
        self.source_html_url
            .clone()
            .unwrap_or_else(|| match self.metadata {
                TaskMetadata::Todoist(_) => DEFAULT_TODOIST_HTML_URL.parse::<Uri>().unwrap(),
            })
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
        write!(f, "{s}")
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

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct TaskCreation {
    pub title: String,
    pub body: Option<String>,
    pub project: ProjectSummary,
    pub due_at: Option<DueDate>,
    pub priority: TaskPriority,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct TaskPlanning {
    pub project: ProjectSummary,
    pub due_at: Option<DueDate>,
    pub priority: TaskPriority,
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
    Default,
)]
#[repr(u8)]
pub enum TaskPriority {
    P1 = 1,
    P2 = 2,
    P3 = 3,
    #[default]
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

impl IntoNotification for Task {
    fn into_notification(self, user_id: UserId) -> Notification {
        Notification {
            id: Uuid::new_v4().into(),
            title: self.title.clone(),
            source_id: self.source_id.clone(),
            source_html_url: self.source_html_url.clone(),
            status: if self.status != TaskStatus::Active {
                NotificationStatus::Deleted
            } else {
                NotificationStatus::Unread
            },
            metadata: match self.metadata {
                TaskMetadata::Todoist(_) => NotificationMetadata::Todoist,
            },
            updated_at: self.created_at,
            last_read_at: None,
            snoozed_until: None,
            user_id,
            task_id: Some(self.id),
        }
    }
}

impl From<Task> for Notification {
    fn from(task: Task) -> Self {
        let user_id = task.user_id;
        task.into_notification(user_id)
    }
}

impl From<Task> for NotificationWithTask {
    fn from(task: Task) -> Self {
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
            user_id: task.user_id,
            task: Some(task),
        }
    }
}

macro_attr! {
    // Synchronization sources for tasks
    #[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, EnumFromStr!, EnumDisplay!)]
    pub enum TaskSyncSourceKind {
        Todoist
    }
}

macro_attr! {
    #[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, EnumFromStr!, EnumDisplay!)]
    pub enum TaskSourceKind {
        Todoist
    }
}

impl TryFrom<IntegrationProviderKind> for TaskSyncSourceKind {
    type Error = ();

    fn try_from(provider_kind: IntegrationProviderKind) -> Result<Self, Self::Error> {
        match provider_kind {
            IntegrationProviderKind::Todoist => Ok(Self::Todoist),
            _ => Err(()),
        }
    }
}

pub trait TaskSource: IntegrationProvider {
    fn get_task_source_kind(&self) -> TaskSourceKind;
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
