use universal_inbox::{
    notification::NotificationId, task::Task, third_party::item::ThirdPartyItemCreationResult,
};

#[derive(Default)]
pub struct SlackTasks {
    pub task: Option<Task>,
    pub notification_id: Option<NotificationId>,
    pub creation: Option<Box<ThirdPartyItemCreationResult>>,
    pub patched: Option<Box<Task>>,
    pub edited: Option<Box<ThirdPartyItemCreationResult>>,
    pub waited: Option<std::time::Duration>,
    pub delivered_at: Option<std::time::Instant>,
}
