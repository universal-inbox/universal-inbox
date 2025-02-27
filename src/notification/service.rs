use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    notification::{NotificationStatus, NotificationSyncSourceKind},
    task::TaskId,
    third_party::integrations::google_calendar::GoogleCalendarEventAttendeeResponseStatus,
};

#[derive(Serialize, Deserialize, Debug)]
pub struct SyncNotificationsParameters {
    pub source: Option<NotificationSyncSourceKind>,
    pub asynchronous: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Eq)]
pub struct NotificationPatch {
    pub status: Option<NotificationStatus>,
    pub snoozed_until: Option<DateTime<Utc>>,
    pub task_id: Option<TaskId>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct InvitationPatch {
    pub response_status: GoogleCalendarEventAttendeeResponseStatus,
}
