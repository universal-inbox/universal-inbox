use universal_inbox::{task::Task, third_party::item::ThirdPartyItemCreationResult};

#[derive(Default)]
pub struct SlackTasks {
    pub task: Option<Task>,
    pub creation: Option<Box<ThirdPartyItemCreationResult>>,
    pub patched: Option<Box<Task>>,
    pub edited: Option<Box<ThirdPartyItemCreationResult>>,
}
