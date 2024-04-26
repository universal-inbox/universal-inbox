use std::collections::HashMap;

use dioxus::prelude::*;

use futures_util::StreamExt;
use log::debug;
use uuid::Uuid;

use crate::components::toast_zone::{Toast, ToastKind};

#[derive(Default)]
pub struct ToastUpdate {
    pub id: Uuid,
    pub message: Option<String>,
    pub kind: Option<ToastKind>,
    pub timeout: Option<Option<u128>>,
}

pub enum ToastCommand {
    Push(Toast),
    Close(Uuid),
    Update(ToastUpdate),
}

pub static TOASTS: GlobalSignal<HashMap<Uuid, Toast>> = Signal::global(HashMap::new);

pub async fn toast_service(
    mut rx: UnboundedReceiver<ToastCommand>,
    mut toasts: Signal<HashMap<Uuid, Toast>>,
) {
    loop {
        let msg = rx.next().await;
        match msg {
            Some(ToastCommand::Push(toast)) => {
                debug!("Pushing new Toast {}", toast.id);
                toasts.write().insert(toast.id, toast);
            }
            Some(ToastCommand::Close(id)) => {
                debug!("Closing Toast {}", id);
                toasts.write().remove(&id);
            }
            Some(ToastCommand::Update(toast_update)) => {
                debug!("Got Toast update command for {}", toast_update.id);
                let mut writable_toasts = toasts.write();
                if let Some(toast) = writable_toasts.get(&toast_update.id) {
                    debug!("Updating Toast {}", toast_update.id);
                    let updated_toast = Toast {
                        id: toast_update.id,
                        message: toast_update
                            .message
                            .unwrap_or_else(|| toast.message.clone()),
                        kind: toast_update.kind.unwrap_or(toast.kind),
                        timeout: toast_update.timeout.unwrap_or(toast.timeout),
                    };
                    writable_toasts.insert(updated_toast.id, updated_toast);
                };
            }
            None => {}
        }
    }
}
