use crate::{
    components::toast_zone::{Toast, ToastKind},
    services::{
        api::{call_api, call_api_with_body},
        toast_service::ToastUpdate,
    },
};
use dioxus::{fermi::UseAtomRef, prelude::*};
use futures_util::StreamExt;
use std::collections::HashMap;
use universal_inbox::{Notification, NotificationPatch, NotificationStatus};
use wasm_bindgen::JsValue;

use super::toast_service::ToastCommand;

#[derive(Debug)]
pub enum NotificationCommand {
    Refresh,
    Delete(Notification),
    Unsubscribe(Notification),
}

#[derive(Debug, Default)]
pub struct UniversalInboxUIModel {
    pub selected_notification_index: usize,
    pub footer_help_opened: bool,
}

pub static NOTIFICATIONS: AtomRef<Vec<Notification>> = |_| vec![];
pub static UI_MODEL: AtomRef<UniversalInboxUIModel> = |_| Default::default();

pub async fn notification_service<'a>(
    mut rx: UnboundedReceiver<NotificationCommand>,
    notifications: UseAtomRef<Vec<Notification>>,
    toast_service: CoroutineHandle<ToastCommand>,
) {
    loop {
        let msg = rx.next().await;
        match msg {
            Some(NotificationCommand::Refresh) => {
                let toast = Toast {
                    kind: ToastKind::Loading,
                    message: "Loading notifications...".to_string(),
                    ..Default::default()
                };
                let toast_id = toast.id;
                toast_service.send(ToastCommand::Push(toast));

                let result: Vec<Notification> =
                    call_api("GET", "/notifications?status=Unread", HashMap::new())
                        .await
                        .unwrap();
                notifications.write().extend(result);

                let toast_update = ToastUpdate {
                    id: toast_id,
                    kind: Some(ToastKind::Success),
                    message: Some("Successfully loaded notifications".to_string()),
                    timeout: Some(Some(5_000)),
                };
                toast_service.send(ToastCommand::Update(toast_update));
            }
            Some(NotificationCommand::Delete(notification)) => {
                notifications
                    .write()
                    .retain(|notif| notif.id != notification.id);

                let _result: Notification = call_api_and_notify(
                    "PATCH",
                    &format!("/notifications/{}", notification.id),
                    NotificationPatch {
                        status: Some(NotificationStatus::Deleted),
                    },
                    HashMap::new(),
                    &toast_service,
                    "Deleting notification...",
                    "Successfully deleted notification",
                )
                .await
                .unwrap();
            }
            Some(NotificationCommand::Unsubscribe(notification)) => {
                notifications
                    .write()
                    .retain(|notif| notif.id != notification.id);

                let _result: Notification = call_api_and_notify(
                    "PATCH",
                    &format!("/notifications/{}", notification.id),
                    NotificationPatch {
                        status: Some(NotificationStatus::Unsubscribed),
                    },
                    HashMap::new(),
                    &toast_service,
                    "Unsubscribing from notification...",
                    "Successfully unsubscribed from notification",
                )
                .await
                .unwrap();
            }
            None => {}
        }
    }
}

async fn call_api_and_notify<R: for<'de> serde::de::Deserialize<'de>, B: serde::Serialize>(
    method: &str,
    path: &str,
    body: B,
    headers: HashMap<String, String>,
    toast_service: &CoroutineHandle<ToastCommand>,
    loading_message: &str,
    success_message: &str,
) -> Result<R, JsValue> {
    let toast = Toast {
        kind: ToastKind::Loading,
        message: loading_message.to_string(),
        ..Default::default()
    };
    let toast_id = toast.id;
    toast_service.send(ToastCommand::Push(toast));

    let result: R = call_api_with_body(method, path, body, headers).await?;

    let toast_update = ToastUpdate {
        id: toast_id,
        kind: Some(ToastKind::Success),
        message: Some(success_message.to_string()),
        timeout: Some(Some(5_000)),
    };
    toast_service.send(ToastCommand::Update(toast_update));

    Ok(result)
}
