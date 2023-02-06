use std::collections::HashMap;

use notification::Notification;
use serde::{Deserialize, Serialize};
use task::{Task, TaskId};

#[macro_use]
extern crate macro_attr;

#[macro_use]
extern crate enum_derive;

pub mod notification;
pub mod task;

#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Eq)]
pub struct NotificationsListResult {
    pub notifications: Vec<Notification>,
    pub tasks: Option<HashMap<TaskId, Task>>,
}
