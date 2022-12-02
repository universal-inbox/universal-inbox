use chrono::{TimeZone, Utc};
use http::Uri;

use universal_inbox::notification::{
    integrations::todoist::TodoistTask, Notification, NotificationMetadata, NotificationStatus,
};

pub fn assert_sync_notifications(
    notifications: &[Notification],
    sync_todoist_tasks: &[TodoistTask],
) {
    for notification in notifications.iter() {
        match notification.source_id.as_ref() {
            "1123" => {
                assert_eq!(notification.title, "Task 1".to_string());
                assert_eq!(notification.status, NotificationStatus::Unread);
                assert_eq!(
                    notification.source_html_url,
                    Some(
                        "https://todoist.com/showTask?id=1123"
                            .parse::<Uri>()
                            .unwrap()
                    )
                );
                assert_eq!(
                    notification.updated_at,
                    Utc.with_ymd_and_hms(2019, 12, 11, 22, 36, 50).unwrap()
                );
                assert_eq!(
                    notification.metadata,
                    NotificationMetadata::Todoist(sync_todoist_tasks[0].clone())
                );
            }
            // This notification should be updated
            "1456" => {
                assert_eq!(notification.title, "Task 2".to_string());
                assert_eq!(notification.status, NotificationStatus::Unread);
                assert_eq!(
                    notification.source_html_url,
                    Some(
                        "https://todoist.com/showTask?id=1456"
                            .parse::<Uri>()
                            .unwrap()
                    )
                );
                assert_eq!(
                    notification.updated_at,
                    Utc.with_ymd_and_hms(2019, 12, 11, 22, 37, 50).unwrap()
                );
                assert_eq!(
                    notification.metadata,
                    NotificationMetadata::Todoist(sync_todoist_tasks[1].clone())
                );
            }
            _ => {
                unreachable!("Unexpected notification title '{}'", &notification.title);
            }
        }
    }
}
