use chrono::{DateTime, Utc};

use universal_inbox::notification::Notification;
use universal_inbox_api::integrations::google_drive::{
    GoogleDriveCommentList, GoogleDriveFileList,
};

#[derive(Default)]
pub struct GoogleDriveSync {
    pub about: Option<(String, String)>,
    pub files: Option<GoogleDriveFileList>,
    pub comments: Vec<(usize, CommentsOfFile)>,
    pub modified_time: Option<DateTime<Utc>>,
    pub synced: Vec<Notification>,
}

pub enum CommentsOfFile {
    Listed(GoogleDriveCommentList),
    NotFound,
}
