use std::str::FromStr;

use chrono::{TimeZone, Utc};
use email_address::EmailAddress;
use httpmock::{
    prelude::HttpMockRequest,
    Method::{GET, POST},
    Mock, MockServer,
};
use rstest::*;
use serde_json::json;
use url::Url;

use universal_inbox::{
    integration_connection::IntegrationConnectionId,
    notification::{Notification, NotificationSourceKind, NotificationStatus},
    third_party::{
        integrations::google_mail::{GoogleMailMessageBody, GoogleMailThread},
        item::ThirdPartyItemData,
    },
    user::UserId,
    HasHtmlUrl,
};

use universal_inbox_api::integrations::google_mail::{
    GoogleMailLabelList, GoogleMailThreadList, GoogleMailUserProfile, RawGoogleMailThread,
};

use crate::helpers::{
    load_json_fixture_file, notification::create_notification_from_source_item, TestedApp,
};

pub async fn create_notification_from_google_mail_thread(
    app: &TestedApp,
    google_mail_thread: &GoogleMailThread,
    user_id: UserId,
    google_mail_integration_connection_id: IntegrationConnectionId,
) -> Box<Notification> {
    create_notification_from_source_item(
        app,
        google_mail_thread.id.to_string(),
        ThirdPartyItemData::GoogleMailThread(Box::new(google_mail_thread.clone())),
        (*app
            .notification_service
            .read()
            .await
            .google_mail_service
            .read()
            .await)
            .clone()
            .into(),
        user_id,
        google_mail_integration_connection_id,
    )
    .await
}

pub fn mock_google_mail_get_user_profile_service<'a>(
    google_mail_mock_server: &'a MockServer,
    result: &'a GoogleMailUserProfile,
) -> Mock<'a> {
    google_mail_mock_server.mock(|when, then| {
        when.method(GET)
            .path("/users/me/profile")
            .header("authorization", "Bearer google_mail_test_access_token");
        then.status(200)
            .header("content-type", "application/json")
            .json_body_obj(result);
    })
}

pub fn mock_google_mail_labels_list_service<'a>(
    google_mail_mock_server: &'a MockServer,
    result: &'a GoogleMailLabelList,
) -> Mock<'a> {
    google_mail_mock_server.mock(|when, then| {
        when.method(GET)
            .path("/users/me/labels")
            .header("authorization", "Bearer google_mail_test_access_token");
        then.status(200)
            .header("content-type", "application/json")
            .json_body_obj(result);
    })
}

pub fn mock_google_mail_threads_list_service<'a>(
    google_mail_mock_server: &'a MockServer,
    page_token: Option<&'a str>,
    per_page: usize,
    label_ids: Option<Vec<String>>,
    result: &'a GoogleMailThreadList,
) -> Mock<'a> {
    google_mail_mock_server.mock(|when, then| {
        let mut when = when
            .method(GET)
            .path("/users/me/threads")
            .header("authorization", "Bearer google_mail_test_access_token")
            .query_param("prettyPrint", "false")
            .query_param("maxResults", per_page.to_string());

        if let Some(label_ids) = label_ids {
            for label_id in label_ids {
                when = when.query_param("labelIds", label_id);
            }
        } else {
            when = when.matches(|req: &HttpMockRequest| {
                req.query_params
                    .as_ref()
                    .map(|param| !param.iter().any(|(name, _)| name == "labelIds"))
                    .unwrap_or(true)
            });
        }

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

pub fn mock_google_mail_thread_get_service<'a>(
    google_mail_mock_server: &'a MockServer,
    thread_id: &'a str,
    result: &'a RawGoogleMailThread,
) -> Mock<'a> {
    google_mail_mock_server.mock(|when, then| {
        when.method(GET)
            .path(format!("/users/me/threads/{thread_id}"))
            .header("authorization", "Bearer google_mail_test_access_token")
            .query_param("prettyPrint", "false")
            .query_param("format", "full");
        then.status(200)
            .header("content-type", "application/json")
            .json_body_obj(result);
    })
}

pub fn mock_google_mail_thread_modify_service<'a>(
    google_mail_mock_server: &'a MockServer,
    thread_id: &'a str,
    labels_to_add: Vec<&'a str>,
    labels_to_remove: Vec<&'a str>,
) -> Mock<'a> {
    google_mail_mock_server.mock(|when, then| {
        when.method(POST)
            .path(format!("/users/me/threads/{thread_id}/modify"))
            .body(
                json!({
                    "addLabelIds": labels_to_add,
                    "removeLabelIds": labels_to_remove
                })
                .to_string(),
            )
            .header("authorization", "Bearer google_mail_test_access_token");
        then.status(200).header("content-type", "application/json");
    })
}

pub fn mock_google_mail_get_attachment_service<'a>(
    google_mail_mock_server: &'a MockServer,
    message_id: &'a str,
    attachment_id: &'a str,
    result: &'a GoogleMailMessageBody,
) -> Mock<'a> {
    google_mail_mock_server.mock(|when, then| {
        when.method(GET)
            .path(format!(
                "/users/me/messages/{message_id}/attachments/{attachment_id}"
            ))
            .header("authorization", "Bearer google_mail_test_access_token");
        then.status(200)
            .header("content-type", "application/json")
            .json_body_obj(result);
    })
}

#[fixture]
pub fn google_mail_invitation_attachment() -> GoogleMailMessageBody {
    load_json_fixture_file("google_mail_invitation_attachment.json")
}

#[fixture]
pub fn google_mail_invitation_reply_attachment() -> GoogleMailMessageBody {
    load_json_fixture_file("google_mail_invitation_reply_attachment.json")
}

#[fixture]
pub fn google_mail_user_profile() -> GoogleMailUserProfile {
    load_json_fixture_file("google_mail_user_profile.json")
}

#[fixture]
pub fn google_mail_labels_list() -> GoogleMailLabelList {
    load_json_fixture_file("google_mail_labels_list.json")
}

#[fixture]
pub fn raw_google_mail_thread_get_123() -> RawGoogleMailThread {
    load_json_fixture_file("google_mail_thread_get_123.json")
}

#[fixture]
pub fn google_mail_thread_get_123(
    raw_google_mail_thread_get_123: RawGoogleMailThread,
    google_mail_user_profile: GoogleMailUserProfile,
) -> GoogleMailThread {
    let user_email_address =
        EmailAddress::from_str(&google_mail_user_profile.email_address).unwrap();
    raw_google_mail_thread_get_123.into_google_mail_thread(user_email_address)
}

#[fixture]
pub fn raw_google_mail_thread_get_456() -> RawGoogleMailThread {
    load_json_fixture_file("google_mail_thread_get_456.json")
}

#[fixture]
pub fn google_mail_thread_get_456(
    raw_google_mail_thread_get_456: RawGoogleMailThread,
    google_mail_user_profile: GoogleMailUserProfile,
) -> GoogleMailThread {
    let user_email_address =
        EmailAddress::from_str(&google_mail_user_profile.email_address).unwrap();
    raw_google_mail_thread_get_456.into_google_mail_thread(user_email_address)
}

#[fixture]
pub fn raw_google_mail_thread_with_invitation() -> RawGoogleMailThread {
    load_json_fixture_file("google_mail_thread_with_invitation.json")
}

#[fixture]
pub fn google_mail_thread_with_invitation(
    raw_google_mail_thread_with_invitation: RawGoogleMailThread,
    google_mail_user_profile: GoogleMailUserProfile,
) -> GoogleMailThread {
    let user_email_address =
        EmailAddress::from_str(&google_mail_user_profile.email_address).unwrap();
    raw_google_mail_thread_with_invitation.into_google_mail_thread(user_email_address)
}

#[fixture]
pub fn raw_google_mail_thread_with_invitation_reply() -> RawGoogleMailThread {
    load_json_fixture_file("google_mail_thread_with_invitation_reply.json")
}

#[fixture]
pub fn google_mail_thread_with_invitation_reply(
    raw_google_mail_thread_with_invitation_reply: RawGoogleMailThread,
    google_mail_user_profile: GoogleMailUserProfile,
) -> GoogleMailThread {
    let user_email_address =
        EmailAddress::from_str(&google_mail_user_profile.email_address).unwrap();
    raw_google_mail_thread_with_invitation_reply.into_google_mail_thread(user_email_address)
}

pub fn assert_sync_notifications(
    notifications: &[Notification],
    google_mail_thread_123: &GoogleMailThread,
    google_mail_thread_456: &GoogleMailThread,
    expected_user_id: UserId,
) {
    for notification in notifications.iter() {
        assert_eq!(notification.user_id, expected_user_id);
        match notification.source_item.source_id.as_ref() {
            "123" => {
                assert_eq!(notification.title, "News from Universal Inbox".to_string());
                assert_eq!(notification.kind, NotificationSourceKind::GoogleMail);
                assert_eq!(notification.status, NotificationStatus::Unread);
                assert_eq!(
                    notification.get_html_url(),
                    "https://mail.google.com/mail/u/user@example.com/#inbox/123"
                        .parse::<Url>()
                        .unwrap()
                );
                assert_eq!(notification.last_read_at, None,);
                assert_eq!(
                    notification.source_item.data,
                    ThirdPartyItemData::GoogleMailThread(Box::new(google_mail_thread_123.clone()))
                );
            }
            // This notification should be updated
            "456" => {
                assert_eq!(notification.title, "test 456".to_string());
                assert_eq!(notification.kind, NotificationSourceKind::GoogleMail);
                assert_eq!(notification.status, NotificationStatus::Read);
                assert_eq!(
                    notification.get_html_url(),
                    "https://mail.google.com/mail/u/user@example.com/#inbox/456"
                        .parse::<Url>()
                        .unwrap()
                );
                assert_eq!(
                    notification.last_read_at,
                    Some(Utc.with_ymd_and_hms(2023, 9, 13, 20, 27, 16).unwrap())
                );
                assert_eq!(
                    notification.source_item.data,
                    ThirdPartyItemData::GoogleMailThread(Box::new(google_mail_thread_456.clone()))
                );
            }
            _ => {
                unreachable!("Unexpected notification title '{}'", &notification.title);
            }
        }
    }
}
