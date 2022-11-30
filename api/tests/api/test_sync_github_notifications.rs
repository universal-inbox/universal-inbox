use chrono::{TimeZone, Utc};
use rstest::*;

use crate::helpers::{
    create_notification, get_notification,
    github::{
        assert_sync_notifications, create_notification_from_github_notification,
        mock_github_notifications_service, sync_github_notifications,
    },
    sync_notifications, tested_app, TestedApp,
};
use universal_inbox::{
    integrations::github::GithubNotification, Notification, NotificationMetadata,
    NotificationStatus,
};
use universal_inbox_api::{
    integrations::github, universal_inbox::notification::source::NotificationSourceKind,
};

#[rstest]
#[tokio::test]
async fn test_sync_notifications_should_add_new_notification_and_update_existing_one(
    #[future] tested_app: TestedApp,
    // Vec[GithubNotification { source_id: "123", ... }, GithubNotification { source_id: "456", ... } ]
    sync_github_notifications: Vec<GithubNotification>,
) {
    let app = tested_app.await;
    let existing_notification = create_notification(
        &app.app_address,
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

    let github_notifications_mock =
        mock_github_notifications_service(&app.github_mock_server, "1", &sync_github_notifications);
    let empty_result = Vec::<GithubNotification>::new();
    let github_notifications_mock2 =
        mock_github_notifications_service(&app.github_mock_server, "2", &empty_result);

    let notifications: Vec<Notification> =
        sync_notifications(&app.app_address, Some(NotificationSourceKind::Github)).await;

    assert_eq!(notifications.len(), sync_github_notifications.len());
    assert_sync_notifications(&notifications, &sync_github_notifications);
    github_notifications_mock.assert();
    github_notifications_mock2.assert();

    let updated_notification = get_notification(&app.app_address, existing_notification.id).await;
    assert_eq!(updated_notification.id, existing_notification.id);
    assert_eq!(
        updated_notification.source_id,
        existing_notification.source_id
    );
    assert_eq!(updated_notification.status, NotificationStatus::Read);
    assert_eq!(
        updated_notification.updated_at,
        Utc.with_ymd_and_hms(2014, 11, 7, 23, 1, 45).unwrap()
    );
    assert_eq!(
        updated_notification.last_read_at,
        Some(Utc.with_ymd_and_hms(2014, 11, 7, 23, 2, 45).unwrap())
    );
    assert_eq!(
        updated_notification.metadata,
        NotificationMetadata::Github(sync_github_notifications[1].clone())
    );
}

#[rstest]
#[tokio::test]
async fn test_sync_notifications_should_mark_deleted_notification_without_subscription(
    #[future] tested_app: TestedApp,
    // Vec[GithubNotification { source_id: "123", ... }, GithubNotification { source_id: "456", ... } ]
    sync_github_notifications: Vec<GithubNotification>,
) {
    let app = tested_app.await;
    for github_notification in sync_github_notifications.iter() {
        create_notification_from_github_notification(&app.app_address, github_notification).await;
    }
    // to be deleted during sync
    let existing_notification = create_notification(
        &app.app_address,
        Box::new(Notification {
            id: uuid::Uuid::new_v4(),
            title: "Greetings 3".to_string(),
            status: NotificationStatus::Unread,
            source_id: "789".to_string(),
            source_html_url: github::get_html_url_from_api_url(
                &sync_github_notifications[1].subject.url,
            ),
            metadata: NotificationMetadata::Github(sync_github_notifications[1].clone()), // reusing github notification but not useful
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

    let notifications: Vec<Notification> =
        sync_notifications(&app.app_address, Some(NotificationSourceKind::Github)).await;

    assert_eq!(notifications.len(), sync_github_notifications.len());
    assert_sync_notifications(&notifications, &sync_github_notifications);
    github_notifications_mock.assert();
    github_notifications_mock2.assert();

    let deleted_notification = get_notification(&app.app_address, existing_notification.id).await;
    assert_eq!(deleted_notification.id, existing_notification.id);
    assert_eq!(deleted_notification.status, NotificationStatus::Deleted);
}
