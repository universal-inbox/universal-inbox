use universal_inbox::{
    integration_connection::IntegrationConnectionId,
    notification::Notification,
    third_party::{integrations::ticktick::TickTickItem, item::ThirdPartyItemData},
    user::UserId,
};

use crate::helpers::{TestedApp, notification::create_notification_from_source_item};

pub async fn create_notification_from_ticktick_item(
    app: &TestedApp,
    ticktick_item: &TickTickItem,
    user_id: UserId,
    ticktick_integration_connection_id: IntegrationConnectionId,
) -> Box<Notification> {
    create_notification_from_source_item(
        app,
        ticktick_item.id.clone(),
        ThirdPartyItemData::TickTickItem(Box::new(ticktick_item.clone())),
        app.task_service.read().await.ticktick_service.clone(),
        user_id,
        ticktick_integration_connection_id,
    )
    .await
}
