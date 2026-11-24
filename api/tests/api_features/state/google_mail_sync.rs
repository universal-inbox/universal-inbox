use serde_json::Value;

use universal_inbox::{
    notification::Notification,
    third_party::integrations::{
        google_calendar::GoogleCalendarEventAttendeeResponseStatus,
        google_mail::{GoogleMailMessageBody, GoogleMailThread},
    },
};

#[derive(Default)]
pub struct GoogleMailSync {
    pub threads: Vec<GoogleMailThread>,
    pub paged: bool,
    pub attachment: Option<Attachment>,
    pub calendar: Option<CalendarPages>,
    pub answer: Option<GoogleCalendarEventAttendeeResponseStatus>,
    pub synced: Vec<Notification>,
}

pub struct Attachment {
    pub message_id: &'static str,
    pub attachment_id: &'static str,
    pub body: GoogleMailMessageBody,
}

pub struct CalendarPages {
    pub ical_uid: &'static str,
    pub pages: Vec<(Option<&'static str>, Value)>,
}
