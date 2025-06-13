use std::{
    io::BufReader,
    sync::{Arc, Weak},
};

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use chrono::{DateTime, Timelike, Utc};
use http::{HeaderMap, HeaderValue};
use ical::IcalParser;
use itertools::Itertools;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware, Extension};
use reqwest_tracing::{
    DisableOtelPropagation, OtelPathNames, SpanBackendWithUrl, TracingMiddleware,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_with::serde_as;
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;
use tracing::{debug, warn};
use url::Url;
use uuid::Uuid;
use wiremock::{
    matchers::{method, path, path_regex},
    Mock, MockServer, ResponseTemplate,
};

use universal_inbox::{
    integration_connection::{
        integrations::google_mail::{GoogleMailConfig, GoogleMailContext},
        provider::{
            IntegrationConnectionContext, IntegrationProvider, IntegrationProviderKind,
            IntegrationProviderSource,
        },
        IntegrationConnection, IntegrationConnectionId,
    },
    notification::{Notification, NotificationSource, NotificationSourceKind, NotificationStatus},
    third_party::{
        integrations::google_mail::{
            EmailAddress, GoogleMailLabel, GoogleMailMessage, GoogleMailMessageBody,
            GoogleMailThread, MessageSelection, GOOGLE_MAIL_INBOX_LABEL, GOOGLE_MAIL_UNREAD_LABEL,
        },
        item::{ThirdPartyItem, ThirdPartyItemFromSource, ThirdPartyItemSourceKind},
    },
    user::UserId,
    utils::base64::decode_base64,
};

use crate::{
    integrations::{
        google_calendar::GoogleCalendarService, notification::ThirdPartyNotificationSourceService,
        oauth2::AccessToken, third_party::ThirdPartyItemSourceService, APP_USER_AGENT,
    },
    universal_inbox::{
        integration_connection::service::IntegrationConnectionService,
        notification::service::NotificationService, UniversalInboxError,
    },
};

#[derive(Clone)]
pub struct GoogleMailService {
    google_mail_base_url: String,
    google_mail_base_path: String,
    page_size: usize,
    integration_connection_service: Weak<RwLock<IntegrationConnectionService>>,
    notification_service: Weak<RwLock<NotificationService>>,
    google_calendar_service: Arc<GoogleCalendarService>,
}

static DEFAULT_SUBJECT: &str = "No subject";
static GOOGLE_MAIL_BASE_URL: &str = "https://gmail.googleapis.com/gmail/v1";

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GoogleMailThreadList {
    pub threads: Option<Vec<GoogleMailThreadMinimal>>,
    #[serde(rename = "resultSizeEstimate")]
    pub result_size_estimate: usize,
    #[serde(rename = "nextPageToken")]
    pub next_page_token: Option<String>,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GoogleMailThreadMinimal {
    pub id: String,
    pub snippet: String,
    #[serde(rename = "historyId")]
    pub history_id: String,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GoogleMailUserProfile {
    #[serde(rename = "emailAddress")]
    pub email_address: String,
    #[serde(rename = "messagesTotal")]
    pub messages_total: u64,
    #[serde(rename = "threadsTotal")]
    pub threads_total: u64,
    #[serde(rename = "historyId")]
    pub history_id: String,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct RawGoogleMailThread {
    pub id: String,
    #[serde(rename = "historyId")]
    pub history_id: String,
    pub messages: Vec<GoogleMailMessage>,
}

impl RawGoogleMailThread {
    pub fn into_google_mail_thread(self, user_email_address: EmailAddress) -> GoogleMailThread {
        GoogleMailThread {
            id: self.id,
            history_id: self.history_id,
            messages: self.messages,
            user_email_address,
        }
    }
}

impl From<GoogleMailThread> for RawGoogleMailThread {
    fn from(thread: GoogleMailThread) -> Self {
        RawGoogleMailThread {
            id: thread.id,
            history_id: thread.history_id,
            messages: thread.messages,
        }
    }
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GoogleMailLabelList {
    pub labels: Option<Vec<RawGoogleMailLabel>>,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct RawGoogleMailLabel {
    pub id: String,
    pub name: String,
    #[serde(rename = "messageListVisibility")]
    pub message_list_visibility: Option<GoogleMailMessageListVisibility>,
    #[serde(rename = "labelListVisibility")]
    pub label_list_visibility: Option<GoogleMailLabelListVisibility>,
    pub r#type: GoogleMailLabelType,
}

impl From<RawGoogleMailLabel> for GoogleMailLabel {
    fn from(label: RawGoogleMailLabel) -> Self {
        GoogleMailLabel {
            id: label.id,
            name: label.name,
        }
    }
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub enum GoogleMailMessageListVisibility {
    #[serde(rename = "hide")]
    Hide,
    #[serde(rename = "show")]
    Show,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub enum GoogleMailLabelListVisibility {
    #[serde(rename = "labelHide")]
    LabelHide,
    #[serde(rename = "labelShowIfUnread")]
    LabelShowIfUnread,
    #[serde(rename = "labelShow")]
    LabelShow,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub enum GoogleMailLabelType {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "system")]
    System,
}

impl GoogleMailService {
    pub fn new(
        google_mail_base_url: Option<String>,
        page_size: usize,
        integration_connection_service: Weak<RwLock<IntegrationConnectionService>>,
        notification_service: Weak<RwLock<NotificationService>>,
        google_calendar_service: Arc<GoogleCalendarService>,
    ) -> Result<GoogleMailService, UniversalInboxError> {
        let google_mail_base_url =
            google_mail_base_url.unwrap_or_else(|| GOOGLE_MAIL_BASE_URL.to_string());
        let google_mail_base_path = Url::parse(&google_mail_base_url)
            .context("Failed to parse Google Mail base URL")?
            .path()
            .to_string();
        Ok(GoogleMailService {
            google_mail_base_url,
            google_mail_base_path: if &google_mail_base_path == "/" {
                "".to_string()
            } else {
                google_mail_base_path
            },
            page_size,
            integration_connection_service,
            notification_service,
            google_calendar_service,
        })
    }

    pub async fn mock_all(mock_server: &MockServer) {
        Mock::given(method("GET"))
            .and(path("/users/me/profile"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_json(&GoogleMailUserProfile {
                        email_address: "test@test.com".to_string(),
                        messages_total: 0,
                        threads_total: 0,
                        history_id: "123".to_string(),
                    }),
            )
            .mount(mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path_regex("/users/me/threads/.*"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_json(&RawGoogleMailThread {
                        id: "123".to_string(),
                        history_id: "123".to_string(),
                        messages: vec![],
                    }),
            )
            .mount(mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/users/me/threads"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_json(&GoogleMailThreadList {
                        threads: None,
                        result_size_estimate: 0,
                        next_page_token: None,
                    }),
            )
            .mount(mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/users/me/labels"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_json(&GoogleMailLabelList { labels: None }),
            )
            .mount(mock_server)
            .await;

        Mock::given(method("POST"))
            .and(path_regex("/users/me/threads/[^/]*/modify"))
            .respond_with(
                ResponseTemplate::new(200).insert_header("content-type", "application/json"),
            )
            .mount(mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/users/me/messages/[^/]*/attachments/[^/]*"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_json(&GoogleMailMessageBody {
                        size: 20,
                        data: None,
                        attachment_id: None,
                    }),
            )
            .mount(mock_server)
            .await;
    }

    pub fn set_notification_service(
        &mut self,
        notification_service: Weak<RwLock<NotificationService>>,
    ) {
        self.notification_service = notification_service;
    }

    fn build_google_mail_client(
        &self,
        access_token: &AccessToken,
    ) -> Result<ClientWithMiddleware, UniversalInboxError> {
        let mut headers = HeaderMap::new();

        let mut auth_header_value: HeaderValue = format!("Bearer {access_token}").parse().unwrap();
        auth_header_value.set_sensitive(true);
        headers.insert("Authorization", auth_header_value);

        let reqwest_client = reqwest::Client::builder()
            .default_headers(headers)
            .user_agent(APP_USER_AGENT)
            .build()
            .context("Failed to build Google mail client")?;
        // Tips: The reqwest_retry crate may help with rate limit errors
        // https://docs.rs/reqwest-retry/0.3.0/reqwest_retry/trait.RetryableStrategy.html
        Ok(ClientBuilder::new(reqwest_client)
            .with_init(Extension(
                OtelPathNames::known_paths([
                    format!("{}/users/me/profile", self.google_mail_base_path),
                    format!(
                        "{}/users/me/threads/{{thread_id}}/modify",
                        self.google_mail_base_path
                    ),
                    format!(
                        "{}/users/me/threads/{{thread_id}}",
                        self.google_mail_base_path
                    ),
                    format!("{}/users/me/threads*", self.google_mail_base_path),
                    format!("{}/users/me/labels", self.google_mail_base_path),
                    format!(
                        "{}/users/me/messages/{{message_id}}/attachments/{{attachment_id}}",
                        self.google_mail_base_path
                    ),
                ])
                .context("Cannot build Otel path names")?,
            ))
            .with_init(Extension(DisableOtelPropagation))
            .with(TracingMiddleware::<SpanBackendWithUrl>::new())
            .build())
    }

    pub async fn get_user_profile(
        &self,
        access_token: &AccessToken,
    ) -> Result<GoogleMailUserProfile, UniversalInboxError> {
        let url = format!("{}/users/me/profile", self.google_mail_base_url);

        let response = self
            .build_google_mail_client(access_token)?
            .get(&url)
            .send()
            .await
            .context("Cannot fetch user profile from GoogleMail API".to_string())?
            .text()
            .await
            .context("Failed to fetch user profile response from GoogleMail API".to_string())?;

        let user_profile: GoogleMailUserProfile = serde_json::from_str(&response)
            .map_err(|err| UniversalInboxError::from_json_serde_error(err, response))?;

        Ok(user_profile)
    }

    async fn get_thread(
        &self,
        thread_id: &str,
        access_token: &AccessToken,
    ) -> Result<RawGoogleMailThread, UniversalInboxError> {
        let url = format!(
            "{}/users/me/threads/{thread_id}?prettyPrint=false&format=full",
            self.google_mail_base_url,
        );

        let response = self
            .build_google_mail_client(access_token)?
            .get(&url)
            .send()
            .await
            .context(format!(
                "Cannot fetch thread {thread_id} from GoogleMail API"
            ))?
            .text()
            .await
            .context(format!(
                "Failed to fetch thread {thread_id} response from GoogleMail API"
            ))?;

        let thread: RawGoogleMailThread = serde_json::from_str(&response)
            .map_err(|err| UniversalInboxError::from_json_serde_error(err, response))?;

        Ok(thread)
    }

    pub async fn list_threads(
        &self,
        page_token: Option<String>,
        per_page: usize,
        label_ids: Vec<String>,
        access_token: &AccessToken,
    ) -> Result<GoogleMailThreadList, UniversalInboxError> {
        let url = format!(
            "{}/users/me/threads?prettyPrint=false&maxResults={per_page}{}{}",
            self.google_mail_base_url,
            label_ids
                .iter()
                .map(|label| format!("&labelIds={label}"))
                .join(""),
            page_token
                .map(|token| format!("&pageToken={token}"))
                .unwrap_or_default()
        );

        let response = self
            .build_google_mail_client(access_token)?
            .get(&url)
            .send()
            .await
            .context("Cannot fetch threads from GoogleMail API")?
            .text()
            .await
            .context("Failed to fetch threads response from GoogleMail API")?;

        let thread_list: GoogleMailThreadList = serde_json::from_str(&response)
            .map_err(|err| UniversalInboxError::from_json_serde_error(err, response))?;

        Ok(thread_list)
    }

    async fn get_attachment(
        &self,
        message_id: &str,
        attachment_id: &str,
        access_token: &AccessToken,
    ) -> Result<GoogleMailMessageBody, UniversalInboxError> {
        let url = format!(
            "{}/users/me/messages/{message_id}/attachments/{attachment_id}",
            self.google_mail_base_url,
        );

        let response = self
            .build_google_mail_client(access_token)?
            .get(&url)
            .send()
            .await
            .context("Cannot fetch attachment from GoogleMail API")?
            .text()
            .await
            .context("Failed to fetch attachment response from GoogleMail API")?;

        let attachment: GoogleMailMessageBody = serde_json::from_str(&response)
            .map_err(|err| UniversalInboxError::from_json_serde_error(err, response))?;

        Ok(attachment)
    }

    async fn modify_thread(
        &self,
        thread_id: &str,
        label_ids_to_add: Vec<&str>,
        label_ids_to_remove: Vec<&str>,
        access_token: &AccessToken,
    ) -> Result<(), UniversalInboxError> {
        let url = format!(
            "{}/users/me/threads/{thread_id}/modify",
            self.google_mail_base_url
        );
        let body = json!({
            "addLabelIds": label_ids_to_add,
            "removeLabelIds": label_ids_to_remove
        });

        let response = self
            .build_google_mail_client(access_token)?
            .post(&url)
            .json(&body)
            .send()
            .await
            .context(format!(
                "Cannot modify thread {thread_id} from GoogleMail API"
            ))?;

        match response.error_for_status() {
            Ok(_) => Ok(()),
            Err(err) if err.status() == Some(reqwest::StatusCode::NOT_FOUND) => Ok(()),
            Err(error) => {
                tracing::error!("An error occurred when trying to modify Google Mail thread `{thread_id}` labels: {}", error);
                Err(UniversalInboxError::Unexpected(anyhow!(
                    "Failed to modify Google Mail thread `{thread_id}` labels"
                )))
            }
        }
    }

    async fn archive_thread(
        &self,
        thread_id: &str,
        synced_label_id: &str,
        access_token: &AccessToken,
    ) -> Result<(), UniversalInboxError> {
        self.modify_thread(
            thread_id,
            vec![],
            vec![GOOGLE_MAIL_INBOX_LABEL, synced_label_id],
            access_token,
        )
        .await
    }

    pub async fn list_labels(
        &self,
        access_token: &AccessToken,
    ) -> Result<GoogleMailLabelList, UniversalInboxError> {
        let url = format!("{}/users/me/labels", self.google_mail_base_url);

        let response = self
            .build_google_mail_client(access_token)?
            .get(&url)
            .send()
            .await
            .context("Cannot fetch labels from GoogleMail API")?
            .text()
            .await
            .context("Failed to fetch labels response from GoogleMail API")?;

        let labels: GoogleMailLabelList = serde_json::from_str(&response)
            .map_err(|err| UniversalInboxError::from_json_serde_error(err, response))?;

        Ok(labels)
    }

    fn get_config(
        integration_connection: &IntegrationConnection,
    ) -> Result<GoogleMailConfig, UniversalInboxError> {
        let IntegrationProvider::GoogleMail { config, .. } = &integration_connection.provider
        else {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Integration connection ({}) provider is not Google Mail",
                integration_connection.id
            )));
        };

        Ok(config.clone())
    }

    /// Derive a ThirdPartyItem from a GoogleMailThread and cannot fail as it is a best-effort operation
    /// In case of failure, None is returned and the thread will be used as source
    async fn derive_third_party_item_from_google_mail_thread(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        google_mail_thread: &GoogleMailThread,
        user_id: UserId,
        integration_connection_id: IntegrationConnectionId,
        access_token: &AccessToken,
    ) -> Option<ThirdPartyItem> {
        if let Some(message) = google_mail_thread.messages.first() {
            if let Some(ref attachment_id) = message
                .payload
                .find_attachment_id_for_mime_type("text/calendar")
            {
                let mut derived_third_party_item = self
                    .derive_third_party_item_from_google_mail_invitation(
                        executor,
                        &message.id,
                        attachment_id,
                        user_id,
                        access_token,
                    )
                    .await
                    .inspect_err(|err| {
                        warn!(
                            "Failed to derive Google Mail invitation from thread `{}`: {err:?}",
                            google_mail_thread.id
                        );
                    })
                    .ok()??;
                derived_third_party_item.source_item = Some(Box::new(
                    google_mail_thread
                        .clone()
                        .into_third_party_item(user_id, integration_connection_id),
                ));
                return Some(derived_third_party_item);
            }
        }

        None
    }

    async fn derive_third_party_item_from_google_mail_invitation(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        message_id: &str,
        attachment_id: &str,
        user_id: UserId,
        gmail_access_token: &AccessToken,
    ) -> Result<Option<ThirdPartyItem>, UniversalInboxError> {
        let (gcal_access_token, gcal_integration_connection) = self
            .integration_connection_service
            .upgrade()
            .context("Unable to access integration_connection_service from google_mail_service")?
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::GoogleCalendar, user_id)
            .await?
            .ok_or_else(|| {
                anyhow!("Cannot find Google Calendar access token for user `{user_id}`")
            })?;

        let IntegrationProvider::GoogleCalendar { config } = gcal_integration_connection.provider
        else {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Integration connection `{}` provider is not a Google Calendar integration connection",
                gcal_integration_connection.id
            )));
        };

        if !config.sync_event_details_enabled {
            debug!(
                "Google Calendar integration connection `{}` does not have event details sync enabled",
                gcal_integration_connection.id
            );
            return Ok(None);
        }

        debug!(
            "Fetching Google Mail calendar attachment for message `{message_id}`: {attachment_id}"
        );
        let attachment = self
            .get_attachment(message_id, attachment_id, gmail_access_token)
            .await
            .with_context(|| {
                format!(
                    "Failed to fetch Google Mail calendar attachment for message `{message_id}`"
                )
            })?;
        let data = attachment.data.with_context(|| {
            format!("No `body` found in Google Mail attachement for message `{message_id}`")
        })?;
        let raw_vcal_event = decode_base64(&data).with_context(|| {
            format!("Failed to decode Google Mail calendar attachment for message `{message_id}`")
        })?;
        let mut vcal_events = IcalParser::new(BufReader::new(raw_vcal_event.as_bytes()));
        let vcal = vcal_events
            .next()
            .ok_or_else(|| anyhow!("Failed to find VCalendar"))?
            .context("Failed to parse VCalendar")?;

        // Extract the METHOD property from the vcal
        let vcal_method = vcal
            .properties
            .iter()
            .find_map(|p| {
                if p.name == "METHOD" {
                    p.value.clone()
                } else {
                    None
                }
            })
            .and_then(|method_str| method_str.parse().ok())
            .unwrap_or_default(); // Default to REQUEST

        let vcal_event = vcal
            .events
            .first()
            .ok_or_else(|| anyhow!("Failed to parse VCal events"))?;

        let vcal_uid = vcal_event
            .properties
            .iter()
            .find_map(|p| {
                if p.name == "UID" {
                    p.value.clone()
                } else {
                    None
                }
            })
            .ok_or_else(|| anyhow!("Failed to parse VCal events"))?;

        let mut event = self
            .google_calendar_service
            .get_event("primary", &vcal_uid, &gcal_access_token)
            .await
            .with_context(|| {
                format!("Failed to fetch Google Calendar event with iCalUID `{vcal_uid}`")
            })?;

        // Set the method from the vcal attachment
        event.method = vcal_method;

        Ok(Some(event.into_third_party_item(
            user_id,
            gcal_integration_connection.id,
        )))
    }
}

#[async_trait]
impl ThirdPartyItemSourceService<GoogleMailThread> for GoogleMailService {
    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user.id = user_id.to_string()),
        err
    )]
    async fn fetch_items(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<Vec<ThirdPartyItem>, UniversalInboxError> {
        let (access_token, integration_connection) = self
            .integration_connection_service
            .upgrade()
            .context("Unable to access integration_connection_service from google_mail_service")?
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::GoogleMail, user_id)
            .await?
            .ok_or_else(|| {
                anyhow!("Cannot fetch Google Mail notifications without an access token")
            })?;

        let labels = self
            .list_labels(&access_token)
            .await
            .context("Failed to fetch Google Mail labels")?;

        let config = GoogleMailService::get_config(&integration_connection)?;

        let user_email_address = match &integration_connection.provider {
            IntegrationProvider::GoogleMail {
                context:
                    Some(GoogleMailContext {
                        user_email_address, ..
                    }),
                ..
            } => user_email_address.clone(),
            _ => {
                let GoogleMailUserProfile { email_address, .. } =
                    self.get_user_profile(&access_token).await?;
                email_address.into()
            }
        };

        self.integration_connection_service
            .upgrade()
            .context("Unable to access integration_connection_service from google_mail_service")?
            .read()
            .await
            .update_integration_connection_context(
                executor,
                integration_connection.id,
                IntegrationConnectionContext::GoogleMail(GoogleMailContext {
                    user_email_address: user_email_address.clone(),
                    labels: labels
                        .labels
                        .unwrap_or_default()
                        .into_iter()
                        .map(|label| label.into())
                        .collect(),
                }),
            )
            .await
            .map_err(|_| {
                anyhow!(
                    "Failed to update Google Mail integration connection {} context",
                    integration_connection.id
                )
            })?;

        let mut page_token: Option<String> = None;
        let mut google_mail_threads: Vec<GoogleMailThread> = vec![];
        loop {
            let google_mail_thread_list = self
                .list_threads(
                    page_token,
                    self.page_size,
                    vec![config.synced_label.id.clone()],
                    &access_token,
                )
                .await?;

            // Tips: The batch API can be used for better performance
            // https://developers.google.com/gmail/api/guides/batch
            for thread in &google_mail_thread_list.threads.unwrap_or_default() {
                let raw_google_mail_thread = self.get_thread(&thread.id, &access_token).await?;
                google_mail_threads.push(
                    raw_google_mail_thread.into_google_mail_thread(user_email_address.clone()),
                );
            }

            if let Some(next_page_token) = google_mail_thread_list.next_page_token {
                page_token = Some(next_page_token);
            } else {
                break;
            };
        }

        let mut third_party_items = vec![];
        for mut google_mail_thread in google_mail_threads {
            let existing_notification = self
                .notification_service
                .upgrade()
                .context("Unable to access notification_service from google_mail_service")?
                .read()
                .await
                .get_notification_for_source_id(executor, &google_mail_thread.id, user_id)
                .await?;

            if existing_notification.map(|notif| notif.status)
                == Some(NotificationStatus::Unsubscribed)
            {
                let first_unread_message_index = google_mail_thread
                    .messages
                    .iter()
                    .position(|msg| msg.is_tagged_with(GOOGLE_MAIL_UNREAD_LABEL));
                let clear_labels = if let Some(i) = first_unread_message_index {
                    let has_directly_addressed_messages =
                        google_mail_thread.messages.iter().skip(i).any(|msg| {
                            msg.payload.headers.iter().any(|header| {
                                header.name == *"To"
                                    && header
                                        .value
                                        .contains(&google_mail_thread.user_email_address.0)
                            })
                        });
                    if has_directly_addressed_messages {
                        false
                    } else {
                        google_mail_thread
                            .remove_labels(vec![GOOGLE_MAIL_INBOX_LABEL, &config.synced_label.id]);
                        true
                    }
                } else {
                    google_mail_thread
                        .remove_labels(vec![GOOGLE_MAIL_INBOX_LABEL, &config.synced_label.id]);
                    true
                };

                if clear_labels {
                    self.modify_thread(
                        &google_mail_thread.id,
                        vec![],
                        vec![GOOGLE_MAIL_INBOX_LABEL, &config.synced_label.id],
                        &access_token,
                    )
                    .await?;
                }
            }

            let third_party_item = self
                .derive_third_party_item_from_google_mail_thread(
                    executor,
                    &google_mail_thread,
                    user_id,
                    integration_connection.id,
                    &access_token,
                )
                .await
                .unwrap_or_else(|| {
                    google_mail_thread.into_third_party_item(user_id, integration_connection.id)
                });

            third_party_items.push(third_party_item);
        }

        Ok(third_party_items)
    }

    fn is_sync_incremental(&self) -> bool {
        false
    }

    fn get_third_party_item_source_kind(&self) -> ThirdPartyItemSourceKind {
        ThirdPartyItemSourceKind::GoogleMailThread
    }
}

#[async_trait]
impl ThirdPartyNotificationSourceService<GoogleMailThread> for GoogleMailService {
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            source_id = source_third_party_item.source_id,
            third_party_item_id = source_third_party_item.id.to_string(),
            user.id = user_id.to_string(),
        ),
        err
    )]
    async fn third_party_item_into_notification(
        &self,
        source: &GoogleMailThread,
        source_third_party_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<Box<Notification>, UniversalInboxError> {
        let title = source
            .get_message_header(MessageSelection::First, "Subject")
            .unwrap_or_else(|| DEFAULT_SUBJECT.to_string());
        let first_unread_message_index = source
            .messages
            .iter()
            .position(|msg| msg.is_tagged_with(GOOGLE_MAIL_UNREAD_LABEL));
        let last_read_at = if let Some(i) = first_unread_message_index {
            (i > 0).then(|| source.messages[i - 1].internal_date)
        } else {
            Some(source.messages[source.messages.len() - 1].internal_date)
        };
        let thread_is_archived = !source.is_tagged_with(GOOGLE_MAIL_INBOX_LABEL, None);
        let status = if thread_is_archived {
            NotificationStatus::Unsubscribed
        } else {
            let thread_is_unread = source.is_tagged_with(GOOGLE_MAIL_UNREAD_LABEL, None);
            if thread_is_unread {
                NotificationStatus::Unread
            } else {
                NotificationStatus::Read
            }
        };

        Ok(Box::new(Notification {
            id: Uuid::new_v4().into(),
            title,
            status,
            created_at: Utc::now().with_nanosecond(0).unwrap(),
            updated_at: Utc::now().with_nanosecond(0).unwrap(),
            last_read_at,
            snoozed_until: None,
            user_id,
            kind: NotificationSourceKind::GoogleMail,
            source_item: source_third_party_item.clone(),
            task_id: None,
        }))
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(third_party_item_id = source_item.id.to_string(), user.id = user_id.to_string()),
        err
    )]
    async fn delete_notification_from_source(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        source_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let (access_token, integration_connection) = self
            .integration_connection_service
            .upgrade()
            .context("Unable to access integration_connection_service from google_mail_service")?
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::GoogleMail, user_id)
            .await?
            .ok_or_else(|| {
                anyhow!("Cannot delete GoogleMail notification without an access token")
            })?;
        let config = GoogleMailService::get_config(&integration_connection)?;

        self.archive_thread(
            &source_item.source_id,
            &config.synced_label.id,
            &access_token,
        )
        .await
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(third_party_item_id = source_item.id.to_string(), user.id = user_id.to_string()),
        err
    )]
    async fn unsubscribe_notification_from_source(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        source_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let (access_token, integration_connection) = self
            .integration_connection_service
            .upgrade()
            .context("Unable to access integration_connection_service from google_mail_service")?
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::GoogleMail, user_id)
            .await?
            .ok_or_else(|| {
                anyhow!("Cannot unsubscribe from GoogleMail notifications without an access token")
            })?;
        let config = GoogleMailService::get_config(&integration_connection)?;

        self.archive_thread(
            &source_item.source_id,
            &config.synced_label.id,
            &access_token,
        )
        .await
    }

    async fn snooze_notification_from_source(
        &self,
        _executor: &mut Transaction<'_, Postgres>,
        _source_item: &ThirdPartyItem,
        _snoozed_until_at: DateTime<Utc>,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        // Google Mail threads cannot be snoozed from the API => no-op
        Ok(())
    }
}

impl IntegrationProviderSource for GoogleMailService {
    fn get_integration_provider_kind(&self) -> IntegrationProviderKind {
        IntegrationProviderKind::GoogleMail
    }
}

impl NotificationSource for GoogleMailService {
    fn get_notification_source_kind(&self) -> NotificationSourceKind {
        NotificationSourceKind::GoogleMail
    }

    // Snoozing messages is available in Google Mail but not via their public API
    fn is_supporting_snoozed_notifications(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::*;

    mod notification_conversion {
        use super::*;
        use chrono::TimeZone;
        use pretty_assertions::assert_eq;
        use universal_inbox::{
            third_party::integrations::google_mail::{
                GoogleMailMessageHeader, GoogleMailMessagePayload, GOOGLE_MAIL_STARRED_LABEL,
            },
            HasHtmlUrl,
        };

        #[fixture]
        fn google_mail_service() -> GoogleMailService {
            GoogleMailService::new(
                Some("https://gmail.googleapis.com/gmail/v1".to_string()),
                10,
                Weak::new(),
                Weak::new(),
                Arc::new(
                    GoogleCalendarService::new(
                        Some("https://calendar.googleapis.com/calendar/v3".to_string()),
                        Weak::new(),
                    )
                    .unwrap(),
                ),
            )
            .unwrap()
        }

        #[rstest]
        #[tokio::test]
        async fn test_google_mail_thread_into_notification(google_mail_service: GoogleMailService) {
            let google_mail_thread = GoogleMailThread {
                id: "18a909f8178".to_string(),
                history_id: "1234".to_string(),
                user_email_address: "test@example.com".to_string().into(),
                messages: vec![
                    GoogleMailMessage {
                        id: "18a909f8178".to_string(),
                        thread_id: "18a909f8178".to_string(),
                        label_ids: Some(vec![GOOGLE_MAIL_INBOX_LABEL.to_string()]),
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
                            ..Default::default()
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
                            ..Default::default()
                        },
                    },
                ],
            };
            let user_id = Uuid::new_v4().into();
            let google_mail_thread_tpi = google_mail_thread
                .clone()
                .into_third_party_item(user_id, Uuid::new_v4().into());

            let google_mail_notification = google_mail_service
                .third_party_item_into_notification(
                    &google_mail_thread,
                    &google_mail_thread_tpi,
                    user_id,
                )
                .await
                .unwrap();

            assert_eq!(google_mail_notification.title, "test subject".to_string());
            assert_eq!(
                google_mail_notification.source_item.source_id,
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
                google_mail_notification.last_read_at,
                Some(Utc.with_ymd_and_hms(2023, 9, 13, 20, 19, 32).unwrap())
            );
        }

        #[rstest]
        #[tokio::test]
        async fn test_google_mail_thread_with_missing_headers_into_notification(
            google_mail_service: GoogleMailService,
        ) {
            let google_mail_thread = GoogleMailThread {
                id: "18a909f8178".to_string(),
                history_id: "1234".to_string(),
                user_email_address: "test@example.com".to_string().into(),
                messages: vec![
                    GoogleMailMessage {
                        id: "18a909f8178".to_string(),
                        thread_id: "18a909f8178".to_string(),
                        label_ids: Some(vec![
                            GOOGLE_MAIL_INBOX_LABEL.to_string(),
                            GOOGLE_MAIL_UNREAD_LABEL.to_string(),
                        ]),
                        snippet: "test".to_string(),
                        size_estimate: 4,
                        history_id: "5678".to_string(),
                        internal_date: Utc.with_ymd_and_hms(2023, 9, 13, 20, 19, 32).unwrap(),
                        payload: GoogleMailMessagePayload {
                            mime_type: "multipart/mixed".to_string(),
                            headers: vec![],
                            ..Default::default()
                        },
                    },
                    GoogleMailMessage {
                        id: "18a909f8179".to_string(),
                        thread_id: "18a909f8178".to_string(),
                        label_ids: Some(vec![
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
                            ..Default::default()
                        },
                    },
                ],
            };
            let user_id = Uuid::new_v4().into();
            let google_mail_thread_tpi = google_mail_thread
                .clone()
                .into_third_party_item(user_id, Uuid::new_v4().into());

            let google_mail_notification = google_mail_service
                .third_party_item_into_notification(
                    &google_mail_thread,
                    &google_mail_thread_tpi,
                    user_id,
                )
                .await
                .unwrap();

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
        #[tokio::test]
        async fn test_google_mail_read_thread_into_notification(
            google_mail_service: GoogleMailService,
        ) {
            let google_mail_thread = GoogleMailThread {
                id: "18a909f8178".to_string(),
                history_id: "1234".to_string(),
                user_email_address: "test@example.com".to_string().into(),
                messages: vec![
                    GoogleMailMessage {
                        id: "18a909f8178".to_string(),
                        thread_id: "18a909f8178".to_string(),
                        label_ids: Some(vec![GOOGLE_MAIL_INBOX_LABEL.to_string()]),
                        snippet: "test".to_string(),
                        size_estimate: 4,
                        history_id: "5678".to_string(),
                        internal_date: Utc.with_ymd_and_hms(2023, 9, 13, 20, 19, 32).unwrap(),
                        payload: GoogleMailMessagePayload {
                            mime_type: "multipart/mixed".to_string(),
                            headers: vec![],
                            ..Default::default()
                        },
                    },
                    GoogleMailMessage {
                        id: "18a909f8179".to_string(),
                        thread_id: "18a909f8178".to_string(),
                        label_ids: Some(vec![GOOGLE_MAIL_INBOX_LABEL.to_string()]),
                        snippet: "test 2".to_string(),
                        size_estimate: 6,
                        history_id: "5678".to_string(),
                        internal_date: Utc.with_ymd_and_hms(2023, 9, 13, 20, 19, 33).unwrap(),
                        payload: GoogleMailMessagePayload {
                            mime_type: "multipart/mixed".to_string(),
                            headers: vec![],
                            ..Default::default()
                        },
                    },
                ],
            };
            let user_id = Uuid::new_v4().into();
            let google_mail_thread_tpi = google_mail_thread
                .clone()
                .into_third_party_item(user_id, Uuid::new_v4().into());

            let google_mail_notification = google_mail_service
                .third_party_item_into_notification(
                    &google_mail_thread,
                    &google_mail_thread_tpi,
                    user_id,
                )
                .await
                .unwrap();

            assert_eq!(google_mail_notification.status, NotificationStatus::Read);
            assert_eq!(
                google_mail_notification.last_read_at,
                Some(Utc.with_ymd_and_hms(2023, 9, 13, 20, 19, 33).unwrap())
            );
        }

        #[rstest]
        #[tokio::test]
        async fn test_google_mail_unsubscribed_thread_with_no_new_message_into_notification(
            google_mail_service: GoogleMailService,
        ) {
            let google_mail_thread = GoogleMailThread {
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
                            ..Default::default()
                        },
                    },
                    GoogleMailMessage {
                        id: "18a909f8179".to_string(),
                        thread_id: "18a909f8178".to_string(),
                        label_ids: Some(vec![GOOGLE_MAIL_STARRED_LABEL.to_string()]),
                        snippet: "test 2".to_string(),
                        size_estimate: 6,
                        history_id: "5678".to_string(),
                        internal_date: Utc.with_ymd_and_hms(2023, 9, 13, 20, 19, 33).unwrap(),
                        payload: GoogleMailMessagePayload {
                            mime_type: "multipart/mixed".to_string(),
                            headers: vec![],
                            ..Default::default()
                        },
                    },
                ],
            };
            let user_id = Uuid::new_v4().into();
            let google_mail_thread_tpi = google_mail_thread
                .clone()
                .into_third_party_item(user_id, Uuid::new_v4().into());

            let google_mail_notification = google_mail_service
                .third_party_item_into_notification(
                    &google_mail_thread,
                    &google_mail_thread_tpi,
                    user_id,
                )
                .await
                .unwrap();

            assert_eq!(
                google_mail_notification.status,
                NotificationStatus::Unsubscribed
            );
            assert_eq!(
                google_mail_notification.last_read_at,
                Some(Utc.with_ymd_and_hms(2023, 9, 13, 20, 19, 33).unwrap())
            );
        }

        #[rstest]
        #[tokio::test]
        async fn test_google_mail_unsubscribed_thread_with_new_unread_message_into_notification(
            google_mail_service: GoogleMailService,
        ) {
            let google_mail_thread = GoogleMailThread {
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
                            ..Default::default()
                        },
                    },
                    GoogleMailMessage {
                        id: "18a909f8179".to_string(),
                        thread_id: "18a909f8178".to_string(),
                        label_ids: Some(vec![
                            GOOGLE_MAIL_STARRED_LABEL.to_string(),
                            GOOGLE_MAIL_UNREAD_LABEL.to_string(),
                        ]),
                        snippet: "test 2".to_string(),
                        size_estimate: 6,
                        history_id: "5678".to_string(),
                        internal_date: Utc.with_ymd_and_hms(2023, 9, 13, 20, 19, 33).unwrap(),
                        payload: GoogleMailMessagePayload {
                            mime_type: "multipart/mixed".to_string(),
                            headers: vec![],
                            ..Default::default()
                        },
                    },
                ],
            };
            let user_id = Uuid::new_v4().into();
            let google_mail_thread_tpi = google_mail_thread
                .clone()
                .into_third_party_item(user_id, Uuid::new_v4().into());

            let google_mail_notification = google_mail_service
                .third_party_item_into_notification(
                    &google_mail_thread,
                    &google_mail_thread_tpi,
                    user_id,
                )
                .await
                .unwrap();

            assert_eq!(
                google_mail_notification.status,
                NotificationStatus::Unsubscribed
            );
            assert_eq!(
                google_mail_notification.last_read_at,
                Some(Utc.with_ymd_and_hms(2023, 9, 13, 20, 19, 32).unwrap())
            );
        }

        #[rstest]
        #[tokio::test]
        async fn test_google_mail_unsubscribed_thread_with_new_unread_message_directly_addressed_into_notification(
            google_mail_service: GoogleMailService,
        ) {
            let google_mail_thread = GoogleMailThread {
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
                            ..Default::default()
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
                            ..Default::default()
                        },
                    },
                ],
            };
            let user_id = Uuid::new_v4().into();
            let google_mail_thread_tpi = google_mail_thread
                .clone()
                .into_third_party_item(user_id, Uuid::new_v4().into());

            let google_mail_notification = google_mail_service
                .third_party_item_into_notification(
                    &google_mail_thread,
                    &google_mail_thread_tpi,
                    user_id,
                )
                .await
                .unwrap();

            assert_eq!(google_mail_notification.status, NotificationStatus::Unread);
            assert_eq!(
                google_mail_notification.last_read_at,
                Some(Utc.with_ymd_and_hms(2023, 9, 13, 20, 19, 32).unwrap())
            );
        }
    }
}
