use std::fmt;

use anyhow::anyhow;
use chrono::{DateTime, Timelike, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_with::serde_as;
use url::Url;
use uuid::Uuid;

use crate::{
    integration_connection::IntegrationConnectionId,
    third_party::item::{ThirdPartyItem, ThirdPartyItemData, ThirdPartyItemFromSource},
    user::UserId,
    HasHtmlUrl,
};

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
                let Some(datetime) = DateTime::from_timestamp(timestamp / 1000, 0) else {
                    return Err(format!("Invalid timestamp {timestamp}"));
                };
                Ok(datetime)
            })
            .map_err(serde::de::Error::custom)
    }
}

impl GoogleMailThread {
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
}

impl ThirdPartyItemFromSource for GoogleMailThread {
    fn into_third_party_item(
        self,
        user_id: UserId,
        integration_connection_id: IntegrationConnectionId,
    ) -> ThirdPartyItem {
        ThirdPartyItem {
            id: Uuid::new_v4().into(),
            source_id: self.id.clone(),
            data: ThirdPartyItemData::GoogleMailThread(Box::new(self.clone())),
            created_at: Utc::now().with_nanosecond(0).unwrap(),
            updated_at: Utc::now().with_nanosecond(0).unwrap(),
            user_id,
            integration_connection_id,
        }
    }
}

impl HasHtmlUrl for GoogleMailThread {
    fn get_html_url(&self) -> Url {
        format!(
            "https://mail.google.com/mail/u/{}/#inbox/{}",
            self.user_email_address, self.id
        )
        .parse::<Url>()
        .unwrap_or_else(|_| DEFAULT_GOOGLE_MAIL_HTML_URL.parse::<Url>().unwrap())
    }
}

impl TryFrom<ThirdPartyItem> for GoogleMailThread {
    type Error = anyhow::Error;

    fn try_from(item: ThirdPartyItem) -> Result<Self, Self::Error> {
        match item.data {
            ThirdPartyItemData::GoogleMailThread(thread) => Ok(*thread),
            _ => Err(anyhow!(
                "Unable to convert ThirdPartyItem {} to GoogleMailThread",
                item.id
            )),
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
}
