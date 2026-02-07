use chrono::{DateTime, NaiveDate, Utc};
use graphql_client::{Error, GraphQLQuery, Response};
use rstest::*;
use url::Url;
use uuid::Uuid;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use universal_inbox::{
    HasHtmlUrl,
    integration_connection::IntegrationConnectionId,
    notification::{Notification, NotificationStatus},
    third_party::{
        integrations::linear::{LinearNotification, LinearProject},
        item::ThirdPartyItemData,
    },
    user::UserId,
};

use universal_inbox_api::integrations::linear::graphql::{
    AssignedIssuesQuery, IssueUpdateState, IssueUpdateSubscribers, NotificationArchive,
    NotificationSubscribersQuery, NotificationUpdateSnoozedUntilAt, NotificationsQuery,
    assigned_issues_query,
    issue_update_state::{self, IssueUpdateStateIssueUpdate},
    issue_update_subscribers::{self, IssueUpdateSubscribersIssueUpdate},
    notification_archive::{self, NotificationArchiveNotificationArchiveAll},
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
    notifications_query,
};

use crate::helpers::{
    TestedApp, load_json_fixture_file, notification::create_notification_from_source_item,
};

pub async fn create_notification_from_linear_notification(
    app: &TestedApp,
    linear_notification: &LinearNotification,
    user_id: UserId,
    linear_integration_connection_id: IntegrationConnectionId,
) -> Box<Notification> {
    let linear_notification_id = match &linear_notification {
        LinearNotification::IssueNotification { id, .. } => id.to_string(),
        LinearNotification::ProjectNotification { id, .. } => id.to_string(),
    };
    create_notification_from_source_item(
        app,
        linear_notification_id,
        ThirdPartyItemData::LinearNotification(Box::new(linear_notification.clone())),
        app.notification_service.read().await.linear_service.clone(),
        user_id,
        linear_integration_connection_id,
    )
    .await
}

pub async fn mock_linear_notifications_query(
    linear_mock_server: &MockServer,
    result: &Response<notifications_query::ResponseData>,
) {
    let expected_request_body = NotificationsQuery::build_query(notifications_query::Variables {});
    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_json(&expected_request_body))
        .and(header("authorization", "Bearer linear_test_access_token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(result),
        )
        .mount(linear_mock_server)
        .await;
}

pub async fn mock_linear_issue_notification_subscribers_query(
    linear_mock_server: &MockServer,
    notification_id: String,
    user_id: String,
    issue_id: String,
    subscriber_ids: Vec<String>,
) {
    let expected_subscribers_request_body =
        NotificationSubscribersQuery::build_query(notification_subscribers_query::Variables {
            id: notification_id,
        });
    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_json(&expected_subscribers_request_body))
        .and(header("authorization", "Bearer linear_test_access_token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(&Response {
                    data: Some(notification_subscribers_query::ResponseData {
                        notification: NotificationSubscribersQueryNotification::IssueNotification(NotificationSubscribersQueryNotificationOnIssueNotification  {
                            user: NotificationSubscribersQueryNotificationOnIssueNotificationUser {
                                id: user_id
                            },
                            issue: NotificationSubscribersQueryNotificationOnIssueNotificationIssue {
                                id: issue_id,
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
                }),
        )
        .mount(linear_mock_server)
        .await;
}

pub async fn mock_linear_project_notification_subscribers_query(
    linear_mock_server: &MockServer,
    notification_id: String,
) {
    let expected_subscribers_request_body =
        NotificationSubscribersQuery::build_query(notification_subscribers_query::Variables {
            id: notification_id,
        });
    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_json(&expected_subscribers_request_body))
        .and(header("authorization", "Bearer linear_test_access_token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(&Response {
                    data: Some(notification_subscribers_query::ResponseData {
                        notification: NotificationSubscribersQueryNotification::ProjectNotification,
                    }),
                    errors: Some(vec![]),
                    extensions: None,
                }),
        )
        .mount(linear_mock_server)
        .await;
}

pub async fn mock_linear_update_issue_subscribers_query(
    linear_mock_server: &MockServer,
    issue_id: String,
    subscriber_ids: Vec<String>,
    successful_response: bool,
    errors: Option<Vec<Error>>,
) {
    let expected_update_request_body =
        IssueUpdateSubscribers::build_query(issue_update_subscribers::Variables {
            id: issue_id,
            subscriber_ids,
        });
    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_json(&expected_update_request_body))
        .and(header("authorization", "Bearer linear_test_access_token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(&Response {
                    data: Some(issue_update_subscribers::ResponseData {
                        issue_update: IssueUpdateSubscribersIssueUpdate {
                            success: successful_response,
                        },
                    }),
                    errors,
                    extensions: None,
                }),
        )
        .mount(linear_mock_server)
        .await;
}

pub async fn mock_linear_archive_notification_query(
    linear_mock_server: &MockServer,
    notification_id: String,
    successful_response: bool,
    errors: Option<Vec<Error>>,
) {
    let expected_request_body = NotificationArchive::build_query(notification_archive::Variables {
        id: notification_id,
    });
    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_json(&expected_request_body))
        .and(header("authorization", "Bearer linear_test_access_token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(&Response {
                    data: if errors.is_none() {
                        Some(notification_archive::ResponseData {
                            notification_archive_all: NotificationArchiveNotificationArchiveAll {
                                success: successful_response,
                            },
                        })
                    } else {
                        None
                    },
                    errors,
                    extensions: None,
                }),
        )
        .mount(linear_mock_server)
        .await;
}

pub async fn mock_linear_update_notification_snoozed_until_at_query(
    linear_mock_server: &MockServer,
    notification_id: String,
    snoozed_until_at: DateTime<Utc>,
) {
    let expected_update_request_body = NotificationUpdateSnoozedUntilAt::build_query(
        notification_update_snoozed_until_at::Variables {
            id: notification_id,
            snoozed_until_at,
        },
    );
    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_json(&expected_update_request_body))
        .and(header("authorization", "Bearer linear_test_access_token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(&Response {
                    data: Some(notification_update_snoozed_until_at::ResponseData {
                        notification_update: NotificationUpdateSnoozedUntilAtNotificationUpdate {
                            success: true,
                        },
                    }),
                    errors: None,
                    extensions: None,
                }),
        )
        .mount(linear_mock_server)
        .await;
}

pub async fn mock_linear_assigned_issues_query(
    linear_mock_server: &MockServer,
    result: &Response<assigned_issues_query::ResponseData>,
) {
    let expected_request_body =
        AssignedIssuesQuery::build_query(assigned_issues_query::Variables {});
    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_json(&expected_request_body))
        .and(header("authorization", "Bearer linear_test_access_token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(result),
        )
        .mount(linear_mock_server)
        .await;
}

pub async fn mock_linear_update_issue_state_query(
    linear_mock_server: &MockServer,
    issue_id: Uuid,
    state_id: Uuid,
    successful_response: bool,
    errors: Option<Vec<Error>>,
) {
    let expected_update_request_body =
        IssueUpdateState::build_query(issue_update_state::Variables {
            id: issue_id.to_string(),
            state_id: state_id.to_string(),
        });
    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_json(&expected_update_request_body))
        .and(header("authorization", "Bearer linear_test_access_token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(&Response {
                    data: Some(issue_update_state::ResponseData {
                        issue_update: IssueUpdateStateIssueUpdate {
                            success: successful_response,
                        },
                    }),
                    errors,
                    extensions: None,
                }),
        )
        .mount(linear_mock_server)
        .await;
}

pub fn assert_sync_notifications(
    notifications: &[Notification],
    sync_linear_notifications: &[LinearNotification],
    expected_user_id: UserId,
) {
    for notification in notifications.iter() {
        assert_eq!(notification.user_id, expected_user_id);
        match notification.source_item.source_id.as_ref() {
            // This Issue notification should have been updated
            "0c28d222-c599-43bb-af99-fcd3e99daff0" => {
                assert_eq!(
                    notification.title,
                    "Add keyboard shortcuts to scroll the preview pane".to_string()
                );
                assert_eq!(notification.status, NotificationStatus::Read);
                assert_eq!(
                    notification.get_html_url(),
                    "https://linear.app/universal-inbox/issue/UNI-13/test-issue-3"
                        .parse::<Url>()
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
                    notification.source_item.data,
                    ThirdPartyItemData::LinearNotification(Box::new(
                        sync_linear_notifications[2].clone()
                    ))
                );
            }

            // Project notification
            "df45c8cf-c717-4db7-abb9-5c5b73b50cc9" => {
                assert_eq!(notification.title, "Universal Inbox".to_string());
                assert_eq!(notification.status, NotificationStatus::Unread);
                assert_eq!(
                    notification.get_html_url(),
                    "https://linear.app/universal-inbox/project/test-project-33065448b39c"
                        .parse::<Url>()
                        .unwrap()
                );
                assert_eq!(notification.last_read_at, None);
                assert_eq!(
                    notification.source_item.data,
                    ThirdPartyItemData::LinearNotification(Box::new(
                        sync_linear_notifications[0].clone()
                    ))
                );
                match &notification.source_item.data {
                    ThirdPartyItemData::LinearNotification(linear_notification) => {
                        match &**linear_notification {
                            LinearNotification::ProjectNotification {
                                project: LinearProject { id, name, icon, .. },
                                ..
                            } => {
                                assert_eq!(
                                    id,
                                    &Uuid::parse_str("c1b0f0f8-9e16-4335-a540-bda09cc491df")
                                        .unwrap()
                                );
                                assert_eq!(name, "Universal Inbox");
                                assert_eq!(icon, &Some("🚀".to_string()));
                            }
                            _ => {
                                panic!("Expected Linear project notification metadata");
                            }
                        }
                    }
                    _ => {
                        panic!("Expected Linear notification metadata");
                    }
                }
            }
            _ => {
                // Ignore other notifications
            }
        }
    }
}

#[fixture]
pub fn sync_linear_notifications_response() -> Response<notifications_query::ResponseData> {
    load_json_fixture_file("sync_linear_notifications.json")
}

#[fixture]
pub fn sync_linear_tasks_response() -> Response<assigned_issues_query::ResponseData> {
    load_json_fixture_file("sync_linear_tasks.json")
}
