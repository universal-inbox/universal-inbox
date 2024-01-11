use chrono::{NaiveDate, TimeZone, Utc};
use graphql_client::Response;
use rstest::*;
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::linear::LinearConfig,
    },
    notification::{
        integrations::linear::LinearNotification, Notification, NotificationMetadata,
        NotificationSourceKind, NotificationStatus,
    },
    task::integrations::todoist::TodoistItem,
};

use universal_inbox_api::{
    configuration::Settings,
    integrations::{linear::graphql::notifications_query, oauth2::NangoConnection},
};

use crate::helpers::{
    auth::{authenticated_app, AuthenticatedApp},
    integration_connection::{create_and_mock_integration_connection, nango_linear_connection},
    notification::{
        linear::{
            assert_sync_notifications, mock_linear_notifications_query,
            sync_linear_notifications_response,
        },
        sync_notifications,
    },
    rest::{create_resource, get_resource},
    settings,
    task::todoist::{create_task_from_todoist_item, todoist_item},
};

#[rstest]
#[tokio::test]
async fn test_sync_notifications_should_add_new_notification_and_update_existing_one(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    sync_linear_notifications_response: Response<notifications_query::ResponseData>,
    todoist_item: Box<TodoistItem>,
    nango_linear_connection: Box<NangoConnection>,
) {
    let app = authenticated_app.await;
    let existing_todoist_task = create_task_from_todoist_item(
        &app.client,
        &app.app.api_address,
        &todoist_item,
        "Project2".to_string(),
        app.user.id,
    )
    .await;
    let sync_linear_notifications: Vec<LinearNotification> = sync_linear_notifications_response
        .data
        .clone()
        .unwrap()
        .try_into()
        .unwrap();
    let existing_notification: Box<Notification> = match &sync_linear_notifications[2] {
        notif @ LinearNotification::IssueNotification { id, issue, .. } => {
            create_resource(
                &app.client,
                &app.app.api_address,
                "notifications",
                Box::new(Notification {
                    id: Uuid::new_v4().into(),
                    user_id: app.user.id,
                    title: "title to be updated".to_string(),
                    status: NotificationStatus::Unread,
                    source_id: id.to_string(),
                    source_html_url: Some(issue.url.clone()),
                    metadata: NotificationMetadata::Linear(Box::new(notif.clone())),
                    updated_at: Utc.with_ymd_and_hms(2014, 11, 6, 0, 0, 0).unwrap(),
                    last_read_at: None,
                    snoozed_until: Some(Utc.with_ymd_and_hms(2064, 1, 1, 0, 0, 0).unwrap()),
                    details: None,
                    task_id: Some(existing_todoist_task.task.id),
                }),
            )
            .await
        }
        _ => unreachable!(),
    };
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.integrations.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Linear(LinearConfig::enabled()),
        &settings,
        nango_linear_connection,
    )
    .await;

    let linear_notifications_mock = mock_linear_notifications_query(
        &app.app.linear_mock_server,
        &sync_linear_notifications_response,
    );

    let notifications: Vec<Notification> = sync_notifications(
        &app.client,
        &app.app.api_address,
        Some(NotificationSourceKind::Linear),
        false,
    )
    .await;

    assert_eq!(notifications.len(), sync_linear_notifications.len());
    assert_sync_notifications(&notifications, &sync_linear_notifications, app.user.id);
    linear_notifications_mock.assert();

    let updated_notification: Box<Notification> = get_resource(
        &app.client,
        &app.app.api_address,
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
        NaiveDate::from_ymd_opt(2023, 7, 31)
            .unwrap()
            .and_hms_milli_opt(6, 1, 27, 112)
            .unwrap()
            .and_local_timezone(Utc)
            .unwrap()
    );
    assert_eq!(
        updated_notification.last_read_at,
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
        updated_notification.metadata,
        NotificationMetadata::Linear(Box::new(sync_linear_notifications[2].clone()))
    );
    assert_eq!(
        updated_notification.snoozed_until,
        Some(Utc.with_ymd_and_hms(2023, 8, 12, 7, 0, 0).unwrap())
    );
    // `task_id` should not be reset
    assert_eq!(
        updated_notification.task_id,
        Some(existing_todoist_task.task.id)
    );
}
