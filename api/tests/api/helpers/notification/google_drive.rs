use chrono::{DateTime, Utc};
use pretty_assertions::assert_eq;
use rstest::*;
use url::Url;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use universal_inbox::{
    HasHtmlUrl,
    integration_connection::IntegrationConnectionId,
    notification::{Notification, NotificationSourceKind, NotificationStatus},
    third_party::{
        integrations::google_drive::GoogleDriveComment,
        item::{ThirdPartyItemData, ThirdPartyItemFromSource},
    },
    user::UserId,
};

use universal_inbox_api::integrations::google_drive::{
    GoogleDriveAboutResponse, GoogleDriveCommentList, GoogleDriveFileList,
};

use crate::helpers::{
    QueryParamAbsent, TestedApp, load_json_fixture_file,
    notification::create_notification_from_source_item,
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

pub async fn mock_google_drive_get_user_info_service(
    google_drive_mock_server: &MockServer,
    result: &GoogleDriveAboutResponse,
) {
    Mock::given(method("GET"))
        .and(path("/about"))
        .and(query_param("fields", "user(emailAddress,displayName)"))
        .and(header(
            "authorization",
            "Bearer google_drive_test_access_token",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(result),
        )
        .mount(google_drive_mock_server)
        .await;
}

pub async fn mock_google_drive_files_list_service(
    google_drive_mock_server: &MockServer,
    page_token: Option<&str>,
    per_page: usize,
    modified_time: DateTime<Utc>,
    result: &GoogleDriveFileList,
) {
    let mut mock_builder = Mock::given(method("GET"))
        .and(path("/files"))
        .and(header(
            "authorization",
            "Bearer google_drive_test_access_token",
        ))
        .and(query_param("includeItemsFromAllDrives", "true"))
        .and(query_param("supportsAllDrives", "true"))
        .and(query_param(
            "fields",
            "files(id,name,modifiedTime,mimeType),nextPageToken,incompleteSearch",
        ))
        .and(query_param("pageSize", per_page.to_string()))
        .and(query_param(
            "q",
            format!(
                r#"modifiedTime>"{}""#,
                &modified_time.format("%Y-%m-%dT%H:%M:%SZ")
            ),
        ));

    if let Some(page_token) = page_token {
        mock_builder = mock_builder.and(query_param("pageToken", page_token.to_string()));
    } else {
        mock_builder = mock_builder.and(QueryParamAbsent("pageToken".to_string()));
    }

    mock_builder
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(result),
        )
        .mount(google_drive_mock_server)
        .await;
}

pub async fn mock_google_drive_comments_list_service(
    google_drive_mock_server: &MockServer,
    page_token: Option<&str>,
    per_page: usize,
    file_id: &str,
    result: &GoogleDriveCommentList,
) {
    let mut mock_builder = Mock::given(method("GET"))
        .and(path(format!("/files/{}/comments", file_id)))
        .and(header("authorization", "Bearer google_drive_test_access_token"))
        .and(query_param("pageSize", per_page.to_string()))
        .and(query_param(
            "fields",
            "comments(id,content,htmlContent,quotedFileContent,author,createdTime,modifiedTime,resolved,replies),nextPageToken",
        ));

    if let Some(page_token) = page_token {
        mock_builder = mock_builder.and(query_param("pageToken", page_token.to_string()));
    } else {
        mock_builder = mock_builder.and(QueryParamAbsent("pageToken".to_string()));
    }

    mock_builder
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(result),
        )
        .mount(google_drive_mock_server)
        .await;
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
    user_email_address: &str,
    user_display_name: &str,
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
                    ThirdPartyItemData::GoogleDriveComment(Box::new(GoogleDriveComment {
                        user_email_address: Some(user_email_address.to_string()),
                        user_display_name: Some(user_display_name.to_string()),
                        ..google_drive_comment_123.clone()
                    }))
                );
            }
            // This notification should be updated
            // The last reply on this comment is from the user (jane.doe@example.com),
            // so the notification should be marked as Deleted (user already responded)
            "1AbCdEfGhIjKlMnOpQrStUvWxYz#AAAABUiR-5ub_7yjYZKluDfg8a8AAANV" => {
                assert_eq!(
                    notification.title,
                    "Comment on Project Proposal - Q4 2025.docx".to_string()
                );
                assert_eq!(notification.kind, NotificationSourceKind::GoogleDrive);
                assert_eq!(notification.status, NotificationStatus::Deleted);
                assert_eq!(
                    notification.get_html_url(),
                    "https://docs.google.com/document/d/1AbCdEfGhIjKlMnOpQrStUvWxYz/edit?disco=AAAABUiR-5ub_7yjYZKluDfg8a8AAANV"
                        .parse::<Url>()
                        .unwrap()
                );
                assert_eq!(notification.last_read_at, None);
                assert_eq!(
                    notification.source_item.data,
                    ThirdPartyItemData::GoogleDriveComment(Box::new(GoogleDriveComment {
                        user_email_address: Some(user_email_address.to_string()),
                        user_display_name: Some(user_display_name.to_string()),
                        ..google_drive_comment_456.clone()
                    }))
                );
            }
            _ => {
                unreachable!("Unexpected notification title '{}'", &notification.title);
            }
        }
    }
}
