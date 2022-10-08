use crate::services::api::call_api;
use dioxus::{fermi::UseAtomRef, prelude::*};
use futures_util::StreamExt;
use log::debug;
use std::collections::HashMap;
use universal_inbox::Notification;

#[derive(Debug)]
pub enum NotificationCommand {
    Refresh,
}

pub static NOTIFICATIONS: AtomRef<Vec<Notification>> = |_| vec![];
pub static SELECTED_NOTIFICATION_INDEX: AtomRef<usize> = |_| 0;

pub async fn notification_service(
    mut rx: UnboundedReceiver<NotificationCommand>,
    notifications: UseAtomRef<Vec<Notification>>,
) {
    loop {
        let msg = rx.next().await;
        if let Some(NotificationCommand::Refresh) = msg {
            debug!("Fetching notifications from API");
            let result = call_api("GET", "/notifications", HashMap::new())
                .await
                .unwrap();
            notifications.write().extend(result);
        }
    }
}
