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

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct TodoistItem {
    pub id: String,
    pub parent_id: Option<String>,
    pub project_id: String,
    pub sync_id: Option<String>,
    pub section_id: Option<String>,
    pub content: String,
    pub description: String,
    #[serde(deserialize_with = "labels_format::deserialize")]
    pub labels: Vec<TodoistLabel>,
    pub child_order: i32,
    pub day_order: Option<i32>,
    pub priority: TodoistItemPriority,
    pub checked: bool, // aka. is_completed
    pub is_deleted: bool,
    #[serde(alias = "is_collapsed")]
    pub collapsed: bool,
    pub completed_at: Option<DateTime<Utc>>,
    pub added_at: DateTime<Utc>,
    pub due: Option<TodoistItemDue>,
    pub user_id: String,
    pub added_by_uid: Option<String>,
    pub assigned_by_uid: Option<String>,
    pub responsible_uid: Option<String>,
}

impl HasHtmlUrl for TodoistItem {
    fn get_html_url(&self) -> Url {
        format!("https://todoist.com/showTask?id={}", self.id)
            .parse::<Url>()
            .unwrap()
    }
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

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct TodoistLabel {
    pub name: String,
    #[serde(default)]
    pub color: TodoistColor,
}

impl TodoistLabel {
    pub fn from_name(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            color: TodoistColor::default(),
        }
    }
}

/// Todoist's standard label/project color palette.
///
/// See https://developer.todoist.com/api/v1#tag/Colors/Colors for the canonical list.
/// The `Charcoal` variant is used as the default fallback when a label has not been
/// hydrated with its color (e.g. items deserialized from older persisted JSON, or
/// from the Sync API where item labels are returned as bare strings).
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
pub enum TodoistColor {
    #[serde(alias = "berry_red")]
    BerryRed,
    Red,
    Orange,
    Yellow,
    OliveGreen,
    LimeGreen,
    Green,
    MintGreen,
    Teal,
    SkyBlue,
    LightBlue,
    Blue,
    Grape,
    Violet,
    Lavender,
    Magenta,
    Salmon,
    #[default]
    Charcoal,
    Grey,
    Taupe,
    /// Fallback for unknown / future colors returned by the Todoist API.
    #[serde(other)]
    Unknown,
}

impl TodoistColor {
    /// Hex color (no leading `#`) used by Todoist's web client for each label color.
    pub fn to_hex(&self) -> &'static str {
        match self {
            TodoistColor::BerryRed => "b8255f",
            TodoistColor::Red => "db4035",
            TodoistColor::Orange => "ff9933",
            TodoistColor::Yellow => "fad000",
            TodoistColor::OliveGreen => "afb83b",
            TodoistColor::LimeGreen => "7ecc49",
            TodoistColor::Green => "299438",
            TodoistColor::MintGreen => "6accbc",
            TodoistColor::Teal => "158fad",
            TodoistColor::SkyBlue => "14aaf5",
            TodoistColor::LightBlue => "96c3eb",
            TodoistColor::Blue => "4073ff",
            TodoistColor::Grape => "884dff",
            TodoistColor::Violet => "af38eb",
            TodoistColor::Lavender => "eb96eb",
            TodoistColor::Magenta => "e05194",
            TodoistColor::Salmon => "ff8d85",
            TodoistColor::Charcoal => "808080",
            TodoistColor::Grey => "b8b8b8",
            TodoistColor::Taupe => "ccac93",
            TodoistColor::Unknown => "808080",
        }
    }
}

impl TryFrom<ThirdPartyItem> for TodoistItem {
    type Error = anyhow::Error;

    fn try_from(item: ThirdPartyItem) -> Result<Self, Self::Error> {
        match item.data {
            ThirdPartyItemData::TodoistItem(todoist_item) => Ok(*todoist_item),
            _ => Err(anyhow!(
                "Unable to convert ThirdPartyItem {} to TodoistItem",
                item.id
            )),
        }
    }
}

impl ThirdPartyItemFromSource for TodoistItem {
    fn into_third_party_item(
        self,
        user_id: UserId,
        integration_connection_id: IntegrationConnectionId,
    ) -> ThirdPartyItem {
        ThirdPartyItem {
            id: Uuid::new_v4().into(),
            source_id: self.source_id(),
            data: ThirdPartyItemData::TodoistItem(Box::new(self.clone())),
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

mod due_date_format {
    use super::*;

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

/// Deserializes a `Vec<TodoistLabel>` from either:
///   * An array of bare label-name strings (e.g. `["Food", "Shopping"]`) — this is
///     the shape returned by Todoist's Sync API on items, where labels are referenced
///     by name only and need to be hydrated separately to obtain their color.
///   * An array of label objects (e.g. `[{"name": "Food", "color": "red"}]`) — the
///     hydrated shape we persist in `third_party_item.data` JSONB after enriching
///     with the labels endpoint.
///
/// When a bare string is encountered, the resulting `TodoistLabel` falls back to
/// [`TodoistColor::default`] (`Charcoal`); the sync code is responsible for filling
/// in the proper color before persistence.
mod labels_format {
    use super::*;
    use serde::de::{self, SeqAccess, Visitor};
    use std::fmt;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<TodoistLabel>, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct LabelsVisitor;

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum LabelOrName {
            Name(String),
            Label(TodoistLabel),
        }

        impl<'de> Visitor<'de> for LabelsVisitor {
            type Value = Vec<TodoistLabel>;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a sequence of label names or label objects")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut out = Vec::with_capacity(seq.size_hint().unwrap_or(0));
                while let Some(value) = seq.next_element::<LabelOrName>()? {
                    out.push(match value {
                        LabelOrName::Name(name) => TodoistLabel::from_name(name),
                        LabelOrName::Label(label) => label,
                    });
                }
                Ok(out)
            }
        }

        deserializer
            .deserialize_seq(LabelsVisitor)
            .map_err(de::Error::custom)
    }
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
                    "labels": [
                        { "name": "Food", "color": "red" },
                        { "name": "Shopping", "color": "blue" },
                    ],
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
                labels: vec![
                    TodoistLabel {
                        name: "Food".to_string(),
                        color: TodoistColor::Red,
                    },
                    TodoistLabel {
                        name: "Shopping".to_string(),
                        color: TodoistColor::Blue,
                    },
                ],
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
                    "labels": [
                        { "name": "Food", "color": "red" },
                        { "name": "Shopping", "color": "blue" }
                    ],
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
                labels: vec![
                    TodoistLabel {
                        name: "Food".to_string(),
                        color: TodoistColor::Red,
                    },
                    TodoistLabel {
                        name: "Shopping".to_string(),
                        color: TodoistColor::Blue,
                    },
                ],
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

    /// Backwards compatibility: previously-persisted items (and the Todoist Sync
    /// API on items) represent labels as bare strings. Deserialization must accept
    /// that shape and fall back to the default color so older rows continue to load.
    #[rstest]
    fn test_todoist_item_deserialization_legacy_string_labels() {
        let item: TodoistItem = serde_json::from_str(
            r#"
            {
                "id": "2995104339",
                "parent_id": null,
                "project_id": "2203306141",
                "sync_id": null,
                "section_id": null,
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
                "due": null,
                "user_id": "2671355",
                "added_by_uid": null,
                "assigned_by_uid": null,
                "responsible_uid": null
            }
        "#,
        )
        .unwrap();

        assert_eq!(
            item.labels,
            vec![
                TodoistLabel {
                    name: "Food".to_string(),
                    color: TodoistColor::Charcoal,
                },
                TodoistLabel {
                    name: "Shopping".to_string(),
                    color: TodoistColor::Charcoal,
                },
            ]
        );
    }

    #[rstest]
    fn test_todoist_color_to_hex() {
        assert_eq!(TodoistColor::Red.to_hex(), "db4035");
        assert_eq!(TodoistColor::Blue.to_hex(), "4073ff");
        assert_eq!(TodoistColor::Charcoal.to_hex(), "808080");
    }

    #[rstest]
    fn test_todoist_color_unknown_color_falls_back() {
        // Future / unknown colors must not break deserialization — they map to `Unknown`.
        let label: TodoistLabel =
            serde_json::from_str(r#"{ "name": "ABC", "color": "fuchsia_glow" }"#).unwrap();
        assert_eq!(label.color, TodoistColor::Unknown);
        assert_eq!(label.color.to_hex(), "808080");
    }
}
