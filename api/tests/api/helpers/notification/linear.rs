use chrono::{DateTime, NaiveDate, Utc};
use graphql_client::{Error, GraphQLQuery, Response};
use httpmock::{Method::POST, Mock, MockServer};
use rstest::*;
use url::Url;

use universal_inbox::{
    notification::{
        integrations::linear::LinearNotification, Notification, NotificationMetadata,
        NotificationStatus,
    },
    user::UserId,
    HasHtmlUrl,
};

use universal_inbox_api::integrations::linear::graphql::{
    issue_update_subscribers::{self, IssueUpdateSubscribersIssueUpdate},
    notification_archive::{self, NotificationArchiveNotificationArchive},
    notification_subscribers_query::{
        self, NotificationSubscribersQueryNotification,
        NotificationSubscribersQueryNotificationOnIssueNotification,
        NotificationSubscribersQueryNotificationOnIssueNotificationIssue,
        NotificationSubscribersQueryNotificationOnIssueNotificationIssueSubscribers,
        NotificationSubscribersQueryNotificationOnIssueNotificationIssueSubscribersNodes,
        NotificationSubscribersQueryNotificationOnIssueNotificationUser,
    },
    notification_update_snoozed_until_at::{
        self, NotificationUpdateSnoozedUntilAtNotificationUpdate,
    },
    notifications_query, IssueUpdateSubscribers, NotificationArchive, NotificationSubscribersQuery,
    NotificationUpdateSnoozedUntilAt, NotificationsQuery,
};

use crate::helpers::load_json_fixture_file;

pub fn mock_linear_notifications_query<'a>(
    linear_mock_server: &'a MockServer,
    result: &'a Response<notifications_query::ResponseData>,
) -> Mock<'a> {
    let expected_request_body = NotificationsQuery::build_query(notifications_query::Variables {});
    linear_mock_server.mock(|when, then| {
        when.method(POST)
            .path("/")
            .json_body_obj(&expected_request_body)
            .header("authorization", "Bearer linear_test_access_token");
        then.status(200)
            .header("content-type", "application/json")
            .json_body_obj(result);
    })
}

pub fn mock_linear_issue_notification_subscribers_query(
    linear_mock_server: &MockServer,
    notification_id: String,
    user_id: String,
    subscriber_ids: Vec<String>,
) -> Mock {
    let expected_subscribers_request_body =
        NotificationSubscribersQuery::build_query(notification_subscribers_query::Variables {
            id: notification_id,
        });
    linear_mock_server.mock(|when, then| {
            when.method(POST)
                .path("/")
                .json_body_obj(&expected_subscribers_request_body)
                .header("authorization", "Bearer linear_test_access_token");
            then.status(200)
                .header("content-type", "application/json")
                .json_body_obj(&Response {
                    data: Some(notification_subscribers_query::ResponseData {
                        notification: NotificationSubscribersQueryNotification::IssueNotification(NotificationSubscribersQueryNotificationOnIssueNotification  {
                            user: NotificationSubscribersQueryNotificationOnIssueNotificationUser {
                                id: user_id
                            },
                            issue: NotificationSubscribersQueryNotificationOnIssueNotificationIssue {
                                subscribers: NotificationSubscribersQueryNotificationOnIssueNotificationIssueSubscribers {
                                    nodes: subscriber_ids.into_iter().map(|id| NotificationSubscribersQueryNotificationOnIssueNotificationIssueSubscribersNodes {
                                        id
                                    }).collect()
                                }
                            }
                        })
                    }),
                    errors: Some(vec![]),
                    extensions: None,
                });
        })
}

pub fn mock_linear_project_notification_subscribers_query(
    linear_mock_server: &MockServer,
    notification_id: String,
) -> Mock {
    let expected_subscribers_request_body =
        NotificationSubscribersQuery::build_query(notification_subscribers_query::Variables {
            id: notification_id,
        });
    linear_mock_server.mock(|when, then| {
        when.method(POST)
            .path("/")
            .json_body_obj(&expected_subscribers_request_body)
            .header("authorization", "Bearer linear_test_access_token");
        then.status(200)
            .header("content-type", "application/json")
            .json_body_obj(&Response {
                data: Some(notification_subscribers_query::ResponseData {
                    notification: NotificationSubscribersQueryNotification::ProjectNotification,
                }),
                errors: Some(vec![]),
                extensions: None,
            });
    })
}

pub fn mock_linear_update_issue_subscribers_query(
    linear_mock_server: &MockServer,
    notification_id: String,
    subscriber_ids: Vec<String>,
    successful_response: bool,
    errors: Option<Vec<Error>>,
) -> Mock {
    let expected_update_request_body =
        IssueUpdateSubscribers::build_query(issue_update_subscribers::Variables {
            id: notification_id,
            subscriber_ids,
        });
    linear_mock_server.mock(|when, then| {
        when.method(POST)
            .path("/")
            .json_body_obj(&expected_update_request_body)
            .header("authorization", "Bearer linear_test_access_token");
        then.status(200)
            .header("content-type", "application/json")
            .json_body_obj(&Response {
                data: Some(issue_update_subscribers::ResponseData {
                    issue_update: IssueUpdateSubscribersIssueUpdate {
                        success: successful_response,
                    },
                }),
                errors: Some(errors.unwrap_or_default()),
                extensions: None,
            });
    })
}

pub fn mock_linear_archive_notification_query(
    linear_mock_server: &MockServer,
    notification_id: String,
    successful_response: bool,
    errors: Option<Vec<Error>>,
) -> Mock {
    let expected_request_body = NotificationArchive::build_query(notification_archive::Variables {
        id: notification_id,
    });
    linear_mock_server.mock(|when, then| {
        when.method(POST)
            .path("/")
            .json_body_obj(&expected_request_body)
            .header("authorization", "Bearer linear_test_access_token");
        then.status(200)
            .header("content-type", "application/json")
            .json_body_obj(&Response {
                data: if errors.is_none() {
                    Some(notification_archive::ResponseData {
                        notification_archive: NotificationArchiveNotificationArchive {
                            success: successful_response,
                        },
                    })
                } else {
                    None
                },
                errors,
                extensions: None,
            });
    })
}

pub fn mock_linear_update_notification_snoozed_until_at_query(
    linear_mock_server: &MockServer,
    notification_id: String,
    snoozed_until_at: DateTime<Utc>,
) -> Mock {
    let expected_update_request_body = NotificationUpdateSnoozedUntilAt::build_query(
        notification_update_snoozed_until_at::Variables {
            id: notification_id,
            snoozed_until_at,
        },
    );
    linear_mock_server.mock(|when, then| {
        when.method(POST)
            .path("/")
            .json_body_obj(&expected_update_request_body)
            .header("authorization", "Bearer linear_test_access_token");
        then.status(200)
            .header("content-type", "application/json")
            .json_body_obj(&Response {
                data: Some(notification_update_snoozed_until_at::ResponseData {
                    notification_update: NotificationUpdateSnoozedUntilAtNotificationUpdate {
                        success: true,
                    },
                }),
                errors: None,
                extensions: None,
            });
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
                    notification.get_html_url(),
                    "https://linear.app/universal-inbox/issue/UNI-13/test-issue-3"
                        .parse::<Url>()
                        .unwrap()
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
                    NotificationMetadata::Linear(Box::new(sync_linear_notifications[2].clone()))
                );
            }
            // Project notification
            "df45c8cf-c717-4db7-abb9-5c5b73b50cc9" => {
                assert_eq!(notification.title, "Test project".to_string());
                assert_eq!(notification.status, NotificationStatus::Unread);
                assert_eq!(
                    notification.get_html_url(),
                    "https://linear.app/universal-inbox/project/test-project-33065448b39c"
                        .parse::<Url>()
                        .unwrap()
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
                    NotificationMetadata::Linear(Box::new(sync_linear_notifications[0].clone()))
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
