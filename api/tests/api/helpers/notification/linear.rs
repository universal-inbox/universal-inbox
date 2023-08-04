use actix_http::Uri;
use chrono::{NaiveDate, Utc};
use graphql_client::Response;
use httpmock::{Method::POST, Mock, MockServer};
use rstest::*;

use universal_inbox::{
    notification::{
        integrations::linear::LinearNotification, Notification, NotificationMetadata,
        NotificationStatus,
    },
    user::UserId,
};

use universal_inbox_api::integrations::linear::notifications_query;

use crate::helpers::load_json_fixture_file;

pub fn mock_linear_notifications_service<'a>(
    linear_mock_server: &'a MockServer,
    result: &'a Response<notifications_query::ResponseData>,
) -> Mock<'a> {
    linear_mock_server.mock(|when, then| {
        when.method(POST)
            .path("/")
            .header("authorization", "Bearer linear_test_access_token");
        then.status(200)
            .header("content-type", "application/json")
            .json_body_obj(result);
    })
}

pub fn assert_sync_notifications(
    notifications: &[Notification],
    sync_linear_notifications: &[LinearNotification],
    expected_user_id: UserId,
) {
    for notification in notifications.iter() {
        assert_eq!(notification.user_id, expected_user_id);
        match notification.source_id.as_ref() {
            // This Issue notification should have been updated
            "0c28d222-c599-43bb-af99-fcd3e99daff0" => {
                assert_eq!(notification.title, "Test issue 3".to_string());
                assert_eq!(notification.status, NotificationStatus::Read);
                assert_eq!(
                    notification.source_html_url,
                    Some(
                        "https://linear.app/universal-inbox/issue/UNI-13/test-issue-3"
                            .parse::<Uri>()
                            .unwrap()
                    )
                );
                assert_eq!(
                    notification.updated_at,
                    NaiveDate::from_ymd_opt(2023, 7, 31)
                        .unwrap()
                        .and_hms_milli_opt(6, 1, 27, 112)
                        .unwrap()
                        .and_local_timezone(Utc)
                        .unwrap()
                );
                assert_eq!(
                    notification.last_read_at,
                    Some(
                        NaiveDate::from_ymd_opt(2023, 7, 31)
                            .unwrap()
                            .and_hms_milli_opt(6, 1, 27, 112)
                            .unwrap()
                            .and_local_timezone(Utc)
                            .unwrap()
                    )
                );
                assert_eq!(
                    notification.metadata,
                    NotificationMetadata::Linear(sync_linear_notifications[2].clone())
                );
            }
            // Project notification
            "df45c8cf-c717-4db7-abb9-5c5b73b50cc9" => {
                assert_eq!(notification.title, "Test project".to_string());
                assert_eq!(notification.status, NotificationStatus::Unread);
                assert_eq!(
                    notification.source_html_url,
                    Some(
                        "https://linear.app/universal-inbox/project/test-project-33065448b39c"
                            .parse::<Uri>()
                            .unwrap()
                    )
                );
                assert_eq!(
                    notification.updated_at,
                    NaiveDate::from_ymd_opt(2023, 7, 31)
                        .unwrap()
                        .and_hms_milli_opt(6, 1, 27, 137)
                        .unwrap()
                        .and_local_timezone(Utc)
                        .unwrap()
                );
                assert_eq!(notification.last_read_at, None);
                assert_eq!(
                    notification.metadata,
                    NotificationMetadata::Linear(sync_linear_notifications[0].clone())
                );
            }
            _ => {
                // Ignore other notifications
            }
        }
    }
}

#[fixture]
pub fn sync_linear_notifications_response() -> Response<notifications_query::ResponseData> {
    load_json_fixture_file("/tests/api/fixtures/sync_linear_notifications.json")
}
