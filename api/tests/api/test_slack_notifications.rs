use chrono::{TimeZone, Utc};
use httpmock::Method::POST;
use rstest::*;
use serde_json::json;
use slack_morphism::prelude::SlackPushEvent;
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::slack::SlackConfig,
    },
    notification::{
        service::NotificationPatch, Notification, NotificationDetails, NotificationMetadata,
        NotificationStatus,
    },
    task::{integrations::todoist::TodoistItem, Task},
};
use universal_inbox_api::{configuration::Settings, integrations::oauth2::NangoConnection};

use crate::helpers::{
    auth::{authenticated_app, AuthenticatedApp},
    integration_connection::{create_and_mock_integration_connection, nango_slack_connection},
    notification::{
        create_or_update_notification_details,
        slack::{
            mock_slack_stars_add, mock_slack_stars_remove, slack_notification_details,
            slack_push_star_added_event,
        },
    },
    rest::{create_resource, get_resource, patch_resource, patch_resource_response},
    settings,
    task::todoist::{create_task_from_todoist_item, todoist_item},
};

mod patch_resource {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_patch_slack_notification_status_as_deleted(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        nango_slack_connection: Box<NangoConnection>,
        slack_push_star_added_event: Box<SlackPushEvent>,
        todoist_item: Box<TodoistItem>,
        slack_notification_details: Box<NotificationDetails>,
    ) {
        let app = authenticated_app.await;
        create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.integrations.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
            &settings,
            nango_slack_connection,
            None,
        )
        .await;

        let SlackPushEvent::EventCallback(star_added_event) = &*slack_push_star_added_event else {
            unreachable!("Unexpected event type");
        };

        let slack_stars_add_mock =
            mock_slack_stars_add(&app.app.slack_mock_server, "C05XXX", "1707686216.825719");
        let slack_stars_remove_mock =
            mock_slack_stars_remove(&app.app.slack_mock_server, "C05XXX", "1707686216.825719");
        let existing_todoist_task = create_task_from_todoist_item(
            &app.client,
            &app.app.api_address,
            &todoist_item,
            "Project2".to_string(),
            app.user.id,
        )
        .await;
        let expected_notification = Box::new(Notification {
            id: Uuid::new_v4().into(),
            title: "notif1".to_string(),
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            metadata: NotificationMetadata::Slack(Box::new(star_added_event.clone())),
            updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
            user_id: app.user.id,
            details: Some(*slack_notification_details.clone()),
            task_id: Some(existing_todoist_task.task.id),
        });
        let created_notification: Box<Notification> = create_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            expected_notification.clone(),
        )
        .await;
        create_or_update_notification_details(
            &app,
            created_notification.id,
            *slack_notification_details.clone(),
        )
        .await;

        assert_eq!(created_notification, expected_notification);

        let patched_notification = patch_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            created_notification.id.into(),
            &NotificationPatch {
                status: Some(NotificationStatus::Deleted),
                ..Default::default()
            },
        )
        .await;

        assert_eq!(
            patched_notification,
            Box::new(Notification {
                status: NotificationStatus::Deleted,
                ..*created_notification
            })
        );
        slack_stars_add_mock.assert();
        slack_stars_remove_mock.assert();

        let task: Box<Task> = get_resource(
            &app.client,
            &app.app.api_address,
            "tasks",
            existing_todoist_task.task.id.into(),
        )
        .await;
        assert_eq!(task.status, existing_todoist_task.task.status);
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_slack_notification_status_as_unsubscribed(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        nango_slack_connection: Box<NangoConnection>,
        slack_push_star_added_event: Box<SlackPushEvent>,
        slack_notification_details: Box<NotificationDetails>,
    ) {
        let app = authenticated_app.await;
        create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.integrations.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
            &settings,
            nango_slack_connection,
            None,
        )
        .await;

        let SlackPushEvent::EventCallback(star_added_event) = &*slack_push_star_added_event else {
            unreachable!("Unexpected event type");
        };

        let slack_stars_add_mock =
            mock_slack_stars_add(&app.app.slack_mock_server, "C05XXX", "1707686216.825719");
        let slack_stars_remove_mock =
            mock_slack_stars_remove(&app.app.slack_mock_server, "C05XXX", "1707686216.825719");
        let expected_notification = Box::new(Notification {
            id: Uuid::new_v4().into(),
            title: "notif1".to_string(),
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            metadata: NotificationMetadata::Slack(Box::new(star_added_event.clone())),
            updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
            user_id: app.user.id,
            details: Some(*slack_notification_details.clone()),
            task_id: None,
        });
        let created_notification: Box<Notification> = create_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            expected_notification.clone(),
        )
        .await;
        create_or_update_notification_details(
            &app,
            created_notification.id,
            *slack_notification_details.clone(),
        )
        .await;

        assert_eq!(created_notification, expected_notification);

        let patched_notification = patch_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            created_notification.id.into(),
            &NotificationPatch {
                status: Some(NotificationStatus::Unsubscribed),
                ..Default::default()
            },
        )
        .await;

        assert_eq!(
            patched_notification,
            Box::new(Notification {
                status: NotificationStatus::Unsubscribed,
                ..*created_notification
            })
        );
        slack_stars_add_mock.assert();
        slack_stars_remove_mock.assert();
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_slack_notification_status_with_slack_error(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        nango_slack_connection: Box<NangoConnection>,
        slack_push_star_added_event: Box<SlackPushEvent>,
    ) {
        let app = authenticated_app.await;
        create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.integrations.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
            &settings,
            nango_slack_connection,
            None,
        )
        .await;

        let SlackPushEvent::EventCallback(star_added_event) = &*slack_push_star_added_event else {
            unreachable!("Unexpected event type");
        };

        let slack_stars_add_mock = app.app.slack_mock_server.mock(|when, then| {
            when.method(POST)
                .path("/stars.add")
                .header("authorization", "Bearer slack_test_user_access_token")
                .json_body(json!({"channel": "C05XXX", "timestamp": "1707686216.825719"}));
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({ "ok": false, "error": "error_message" }));
        });

        let expected_notification = Box::new(Notification {
            id: Uuid::new_v4().into(),
            title: "notif1".to_string(),
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            metadata: NotificationMetadata::Slack(Box::new(star_added_event.clone())),
            updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
            user_id: app.user.id,
            details: None,
            task_id: None,
        });
        let created_notification: Box<Notification> = create_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            expected_notification.clone(),
        )
        .await;

        assert_eq!(created_notification, expected_notification);

        let patch_response = patch_resource_response(
            &app.client,
            &app.app.api_address,
            "notifications",
            created_notification.id.into(),
            &NotificationPatch {
                status: Some(NotificationStatus::Unsubscribed),
                ..Default::default()
            },
        )
        .await;

        assert_eq!(patch_response.status(), 500);

        let body = patch_response
            .text()
            .await
            .expect("Cannot get response body");
        assert_eq!(
            body,
            json!({ "message": "Failed to add Slack star" }).to_string()
        );
        slack_stars_add_mock.assert();

        let notification: Box<Notification> = get_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            created_notification.id.into(),
        )
        .await;
        assert_eq!(notification.status, NotificationStatus::Unread);
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_slack_notification_snoozed_until(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        nango_slack_connection: Box<NangoConnection>,
        slack_push_star_added_event: Box<SlackPushEvent>,
    ) {
        let app = authenticated_app.await;
        create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.integrations.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
            &settings,
            nango_slack_connection,
            None,
        )
        .await;
        let snoozed_time = Utc.with_ymd_and_hms(2022, 1, 1, 1, 2, 3).unwrap();

        let SlackPushEvent::EventCallback(star_added_event) = &*slack_push_star_added_event else {
            unreachable!("Unexpected event type");
        };

        let expected_notification = Box::new(Notification {
            id: Uuid::new_v4().into(),
            title: "notif1".to_string(),
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            metadata: NotificationMetadata::Slack(Box::new(star_added_event.clone())),
            updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
            user_id: app.user.id,
            details: None,
            task_id: None,
        });
        let created_notification: Box<Notification> = create_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            expected_notification.clone(),
        )
        .await;

        assert_eq!(created_notification, expected_notification);

        let patched_notification = patch_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            created_notification.id.into(),
            &NotificationPatch {
                snoozed_until: Some(snoozed_time),
                ..Default::default()
            },
        )
        .await;

        assert_eq!(
            patched_notification,
            Box::new(Notification {
                snoozed_until: Some(snoozed_time),
                ..*created_notification
            })
        );
    }
}
