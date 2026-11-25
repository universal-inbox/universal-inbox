use universal_inbox::{
    notification::NotificationWithTask, task::Task, third_party::item::ThirdPartyItemCreationResult,
};

#[derive(Default)]
pub struct TodoistTaskChanges {
    pub items: Vec<(String, Box<ThirdPartyItemCreationResult>)>,
    pub new_project_id: Option<String>,
    pub patched: Option<Box<Task>>,
    pub created: Option<Option<NotificationWithTask>>,
}
