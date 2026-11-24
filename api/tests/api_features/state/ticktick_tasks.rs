use http::StatusCode;

use universal_inbox::{
    task::TaskCreationResult, third_party::integrations::ticktick::TickTickItem,
    third_party::item::ThirdPartyItemCreationResult,
};

#[derive(Default)]
pub struct TickTickTasks {
    pub reported: Option<Vec<TickTickItem>>,
    pub known: Vec<ThirdPartyItemCreationResult>,
    pub results: Vec<TaskCreationResult>,
    pub status: Option<StatusCode>,
}
