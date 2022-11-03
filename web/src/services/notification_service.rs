use crate::services::api::{call_api, call_api_with_body};
use dioxus::{fermi::UseAtomRef, prelude::*};
use futures_util::StreamExt;
use log::debug;
use std::collections::HashMap;
use universal_inbox::{Notification, NotificationPatch, NotificationStatus};

#[derive(Debug)]
pub enum NotificationCommand {
    Refresh,
    MarkAsDone(Notification),
}

#[derive(Debug, Default)]
pub struct UniversalInboxUIModel {
    pub selected_notification_index: usize,
    pub footer_help_opened: bool,
}

pub static NOTIFICATIONS: AtomRef<Vec<Notification>> = |_| vec![];
pub static UI_MODEL: AtomRef<UniversalInboxUIModel> = |_| Default::default();

pub async fn notification_service(
    mut rx: UnboundedReceiver<NotificationCommand>,
    notifications: UseAtomRef<Vec<Notification>>,
) {
    loop {
        let msg = rx.next().await;
        match msg {
            Some(NotificationCommand::Refresh) => {
                debug!("Fetching notifications from API");
                let result: Vec<Notification> =
                    call_api("GET", "/notifications?status=Unread", HashMap::new())
                        .await
                        .unwrap();
                notifications.write().extend(result);
            }
            Some(NotificationCommand::MarkAsDone(notification)) => {
                debug!("Marking {} as done", notification.id);
                notifications
                    .write()
                    .retain(|notif| notif.id != notification.id);

                let _result: Notification = call_api_with_body(
                    "PATCH",
                    &format!("/notifications/{}", notification.id),
                    NotificationPatch {
                        status: Some(NotificationStatus::Done),
                    },
                    HashMap::new(),
                )
                .await
                .unwrap();
            }
            None => {}
        }
    }
}
