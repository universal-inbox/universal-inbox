use universal_inbox::notification::Notification;

#[derive(Default)]
pub struct SlackThreads {
    pub slack_state: Option<SlackThreadState>,
    pub team_id: Option<String>,
    pub synced: Vec<Notification>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlackThreadState {
    Read,
    Unsubscribed,
    Unchanged,
}
