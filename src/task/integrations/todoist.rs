use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use url::Url;

use crate::task::{DueDate, TaskPriority};

pub static DEFAULT_TODOIST_HTML_URL: &str = "https://todoist.com/app/";
pub static TODOIST_INBOX_PROJECT: &str = "Inbox";

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct TodoistItem {
    pub id: String,
    pub parent_id: Option<String>,
    pub project_id: String,
    pub sync_id: Option<String>,
    pub section_id: Option<String>,
    pub content: String,
    pub description: String,
    pub labels: Vec<String>,
    pub child_order: i32,
    pub day_order: Option<i32>,
    pub priority: TodoistItemPriority,
    pub checked: bool, // aka. is_completed
    pub is_deleted: bool,
    pub collapsed: bool,
    pub completed_at: Option<DateTime<Utc>>,
    pub added_at: DateTime<Utc>,
    pub due: Option<TodoistItemDue>,
    pub user_id: String,
    pub added_by_uid: Option<String>,
    pub assigned_by_uid: Option<String>,
    pub responsible_uid: Option<String>,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct TodoistItemDue {
    pub string: String,
    #[serde(with = "due_date_format")]
    pub date: DueDate,
    pub is_recurring: bool,
    pub timezone: Option<String>,
    pub lang: String,
}

impl From<&TodoistItemDue> for DueDate {
    fn from(due: &TodoistItemDue) -> Self {
        due.date.clone()
    }
}

impl From<&DueDate> for TodoistItemDue {
    fn from(due: &DueDate) -> Self {
        Self {
            string: "".to_string(),
            date: due.clone(),
            is_recurring: false, // Not implemented yet
            timezone: None,
            lang: "en".to_string(),
        }
    }
}

#[derive(Serialize_repr, Deserialize_repr, PartialEq, Debug, Clone, Eq, Copy)]
#[repr(u8)]
pub enum TodoistItemPriority {
    P1 = 1,
    P2 = 2,
    P3 = 3,
    P4 = 4,
}

impl From<TodoistItemPriority> for TaskPriority {
    fn from(priority: TodoistItemPriority) -> Self {
        match priority {
            TodoistItemPriority::P1 => TaskPriority::P4,
            TodoistItemPriority::P2 => TaskPriority::P3,
            TodoistItemPriority::P3 => TaskPriority::P2,
            TodoistItemPriority::P4 => TaskPriority::P1,
        }
    }
}

impl From<TaskPriority> for TodoistItemPriority {
    fn from(priority: TaskPriority) -> Self {
        match priority {
            TaskPriority::P1 => TodoistItemPriority::P4,
            TaskPriority::P2 => TodoistItemPriority::P3,
            TaskPriority::P3 => TodoistItemPriority::P2,
            TaskPriority::P4 => TodoistItemPriority::P1,
        }
    }
}

mod due_date_format {
    use serde::{self, Deserialize, Deserializer, Serializer};

    use crate::task::DueDate;

    pub fn serialize<S>(due_date: &DueDate, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&due_date.to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<DueDate, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse::<DueDate>().map_err(serde::de::Error::custom)
    }
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct TodoistProject {
    pub id: String,
    pub name: String,
    pub color: String,
    pub parent_id: Option<String>,
    pub child_order: i32,
    pub collapsed: bool,
    pub shared: bool,
    pub sync_id: Option<String>,
    pub is_deleted: bool,
    pub is_archived: bool,
    pub is_favorite: bool,
    pub view_style: String,
}

pub fn get_task_html_url(item_id: &str) -> Option<Url> {
    format!("https://todoist.com/showTask?id={item_id}")
        .parse::<Url>()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, TimeZone};
    use pretty_assertions::assert_eq;
    use rstest::*;
    use serde_json::json;

    #[rstest]
    fn test_todoist_item_serialization_config() {
        assert_eq!(
            json!(
                {
                    "id": "2995104339",
                    "parent_id": "2995104589",
                    "project_id": "2203306141",
                    "sync_id": "1234567890",
                    "section_id": "7025",
                    "content": "Buy Milk",
                    "description": "",
                    "labels": ["Food", "Shopping"],
                    "child_order": 1,
                    "day_order": -1,
                    "priority": 1,
                    "checked": false,
                    "is_deleted": false,
                    "collapsed": false,
                    "completed_at": null,
                    "added_at": "2019-12-11T22:36:50Z",
                    "due": {
                        "string": "tomorrow at 12",
                        "date": "2016-09-01",
                        "is_recurring": false,
                        "timezone": "Europe/Moscow",
                        "lang": "en"
                    },
                    "user_id": "2671355",
                    "added_by_uid": "2671355",
                    "assigned_by_uid": "2671362",
                    "responsible_uid": "2671355"
                }
            )
            .to_string(),
            serde_json::to_string(&TodoistItem {
                id: "2995104339".to_string(),
                parent_id: Some("2995104589".to_string()),
                project_id: "2203306141".to_string(),
                sync_id: Some("1234567890".to_string()),
                section_id: Some("7025".to_string()),
                content: "Buy Milk".to_string(),
                description: "".to_string(),
                labels: vec!["Food".to_string(), "Shopping".to_string()],
                child_order: 1,
                day_order: Some(-1),
                priority: TodoistItemPriority::P1,
                checked: false,
                is_deleted: false,
                collapsed: false,
                completed_at: None,
                added_at: Utc.with_ymd_and_hms(2019, 12, 11, 22, 36, 50).unwrap(),
                due: Some(TodoistItemDue {
                    date: DueDate::Date(NaiveDate::from_ymd_opt(2016, 9, 1).unwrap()),
                    is_recurring: false,
                    lang: "en".to_string(),
                    string: "tomorrow at 12".to_string(),
                    timezone: Some("Europe/Moscow".to_string()),
                }),
                user_id: "2671355".to_string(),
                added_by_uid: Some("2671355".to_string()),
                assigned_by_uid: Some("2671362".to_string()),
                responsible_uid: Some("2671355".to_string()),
            })
            .unwrap()
        );
    }

    #[rstest]
    fn test_todoist_item_deserialization_config() {
        assert_eq!(
            serde_json::from_str::<TodoistItem>(
                r#"
                {
                    "id": "2995104339",
                    "parent_id": "2995104589",
                    "project_id": "2203306141",
                    "sync_id": "1234567890",
                    "section_id": "7025",
                    "content": "Buy Milk",
                    "description": "",
                    "labels": ["Food", "Shopping"],
                    "child_order": 1,
                    "day_order": -1,
                    "priority": 1,
                    "checked": false,
                    "is_deleted": false,
                    "collapsed": false,
                    "completed_at": null,
                    "added_at": "2019-12-11T22:36:50Z",
                    "due": {
                        "string": "tomorrow at 12",
                        "date": "2016-09-01",
                        "is_recurring": false,
                        "timezone": "Europe/Moscow",
                        "lang": "en"
                    },
                    "user_id": "2671355",
                    "added_by_uid": "2671355",
                    "assigned_by_uid": "2671362",
                    "responsible_uid": "2671355"
                }
            "#
            )
            .unwrap(),
            TodoistItem {
                id: "2995104339".to_string(),
                parent_id: Some("2995104589".to_string()),
                project_id: "2203306141".to_string(),
                sync_id: Some("1234567890".to_string()),
                section_id: Some("7025".to_string()),
                content: "Buy Milk".to_string(),
                description: "".to_string(),
                labels: vec!["Food".to_string(), "Shopping".to_string()],
                child_order: 1,
                day_order: Some(-1),
                priority: TodoistItemPriority::P1,
                checked: false,
                is_deleted: false,
                collapsed: false,
                completed_at: None,
                added_at: Utc.with_ymd_and_hms(2019, 12, 11, 22, 36, 50).unwrap(),
                due: Some(TodoistItemDue {
                    date: DueDate::Date(NaiveDate::from_ymd_opt(2016, 9, 1).unwrap()),
                    is_recurring: false,
                    lang: "en".to_string(),
                    string: "tomorrow at 12".to_string(),
                    timezone: Some("Europe/Moscow".to_string()),
                }),
                user_id: "2671355".to_string(),
                added_by_uid: Some("2671355".to_string()),
                assigned_by_uid: Some("2671362".to_string()),
                responsible_uid: Some("2671355".to_string()),
            }
        );
    }
}
