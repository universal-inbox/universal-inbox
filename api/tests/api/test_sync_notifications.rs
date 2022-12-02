use chrono::{TimeZone, Utc};
use rstest::*;

use universal_inbox::notification::{
    integrations::{github::GithubNotification, todoist::TodoistTask},
    Notification, NotificationMetadata, NotificationStatus,
};
use universal_inbox_api::integrations::github;

use crate::helpers::{
    notification::{
        github::{
            self as github_helper, mock_github_notifications_service, sync_github_notifications,
        },
        sync_notifications,
        todoist::{self as todoist_helper},
    },
    rest::{create_resource, get_resource},
    task::todoist::{mock_todoist_tasks_service, sync_todoist_tasks},
    tested_app, TestedApp,
};

#[rstest]
#[tokio::test]
async fn test_sync_notifications_should_add_new_notification_and_update_existing_one(
    #[future] tested_app: TestedApp,
    // Vec[GithubNotification { source_id: "123", ... }, GithubNotification { source_id: "456", ... } ]
    sync_github_notifications: Vec<GithubNotification>,
    // Vec[TodoistTask { source_id: "123", ... }, GithubNotification { source_id: "456", ... } ]
    sync_todoist_tasks: Vec<TodoistTask>,
) {
    let app = tested_app.await;
    let existing_github_notification = create_resource(
        &app.app_address,
        "notifications",
        Box::new(Notification {
            id: uuid::Uuid::new_v4(),
            title: "Greetings 2".to_string(),
            status: NotificationStatus::Unread,
            source_id: "456".to_string(),
            source_html_url: github::get_html_url_from_api_url(
                &sync_github_notifications[1].subject.url,
            ),
            metadata: NotificationMetadata::Github(sync_github_notifications[1].clone()),
            updated_at: Utc.with_ymd_and_hms(2014, 11, 6, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
        }),
    )
    .await;
    let existing_todoist_notification = create_resource(
        &app.app_address,
        "notifications",
        Box::new(Notification {
            id: uuid::Uuid::new_v4(),
            title: "Other task".to_string(),
            status: NotificationStatus::Unread,
            source_id: "1456".to_string(),
            source_html_url: Some(sync_todoist_tasks[1].url.clone()),
            metadata: NotificationMetadata::Todoist(sync_todoist_tasks[1].clone()),
            updated_at: Utc.with_ymd_and_hms(2014, 11, 6, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
        }),
    )
    .await;

    let github_notifications_mock =
        mock_github_notifications_service(&app.github_mock_server, "1", &sync_github_notifications);
    let empty_result = Vec::<GithubNotification>::new();
    let github_notifications_mock2 =
        mock_github_notifications_service(&app.github_mock_server, "2", &empty_result);
    let todoist_tasks_mock =
        mock_todoist_tasks_service(&app.todoist_mock_server, &sync_todoist_tasks);

    let synced_notifications: Vec<Notification> = sync_notifications(&app.app_address, None).await;

    assert_eq!(
        synced_notifications.len(),
        sync_github_notifications.len() + sync_todoist_tasks.len()
    );
    let (synced_github_notifications, synced_todoist_notifications): (
        Vec<Notification>,
        Vec<Notification>,
    ) = synced_notifications
        .into_iter()
        .partition(|notif| match notif.metadata {
            NotificationMetadata::Github(_) => true,
            NotificationMetadata::Todoist(_) => false,
        });
    github_helper::assert_sync_notifications(
        &synced_github_notifications,
        &sync_github_notifications,
    );
    todoist_helper::assert_sync_notifications(&synced_todoist_notifications, &sync_todoist_tasks);

    github_notifications_mock.assert();
    github_notifications_mock2.assert();
    todoist_tasks_mock.assert();

    let updated_github_notification: Box<Notification> = get_resource(
        &app.app_address,
        "notifications",
        existing_github_notification.id,
    )
    .await;
    assert_eq!(
        updated_github_notification.id,
        existing_github_notification.id
    );
    assert_eq!(
        updated_github_notification.source_id,
        existing_github_notification.source_id
    );
    assert_eq!(updated_github_notification.status, NotificationStatus::Read);
    assert_eq!(
        updated_github_notification.updated_at,
        Utc.with_ymd_and_hms(2014, 11, 7, 23, 1, 45).unwrap()
    );
    assert_eq!(
        updated_github_notification.last_read_at,
        Some(Utc.with_ymd_and_hms(2014, 11, 7, 23, 2, 45).unwrap())
    );
    assert_eq!(
        updated_github_notification.metadata,
        NotificationMetadata::Github(sync_github_notifications[1].clone())
    );

    let updated_todoist_notification: Box<Notification> = get_resource(
        &app.app_address,
        "notifications",
        existing_todoist_notification.id,
    )
    .await;
    assert_eq!(
        updated_todoist_notification.id,
        existing_todoist_notification.id
    );
    assert_eq!(
        updated_todoist_notification.source_id,
        existing_todoist_notification.source_id
    );
    assert_eq!(
        updated_todoist_notification.status,
        NotificationStatus::Unread
    );
    assert_eq!(
        updated_todoist_notification.updated_at,
        Utc.with_ymd_and_hms(2019, 12, 11, 22, 37, 50).unwrap()
    );
    assert_eq!(updated_todoist_notification.last_read_at, None);
    assert_eq!(
        updated_todoist_notification.metadata,
        NotificationMetadata::Todoist(sync_todoist_tasks[1].clone())
    );
}
