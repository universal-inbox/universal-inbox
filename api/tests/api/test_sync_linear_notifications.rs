use chrono::{NaiveDate, TimeZone, Timelike, Utc};
use graphql_client::Response;
use rstest::*;
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::{linear::LinearConfig, todoist::TodoistConfig},
        provider::IntegrationProviderKind,
        IntegrationConnectionStatus,
    },
    notification::{
        service::NotificationPatch, Notification, NotificationSourceKind, NotificationStatus,
    },
    third_party::{
        integrations::{
            linear::{LinearIssue, LinearNotification},
            todoist::TodoistItem,
        },
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
        create_and_mock_integration_connection, get_integration_connection_per_provider,
        nango_linear_connection, nango_todoist_connection,
    },
    notification::{
        linear::{
            assert_sync_notifications, create_notification_from_linear_notification,
            mock_linear_notifications_query, sync_linear_notifications_response,
        },
        list_notifications, sync_notifications, update_notification,
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
    let todoist_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings,
        nango_todoist_connection,
        None,
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
        "third_party/task/items",
        Box::new(ThirdPartyItem {
            id: Uuid::new_v4().into(),
            source_id: todoist_item.id.clone(),
            created_at: Utc::now().with_nanosecond(0).unwrap(),
            updated_at: Utc::now().with_nanosecond(0).unwrap(),
            user_id: app.user.id,
            data: ThirdPartyItemData::TodoistItem(Box::new(TodoistItem {
                project_id: "2222".to_string(), // ie. "Project2"
                added_at: Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
                ..*todoist_item.clone()
            })),
            integration_connection_id: todoist_integration_connection.id,
            source_item: None,
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

    let linear_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Linear(LinearConfig::enabled()),
        &settings,
        nango_linear_connection,
        None,
        None,
    )
    .await;

    let existing_notification = create_notification_from_linear_notification(
        &app.app,
        &sync_linear_notifications[2],
        app.user.id,
        linear_integration_connection.id,
    )
    .await;
    let existing_notification = update_notification(
        &app,
        existing_notification.id,
        &NotificationPatch {
            task_id: Some(existing_task_id),
            snoozed_until: Some(Utc.with_ymd_and_hms(2064, 1, 1, 0, 0, 0).unwrap()),
            ..NotificationPatch::default()
        },
        app.user.id,
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
    assert_eq!(updated_notification.kind, NotificationSourceKind::Linear);
    assert_eq!(
        updated_notification.source_item.source_id,
        existing_notification.source_item.source_id
    );
    assert_eq!(updated_notification.status, NotificationStatus::Read);
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
        updated_notification.source_item.data,
        ThirdPartyItemData::LinearNotification(Box::new(sync_linear_notifications[2].clone()))
    );
    assert_eq!(
        updated_notification.snoozed_until,
        Some(Utc.with_ymd_and_hms(2023, 8, 12, 7, 0, 0).unwrap())
    );
    // `task_id` should not be reset
    assert_eq!(updated_notification.task_id, Some(existing_task_id));

    let integration_connection = get_integration_connection_per_provider(
        &app,
        app.user.id,
        IntegrationProviderKind::Linear,
        None,
        None,
    )
    .await
    .unwrap();
    assert!(integration_connection
        .last_notifications_sync_started_at
        .is_some());
    assert!(integration_connection
        .last_notifications_sync_completed_at
        .is_some());
    assert!(integration_connection
        .last_notifications_sync_failure_message
        .is_none());
    assert_eq!(integration_connection.notifications_sync_failures, 0);
    assert_eq!(
        integration_connection.status,
        IntegrationConnectionStatus::Validated
    );
    assert!(integration_connection.failure_message.is_none(),);
}

#[rstest]
#[tokio::test]
async fn test_sync_linear_notifications_should_mark_existing_notifications_for_same_issue_as_deleted(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    mut sync_linear_notifications_response: Response<notifications_query::ResponseData>,
    nango_linear_connection: Box<NangoConnection>,
) {
    let app = authenticated_app.await;
    let linear_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Linear(LinearConfig::enabled()),
        &settings,
        nango_linear_connection,
        None,
        None,
    )
    .await;

    let sync_linear_notifications: Vec<LinearNotification> = sync_linear_notifications_response
        .data
        .clone()
        .unwrap()
        .try_into()
        .unwrap();

    let issue_notification1 = sync_linear_notifications[2].clone();
    let LinearNotification::IssueNotification {
        id: issue_notification1_id,
        issue: LinearIssue { id: issue_id, .. },
        ..
    } = &issue_notification1
    else {
        unreachable!("Linear notification was supposed to be an issue notification");
    };
    let issue_notification2_id = Uuid::new_v4();

    let existing_notification = create_notification_from_linear_notification(
        &app.app,
        &issue_notification1,
        app.user.id,
        linear_integration_connection.id,
    )
    .await;
    assert_eq!(existing_notification.status, NotificationStatus::Read);

    // Add a new Linear notification to the mock response
    let modified_data = sync_linear_notifications_response.data.as_mut().unwrap();
    let nodes = &mut modified_data.notifications.nodes;
    let mut new_update_node = nodes[2].clone();
    new_update_node.id = issue_notification2_id.to_string();
    new_update_node.updated_at = Utc::now();
    if let notifications_query::NotificationsQueryNotificationsNodes {
        on:
            notifications_query::NotificationsQueryNotificationsNodesOn::IssueNotification(
                notifications_query::NotificationsQueryNotificationsNodesOnIssueNotification {
                    comment,
                    ..
                },
            ),
        ..
    } = &mut new_update_node
    {
        *comment = Some(
            notifications_query::NotificationsQueryNotificationsNodesOnIssueNotificationComment {
                body: "Updated comment".to_string(),
                updated_at: Utc::now(),
                user: None,
                url: "https://test.com".to_string(),
                parent: None,
            },
        )
    }
    nodes.push(new_update_node);

    let linear_notifications_mock = mock_linear_notifications_query(
        &app.app.linear_mock_server,
        &sync_linear_notifications_response,
    );

    let synced_notifications: Vec<Notification> = sync_notifications(
        &app.client,
        &app.app.api_address,
        Some(NotificationSourceKind::Linear),
        false,
    )
    .await;
    linear_notifications_mock.assert();
    // 6 notifications were fetched but 2 for the same Linear issue
    assert_eq!(synced_notifications.len(), 5);

    let notifications = list_notifications(
        &app.client,
        &app.app.api_address,
        vec![],
        false,
        None,
        None,
        false,
    )
    .await;
    // Find the new notification for the same issue
    let notifications_for_linear_notification: Vec<_> = notifications
        .iter()
        .filter(|n| {
            if let ThirdPartyItemData::LinearNotification(linear_notification) = &n.source_item.data
            {
                if let LinearNotification::IssueNotification { issue, .. } = &**linear_notification
                {
                    issue.id == *issue_id
                } else {
                    false
                }
            } else {
                false
            }
        })
        .collect();

    assert_eq!(notifications_for_linear_notification.len(), 2);
    for notification in notifications_for_linear_notification {
        if notification.source_item.source_id == issue_notification1_id.to_string() {
            assert_eq!(notification.status, NotificationStatus::Deleted);
        } else if notification.source_item.source_id == issue_notification2_id.to_string() {
            assert_eq!(notification.status, NotificationStatus::Read);
        } else {
            unreachable!("Unexpected notification ID");
        }
    }
}
