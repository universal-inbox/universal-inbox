use universal_inbox::{
    integration_connection::IntegrationConnectionId,
    notification::Notification,
    third_party::{integrations::todoist::TodoistItem, item::ThirdPartyItemData},
    user::UserId,
};

use crate::helpers::{TestedApp, notification::create_notification_from_source_item};

pub async fn create_notification_from_todoist_item(
    app: &TestedApp,
    todoist_item: &TodoistItem,
    user_id: UserId,
    todoist_integration_connection_id: IntegrationConnectionId,
) -> Box<Notification> {
    create_notification_from_source_item(
        app,
        todoist_item.id.clone(),
        ThirdPartyItemData::TodoistItem(Box::new(todoist_item.clone())),
        app.task_service.read().await.todoist_service.clone(),
        user_id,
        todoist_integration_connection_id,
    )
    .await
}
