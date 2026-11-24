use chrono::{DateTime, Utc};
use wiremock::MockGuard;

use universal_inbox::{
    notification::{Notification, NotificationStatus},
    task::TaskId,
    third_party::integrations::github::GithubNotification,
};

#[derive(Default)]
pub struct GithubSync {
    pub reported: Vec<GithubNotification>,
    pub pull_request: Option<PullRequest>,
    pub synced: Vec<Notification>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub kept: Option<KeptTriage>,
    pub read_count: Option<usize>,
    pub anything: Option<MockGuard>,
}

pub struct KeptTriage {
    pub status: NotificationStatus,
    pub task_id: Option<TaskId>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Copy)]
pub enum PullRequest {
    WithCommits,
    WithoutCommits,
}
