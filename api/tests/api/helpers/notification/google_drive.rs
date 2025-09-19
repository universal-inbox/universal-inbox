use chrono::{DateTime, Utc};
use httpmock::{prelude::HttpMockRequest, Method::GET, Mock, MockServer};
use pretty_assertions::assert_eq;
use rstest::*;
use url::Url;

use universal_inbox::{
    integration_connection::IntegrationConnectionId,
    notification::{Notification, NotificationSourceKind, NotificationStatus},
    third_party::{
        integrations::google_drive::GoogleDriveComment,
        item::{ThirdPartyItemData, ThirdPartyItemFromSource},
    },
    user::UserId,
    HasHtmlUrl,
};

use universal_inbox_api::integrations::google_drive::{
    GoogleDriveAboutResponse, GoogleDriveCommentList, GoogleDriveFileList,
};

use crate::helpers::{
    load_json_fixture_file, notification::create_notification_from_source_item, TestedApp,
};

pub async fn create_notification_from_google_drive_comment(
    app: &TestedApp,
    google_drive_comment: &GoogleDriveComment,
    user_id: UserId,
    google_drive_integration_connection_id: IntegrationConnectionId,
) -> Box<Notification> {
    create_notification_from_source_item(
        app,
        google_drive_comment.source_id(),
        ThirdPartyItemData::GoogleDriveComment(Box::new(google_drive_comment.clone())),
        (*app
            .notification_service
            .read()
            .await
            .google_drive_service
            .read()
            .await)
            .clone()
            .into(),
        user_id,
        google_drive_integration_connection_id,
    )
    .await
}

pub fn mock_google_drive_get_user_info_service<'a>(
    google_drive_mock_server: &'a MockServer,
    result: &'a GoogleDriveAboutResponse,
) -> Mock<'a> {
    google_drive_mock_server.mock(|when, then| {
        when.method(GET)
            .path("/about")
            .query_param("fields", "user(emailAddress,displayName)")
            .header("authorization", "Bearer google_drive_test_access_token");
        then.status(200)
            .header("content-type", "application/json")
            .json_body_obj(result);
    })
}

pub fn mock_google_drive_files_list_service<'a>(
    google_drive_mock_server: &'a MockServer,
    page_token: Option<&'a str>,
    per_page: usize,
    modified_time: DateTime<Utc>,
    result: &'a GoogleDriveFileList,
) -> Mock<'a> {
    google_drive_mock_server.mock(|when, then| {
        let when = when
            .method(GET)
            .path("/files")
            .header("authorization", "Bearer google_drive_test_access_token")
            .query_param("includeItemsFromAllDrives", "true")
            .query_param("supportsAllDrives", "true")
            .query_param(
                "fields",
                "files(id,name,modifiedTime,mimeType),nextPageToken,incompleteSearch",
            )
            .query_param("pageSize", per_page.to_string())
            .query_param(
                "q",
                format!(
                    r#"modifiedTime>"{}""#,
                    &modified_time.format("%Y-%m-%dT%H:%M:%SZ")
                ),
            );

        if let Some(page_token) = page_token {
            when.query_param("pageToken", page_token.to_string());
        } else {
            when.matches(|req: &HttpMockRequest| {
                req.query_params
                    .as_ref()
                    .map(|param| !param.iter().any(|(name, _)| name == "pageToken"))
                    .unwrap_or(true)
            });
        }

        then.status(200)
            .header("content-type", "application/json")
            .json_body_obj(result);
    })
}

pub fn mock_google_drive_comments_list_service<'a>(
    google_drive_mock_server: &'a MockServer,
    page_token: Option<&'a str>,
    per_page: usize,
    file_id: &str,
    result: &'a GoogleDriveCommentList,
) -> Mock<'a> {
    google_drive_mock_server.mock(|when, then| {
        let when = when
            .method(GET)
            .path(format!("/files/{}/comments", file_id))
            .header("authorization", "Bearer google_drive_test_access_token")
            .query_param("pageSize", per_page.to_string())
            .query_param(
                "fields",
                "comments(id,content,htmlContent,quotedFileContent,author,createdTime,modifiedTime,resolved,replies),nextPageToken",
            );

        if let Some(page_token) = page_token {
            when.query_param("pageToken", page_token.to_string());
        } else {
            when.matches(|req: &HttpMockRequest| {
                req.query_params
                    .as_ref()
                    .map(|param| !param.iter().any(|(name, _)| name == "pageToken"))
                    .unwrap_or(true)
            });
        }

        then.status(200)
            .header("content-type", "application/json")
            .json_body_obj(result);
    })
}

#[fixture]
pub fn google_drive_files_list() -> GoogleDriveFileList {
    load_json_fixture_file("google_drive/google_drive_files_list.json")
}

#[fixture]
pub fn google_drive_comment_123() -> GoogleDriveComment {
    load_json_fixture_file("google_drive/google_drive_comment_123.json")
}

#[fixture]
pub fn google_drive_comment_456() -> GoogleDriveComment {
    load_json_fixture_file("google_drive/google_drive_comment_456.json")
}

#[fixture]
pub fn google_drive_comments_list() -> GoogleDriveCommentList {
    load_json_fixture_file("google_drive/google_drive_comments_list.json")
}

pub fn assert_sync_notifications(
    notifications: &[Notification],
    google_drive_comment_123: &GoogleDriveComment,
    google_drive_comment_456: &GoogleDriveComment,
    expected_user_id: UserId,
) {
    for notification in notifications.iter() {
        assert_eq!(notification.user_id, expected_user_id);
        match notification.source_item.source_id.as_ref() {
            "1AbCdEfGhIjKlMnOpQrStUvWxYz#AAAABUiR-5ub_7yjYZKluDfg8a8AAANM" => {
                assert_eq!(
                    notification.title,
                    "Comment on Project Proposal - Q4 2025.docx".to_string()
                );
                assert_eq!(notification.kind, NotificationSourceKind::GoogleDrive);
                assert_eq!(notification.status, NotificationStatus::Unread);
                assert_eq!(
                    notification.get_html_url(),
                    "https://docs.google.com/document/d/1AbCdEfGhIjKlMnOpQrStUvWxYz/edit?disco=AAAABUiR-5ub_7yjYZKluDfg8a8AAANM"
                        .parse::<Url>()
                        .unwrap()
                );
                assert_eq!(notification.last_read_at, None);
                assert_eq!(
                    notification.source_item.data,
                    ThirdPartyItemData::GoogleDriveComment(Box::new(
                        google_drive_comment_123.clone()
                    ))
                );
            }
            // This notification should be updated
            "1AbCdEfGhIjKlMnOpQrStUvWxYz#AAAABUiR-5ub_7yjYZKluDfg8a8AAANV" => {
                assert_eq!(
                    notification.title,
                    "Comment on Project Proposal - Q4 2025.docx".to_string()
                );
                assert_eq!(notification.kind, NotificationSourceKind::GoogleDrive);
                assert_eq!(notification.status, NotificationStatus::Unread);
                assert_eq!(
                    notification.get_html_url(),
                    "https://docs.google.com/document/d/1AbCdEfGhIjKlMnOpQrStUvWxYz/edit?disco=AAAABUiR-5ub_7yjYZKluDfg8a8AAANV"
                        .parse::<Url>()
                        .unwrap()
                );
                assert_eq!(notification.last_read_at, None);
                assert_eq!(
                    notification.source_item.data,
                    ThirdPartyItemData::GoogleDriveComment(Box::new(
                        google_drive_comment_456.clone()
                    ))
                );
            }
            _ => {
                unreachable!("Unexpected notification title '{}'", &notification.title);
            }
        }
    }
}
