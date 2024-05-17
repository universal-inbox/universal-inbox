use serde::{Deserialize, Serialize};
pub static DEFAULT_TODOIST_HTML_URL: &str = "https://todoist.com/app/";
pub static TODOIST_INBOX_PROJECT: &str = "Inbox";

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
