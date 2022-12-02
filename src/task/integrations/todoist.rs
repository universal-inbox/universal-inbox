use chrono::{DateTime, NaiveDate, Utc};
use http::Uri;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use serde_with::{serde_as, DisplayFromStr};

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct TodoistTask {
    pub id: String,
    pub project_id: String,
    pub section_id: Option<String>,
    pub content: String,
    pub description: String,
    pub is_completed: bool,
    pub labels: Vec<String>,
    pub parent_id: Option<String>,
    pub order: u32,
    pub priority: TodoistTaskPriority,
    pub due: Option<TodoistTaskDue>,
    #[serde_as(as = "DisplayFromStr")]
    pub url: Uri,
    pub comment_count: u32,
    pub created_at: DateTime<Utc>,
    pub creator_id: String,
    pub assignee_id: Option<String>,
    pub assigner_id: Option<String>,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct TodoistTaskDue {
    pub string: String,
    #[serde(with = "date_format")]
    pub date: NaiveDate,
    pub is_recurring: bool,
    pub datetime: Option<DateTime<Utc>>,
    pub timezone: Option<String>,
}

#[derive(Serialize_repr, Deserialize_repr, PartialEq, Debug, Clone, Eq, Copy)]
#[repr(u8)]
pub enum TodoistTaskPriority {
    P1 = 1,
    P2 = 2,
    P3 = 3,
    P4 = 4,
}

mod date_format {
    use chrono::NaiveDate;
    use serde::{self, Deserialize, Deserializer, Serializer};

    const FORMAT: &str = "%Y-%m-%d";

    pub fn serialize<S>(date: &NaiveDate, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = format!("{}", date.format(FORMAT));
        serializer.serialize_str(&s)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<NaiveDate, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse::<NaiveDate>().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use rstest::*;
    use serde_json::json;

    #[rstest]
    fn test_todoist_task_serialization_config() {
        assert_eq!(
            json!(
                {
                    "id": "2995104339",
                    "project_id": "2203306141",
                    "section_id": "7025",
                    "content": "Buy Milk",
                    "description": "",
                    "is_completed": false,
                    "labels": ["Food", "Shopping"],
                    "parent_id": "2995104589",
                    "order": 1,
                    "priority": 1,
                    "due": {
                        "string": "tomorrow at 12",
                        "date": "2016-09-01",
                        "is_recurring": false,
                        "datetime": "2016-09-01T12:00:00Z",
                        "timezone": "Europe/Moscow"
                    },
                    "url": "https://todoist.com/showTask?id=2995104339",
                    "comment_count": 10,
                    "created_at": "2019-12-11T22:36:50Z",
                    "creator_id": "2671355",
                    "assignee_id": "2671362",
                    "assigner_id": "2671355",
                }
            )
            .to_string(),
            serde_json::to_string(&TodoistTask {
                creator_id: "2671355".to_string(),
                created_at: Utc.with_ymd_and_hms(2019, 12, 11, 22, 36, 50).unwrap(),
                assignee_id: Some("2671362".to_string()),
                assigner_id: Some("2671355".to_string()),
                comment_count: 10,
                is_completed: false,
                content: "Buy Milk".to_string(),
                description: "".to_string(),
                due: Some(TodoistTaskDue {
                    date: NaiveDate::from_ymd_opt(2016, 9, 1).unwrap(),
                    is_recurring: false,
                    datetime: Some(Utc.with_ymd_and_hms(2016, 9, 1, 12, 0, 0).unwrap()),
                    string: "tomorrow at 12".to_string(),
                    timezone: Some("Europe/Moscow".to_string()),
                }),
                id: "2995104339".to_string(),
                labels: vec!["Food".to_string(), "Shopping".to_string()],
                order: 1,
                priority: TodoistTaskPriority::P1,
                project_id: "2203306141".to_string(),
                section_id: Some("7025".to_string()),
                parent_id: Some("2995104589".to_string()),
                url: "https://todoist.com/showTask?id=2995104339"
                    .parse::<Uri>()
                    .unwrap()
            })
            .unwrap()
        );
    }

    #[rstest]
    fn test_todoist_task_deserialization_config() {
        assert_eq!(
            serde_json::from_str::<TodoistTask>(
                r#"
                {
                    "creator_id": "2671355",
                    "created_at": "2019-12-11T22:36:50.000000Z",
                    "assignee_id": "2671362",
                    "assigner_id": "2671355",
                    "comment_count": 10,
                    "is_completed": false,
                    "content": "Buy Milk",
                    "description": "",
                    "due": {
                        "date": "2016-09-01",
                        "is_recurring": false,
                        "datetime": "2016-09-01T12:00:00.000000Z",
                        "string": "tomorrow at 12",
                        "timezone": "Europe/Moscow"
                    },
                    "id": "2995104339",
                    "labels": ["Food", "Shopping"],
                    "order": 1,
                    "priority": 1,
                    "project_id": "2203306141",
                    "section_id": "7025",
                    "parent_id": "2995104589",
                    "url": "https://todoist.com/showTask?id=2995104339"
                }
            "#
            )
            .unwrap(),
            TodoistTask {
                creator_id: "2671355".to_string(),
                created_at: Utc.with_ymd_and_hms(2019, 12, 11, 22, 36, 50).unwrap(),
                assignee_id: Some("2671362".to_string()),
                assigner_id: Some("2671355".to_string()),
                comment_count: 10,
                is_completed: false,
                content: "Buy Milk".to_string(),
                description: "".to_string(),
                due: Some(TodoistTaskDue {
                    date: NaiveDate::from_ymd_opt(2016, 9, 1).unwrap(),
                    is_recurring: false,
                    datetime: Some(Utc.with_ymd_and_hms(2016, 9, 1, 12, 0, 0).unwrap()),
                    string: "tomorrow at 12".to_string(),
                    timezone: Some("Europe/Moscow".to_string())
                }),
                id: "2995104339".to_string(),
                labels: vec!["Food".to_string(), "Shopping".to_string()],
                order: 1,
                priority: TodoistTaskPriority::P1,
                project_id: "2203306141".to_string(),
                section_id: Some("7025".to_string()),
                parent_id: Some("2995104589".to_string()),
                url: "https://todoist.com/showTask?id=2995104339"
                    .parse::<Uri>()
                    .unwrap()
            }
        );
    }
}
