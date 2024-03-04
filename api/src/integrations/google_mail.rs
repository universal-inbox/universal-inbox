use std::sync::{Arc, Weak};

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use http::{HeaderMap, HeaderValue};
use itertools::Itertools;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_tracing::{SpanBackendWithUrl, TracingMiddleware};
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_with::serde_as;
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;

use universal_inbox::{
    integration_connection::{
        integrations::google_mail::{GoogleMailConfig, GoogleMailContext},
        provider::{
            IntegrationConnectionContext, IntegrationProvider, IntegrationProviderKind,
            IntegrationProviderSource,
        },
        IntegrationConnection,
    },
    notification::{
        integrations::google_mail::{
            EmailAddress, GoogleMailLabel, GoogleMailMessage, GoogleMailThread,
            GOOGLE_MAIL_INBOX_LABEL,
        },
        Notification, NotificationDetails, NotificationSource, NotificationSourceKind,
        NotificationStatus,
    },
    user::UserId,
};

use crate::{
    integrations::{
        notification::{NotificationSourceService, NotificationSyncSourceService},
        oauth2::AccessToken,
        APP_USER_AGENT,
    },
    universal_inbox::{
        integration_connection::service::IntegrationConnectionService,
        notification::service::NotificationService, UniversalInboxError,
    },
};

#[derive(Clone, Debug)]
pub struct GoogleMailService {
    google_mail_base_url: String,
    page_size: usize,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    notification_service: Weak<RwLock<NotificationService>>,
}

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
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
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
        integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
        notification_service: Weak<RwLock<NotificationService>>,
    ) -> Result<GoogleMailService, UniversalInboxError> {
        Ok(GoogleMailService {
            google_mail_base_url: google_mail_base_url
                .unwrap_or_else(|| GOOGLE_MAIL_BASE_URL.to_string()),
            page_size,
            integration_connection_service,
            notification_service,
        })
    }

    pub fn set_notification_service(
        &mut self,
        notification_service: Weak<RwLock<NotificationService>>,
    ) {
        self.notification_service = notification_service;
    }

    pub async fn get_user_profile(
        &self,
        access_token: &AccessToken,
    ) -> Result<GoogleMailUserProfile, UniversalInboxError> {
        let url = format!("{}/users/me/profile", self.google_mail_base_url);

        let response = build_google_mail_client(access_token)
            .context("Failed to build GoogleMail client")?
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
            "{}/users/me/threads/{thread_id}?prettyPrint=false&format=metadata{}",
            self.google_mail_base_url,
            ["To", "Date", "Subject", "From"]
                .iter()
                .map(|header| format!("&metadataHeaders={header}"))
                .join(""),
        );

        let response = build_google_mail_client(access_token)
            .context("Failed to build GoogleMail client")?
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

        let response = build_google_mail_client(access_token)
            .context("Failed to build GoogleMail client")?
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

        let response = build_google_mail_client(access_token)
            .context("Failed to build GoogleMail client")?
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

        let response = build_google_mail_client(access_token)
            .context("Failed to build GoogleMail client")?
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
}

fn build_google_mail_client(
    access_token: &AccessToken,
) -> Result<ClientWithMiddleware, reqwest::Error> {
    let mut headers = HeaderMap::new();

    let mut auth_header_value: HeaderValue = format!("Bearer {access_token}").parse().unwrap();
    auth_header_value.set_sensitive(true);
    headers.insert("Authorization", auth_header_value);

    let reqwest_client = reqwest::Client::builder()
        .default_headers(headers)
        .user_agent(APP_USER_AGENT)
        .build()?;
    // Tips: The reqwest_retry crate may help with rate limit errors
    // https://docs.rs/reqwest-retry/0.3.0/reqwest_retry/trait.RetryableStrategy.html
    Ok(ClientBuilder::new(reqwest_client)
        .with(TracingMiddleware::<SpanBackendWithUrl>::new())
        .build())
}

#[async_trait]
impl NotificationSyncSourceService for GoogleMailService {
    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(level = "debug", skip(self, executor), err)]
    async fn fetch_all_notifications<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        user_id: UserId,
    ) -> Result<Vec<Notification>, UniversalInboxError> {
        let (access_token, integration_connection) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::GoogleMail, None, user_id)
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

            // Tips: The batch API can be used to for better performance
            // https://developers.google.com/gmail/api/guides/batch
            for thread in &google_mail_thread_list.threads.unwrap_or_default() {
                let google_mail_thread = self
                    .get_thread(&thread.id, &access_token)
                    .await?
                    .into_google_mail_thread(user_email_address.clone());
                google_mail_threads.push(google_mail_thread);
            }

            if let Some(next_page_token) = google_mail_thread_list.next_page_token {
                page_token = Some(next_page_token);
            } else {
                break;
            };
        }

        let mut notifications: Vec<Notification> = vec![];
        for google_mail_thread in google_mail_threads {
            let existing_notification = self
                .notification_service
                .upgrade()
                .context("Unable to access notification_service from google_mail_service")?
                .read()
                .await
                .get_notification_for_source_id(executor, &google_mail_thread.id, user_id)
                .await?;
            let existing_notification_status = existing_notification.map(|notif| notif.status);
            let notification = google_mail_thread.into_notification(
                user_id,
                existing_notification_status,
                &config.synced_label.id,
            );
            if notification.status == NotificationStatus::Unsubscribed {
                self.modify_thread(
                    &notification.source_id,
                    vec![],
                    vec![GOOGLE_MAIL_INBOX_LABEL, &config.synced_label.id],
                    &access_token,
                )
                .await?;
            }
            notifications.push(notification);
        }

        Ok(notifications)
    }
}

#[async_trait]
impl NotificationSourceService for GoogleMailService {
    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(level = "debug", skip(self, executor), err)]
    async fn delete_notification_from_source<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        source_id: &str,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let (access_token, integration_connection) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::GoogleMail, None, user_id)
            .await?
            .ok_or_else(|| {
                anyhow!("Cannot delete GoogleMail notification without an access token")
            })?;
        let config = GoogleMailService::get_config(&integration_connection)?;

        self.archive_thread(source_id, &config.synced_label.id, &access_token)
            .await
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(level = "debug", skip(self, executor), err)]
    async fn unsubscribe_notification_from_source<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        source_id: &str,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let (access_token, integration_connection) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::GoogleMail, None, user_id)
            .await?
            .ok_or_else(|| {
                anyhow!("Cannot unsubscribe from GoogleMail notifications without an access token")
            })?;
        let config = GoogleMailService::get_config(&integration_connection)?;

        self.archive_thread(source_id, &config.synced_label.id, &access_token)
            .await
    }

    async fn snooze_notification_from_source<'a>(
        &self,
        _executor: &mut Transaction<'a, Postgres>,
        _source_id: &str,
        _snoozed_until_at: DateTime<Utc>,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        // Google Mail threads cannot be snoozed from the API => no-op
        Ok(())
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(level = "debug", skip(self, _executor, _notification), fields(notification_id = _notification.id.0.to_string()), err)]
    async fn fetch_notification_details<'a>(
        &self,
        _executor: &mut Transaction<'a, Postgres>,
        _notification: &Notification,
        _user_id: UserId,
    ) -> Result<Option<NotificationDetails>, UniversalInboxError> {
        // Google Mail threads details are fetch as part of the get_thread call in
        // fetch_all_notification
        // Should it be moved here?
        Ok(None)
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
