use std::fmt;

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_with::serde_as;
use url::Url;
use uuid::Uuid;

use crate::{
    notification::{Notification, NotificationMetadata, NotificationStatus},
    user::UserId,
};

const DEFAULT_SUBJECT: &str = "No subject";
pub const GOOGLE_MAIL_UNREAD_LABEL: &str = "UNREAD";
pub const GOOGLE_MAIL_INBOX_LABEL: &str = "INBOX";
pub const GOOGLE_MAIL_STARRED_LABEL: &str = "STARRED";
pub const GOOGLE_MAIL_IMPORTANT_LABEL: &str = "IMPORTANT";
pub const DEFAULT_GOOGLE_MAIL_HTML_URL: &str = "https://mail.google.com";

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, Hash)]
#[serde(transparent)]
pub struct EmailAddress(pub String);

impl fmt::Display for EmailAddress {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<EmailAddress> for String {
    fn from(email_address: EmailAddress) -> Self {
        email_address.0
    }
}

impl From<String> for EmailAddress {
    fn from(email_address: String) -> Self {
        Self(email_address)
    }
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GoogleMailThread {
    pub id: String,
    pub user_email_address: EmailAddress,
    #[serde(rename = "historyId")]
    pub history_id: String,
    pub messages: Vec<GoogleMailMessage>,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GoogleMailMessage {
    pub id: String,
    #[serde(rename = "threadId")]
    pub thread_id: String,
    #[serde(rename = "labelIds")]
    pub label_ids: Option<Vec<String>>,
    pub snippet: String,
    pub payload: GoogleMailMessagePayload,
    #[serde(rename = "sizeEstimate")]
    pub size_estimate: usize,
    #[serde(rename = "historyId")]
    pub history_id: String,
    #[serde(with = "message_date_format")]
    #[serde(rename = "internalDate")]
    pub internal_date: DateTime<Utc>,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GoogleMailMessagePayload {
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    pub headers: Vec<GoogleMailMessageHeader>,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GoogleMailMessageHeader {
    pub name: String,
    pub value: String,
}

#[derive(PartialEq)]
pub enum MessageSelection {
    First,
    Last,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GoogleMailLabel {
    pub id: String,
    pub name: String,
}

mod message_date_format {
    use super::*;

    pub fn serialize<S>(message_date: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&message_date.timestamp_millis().to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse::<i64>()
            .map_err(|err| format!("Failed to parse i64: {err}"))
            .and_then(|timestamp| {
                let Some(naive_datetime) = NaiveDateTime::from_timestamp_opt(timestamp / 1000, 0)
                else {
                    return Err(format!("Invalid timestamp {timestamp}"));
                };
                Ok(DateTime::from_naive_utc_and_offset(naive_datetime, Utc))
            })
            .map_err(serde::de::Error::custom)
    }
}

impl GoogleMailThread {
    pub fn get_html_url_from_metadata(&self) -> Url {
        format!(
            "https://mail.google.com/mail/u/{}/#inbox/{}",
            self.user_email_address, self.id
        )
        .parse::<Url>()
        .unwrap_or_else(|_| DEFAULT_GOOGLE_MAIL_HTML_URL.parse::<Url>().unwrap())
    }

    pub fn get_message_header(
        &self,
        message_selection: MessageSelection,
        header_name: &str,
    ) -> Option<String> {
        let message_index = if message_selection == MessageSelection::First {
            0
        } else {
            self.messages.len() - 1
        };
        let message = &self.messages[message_index];
        message.get_header(header_name)
    }

    pub fn is_tagged_with(
        &self,
        label_id: &str,
        message_selection: Option<MessageSelection>,
    ) -> bool {
        if let Some(message_selection) = message_selection {
            let message_index = if message_selection == MessageSelection::First {
                0
            } else {
                self.messages.len() - 1
            };
            let message = &self.messages[message_index];
            return message.is_tagged_with(label_id);
        }

        self.messages.iter().any(|msg| msg.is_tagged_with(label_id))
    }

    pub fn remove_labels(&mut self, labels_to_remove: Vec<&str>) {
        for msg in &mut self.messages {
            msg.label_ids = msg.label_ids.as_ref().map(|label_ids| {
                label_ids
                    .iter()
                    .filter(|label| !labels_to_remove.contains(&label.as_str()))
                    .map(|label| label.to_string())
                    .collect::<Vec<String>>()
            });
        }
    }

    pub fn into_notification(
        mut self,
        user_id: UserId,
        current_notification_status: Option<NotificationStatus>,
        synced_label_id: &str,
    ) -> Notification {
        let title = self
            .get_message_header(MessageSelection::First, "Subject")
            .unwrap_or_else(|| DEFAULT_SUBJECT.to_string());
        let updated_at = self.messages[self.messages.len() - 1].internal_date;
        let first_unread_message_index = self
            .messages
            .iter()
            .position(|msg| msg.is_tagged_with(GOOGLE_MAIL_UNREAD_LABEL));
        let last_read_at = if let Some(i) = first_unread_message_index {
            (i > 0).then(|| self.messages[i - 1].internal_date)
        } else {
            Some(self.messages[self.messages.len() - 1].internal_date)
        };
        let status = if let Some(NotificationStatus::Unsubscribed) = current_notification_status {
            // has unread messages
            if let Some(i) = first_unread_message_index {
                let has_directly_addressed_messages = self.messages.iter().skip(i).any(|msg| {
                    msg.payload.headers.iter().any(|header| {
                        header.name == *"To" && header.value.contains(&self.user_email_address.0)
                    })
                });
                if has_directly_addressed_messages {
                    NotificationStatus::Unread
                } else {
                    self.remove_labels(vec![GOOGLE_MAIL_INBOX_LABEL, synced_label_id]);
                    NotificationStatus::Unsubscribed
                }
            } else {
                self.remove_labels(vec![GOOGLE_MAIL_INBOX_LABEL, synced_label_id]);
                NotificationStatus::Unsubscribed
            }
        } else {
            let thread_is_unread = self.is_tagged_with(GOOGLE_MAIL_UNREAD_LABEL, None);
            if thread_is_unread {
                NotificationStatus::Unread
            } else {
                NotificationStatus::Read
            }
        };

        Notification {
            id: Uuid::new_v4().into(),
            title,
            source_id: self.id.clone(),
            status,
            metadata: NotificationMetadata::GoogleMail(Box::new(self)),
            updated_at,
            last_read_at,
            snoozed_until: None,
            user_id,
            details: None,
            task_id: None,
        }
    }
}

impl GoogleMailMessage {
    pub fn get_header(&self, header_name: &str) -> Option<String> {
        self.payload
            .headers
            .iter()
            .find(|header| header.name == header_name)
            .map(|header| header.value.clone())
    }

    pub fn is_tagged_with(&self, label_id: &str) -> bool {
        self.label_ids
            .as_ref()
            .map(|label_ids| label_ids.contains(&label_id.to_string()))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use rstest::*;

    mod de_serialization {
        use super::*;
        use pretty_assertions::assert_eq;
        use serde_json::json;

        #[rstest]
        fn test_google_mail_thread_serialization_config() {
            assert_eq!(
                json!(
                    {
                        "id": "18a909f8178",
                        "user_email_address": "test@example.com",
                        "historyId": "1234",
                        "messages": [
                            {
                                "id": "18a909f8178",
                                "threadId": "18a909f8178",
                                "labelIds": [GOOGLE_MAIL_UNREAD_LABEL],
                                "snippet": "test",
                                "payload": {
                                    "mimeType": "multipart/mixed",
                                    "headers": [
                                        {
                                            "name": "Subject",
                                            "value": "test subject"
                                        }
                                    ]
                                },
                                "sizeEstimate": 1,
                                "historyId": "5678",
                                "internalDate": "1694636372000"
                            }
                        ]
                    }
                )
                .to_string(),
                serde_json::to_string(&GoogleMailThread {
                    id: "18a909f8178".to_string(),
                    history_id: "1234".to_string(),
                    user_email_address: "test@example.com".to_string().into(),
                    messages: vec![GoogleMailMessage {
                        id: "18a909f8178".to_string(),
                        thread_id: "18a909f8178".to_string(),
                        label_ids: Some(vec![GOOGLE_MAIL_UNREAD_LABEL.to_string()]),
                        snippet: "test".to_string(),
                        size_estimate: 1,
                        history_id: "5678".to_string(),
                        internal_date: Utc.with_ymd_and_hms(2023, 9, 13, 20, 19, 32).unwrap(),
                        payload: GoogleMailMessagePayload {
                            mime_type: "multipart/mixed".to_string(),
                            headers: vec![GoogleMailMessageHeader {
                                name: "Subject".to_string(),
                                value: "test subject".to_string()
                            }]
                        }
                    }]
                })
                .unwrap()
            );
        }

        #[rstest]
        fn test_google_mail_thread_deserialization_config() {
            assert_eq!(
                serde_json::from_str::<GoogleMailThread>(
                    r#"
                {
                    "id": "18a909f8178",
                    "historyId": "1234",
                    "user_email_address": "test@example.com",
                    "messages": [
                        {
                            "id": "18a909f8178",
                            "threadId": "18a909f8178",
                            "labelIds": ["UNREAD"],
                            "snippet": "test",
                            "sizeEstimate": 1,
                            "historyId": "5678",
                            "internalDate": "1694636372000",
                            "payload": {
                                "mimeType": "multipart/mixed",
                                "headers": [
                                    {
                                        "name": "Subject",
                                        "value": "test subject"
                                    }
                                ]
                            }
                        }
                    ]
                }
            "#
                )
                .unwrap(),
                GoogleMailThread {
                    id: "18a909f8178".to_string(),
                    history_id: "1234".to_string(),
                    user_email_address: "test@example.com".to_string().into(),
                    messages: vec![GoogleMailMessage {
                        id: "18a909f8178".to_string(),
                        thread_id: "18a909f8178".to_string(),
                        label_ids: Some(vec![GOOGLE_MAIL_UNREAD_LABEL.to_string()]),
                        snippet: "test".to_string(),
                        size_estimate: 1,
                        history_id: "5678".to_string(),
                        internal_date: Utc.with_ymd_and_hms(2023, 9, 13, 20, 19, 32).unwrap(),
                        payload: GoogleMailMessagePayload {
                            mime_type: "multipart/mixed".to_string(),
                            headers: vec![GoogleMailMessageHeader {
                                name: "Subject".to_string(),
                                value: "test subject".to_string()
                            }]
                        }
                    }]
                }
            );
        }
    }

    mod notification_conversion {
        use crate::HasHtmlUrl;

        use super::*;
        use pretty_assertions::assert_eq;

        #[rstest]
        fn test_google_mail_thread_into_notification() {
            let google_mail_notification = GoogleMailThread {
                id: "18a909f8178".to_string(),
                history_id: "1234".to_string(),
                user_email_address: "test@example.com".to_string().into(),
                messages: vec![
                    GoogleMailMessage {
                        id: "18a909f8178".to_string(),
                        thread_id: "18a909f8178".to_string(),
                        label_ids: None,
                        snippet: "test".to_string(),
                        size_estimate: 4,
                        history_id: "5678".to_string(),
                        internal_date: Utc.with_ymd_and_hms(2023, 9, 13, 20, 19, 32).unwrap(),
                        payload: GoogleMailMessagePayload {
                            mime_type: "multipart/mixed".to_string(),
                            headers: vec![
                                GoogleMailMessageHeader {
                                    name: "Subject".to_string(),
                                    value: "test subject".to_string(),
                                },
                                GoogleMailMessageHeader {
                                    name: "To".to_string(),
                                    value: "dest@example.com".to_string(),
                                },
                            ],
                        },
                    },
                    GoogleMailMessage {
                        id: "18a909f8179".to_string(),
                        thread_id: "18a909f8178".to_string(),
                        label_ids: Some(vec![GOOGLE_MAIL_UNREAD_LABEL.to_string()]),
                        snippet: "test 2".to_string(),
                        size_estimate: 6,
                        history_id: "5678".to_string(),
                        internal_date: Utc.with_ymd_and_hms(2023, 9, 13, 20, 19, 33).unwrap(),
                        payload: GoogleMailMessagePayload {
                            mime_type: "multipart/mixed".to_string(),
                            headers: vec![GoogleMailMessageHeader {
                                name: "Subject".to_string(),
                                value: "test subject".to_string(),
                            }],
                        },
                    },
                ],
            }
            .into_notification(Uuid::new_v4().into(), None, GOOGLE_MAIL_STARRED_LABEL);

            assert_eq!(google_mail_notification.title, "test subject".to_string());
            assert_eq!(
                google_mail_notification.source_id,
                "18a909f8178".to_string()
            );
            assert_eq!(
                google_mail_notification.get_html_url(),
                "https://mail.google.com/mail/u/test@example.com/#inbox/18a909f8178"
                    .parse::<Url>()
                    .unwrap()
            );
            assert_eq!(google_mail_notification.status, NotificationStatus::Unread);
            assert_eq!(
                google_mail_notification.updated_at,
                Utc.with_ymd_and_hms(2023, 9, 13, 20, 19, 33).unwrap()
            );
            assert_eq!(
                google_mail_notification.last_read_at,
                Some(Utc.with_ymd_and_hms(2023, 9, 13, 20, 19, 32).unwrap())
            );
        }

        #[rstest]
        fn test_google_mail_thread_with_missing_headers_into_notification() {
            let google_mail_notification = GoogleMailThread {
                id: "18a909f8178".to_string(),
                history_id: "1234".to_string(),
                user_email_address: "test@example.com".to_string().into(),
                messages: vec![
                    GoogleMailMessage {
                        id: "18a909f8178".to_string(),
                        thread_id: "18a909f8178".to_string(),
                        label_ids: Some(vec![GOOGLE_MAIL_UNREAD_LABEL.to_string()]),
                        snippet: "test".to_string(),
                        size_estimate: 4,
                        history_id: "5678".to_string(),
                        internal_date: Utc.with_ymd_and_hms(2023, 9, 13, 20, 19, 32).unwrap(),
                        payload: GoogleMailMessagePayload {
                            mime_type: "multipart/mixed".to_string(),
                            headers: vec![],
                        },
                    },
                    GoogleMailMessage {
                        id: "18a909f8179".to_string(),
                        thread_id: "18a909f8178".to_string(),
                        label_ids: Some(vec![GOOGLE_MAIL_UNREAD_LABEL.to_string()]),
                        snippet: "test 2".to_string(),
                        size_estimate: 6,
                        history_id: "5678".to_string(),
                        internal_date: Utc.with_ymd_and_hms(2023, 9, 13, 20, 19, 33).unwrap(),
                        payload: GoogleMailMessagePayload {
                            mime_type: "multipart/mixed".to_string(),
                            headers: vec![],
                        },
                    },
                ],
            }
            .into_notification(Uuid::new_v4().into(), None, GOOGLE_MAIL_STARRED_LABEL);

            assert_eq!(google_mail_notification.title, DEFAULT_SUBJECT.to_string());
            assert_eq!(
                google_mail_notification.get_html_url(),
                "https://mail.google.com/mail/u/test@example.com/#inbox/18a909f8178"
                    .parse::<Url>()
                    .unwrap()
            );
            assert_eq!(google_mail_notification.status, NotificationStatus::Unread);
            assert_eq!(google_mail_notification.last_read_at, None);
        }

        #[rstest]
        fn test_google_mail_read_thread_into_notification() {
            let google_mail_notification = GoogleMailThread {
                id: "18a909f8178".to_string(),
                history_id: "1234".to_string(),
                user_email_address: "test@example.com".to_string().into(),
                messages: vec![
                    GoogleMailMessage {
                        id: "18a909f8178".to_string(),
                        thread_id: "18a909f8178".to_string(),
                        label_ids: None,
                        snippet: "test".to_string(),
                        size_estimate: 4,
                        history_id: "5678".to_string(),
                        internal_date: Utc.with_ymd_and_hms(2023, 9, 13, 20, 19, 32).unwrap(),
                        payload: GoogleMailMessagePayload {
                            mime_type: "multipart/mixed".to_string(),
                            headers: vec![],
                        },
                    },
                    GoogleMailMessage {
                        id: "18a909f8179".to_string(),
                        thread_id: "18a909f8178".to_string(),
                        label_ids: None,
                        snippet: "test 2".to_string(),
                        size_estimate: 6,
                        history_id: "5678".to_string(),
                        internal_date: Utc.with_ymd_and_hms(2023, 9, 13, 20, 19, 33).unwrap(),
                        payload: GoogleMailMessagePayload {
                            mime_type: "multipart/mixed".to_string(),
                            headers: vec![],
                        },
                    },
                ],
            }
            .into_notification(Uuid::new_v4().into(), None, GOOGLE_MAIL_STARRED_LABEL);

            assert_eq!(google_mail_notification.status, NotificationStatus::Read);
            assert_eq!(
                google_mail_notification.last_read_at,
                Some(Utc.with_ymd_and_hms(2023, 9, 13, 20, 19, 33).unwrap())
            );
        }

        #[rstest]
        fn test_google_mail_unsubscribed_thread_with_no_new_message_into_notification() {
            let google_mail_notification = GoogleMailThread {
                id: "18a909f8178".to_string(),
                history_id: "1234".to_string(),
                user_email_address: "test@example.com".to_string().into(),
                messages: vec![
                    GoogleMailMessage {
                        id: "18a909f8178".to_string(),
                        thread_id: "18a909f8178".to_string(),
                        label_ids: Some(vec![GOOGLE_MAIL_STARRED_LABEL.to_string()]),
                        snippet: "test".to_string(),
                        size_estimate: 4,
                        history_id: "5678".to_string(),
                        internal_date: Utc.with_ymd_and_hms(2023, 9, 13, 20, 19, 32).unwrap(),
                        payload: GoogleMailMessagePayload {
                            mime_type: "multipart/mixed".to_string(),
                            headers: vec![],
                        },
                    },
                    GoogleMailMessage {
                        id: "18a909f8179".to_string(),
                        thread_id: "18a909f8178".to_string(),
                        label_ids: Some(vec![
                            GOOGLE_MAIL_INBOX_LABEL.to_string(),
                            GOOGLE_MAIL_STARRED_LABEL.to_string(),
                        ]),
                        snippet: "test 2".to_string(),
                        size_estimate: 6,
                        history_id: "5678".to_string(),
                        internal_date: Utc.with_ymd_and_hms(2023, 9, 13, 20, 19, 33).unwrap(),
                        payload: GoogleMailMessagePayload {
                            mime_type: "multipart/mixed".to_string(),
                            headers: vec![],
                        },
                    },
                ],
            }
            .into_notification(
                Uuid::new_v4().into(),
                Some(NotificationStatus::Unsubscribed),
                GOOGLE_MAIL_STARRED_LABEL,
            );

            assert_eq!(
                google_mail_notification.status,
                NotificationStatus::Unsubscribed
            );
            match google_mail_notification.metadata {
                NotificationMetadata::GoogleMail(thread) => {
                    assert_eq!(thread.messages[0].label_ids, Some(vec![]));
                    assert_eq!(thread.messages[1].label_ids, Some(vec![]));
                }
                _ => unreachable!("Google Mail notification should match previous pattern"),
            };
            assert_eq!(
                google_mail_notification.last_read_at,
                Some(Utc.with_ymd_and_hms(2023, 9, 13, 20, 19, 33).unwrap())
            );
        }

        #[rstest]
        fn test_google_mail_unsubscribed_thread_with_new_unread_message_into_notification() {
            let google_mail_notification = GoogleMailThread {
                id: "18a909f8178".to_string(),
                history_id: "1234".to_string(),
                user_email_address: "test@example.com".to_string().into(),
                messages: vec![
                    GoogleMailMessage {
                        id: "18a909f8178".to_string(),
                        thread_id: "18a909f8178".to_string(),
                        label_ids: Some(vec![GOOGLE_MAIL_STARRED_LABEL.to_string()]),
                        snippet: "test".to_string(),
                        size_estimate: 4,
                        history_id: "5678".to_string(),
                        internal_date: Utc.with_ymd_and_hms(2023, 9, 13, 20, 19, 32).unwrap(),
                        payload: GoogleMailMessagePayload {
                            mime_type: "multipart/mixed".to_string(),
                            headers: vec![],
                        },
                    },
                    GoogleMailMessage {
                        id: "18a909f8179".to_string(),
                        thread_id: "18a909f8178".to_string(),
                        label_ids: Some(vec![
                            GOOGLE_MAIL_STARRED_LABEL.to_string(),
                            GOOGLE_MAIL_INBOX_LABEL.to_string(),
                            GOOGLE_MAIL_UNREAD_LABEL.to_string(),
                        ]),
                        snippet: "test 2".to_string(),
                        size_estimate: 6,
                        history_id: "5678".to_string(),
                        internal_date: Utc.with_ymd_and_hms(2023, 9, 13, 20, 19, 33).unwrap(),
                        payload: GoogleMailMessagePayload {
                            mime_type: "multipart/mixed".to_string(),
                            headers: vec![],
                        },
                    },
                ],
            }
            .into_notification(
                Uuid::new_v4().into(),
                Some(NotificationStatus::Unsubscribed),
                GOOGLE_MAIL_STARRED_LABEL,
            );

            assert_eq!(
                google_mail_notification.status,
                NotificationStatus::Unsubscribed
            );
            match google_mail_notification.metadata {
                NotificationMetadata::GoogleMail(thread) => {
                    assert_eq!(thread.messages[0].label_ids, Some(vec![]));
                    // message is archived but kept unread
                    assert_eq!(
                        thread.messages[1].label_ids,
                        Some(vec![GOOGLE_MAIL_UNREAD_LABEL.to_string()])
                    );
                }
                _ => unreachable!("Google Mail notification should match previous pattern"),
            };
            assert_eq!(
                google_mail_notification.last_read_at,
                Some(Utc.with_ymd_and_hms(2023, 9, 13, 20, 19, 32).unwrap())
            );
        }

        #[rstest]
        fn test_google_mail_unsubscribed_thread_with_new_unread_message_directly_addressed_into_notification(
        ) {
            let google_mail_notification = GoogleMailThread {
                id: "18a909f8178".to_string(),
                history_id: "1234".to_string(),
                user_email_address: "test@example.com".to_string().into(),
                messages: vec![
                    GoogleMailMessage {
                        id: "18a909f8178".to_string(),
                        thread_id: "18a909f8178".to_string(),
                        label_ids: Some(vec![GOOGLE_MAIL_STARRED_LABEL.to_string()]),
                        snippet: "test".to_string(),
                        size_estimate: 4,
                        history_id: "5678".to_string(),
                        internal_date: Utc.with_ymd_and_hms(2023, 9, 13, 20, 19, 32).unwrap(),
                        payload: GoogleMailMessagePayload {
                            mime_type: "multipart/mixed".to_string(),
                            headers: vec![],
                        },
                    },
                    GoogleMailMessage {
                        id: "18a909f8179".to_string(),
                        thread_id: "18a909f8178".to_string(),
                        label_ids: Some(vec![
                            GOOGLE_MAIL_STARRED_LABEL.to_string(),
                            GOOGLE_MAIL_INBOX_LABEL.to_string(),
                            GOOGLE_MAIL_UNREAD_LABEL.to_string(),
                        ]),
                        snippet: "test 2".to_string(),
                        size_estimate: 6,
                        history_id: "5678".to_string(),
                        internal_date: Utc.with_ymd_and_hms(2023, 9, 13, 20, 19, 33).unwrap(),
                        payload: GoogleMailMessagePayload {
                            mime_type: "multipart/mixed".to_string(),
                            headers: vec![GoogleMailMessageHeader {
                                name: "To".to_string(),
                                value: "test@example.com".to_string(),
                            }],
                        },
                    },
                ],
            }
            .into_notification(
                Uuid::new_v4().into(),
                Some(NotificationStatus::Unsubscribed),
                GOOGLE_MAIL_STARRED_LABEL,
            );

            assert_eq!(google_mail_notification.status, NotificationStatus::Unread);
            match google_mail_notification.metadata {
                NotificationMetadata::GoogleMail(thread) => {
                    assert_eq!(
                        thread.messages[0].label_ids,
                        Some(vec![GOOGLE_MAIL_STARRED_LABEL.to_string()])
                    );
                    // message is kept untouched
                    assert_eq!(
                        thread.messages[1].label_ids,
                        Some(vec![
                            GOOGLE_MAIL_STARRED_LABEL.to_string(),
                            GOOGLE_MAIL_INBOX_LABEL.to_string(),
                            GOOGLE_MAIL_UNREAD_LABEL.to_string()
                        ])
                    );
                }
                _ => unreachable!("Google Mail notification should match previous pattern"),
            };
            assert_eq!(
                google_mail_notification.last_read_at,
                Some(Utc.with_ymd_and_hms(2023, 9, 13, 20, 19, 32).unwrap())
            );
        }
    }
}
