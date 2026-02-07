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

pub static VIEWPORT_WIDTH: GlobalSignal<f64> = Signal::global(|| 0.0);

pub fn current_toast_limit(width: f64) -> usize {
    if width >= 1024.0 { 5 } else { 1 }
}

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

                // Check if we need to dismiss old toasts
                let current_limit = current_toast_limit(VIEWPORT_WIDTH());
                let mut toast_list: Vec<_> = toasts
                    .read()
                    .iter()
                    .filter(|(_, t)| !t.dismissing) // Only count non-dismissing toasts
                    .map(|(id, toast)| (*id, toast.created_at))
                    .collect();

                if toast_list.len() > current_limit {
                    let excess_count = toast_list.len() - current_limit;

                    // Sort toasts by created_at to find oldest
                    toast_list.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

                    // Mark the oldest toasts as dismissing
                    for (toast_id, _) in toast_list.iter().take(excess_count) {
                        debug!("Auto-dismissing old Toast {} due to limit", toast_id);
                        let mut writable_toasts = toasts.write();
                        if let Some(toast) = writable_toasts.get(toast_id) {
                            let mut dismissing_toast = toast.clone();
                            dismissing_toast.dismissing = true;
                            writable_toasts.insert(*toast_id, dismissing_toast);
                        }
                    }
                }
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
                        created_at: toast.created_at,
                        dismissing: toast.dismissing,
                    };
                    writable_toasts.insert(updated_toast.id, updated_toast);
                };
            }
            None => {}
        }
    }
}
