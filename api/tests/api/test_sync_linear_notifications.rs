use chrono::{NaiveDate, TimeZone, Timelike, Utc};
use graphql_client::Response;
use rstest::*;
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::{linear::LinearConfig, todoist::TodoistConfig},
    },
    notification::{
        integrations::linear::LinearNotification, Notification, NotificationMetadata,
        NotificationSourceKind, NotificationStatus,
    },
    third_party::{
        integrations::todoist::TodoistItem,
        item::{ThirdPartyItem, ThirdPartyItemCreationResult, ThirdPartyItemData},
    },
};

use universal_inbox_api::{
    configuration::Settings,
    integrations::{
        linear::graphql::notifications_query, oauth2::NangoConnection, todoist::TodoistSyncResponse,
    },
};

use crate::helpers::{
    auth::{authenticated_app, AuthenticatedApp},
    integration_connection::{
        create_and_mock_integration_connection, nango_linear_connection, nango_todoist_connection,
    },
    notification::{
        linear::{
            assert_sync_notifications, mock_linear_notifications_query,
            sync_linear_notifications_response,
        },
        sync_notifications,
    },
    rest::{create_resource, get_resource},
    settings,
    task::todoist::{
        mock_todoist_sync_resources_service, sync_todoist_projects_response, todoist_item,
    },
};

#[rstest]
#[tokio::test]
async fn test_sync_notifications_should_add_new_notification_and_update_existing_one(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    sync_linear_notifications_response: Response<notifications_query::ResponseData>,
    todoist_item: Box<TodoistItem>,
    sync_todoist_projects_response: TodoistSyncResponse,
    nango_linear_connection: Box<NangoConnection>,
    nango_todoist_connection: Box<NangoConnection>,
) {
    let app = authenticated_app.await;
    let integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.integrations.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings,
        nango_todoist_connection,
        None,
    )
    .await;
    mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
        "projects",
        &sync_todoist_projects_response,
        None,
    );

    let creation: Box<ThirdPartyItemCreationResult> = create_resource(
        &app.client,
        &app.app.api_address,
        "third_party/items",
        Box::new(ThirdPartyItem {
            id: Uuid::new_v4().into(),
            source_id: todoist_item.id.clone(),
            created_at: Utc::now().with_nanosecond(0).unwrap(),
            updated_at: Utc::now().with_nanosecond(0).unwrap(),
            user_id: app.user.id,
            data: ThirdPartyItemData::TodoistItem(TodoistItem {
                project_id: "2222".to_string(), // ie. "Project2"
                added_at: Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
                ..*todoist_item.clone()
            }),
            integration_connection_id: integration_connection.id,
        }),
    )
    .await;
    let existing_task_id = creation.task.as_ref().unwrap().id;

    let sync_linear_notifications: Vec<LinearNotification> = sync_linear_notifications_response
        .data
        .clone()
        .unwrap()
        .try_into()
        .unwrap();

    // Assert parsing of comments is correct
    // This notification has comment with a parent comment and children
    // Only the parent comment and its children should be parsed
    match &sync_linear_notifications[2] {
        LinearNotification::IssueNotification {
            comment: Some(comment),
            ..
        } => {
            assert_eq!(comment.body, "Initial comment".to_string());
            assert_eq!(comment.children.len(), 2);
            assert_eq!(comment.children[0].body, "other comment".to_string());
            assert_eq!(comment.children[1].body, "answer comment".to_string());
        }
        _ => {
            unreachable!("Expected Linear issue notification");
        }
    }

    // This notification has comment with no parent, thus we don't fetch its children
    match &sync_linear_notifications[3] {
        LinearNotification::IssueNotification {
            comment: Some(comment),
            ..
        } => {
            assert_eq!(comment.body, "comment without parent".to_string());
            assert!(comment.children.is_empty());
        }
        _ => {
            unreachable!("Expected Linear issue notification");
        }
    }

    let existing_notification: Box<Notification> = match &sync_linear_notifications[2] {
        notif @ LinearNotification::IssueNotification { id, .. } => {
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
                    metadata: NotificationMetadata::Linear(Box::new(notif.clone())),
                    updated_at: Utc.with_ymd_and_hms(2014, 11, 6, 0, 0, 0).unwrap(),
                    last_read_at: None,
                    snoozed_until: Some(Utc.with_ymd_and_hms(2064, 1, 1, 0, 0, 0).unwrap()),
                    details: None,
                    task_id: Some(existing_task_id),
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
        None,
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
    assert_eq!(updated_notification.task_id, Some(existing_task_id));
}
