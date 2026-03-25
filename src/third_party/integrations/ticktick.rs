use anyhow::anyhow;
use chrono::{DateTime, Timelike, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
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

/// A TickTick tag with optional color metadata.
///
/// TickTick's V1 task endpoints return tags as bare strings (`["work", "shopping"]`).
/// Color metadata is fetched separately from the tag listing endpoint and merged in
/// during sync. We accept both shapes on deserialization so existing JSONB rows
/// (string-only tags persisted before this change) keep loading without a migration.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TickTickTag {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

impl TickTickTag {
    pub fn new<S: Into<String>>(name: S) -> Self {
        Self {
            name: name.into(),
            color: None,
        }
    }

    pub fn with_color<S: Into<String>, C: Into<String>>(name: S, color: C) -> Self {
        Self {
            name: name.into(),
            color: Some(color.into()),
        }
    }
}

/// Deserialize a list of tags accepting either bare strings or full
/// `{ "name": ..., "color": ... }` objects. Older payloads/JSONB rows used
/// the bare-string form; we keep reading them so no DB migration is required.
fn deserialize_optional_ticktick_tags<'de, D>(
    deserializer: D,
) -> Result<Option<Vec<TickTickTag>>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum RawTag {
        Name(String),
        Full {
            name: String,
            #[serde(default)]
            color: Option<String>,
        },
    }

    let raw: Option<Vec<RawTag>> = Option::deserialize(deserializer)?;
    Ok(raw.map(|tags| {
        tags.into_iter()
            .map(|raw| match raw {
                RawTag::Name(name) => TickTickTag { name, color: None },
                RawTag::Full { name, color } => TickTickTag { name, color },
            })
            .collect()
    }))
}

/// Serialize tags so that a tag without a color is emitted as a bare string.
/// This keeps outbound API payloads to TickTick (which only accept string tags)
/// in their expected shape, while still letting us round-trip color metadata
/// for tags that have it.
fn serialize_optional_ticktick_tags<S>(
    tags: &Option<Vec<TickTickTag>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    use serde::ser::SerializeSeq;

    match tags {
        None => serializer.serialize_none(),
        Some(tags) => {
            let mut seq = serializer.serialize_seq(Some(tags.len()))?;
            for tag in tags {
                if tag.color.is_some() {
                    seq.serialize_element(tag)?;
                } else {
                    seq.serialize_element(&tag.name)?;
                }
            }
            seq.end()
        }
    }
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
    #[serde(
        default,
        deserialize_with = "deserialize_optional_ticktick_tags",
        serialize_with = "serialize_optional_ticktick_tags"
    )]
    pub tags: Option<Vec<TickTickTag>>,
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

    /// Returns just the tag names, dropping any color metadata.
    /// Used by downstream task-creation paths that expose tags as `Vec<String>`.
    pub fn tag_names(&self) -> Vec<String> {
        self.tags
            .as_ref()
            .map(|tags| tags.iter().map(|t| t.name.clone()).collect())
            .unwrap_or_default()
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
        assert_eq!(ticktick_item.tags, Some(vec![TickTickTag::new("shopping")]));
        assert_eq!(ticktick_item.tag_names(), vec!["shopping".to_string()]);
    }

    #[rstest]
    fn test_ticktick_item_deserializes_tags_with_color() {
        // The /open/v1/tag endpoint returns objects with name + color.
        // We accept that shape directly so sync code can persist enriched tags
        // back into the JSONB-stored TickTickItem.
        let ticktick_item: TickTickItem = serde_json::from_value(json!({
            "id": "task1",
            "projectId": "proj1",
            "title": "Tagged",
            "priority": 0,
            "status": 0,
            "tags": [
                {"name": "shopping", "color": "FF6161"},
                "groceries",
                {"name": "errand"}
            ],
        }))
        .unwrap();

        assert_eq!(
            ticktick_item.tags,
            Some(vec![
                TickTickTag::with_color("shopping", "FF6161"),
                TickTickTag::new("groceries"),
                TickTickTag::new("errand"),
            ])
        );
    }

    #[rstest]
    fn test_ticktick_tag_serialization_preserves_shape() {
        // Bare-name tags serialize as plain strings (so outbound TickTick API
        // payloads stay compatible); colored tags serialize as objects so we
        // can round-trip color through JSONB storage.
        let item = TickTickItem {
            id: "x".to_string(),
            project_id: "p".to_string(),
            title: "t".to_string(),
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
            tags: Some(vec![
                TickTickTag::new("plain"),
                TickTickTag::with_color("colored", "FF6161"),
            ]),
            created_time: None,
            modified_time: None,
        };

        let serialized = serde_json::to_value(&item).unwrap();
        let tags_value = &serialized["tags"];
        assert_eq!(tags_value[0], json!("plain"));
        assert_eq!(tags_value[1], json!({"name": "colored", "color": "FF6161"}));
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
        assert_eq!(ticktick_item.tag_names(), Vec::<String>::new());
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
