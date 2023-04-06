use chrono::{TimeZone, Utc};
use rstest::*;
use uuid::Uuid;

use universal_inbox::notification::{
    integrations::github::GithubNotification, Notification, NotificationMetadata,
    NotificationStatus,
};

use universal_inbox_api::integrations::{github, notification::NotificationSourceKind};

use crate::helpers::{
    auth::{authenticated_app, AuthenticatedApp},
    notification::{
        github::{
            assert_sync_notifications, create_notification_from_github_notification,
            mock_github_notifications_service, sync_github_notifications,
        },
        sync_notifications,
    },
    rest::{create_resource, get_resource},
};

#[rstest]
#[tokio::test]
async fn test_sync_notifications_should_add_new_notification_and_update_existing_one(
    #[future] authenticated_app: AuthenticatedApp,
    // Vec[GithubNotification { source_id: "123", ... }, GithubNotification { source_id: "456", ... } ]
    sync_github_notifications: Vec<GithubNotification>,
) {
    let app = authenticated_app.await;
    let existing_notification: Box<Notification> = create_resource(
        &app.client,
        &app.app_address,
        "notifications",
        Box::new(Notification {
            id: Uuid::new_v4().into(),
            user_id: app.user.id,
            title: "Greetings 2".to_string(),
            status: NotificationStatus::Unread,
            source_id: sync_github_notifications[1].id.clone(),
            source_html_url: github::get_html_url_from_api_url(
                &sync_github_notifications[1].subject.url,
            ),
            metadata: NotificationMetadata::Github(sync_github_notifications[1].clone()),
            updated_at: Utc.with_ymd_and_hms(2014, 11, 6, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
            task_id: None,
        }),
    )
    .await;

    let github_notifications_mock =
        mock_github_notifications_service(&app.github_mock_server, "1", &sync_github_notifications);
    let empty_result = Vec::<GithubNotification>::new();
    let github_notifications_mock2 =
        mock_github_notifications_service(&app.github_mock_server, "2", &empty_result);

    let notifications: Vec<Notification> = sync_notifications(
        &app.client,
        &app.app_address,
        Some(NotificationSourceKind::Github),
    )
    .await;

    assert_eq!(notifications.len(), sync_github_notifications.len());
    assert_sync_notifications(&notifications, &sync_github_notifications, app.user.id);
    github_notifications_mock.assert();
    github_notifications_mock2.assert();

    let updated_notification: Box<Notification> = get_resource(
        &app.client,
        &app.app_address,
        "notifications",
        existing_notification.id.into(),
    )
    .await;
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
    #[future] authenticated_app: AuthenticatedApp,
    // Vec[GithubNotification { source_id: "123", ... }, GithubNotification { source_id: "456", ... } ]
    sync_github_notifications: Vec<GithubNotification>,
) {
    let app = authenticated_app.await;
    for github_notification in sync_github_notifications.iter() {
        create_notification_from_github_notification(
            &app.client,
            &app.app_address,
            github_notification,
            app.user.id,
        )
        .await;
    }
    // to be deleted during sync
    let existing_notification: Box<Notification> = create_resource(
        &app.client,
        &app.app_address,
        "notifications",
        Box::new(Notification {
            id: Uuid::new_v4().into(),
            user_id: app.user.id,
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
            task_id: None,
        }),
    )
    .await;

    let github_notifications_mock =
        mock_github_notifications_service(&app.github_mock_server, "1", &sync_github_notifications);
    let empty_result = Vec::<GithubNotification>::new();
    let github_notifications_mock2 =
        mock_github_notifications_service(&app.github_mock_server, "2", &empty_result);

    let notifications: Vec<Notification> = sync_notifications(
        &app.client,
        &app.app_address,
        Some(NotificationSourceKind::Github),
    )
    .await;

    assert_eq!(notifications.len(), sync_github_notifications.len());
    assert_sync_notifications(&notifications, &sync_github_notifications, app.user.id);
    github_notifications_mock.assert();
    github_notifications_mock2.assert();

    let deleted_notification: Box<Notification> = get_resource(
        &app.client,
        &app.app_address,
        "notifications",
        existing_notification.id.into(),
    )
    .await;
    assert_eq!(deleted_notification.id, existing_notification.id);
    assert_eq!(deleted_notification.status, NotificationStatus::Deleted);
}
