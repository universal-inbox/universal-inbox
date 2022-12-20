use chrono::{TimeZone, Utc};
use rstest::*;
use serde_json::json;

use universal_inbox::{
    notification::{Notification, NotificationMetadata, NotificationPatch, NotificationStatus},
    task::integrations::todoist::get_task_html_url,
};

use crate::helpers::{
    rest::{create_resource, patch_resource_response},
    tested_app, TestedApp,
};

mod patch_notification {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_patch_todoist_notification_status(
        #[future] tested_app: TestedApp,
        #[values(NotificationStatus::Unsubscribed, NotificationStatus::Deleted)]
        new_status: NotificationStatus,
    ) {
        let app = tested_app.await;
        let expected_notification = Box::new(Notification {
            id: uuid::Uuid::new_v4(),
            title: "task 1".to_string(),
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            source_html_url: get_task_html_url("1234"),
            metadata: NotificationMetadata::Todoist,
            updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
            task_id: None,
            task_source_id: None,
        });
        let created_notification: Box<Notification> = create_resource(
            &app.app_address,
            "notifications",
            expected_notification.clone(),
        )
        .await;

        assert_eq!(created_notification, expected_notification);

        let response = patch_resource_response(
            &app.app_address,
            "notifications",
            created_notification.id,
            &NotificationPatch {
                status: Some(new_status),
                ..Default::default()
            },
        )
        .await;

        assert_eq!(response.status(), 400);

        let body = response.text().await.expect("Cannot get response body");
        assert_eq!(
            body,
            json!({
                "message": format!(
                    "Unsupported action: Cannot update the status of Todoist notification {}, update task's project",
                    expected_notification.id
                )
            })
            .to_string()
        );
    }
}
