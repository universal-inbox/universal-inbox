use std::{str::FromStr, sync::Weak, time::Duration};

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use email_address::EmailAddress;
use http::{HeaderMap, HeaderValue};

use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;
use tracing::debug;
use url::Url;
use uuid::Uuid;

use wiremock::{
    matchers::{method, path, path_regex},
    Mock, MockServer, ResponseTemplate,
};

use universal_inbox::{
    integration_connection::{
        integrations::google_drive::GoogleDriveContext,
        provider::{
            IntegrationConnectionContext, IntegrationProvider, IntegrationProviderKind,
            IntegrationProviderSource,
        },
    },
    notification::{
        Notification, NotificationId, NotificationSource, NotificationSourceKind,
        NotificationStatus,
    },
    third_party::{
        integrations::google_drive::{
            GoogleDriveComment, GoogleDriveCommentAuthor, GoogleDriveCommentReply,
        },
        item::{ThirdPartyItem, ThirdPartyItemFromSource, ThirdPartyItemSourceKind},
    },
    user::UserId,
};

use crate::{
    integrations::{
        notification::ThirdPartyNotificationSourceService, oauth2::AccessToken,
        third_party::ThirdPartyItemSourceService,
    },
    universal_inbox::{
        integration_connection::service::IntegrationConnectionService,
        notification::service::NotificationService, UniversalInboxError,
    },
    utils::api::ApiClient,
};

#[derive(Clone)]
pub struct GoogleDriveService {
    google_drive_base_url: String,
    google_drive_base_path: String,
    page_size: usize,
    integration_connection_service: Weak<RwLock<IntegrationConnectionService>>,
    notification_service: Weak<RwLock<NotificationService>>,
    max_retry_duration: Duration,
}

static GOOGLE_DRIVE_BASE_URL: &str = "https://www.googleapis.com/drive/v3";

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GoogleDriveFileList {
    pub files: Option<Vec<GoogleDriveFile>>,
    #[serde(rename = "nextPageToken")]
    pub next_page_token: Option<String>,
    #[serde(rename = "incompleteSearch")]
    pub incomplete_search: Option<bool>,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GoogleDriveFile {
    pub id: String,
    pub name: String,
    #[serde(rename = "modifiedTime")]
    pub modified_time: DateTime<Utc>,
    #[serde(rename = "mimeType")]
    pub mime_type: String,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GoogleDriveCommentList {
    pub comments: Option<Vec<RawGoogleDriveComment>>,
    #[serde(rename = "nextPageToken")]
    pub next_page_token: Option<String>,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct RawGoogleDriveComment {
    pub id: String,
    pub content: String,
    #[serde(rename = "htmlContent")]
    pub html_content: Option<String>,
    #[serde(rename = "quotedFileContent")]
    pub quoted_file_content: Option<RawGoogleDriveQuotedFileContent>,
    pub author: RawGoogleDriveCommentAuthor,
    #[serde(rename = "createdTime")]
    pub created_time: DateTime<Utc>,
    #[serde(rename = "modifiedTime")]
    pub modified_time: DateTime<Utc>,
    #[serde(default)]
    pub resolved: Option<bool>,
    pub replies: Option<Vec<RawGoogleDriveCommentReply>>,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct RawGoogleDriveQuotedFileContent {
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    pub value: String,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct RawGoogleDriveCommentAuthor {
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "emailAddress")]
    pub email_address: Option<String>,
    #[serde(rename = "photoLink")]
    pub photo_link: Option<String>,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct RawGoogleDriveCommentReply {
    pub id: String,
    pub content: String,
    #[serde(rename = "htmlContent")]
    pub html_content: Option<String>,
    pub author: RawGoogleDriveCommentAuthor,
    #[serde(rename = "createdTime")]
    pub created_time: DateTime<Utc>,
    #[serde(rename = "modifiedTime")]
    pub modified_time: DateTime<Utc>,
}

impl RawGoogleDriveCommentReply {
    fn into_google_drive_comment_reply(self) -> GoogleDriveCommentReply {
        GoogleDriveCommentReply {
            id: self.id,
            content: self.content,
            html_content: self.html_content,
            author: GoogleDriveCommentAuthor {
                display_name: self.author.display_name,
                email_address: self.author.email_address,
                photo_link: self.author.photo_link,
            },
            created_time: self.created_time,
            modified_time: self.modified_time,
        }
    }
}

impl RawGoogleDriveComment {
    fn into_google_drive_comment(
        self,
        file_name: String,
        file_id: String,
        file_mime_type: String,
    ) -> GoogleDriveComment {
        let replies = self
            .replies
            .unwrap_or_default()
            .into_iter()
            .map(|reply| reply.into_google_drive_comment_reply())
            .collect();

        GoogleDriveComment {
            id: self.id,
            file_id,
            file_name,
            file_mime_type,
            content: self.content,
            html_content: self.html_content,
            quoted_file_content: self.quoted_file_content.map(|q| q.value),
            author: GoogleDriveCommentAuthor {
                display_name: self.author.display_name,
                email_address: self.author.email_address,
                photo_link: self.author.photo_link,
            },
            created_time: self.created_time,
            modified_time: self.modified_time,
            resolved: self.resolved,
            replies,
            user_email_address: None,
            user_display_name: None,
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct GoogleDriveAboutResponse {
    pub user: GoogleDriveUserInfo,
}

#[derive(Deserialize, Serialize)]
pub struct GoogleDriveUserInfo {
    #[serde(rename = "emailAddress")]
    pub email_address: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
}

impl GoogleDriveService {
    pub fn new(
        google_drive_base_url: Option<String>,
        page_size: usize,
        integration_connection_service: Weak<RwLock<IntegrationConnectionService>>,
        notification_service: Weak<RwLock<NotificationService>>,
        max_retry_duration: Duration,
    ) -> Result<GoogleDriveService, UniversalInboxError> {
        let google_drive_base_url =
            google_drive_base_url.unwrap_or_else(|| GOOGLE_DRIVE_BASE_URL.to_string());
        let google_drive_base_path = Url::parse(&google_drive_base_url)
            .context("Failed to parse Google Drive base URL")?
            .path()
            .to_string();
        Ok(GoogleDriveService {
            google_drive_base_url,
            google_drive_base_path,
            page_size,
            integration_connection_service,
            notification_service,
            max_retry_duration,
        })
    }

    pub async fn mock_all(mock_server: &MockServer) {
        Mock::given(method("GET"))
            .and(path("/about"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "user": {
                    "emailAddress": "test@example.com"
                }
            })))
            .mount(mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/files"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "files": []
            })))
            .mount(mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path_regex(r"/files/.*/comments"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "comments": []
            })))
            .mount(mock_server)
            .await;
    }

    pub fn set_notification_service(
        &mut self,
        notification_service: Weak<RwLock<NotificationService>>,
    ) {
        self.notification_service = notification_service;
    }

    fn build_google_drive_client(
        &self,
        access_token: &AccessToken,
    ) -> Result<ApiClient, UniversalInboxError> {
        let mut headers = HeaderMap::new();

        let mut auth_header_value: HeaderValue = format!("Bearer {access_token}").parse().unwrap();
        auth_header_value.set_sensitive(true);
        headers.insert("Authorization", auth_header_value);

        ApiClient::build(
            headers,
            [
                format!("{}/about", self.google_drive_base_path),
                format!("{}/files", self.google_drive_base_path),
                format!(
                    "{}/files/{{commment_id}}/comments",
                    self.google_drive_base_path
                ),
            ],
            self.max_retry_duration,
        )
    }

    async fn fetch_files_modified_since(
        &self,
        access_token: &AccessToken,
        modified_time: DateTime<Utc>,
    ) -> Result<Vec<GoogleDriveFile>, UniversalInboxError> {
        debug!("Fetching Google Drive files modified since {modified_time}");
        let mut files = Vec::new();
        let mut page_token: Option<String> = None;

        loop {
            let files_url = format!(
                r#"{}/files?includeItemsFromAllDrives=true&supportsAllDrives=true&fields=files(id,name,modifiedTime,mimeType),nextPageToken,incompleteSearch&pageSize={}&q=modifiedTime>"{}"{}"#,
                self.google_drive_base_url,
                self.page_size,
                modified_time.format("%Y-%m-%dT%H:%M:%SZ"),
                page_token
                    .map(|token| format!("&pageToken={token}"))
                    .unwrap_or_default()
            );
            let file_list: GoogleDriveFileList = self
                .build_google_drive_client(access_token)?
                .get(&files_url)
                .await
                .context("Failed to fetch Google Drive files")?;

            if let Some(mut new_files) = file_list.files {
                files.append(&mut new_files);
            }

            if file_list.next_page_token.is_none() {
                break;
            }
            page_token = file_list.next_page_token;
        }

        Ok(files)
    }

    async fn fetch_comments_for_file(
        &self,
        access_token: &AccessToken,
        file_id: &str,
    ) -> Result<Vec<RawGoogleDriveComment>, UniversalInboxError> {
        let mut comments = Vec::new();
        let mut page_token: Option<String> = None;

        loop {
            let comments_url = format!(
                "{}/files/{}/comments?pageSize={}&fields=comments(id,content,htmlContent,quotedFileContent,author,createdTime,modifiedTime,resolved,replies),nextPageToken{}",
                self.google_drive_base_url,
                file_id,
                self.page_size,
                page_token
                    .as_ref()
                    .map(|token| format!("&pageToken={token}"))
                    .unwrap_or_default()
            );
            let comment_list: GoogleDriveCommentList = self
                .build_google_drive_client(access_token)?
                .get(&comments_url)
                .await
                .context("Failed to fetch Google Drive comments")?;

            if let Some(mut new_comments) = comment_list.comments {
                comments.append(&mut new_comments);
            }

            if comment_list.next_page_token.is_none() {
                break;
            }
            page_token = comment_list.next_page_token;
        }

        Ok(comments)
    }

    async fn get_user_info(
        &self,
        access_token: &AccessToken,
    ) -> Result<GoogleDriveUserInfo, UniversalInboxError> {
        let about_url = format!(
            "{}/about?fields=user(emailAddress,displayName)",
            self.google_drive_base_url
        );

        let about: GoogleDriveAboutResponse = self
            .build_google_drive_client(access_token)?
            .get(&about_url)
            .await
            .context("Failed to fetch Google Drive user info")?;

        Ok(about.user)
    }
}

#[async_trait]
impl ThirdPartyItemSourceService<GoogleDriveComment> for GoogleDriveService {
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
        last_sync_completed_at: Option<DateTime<Utc>>,
    ) -> Result<Vec<ThirdPartyItem>, UniversalInboxError> {
        debug!("Fetching Google Drive comments for user {}", user_id);

        let (access_token, integration_connection) = self
            .integration_connection_service
            .upgrade()
            .ok_or_else(|| {
                UniversalInboxError::Unexpected(anyhow!(
                    "Unable to access integration_connection_service from GoogleDriveService"
                ))
            })?
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::GoogleDrive, user_id)
            .await?
            .ok_or_else(|| {
                UniversalInboxError::Unexpected(anyhow!(
                    "Google Drive access token not found for user {}",
                    user_id
                ))
            })?;

        // Determine modified_time for incremental sync
        let modified_time = last_sync_completed_at.unwrap_or(integration_connection.created_at);

        let files = self
            .fetch_files_modified_since(&access_token, modified_time)
            .await?;

        let mut third_party_items = Vec::new();

        // Get user email and display name for mention detection
        let (user_email, display_name) =
            match &integration_connection.provider {
                IntegrationProvider::GoogleDrive {
                    context:
                        Some(GoogleDriveContext {
                            user_email_address,
                            user_display_name,
                        }),
                    ..
                } => (user_email_address.clone(), user_display_name.clone()),
                _ => {
                    let GoogleDriveUserInfo {
                        email_address,
                        display_name,
                    } = self.get_user_info(&access_token).await?;
                    let user_email_address = EmailAddress::from_str(&email_address)
                        .context("Invalid email address from Google Drive user info")?;
                    self.integration_connection_service
            .upgrade()
            .context("Unable to access integration_connection_service from google_drive_service")?
            .read()
            .await
            .update_integration_connection_context(
                executor,
                integration_connection.id,
                IntegrationConnectionContext::GoogleDrive(GoogleDriveContext {
                    user_email_address: user_email_address.clone(),
                    user_display_name: display_name.clone(),
                }),
            )
            .await
            .map_err(|_| {
                anyhow!(
                    "Failed to update Google Drive integration connection {} context",
                    integration_connection.id
                )
            })?;

                    (user_email_address, display_name)
                }
            };

        for file in &files {
            debug!(
                "Fetching comments for Google Drive file {} ({})",
                file.name, file.id
            );
            let raw_comments = self
                .fetch_comments_for_file(&access_token, &file.id)
                .await?;

            for raw_comment in raw_comments {
                let mut comment = raw_comment.into_google_drive_comment(
                    file.name.clone(),
                    file.id.clone(),
                    file.mime_type.clone(),
                );
                comment.user_email_address = Some(user_email.to_string());
                comment.user_display_name = Some(display_name.clone());

                let existing_notification = self
                    .notification_service
                    .upgrade()
                    .context("Unable to access notification_service from google_drive_service")?
                    .read()
                    .await
                    .get_notification_for_source_id(executor, &comment.source_id(), user_id)
                    .await?;

                if should_create_item(&existing_notification, &comment, &display_name, &user_email)
                {
                    let third_party_item =
                        comment.into_third_party_item(user_id, integration_connection.id);
                    third_party_items.push(third_party_item);
                }
            }
        }

        debug!(
            "Fetched {} Google Drive comments in {} files for user {}",
            third_party_items.len(),
            files.len(),
            user_id
        );

        Ok(third_party_items)
    }

    fn is_sync_incremental(&self) -> bool {
        true
    }

    fn get_third_party_item_source_kind(&self) -> ThirdPartyItemSourceKind {
        ThirdPartyItemSourceKind::GoogleDriveComment
    }
}

#[async_trait]
impl ThirdPartyNotificationSourceService<GoogleDriveComment> for GoogleDriveService {
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            source_id = source_third_party_item.source_id,
            user.id = user_id.to_string(),
        ),
        err
    )]
    async fn third_party_item_into_notification(
        &self,
        source: &GoogleDriveComment,
        source_third_party_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<Box<Notification>, UniversalInboxError> {
        let title = format!("Comment on {}", source.file_name);

        // If the user sent the last reply, mark as Deleted (user already responded)
        let status = if source.is_last_reply_from_user() {
            NotificationStatus::Deleted
        } else {
            NotificationStatus::Unread
        };

        Ok(Box::new(Notification {
            id: NotificationId(Uuid::new_v4()),
            title,
            status,
            created_at: source.created_time,
            updated_at: source.modified_time,
            last_read_at: None,
            snoozed_until: None,
            user_id,
            task_id: None,
            kind: NotificationSourceKind::GoogleDrive,
            source_item: source_third_party_item.clone(),
        }))
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            source_id = source_item.source_id,
            user.id = _user_id.to_string(),
        ),
        err
    )]
    async fn delete_notification_from_source(
        &self,
        _executor: &mut Transaction<'_, Postgres>,
        source_item: &ThirdPartyItem,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        // Google Drive comments cannot be deleted from Universal Inbox
        // This is a no-op as specified in the data model
        Ok(())
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            source_id = source_item.source_id,
            user.id = _user_id.to_string(),
        ),
        err
    )]
    async fn unsubscribe_notification_from_source(
        &self,
        _executor: &mut Transaction<'_, Postgres>,
        source_item: &ThirdPartyItem,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        // Google Drive comments cannot be unsubscribed from Universal Inbox
        // This is a no-op as specified in the data model
        Ok(())
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            source_id = source_item.source_id,
            user.id = _user_id.to_string(),
        ),
        err
    )]
    async fn snooze_notification_from_source(
        &self,
        _executor: &mut Transaction<'_, Postgres>,
        source_item: &ThirdPartyItem,
        _snoozed_until_at: DateTime<Utc>,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        // Google Drive comments cannot be snoozed from Universal Inbox
        // This is a no-op as specified in the data model
        Ok(())
    }
}

fn should_create_item(
    existing_notification: &Option<Notification>,
    comment: &GoogleDriveComment,
    display_name: &str,
    user_email: &EmailAddress,
) -> bool {
    let last_existing_third_party_item_update = existing_notification
        .as_ref()
        .map(|n| n.source_item.updated_at);

    if let Some(last_update) = last_existing_third_party_item_update {
        if existing_notification.as_ref().unwrap().status != NotificationStatus::Unsubscribed {
            // If the existing notification is not unsubscribed, create a new item only if the
            // comment is newer than the existing notification
            return comment.modified_time > last_update;
        }
    }

    comment.is_user_mentioned(
        display_name,
        user_email.as_ref(),
        last_existing_third_party_item_update,
    )
}

#[async_trait]
impl NotificationSource for GoogleDriveService {
    fn get_notification_source_kind(&self) -> NotificationSourceKind {
        NotificationSourceKind::GoogleDrive
    }

    fn is_supporting_snoozed_notifications(&self) -> bool {
        false
    }
}

impl IntegrationProviderSource for GoogleDriveService {
    fn get_integration_provider_kind(&self) -> IntegrationProviderKind {
        IntegrationProviderKind::GoogleDrive
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use pretty_assertions::assert_eq;
    use rstest::*;

    #[rstest]
    fn test_raw_comment_to_google_drive_comment() {
        let raw_comment = RawGoogleDriveComment {
            id: "comment_123".to_string(),
            content: "Test comment".to_string(),
            html_content: Some("<p>Test comment</p>".to_string()),
            quoted_file_content: Some(RawGoogleDriveQuotedFileContent {
                mime_type: "text/plain".to_string(),
                value: "quoted text".to_string(),
            }),
            author: RawGoogleDriveCommentAuthor {
                display_name: "John Doe".to_string(),
                email_address: Some("john@example.com".to_string()),
                photo_link: Some("https://example.com/photo.jpg".to_string()),
            },
            created_time: Utc.with_ymd_and_hms(2025, 9, 28, 9, 0, 0).unwrap(),
            modified_time: Utc.with_ymd_and_hms(2025, 9, 28, 9, 30, 0).unwrap(),
            resolved: Some(false),
            replies: Some(vec![RawGoogleDriveCommentReply {
                id: "reply_123".to_string(),
                content: "Test reply".to_string(),
                html_content: Some("<p>Test reply</p>".to_string()),
                author: RawGoogleDriveCommentAuthor {
                    display_name: "Jane Doe".to_string(),
                    email_address: Some("jane@example.com".to_string()),
                    photo_link: None,
                },
                created_time: Utc.with_ymd_and_hms(2025, 9, 28, 10, 0, 0).unwrap(),
                modified_time: Utc.with_ymd_and_hms(2025, 9, 28, 10, 5, 0).unwrap(),
            }]),
        };

        let comment = raw_comment.into_google_drive_comment(
            "Test Document.docx".to_string(),
            "file_456".to_string(),
            "application/vnd.google-apps.spreadsheet".to_string(),
        );

        assert_eq!(comment.id, "comment_123");
        assert_eq!(comment.file_name, "Test Document.docx");
        assert_eq!(comment.file_id, "file_456");
        assert_eq!(comment.content, "Test comment");
        assert_eq!(comment.author.display_name, "John Doe");
        assert_eq!(comment.replies.len(), 1);
        assert_eq!(comment.replies[0].content, "Test reply");
    }

    mod test_should_create_item {
        use super::*;
        use pretty_assertions::assert_eq;

        #[fixture]
        fn comment_author() -> GoogleDriveCommentAuthor {
            GoogleDriveCommentAuthor {
                display_name: "John Doe".to_string(),
                email_address: Some("john.doe@example.com".to_string()),
                photo_link: Some("https://example.com/photo.jpg".to_string()),
            }
        }

        #[fixture]
        fn comment_reply() -> GoogleDriveCommentReply {
            GoogleDriveCommentReply {
                id: "reply_123".to_string(),
                content: "This is a reply".to_string(),
                html_content: Some("<p>This is a reply</p>".to_string()),
                author: GoogleDriveCommentAuthor {
                    display_name: "Other user".to_string(),
                    email_address: None,
                    photo_link: None,
                },
                created_time: Utc.with_ymd_and_hms(2025, 9, 28, 10, 0, 0).unwrap(),
                modified_time: Utc.with_ymd_and_hms(2025, 9, 28, 10, 5, 0).unwrap(),
            }
        }

        #[fixture]
        fn google_drive_comment(
            comment_author: GoogleDriveCommentAuthor,
            comment_reply: GoogleDriveCommentReply,
        ) -> GoogleDriveComment {
            GoogleDriveComment {
                id: "comment_123".to_string(),
                file_id: "file_456".to_string(),
                file_name: "Test Document.docx".to_string(),
                file_mime_type: "application/vnd.google-apps.document".to_string(),
                content: "This is a test comment".to_string(),
                html_content: Some("<p>This is a test comment</p>".to_string()),
                quoted_file_content: Some("quoted text from document".to_string()),
                author: comment_author,
                created_time: Utc.with_ymd_and_hms(2025, 9, 28, 9, 0, 0).unwrap(),
                modified_time: Utc.with_ymd_and_hms(2025, 9, 28, 9, 30, 0).unwrap(),
                resolved: Some(false),
                replies: vec![comment_reply],
                user_email_address: None,
                user_display_name: None,
            }
        }

        #[fixture]
        fn google_drive_comment_notification(
            google_drive_comment: GoogleDriveComment,
        ) -> Notification {
            Notification {
                id: NotificationId(Uuid::new_v4()),
                title: format!("Comment on {}", google_drive_comment.file_name),
                status: NotificationStatus::Unread,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                last_read_at: None,
                snoozed_until: None,
                user_id: Uuid::new_v4().into(),
                task_id: None,
                kind: NotificationSourceKind::GoogleDrive,
                source_item: google_drive_comment
                    .into_third_party_item(Uuid::new_v4().into(), Uuid::new_v4().into()),
            }
        }

        #[rstest]
        fn test_no_existing_notification_and_no_mention(google_drive_comment: GoogleDriveComment) {
            let existing_notification = None;
            let display_name = "Jane Doe";
            let user_email = &EmailAddress::from_str("jane.doe@example.com").unwrap();

            assert!(!should_create_item(
                &existing_notification,
                &google_drive_comment,
                display_name,
                user_email
            ));
        }

        #[rstest]
        fn test_no_existing_notification_and_mention(
            google_drive_comment: GoogleDriveComment,
            comment_author: GoogleDriveCommentAuthor,
        ) {
            let existing_notification = None;
            let display_name = &comment_author.display_name;
            let user_email =
                &EmailAddress::from_str(comment_author.email_address.as_ref().unwrap()).unwrap();

            assert!(should_create_item(
                &existing_notification,
                &google_drive_comment,
                display_name,
                user_email
            ));
        }

        #[rstest]
        fn test_existing_notification_with_mention_in_already_known_comments(
            google_drive_comment_notification: Notification,
            google_drive_comment: GoogleDriveComment,
            comment_author: GoogleDriveCommentAuthor,
        ) {
            let existing_notification = Some(google_drive_comment_notification);
            let display_name = &comment_author.display_name;
            let user_email =
                &EmailAddress::from_str(comment_author.email_address.as_ref().unwrap()).unwrap();

            // Should not create a new item since the mention is in already known comments
            assert!(!should_create_item(
                &existing_notification,
                &google_drive_comment,
                display_name,
                user_email
            ));
        }

        #[rstest]
        #[case::with_unread_notification(NotificationStatus::Unread, true)]
        #[case::with_read_notification(NotificationStatus::Read, true)]
        #[case::with_unsubscribed_notification(NotificationStatus::Unsubscribed, false)]
        fn test_existing_notification_without_mention_in_new_comments(
            mut google_drive_comment_notification: Notification,
            mut google_drive_comment: GoogleDriveComment,
            #[case] status: NotificationStatus,
            #[case] expected: bool,
        ) {
            google_drive_comment_notification.status = status;
            google_drive_comment.modified_time =
                google_drive_comment_notification.source_item.updated_at
                    + chrono::Duration::minutes(10);
            let existing_notification = Some(google_drive_comment_notification);
            let display_name = "Jane Doe";
            let user_email = &EmailAddress::from_str("jane.doe@example.com").unwrap();

            // Should create a new item since the comment is newer than the existing notification
            // only if the existing notification is not unsubscribed
            assert_eq!(
                should_create_item(
                    &existing_notification,
                    &google_drive_comment,
                    display_name,
                    user_email
                ),
                expected
            );
        }

        #[rstest]
        #[case::with_unread_notification(NotificationStatus::Unread)]
        #[case::with_read_notification(NotificationStatus::Read)]
        #[case::with_unsubscribed_notification(NotificationStatus::Unsubscribed)]
        fn test_existing_notification_with_mention_in_new_comments(
            mut google_drive_comment_notification: Notification,
            mut google_drive_comment: GoogleDriveComment,
            comment_author: GoogleDriveCommentAuthor,
            #[case] status: NotificationStatus,
        ) {
            google_drive_comment_notification.status = status;
            google_drive_comment.modified_time =
                google_drive_comment_notification.source_item.updated_at
                    + chrono::Duration::minutes(10);
            let existing_notification = Some(google_drive_comment_notification);
            let display_name = &comment_author.display_name;
            let user_email =
                &EmailAddress::from_str(comment_author.email_address.as_ref().unwrap()).unwrap();

            // Should create a new item since the mention is newer than the existing notification
            // even if the existing notification is unsubscribed
            assert!(should_create_item(
                &existing_notification,
                &google_drive_comment,
                display_name,
                user_email
            ));
        }
    }
}
