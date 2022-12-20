use std::{
    fmt::{self, Display},
    str::FromStr,
};

use chrono::{DateTime, NaiveDate, NaiveDateTime, ParseError, Utc};
use http::Uri;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use serde_with::{serde_as, DisplayFromStr};
use uuid::Uuid;

use crate::notification::{Notification, NotificationMetadata, NotificationStatus};

use self::integrations::todoist::TodoistItem;

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
    pub due_at: Option<DueDate>,
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

impl From<Task> for Notification {
    fn from(task: Task) -> Self {
        (&task).into()
    }
}

impl From<&Task> for Notification {
    fn from(task: &Task) -> Self {
        Notification {
            id: Uuid::new_v4(),
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
            task_source_id: Some(task.source_id.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
