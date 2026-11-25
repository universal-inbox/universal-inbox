use universal_inbox::{
    notification::NotificationWithTask, task::Task, third_party::item::ThirdPartyItemCreationResult,
};

#[derive(Default)]
pub struct TickTickTaskChanges {
    pub creation: Option<Box<ThirdPartyItemCreationResult>>,
    pub patched: Option<Box<Task>>,
    pub created: Option<Option<NotificationWithTask>>,
    pub item_id: Option<String>,
    pub others_notification: Option<universal_inbox::notification::Notification>,
    pub others_task: Option<Task>,
}
