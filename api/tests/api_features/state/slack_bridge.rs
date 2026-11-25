use universal_inbox::{notification::NotificationId, slack_bridge::SlackBridgePendingActionId};

use crate::helpers::slack_bridge::ActionState;

#[derive(Default)]
pub struct SlackBridge {
    pub action_id: Option<SlackBridgePendingActionId>,
    pub notification_id: Option<NotificationId>,
    pub remembered: Option<ActionState>,
    pub deletion: Option<Deletion>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Deletion {
    Deleted,
    NothingToDelete,
}
