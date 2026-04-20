use anyhow::anyhow;
use chrono::{DateTime, Timelike, Utc};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use url::Url;
use uuid::Uuid;

use crate::{
    HasHtmlUrl,
    integration_connection::IntegrationConnectionId,
    task::{DueDate, TaskPriority},
    third_party::item::{ThirdPartyItem, ThirdPartyItemData, ThirdPartyItemFromSource},
    user::UserId,
};

pub static DEFAULT_TICKTICK_HTML_URL: &str = "https://ticktick.com/webapp/";
pub static TICKTICK_INBOX_PROJECT: &str = "Inbox";

/// TickTick priority levels.
/// TickTick uses: 0 = None, 1 = Low, 3 = Medium, 5 = High
#[derive(Serialize_repr, Deserialize_repr, PartialEq, Debug, Clone, Eq, Copy)]
#[repr(u8)]
pub enum TickTickItemPriority {
    None = 0,
    Low = 1,
    Medium = 3,
    High = 5,
}

/// Mapping: TickTick High(5) → UI P1 (highest), TickTick None(0) → UI P4 (lowest)
impl From<TickTickItemPriority> for TaskPriority {
    fn from(priority: TickTickItemPriority) -> Self {
        match priority {
            TickTickItemPriority::High => TaskPriority::P1,
            TickTickItemPriority::Medium => TaskPriority::P2,
            TickTickItemPriority::Low => TaskPriority::P3,
            TickTickItemPriority::None => TaskPriority::P4,
        }
    }
}

impl From<TaskPriority> for TickTickItemPriority {
    fn from(priority: TaskPriority) -> Self {
        match priority {
            TaskPriority::P1 => TickTickItemPriority::High,
            TaskPriority::P2 => TickTickItemPriority::Medium,
            TaskPriority::P3 => TickTickItemPriority::Low,
            TaskPriority::P4 => TickTickItemPriority::None,
        }
    }
}

/// TickTick task status: 0 = Normal (active), 2 = Completed
#[derive(Serialize_repr, Deserialize_repr, PartialEq, Debug, Clone, Eq, Copy)]
#[repr(u8)]
pub enum TickTickTaskStatus {
    Normal = 0,
    Completed = 2,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TickTickChecklistItem {
    pub id: String,
    pub title: String,
    pub status: TickTickTaskStatus,
    pub sort_order: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TickTickItem {
    pub id: String,
    pub project_id: String,
    pub title: String,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub desc: Option<String>,
    #[serde(default)]
    pub all_day: Option<bool>,
    #[serde(default)]
    pub start_date: Option<DateTime<Utc>>,
    #[serde(default)]
    pub due_date: Option<DateTime<Utc>>,
    #[serde(default)]
    pub time_zone: Option<String>,
    #[serde(default)]
    pub reminders: Option<Vec<String>>,
    #[serde(default)]
    pub repeat: Option<String>,
    pub priority: TickTickItemPriority,
    pub status: TickTickTaskStatus,
    #[serde(default)]
    pub completed_time: Option<DateTime<Utc>>,
    #[serde(default)]
    pub sort_order: Option<i64>,
    #[serde(default)]
    pub items: Option<Vec<TickTickChecklistItem>>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub created_time: Option<DateTime<Utc>>,
    #[serde(default)]
    pub modified_time: Option<DateTime<Utc>>,
}

impl HasHtmlUrl for TickTickItem {
    fn get_html_url(&self) -> Url {
        format!(
            "{}#p/{}/tasks/{}",
            DEFAULT_TICKTICK_HTML_URL, self.project_id, self.id
        )
        .parse::<Url>()
        .unwrap()
    }
}

impl TickTickItem {
    pub fn is_completed(&self) -> bool {
        self.status == TickTickTaskStatus::Completed
    }

    pub fn is_recurring(&self) -> bool {
        self.repeat.is_some()
    }

    pub fn get_due_date(&self) -> Option<DueDate> {
        self.due_date.map(DueDate::DateTimeWithTz)
    }
}

impl TryFrom<ThirdPartyItem> for TickTickItem {
    type Error = anyhow::Error;

    fn try_from(item: ThirdPartyItem) -> Result<Self, Self::Error> {
        match item.data {
            ThirdPartyItemData::TickTickItem(ticktick_item) => Ok(*ticktick_item),
            _ => Err(anyhow!(
                "Unable to convert ThirdPartyItem {} to TickTickItem",
                item.id
            )),
        }
    }
}

impl ThirdPartyItemFromSource for TickTickItem {
    fn into_third_party_item(
        self,
        user_id: UserId,
        integration_connection_id: IntegrationConnectionId,
    ) -> ThirdPartyItem {
        ThirdPartyItem {
            id: Uuid::new_v4().into(),
            source_id: self.source_id(),
            data: ThirdPartyItemData::TickTickItem(Box::new(self.clone())),
            created_at: Utc::now().with_nanosecond(0).unwrap(),
            updated_at: Utc::now().with_nanosecond(0).unwrap(),
            user_id,
            integration_connection_id,
            source_item: None,
        }
    }

    fn source_id(&self) -> String {
        self.id.clone()
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TickTickProject {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub group_id: Option<String>,
    #[serde(default)]
    pub sort_order: Option<i64>,
    #[serde(default)]
    pub view_mode: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use rstest::*;
    use serde_json::json;

    #[rstest]
    fn test_ticktick_item_deserialization() {
        let ticktick_item: TickTickItem = serde_json::from_value(json!({
            "id": "6748a1c38f08de2a2f6e1234",
            "projectId": "6748a1c38f08de2a2f6e5678",
            "title": "Buy groceries",
            "content": "Get milk and eggs",
            "desc": "",
            "allDay": true,
            "startDate": "2024-01-15T00:00:00.000+0000",
            "dueDate": "2024-01-16T00:00:00.000+0000",
            "timeZone": "America/Los_Angeles",
            "priority": 3,
            "status": 0,
            "sortOrder": -1099511627776_i64,
            "tags": ["shopping"],
            "createdTime": "2024-01-10T10:30:00.000+0000",
            "modifiedTime": "2024-01-12T14:20:00.000+0000"
        }))
        .unwrap();

        assert_eq!(ticktick_item.id, "6748a1c38f08de2a2f6e1234");
        assert_eq!(ticktick_item.project_id, "6748a1c38f08de2a2f6e5678");
        assert_eq!(ticktick_item.title, "Buy groceries");
        assert_eq!(ticktick_item.content, Some("Get milk and eggs".to_string()));
        assert_eq!(ticktick_item.priority, TickTickItemPriority::Medium);
        assert_eq!(ticktick_item.status, TickTickTaskStatus::Normal);
        assert_eq!(ticktick_item.tags, Some(vec!["shopping".to_string()]));
    }

    #[rstest]
    fn test_ticktick_item_minimal_deserialization() {
        // TickTick API may return minimal fields
        let ticktick_item: TickTickItem = serde_json::from_value(json!({
            "id": "abc123",
            "projectId": "proj456",
            "title": "Simple task",
            "priority": 0,
            "status": 0
        }))
        .unwrap();

        assert_eq!(ticktick_item.id, "abc123");
        assert_eq!(ticktick_item.title, "Simple task");
        assert_eq!(ticktick_item.priority, TickTickItemPriority::None);
        assert_eq!(ticktick_item.content, None);
        assert_eq!(ticktick_item.due_date, None);
        assert_eq!(ticktick_item.tags, None);
    }

    #[rstest]
    fn test_ticktick_item_completed_deserialization() {
        let ticktick_item: TickTickItem = serde_json::from_value(json!({
            "id": "completed_task",
            "projectId": "proj789",
            "title": "Done task",
            "priority": 5,
            "status": 2,
            "completedTime": "2024-01-15T10:00:00.000+0000"
        }))
        .unwrap();

        assert_eq!(ticktick_item.status, TickTickTaskStatus::Completed);
        assert!(ticktick_item.is_completed());
        assert_eq!(ticktick_item.priority, TickTickItemPriority::High);
    }

    #[rstest]
    fn test_ticktick_priority_to_task_priority() {
        assert_eq!(
            TaskPriority::from(TickTickItemPriority::High),
            TaskPriority::P1
        );
        assert_eq!(
            TaskPriority::from(TickTickItemPriority::Medium),
            TaskPriority::P2
        );
        assert_eq!(
            TaskPriority::from(TickTickItemPriority::Low),
            TaskPriority::P3
        );
        assert_eq!(
            TaskPriority::from(TickTickItemPriority::None),
            TaskPriority::P4
        );
    }

    #[rstest]
    fn test_task_priority_to_ticktick_priority() {
        assert_eq!(
            TickTickItemPriority::from(TaskPriority::P1),
            TickTickItemPriority::High
        );
        assert_eq!(
            TickTickItemPriority::from(TaskPriority::P2),
            TickTickItemPriority::Medium
        );
        assert_eq!(
            TickTickItemPriority::from(TaskPriority::P3),
            TickTickItemPriority::Low
        );
        assert_eq!(
            TickTickItemPriority::from(TaskPriority::P4),
            TickTickItemPriority::None
        );
    }

    #[rstest]
    fn test_ticktick_project_deserialization() {
        let project: TickTickProject = serde_json::from_value(json!({
            "id": "inbox123",
            "name": "Inbox",
            "color": "#4772FA",
            "sortOrder": 0,
            "viewMode": "list"
        }))
        .unwrap();

        assert_eq!(project.id, "inbox123");
        assert_eq!(project.name, "Inbox");
        assert_eq!(project.color, Some("#4772FA".to_string()));
    }

    #[rstest]
    fn test_ticktick_item_html_url() {
        let item = TickTickItem {
            id: "task123".to_string(),
            project_id: "proj456".to_string(),
            title: "Test".to_string(),
            content: None,
            desc: None,
            all_day: None,
            start_date: None,
            due_date: None,
            time_zone: None,
            reminders: None,
            repeat: None,
            priority: TickTickItemPriority::None,
            status: TickTickTaskStatus::Normal,
            completed_time: None,
            sort_order: None,
            items: None,
            tags: None,
            created_time: None,
            modified_time: None,
        };

        assert_eq!(
            item.get_html_url().to_string(),
            "https://ticktick.com/webapp/#p/proj456/tasks/task123"
        );
    }
}
