use std::{
    fmt::{self, Display},
    str::FromStr,
};

use anyhow::anyhow;
use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, ParseError, TimeDelta, Utc};
use clap::ValueEnum;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use serde_with::serde_as;
use url::Url;
use uuid::Uuid;

use crate::{
    integration_connection::provider::{IntegrationProviderKind, IntegrationProviderSource},
    notification::Notification,
    task::integrations::todoist::{DEFAULT_TODOIST_HTML_URL, TODOIST_INBOX_PROJECT},
    third_party::item::{
        ThirdPartyItem, ThirdPartyItemData, ThirdPartyItemSource, ThirdPartyItemSourceKind,
    },
    user::UserId,
    utils::default_value::DefaultValue,
    HasHtmlUrl,
};

pub mod integrations;
pub mod service;

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Task {
    pub id: TaskId,
    pub title: String,
    pub body: String,
    pub status: TaskStatus,
    pub completed_at: Option<DateTime<Utc>>,
    pub priority: TaskPriority,
    pub due_at: Option<DueDate>,
    pub tags: Vec<String>,
    pub parent_id: Option<TaskId>,
    pub project: String,
    pub is_recurring: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub kind: TaskSourceKind,
    pub source_item: ThirdPartyItem,
    pub sink_item: Option<ThirdPartyItem>,
    pub user_id: UserId,
}

impl PartialEq for Task {
    fn eq(&self, other: &Self) -> bool {
        self.title == other.title
            && self.body == other.body
            && self.status == other.status
            && self.priority == other.priority
            && self.due_at == other.due_at
            && self.tags == other.tags
            && self.parent_id == other.parent_id
            && self.project == other.project
            && self.is_recurring == other.is_recurring
            && self.kind == other.kind
            && self.source_item.id == other.source_item.id
            && self.sink_item.as_ref().map(|tpi| tpi.id)
                == other.sink_item.as_ref().map(|tpi| tpi.id)
            && self.user_id == other.user_id
    }
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
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
    pub source_id: ProjectId,
    pub name: String,
}

impl fmt::Display for ProjectSummary {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, Hash)]
#[serde(transparent)]
pub struct ProjectId(pub String);

impl fmt::Display for ProjectId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for ProjectId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for ProjectId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<ProjectId> for String {
    fn from(project_id: ProjectId) -> Self {
        project_id.0
    }
}

impl Task {
    pub fn is_in_inbox(&self) -> bool {
        match self.kind {
            TaskSourceKind::Todoist => self.project == TODOIST_INBOX_PROJECT,
            _ => match &self.sink_item {
                Some(sink_item) => match sink_item.get_third_party_item_source_kind() {
                    ThirdPartyItemSourceKind::Todoist => self.project == TODOIST_INBOX_PROJECT,
                    _ => false,
                },
                _ => false,
            },
        }
    }

    pub fn get_html_project_url(&self) -> Url {
        let Some(sink_item) = &self.sink_item else {
            return DEFAULT_TODOIST_HTML_URL.parse::<Url>().unwrap();
        };
        match &sink_item.data {
            ThirdPartyItemData::TodoistItem(todoist_task) => format!(
                "{DEFAULT_TODOIST_HTML_URL}project/{}",
                todoist_task.project_id
            )
            .parse::<Url>()
            .unwrap(),
            _ => DEFAULT_TODOIST_HTML_URL.parse::<Url>().unwrap(),
        }
    }
}

impl HasHtmlUrl for Task {
    fn get_html_url(&self) -> Url {
        self.source_item.get_html_url()
    }
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

impl DueDate {
    pub fn display_date(&self) -> String {
        match self {
            DueDate::Date(date) => date.format("%Y-%m-%d").to_string(),
            DueDate::DateTime(datetime) => datetime.format("%Y-%m-%d").to_string(),
            DueDate::DateTimeWithTz(datetime) => datetime.format("%Y-%m-%d").to_string(),
        }
    }
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
    #[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, EnumFromStr!, EnumDisplay!)]
    #[serde(tag = "type", content = "content")]
    pub enum PresetDueDate {
        Today,
        Tomorrow,
        ThisWeekend,
        NextWeek,
    }
}

impl DueDate {
    pub fn from_preset(current_date: NaiveDate, preset: PresetDueDate) -> Self {
        match preset {
            PresetDueDate::Today => DueDate::Date(current_date),
            PresetDueDate::Tomorrow => {
                DueDate::Date(current_date + TimeDelta::try_days(1).unwrap())
            }
            PresetDueDate::ThisWeekend => {
                let today = current_date;
                let days_until_saturday = if today.weekday().num_days_from_monday() == 5 {
                    7
                } else {
                    5 - today.weekday().num_days_from_monday()
                };
                let next_saturday =
                    today + TimeDelta::try_days(days_until_saturday as i64).unwrap();
                DueDate::Date(next_saturday)
            }
            PresetDueDate::NextWeek => {
                let today = current_date;
                let days_until_next_monday = 7 - today.weekday().num_days_from_monday();
                let next_monday =
                    today + TimeDelta::try_days(days_until_next_monday as i64).unwrap();
                DueDate::Date(next_monday)
            }
        }
    }
}

impl From<PresetDueDate> for DueDate {
    fn from(preset: PresetDueDate) -> Self {
        DueDate::from_preset(Utc::now().naive_utc().date(), preset)
    }
}

impl From<NaiveDate> for DueDate {
    fn from(date: NaiveDate) -> Self {
        DueDate::Date(date)
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
    pub project_name: Option<String>,
    pub due_at: Option<DueDate>,
    pub priority: TaskPriority,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct TaskCreationConfig {
    pub project_name: Option<String>,
    pub due_at: Option<DueDate>,
    pub priority: TaskPriority,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct CreateOrUpdateTaskRequest {
    pub id: TaskId,
    pub title: String,
    pub body: String,
    pub status: TaskStatus,
    pub completed_at: Option<DateTime<Utc>>,
    pub priority: TaskPriority,
    pub due_at: DefaultValue<Option<DueDate>>,
    pub tags: Vec<String>,
    pub parent_id: Option<TaskId>,
    pub project: DefaultValue<String>,
    pub is_recurring: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub kind: TaskSourceKind,
    pub source_item: ThirdPartyItem,
    pub sink_item: Option<ThirdPartyItem>,
    pub user_id: UserId,
}

impl From<CreateOrUpdateTaskRequest> for Task {
    fn from(request: CreateOrUpdateTaskRequest) -> Self {
        Task {
            id: request.id,
            title: request.title,
            body: request.body,
            status: request.status,
            completed_at: request.completed_at,
            priority: request.priority,
            due_at: request.due_at.into_value(),
            tags: request.tags,
            parent_id: request.parent_id,
            project: request.project.into_value(),
            is_recurring: request.is_recurring,
            created_at: request.created_at,
            updated_at: request.updated_at,
            kind: request.kind,
            source_item: request.source_item,
            sink_item: request.sink_item,
            user_id: request.user_id,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct TaskPlanning {
    pub project_name: String,
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
    Ord,
    PartialOrd,
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

impl Display for TaskPriority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", (*self as i32))
    }
}

macro_attr! {
    // Synchronization sources for tasks
    #[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, EnumFromStr!, EnumDisplay!)]
    pub enum TaskSyncSourceKind {
        Todoist,
        Linear
    }
}

macro_attr! {
    #[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, EnumFromStr!, EnumDisplay!)]
    pub enum TaskSourceKind {
        Todoist,
        Slack,
        Linear
    }
}

impl TryFrom<ThirdPartyItemSourceKind> for TaskSourceKind {
    type Error = anyhow::Error;

    fn try_from(source_kind: ThirdPartyItemSourceKind) -> Result<Self, Self::Error> {
        match source_kind {
            ThirdPartyItemSourceKind::Todoist => Ok(Self::Todoist),
            ThirdPartyItemSourceKind::LinearIssue => Ok(Self::Linear),
            ThirdPartyItemSourceKind::SlackReaction | ThirdPartyItemSourceKind::SlackStar => {
                Ok(Self::Slack)
            }
            _ => Err(anyhow!(
                "ThirdPartyItemSourceKind {source_kind} is not a valid TaskSourceKind"
            )),
        }
    }
}

impl TryFrom<IntegrationProviderKind> for TaskSyncSourceKind {
    type Error = anyhow::Error;

    fn try_from(provider_kind: IntegrationProviderKind) -> Result<Self, Self::Error> {
        match provider_kind {
            IntegrationProviderKind::Todoist => Ok(Self::Todoist),
            IntegrationProviderKind::Linear => Ok(Self::Linear),
            _ => Err(anyhow!(
                "IntegrationProviderKind {provider_kind} is not a valid TaskSyncSourceKind"
            )),
        }
    }
}

impl From<TaskSyncSourceKind> for IntegrationProviderKind {
    fn from(source_kind: TaskSyncSourceKind) -> Self {
        match source_kind {
            TaskSyncSourceKind::Todoist => IntegrationProviderKind::Todoist,
            TaskSyncSourceKind::Linear => IntegrationProviderKind::Linear,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct TaskCreationResult {
    pub task: Task,
    pub notifications: Vec<Notification>,
}

pub trait TaskSource: IntegrationProviderSource {
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
                DueDate::DateTimeWithTz(DateTime::from_naive_utc_and_offset(
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

    mod from_preset_due_date {
        use super::super::*;
        use pretty_assertions::assert_eq;
        use rstest::*;

        #[rstest]
        fn test_from_today_preset_to_due_date() {
            assert_eq!(
                DueDate::from_preset(
                    NaiveDate::from_ymd_opt(2024, 1, 10).unwrap(),
                    PresetDueDate::Today
                ),
                DueDate::Date(NaiveDate::from_ymd_opt(2024, 1, 10).unwrap())
            );
        }

        #[rstest]
        fn test_from_tomorrow_preset_to_due_date() {
            assert_eq!(
                DueDate::from_preset(
                    NaiveDate::from_ymd_opt(2024, 1, 10).unwrap(),
                    PresetDueDate::Tomorrow
                ),
                DueDate::Date(NaiveDate::from_ymd_opt(2024, 1, 11).unwrap())
            );
        }

        #[rstest]
        fn test_from_tomorrow_preset_to_due_date_end_of_year() {
            assert_eq!(
                DueDate::from_preset(
                    NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                    PresetDueDate::Tomorrow
                ),
                DueDate::Date(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap())
            );
        }

        #[rstest]
        fn test_from_this_weekend_preset_to_due_date() {
            assert_eq!(
                DueDate::from_preset(
                    NaiveDate::from_ymd_opt(2024, 1, 10).unwrap(), // Wednesday
                    PresetDueDate::ThisWeekend
                ),
                DueDate::Date(NaiveDate::from_ymd_opt(2024, 1, 13).unwrap())
            );
        }

        #[rstest]
        fn test_from_this_weekend_preset_to_due_date_on_saturday() {
            assert_eq!(
                DueDate::from_preset(
                    NaiveDate::from_ymd_opt(2024, 1, 6).unwrap(), // Saturday
                    PresetDueDate::ThisWeekend
                ),
                DueDate::Date(NaiveDate::from_ymd_opt(2024, 1, 13).unwrap())
            );
        }

        #[rstest]
        fn test_from_next_week_preset_to_due_date() {
            assert_eq!(
                DueDate::from_preset(
                    NaiveDate::from_ymd_opt(2024, 1, 10).unwrap(), // Wednesday
                    PresetDueDate::NextWeek
                ),
                DueDate::Date(NaiveDate::from_ymd_opt(2024, 1, 15).unwrap())
            );
        }

        #[rstest]
        fn test_from_next_week_preset_to_due_date_on_monday() {
            assert_eq!(
                DueDate::from_preset(
                    NaiveDate::from_ymd_opt(2024, 1, 8).unwrap(), // Monday
                    PresetDueDate::NextWeek
                ),
                DueDate::Date(NaiveDate::from_ymd_opt(2024, 1, 15).unwrap())
            );
        }
    }
}
