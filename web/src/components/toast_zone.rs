#![allow(non_snake_case, clippy::derive_partial_eq_without_eq)]

use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::bs_icons::{BsCheckCircle, BsExclamationTriangle, BsX},
    Icon,
};
use gloo_timers::future::TimeoutFuture;
use js_sys::Date;
use uuid::Uuid;

use crate::{
    components::spinner::Spinner,
    services::toast_service::{ToastCommand, TOASTS},
};

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Toast {
    pub id: Uuid,
    pub message: String,
    pub kind: ToastKind,
    pub timeout: Option<u128>,
}

impl Default for Toast {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            message: Default::default(),
            kind: Default::default(),
            timeout: Default::default(),
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum ToastKind {
    #[default]
    Message,
    Loading,
    Success,
    Failure,
}

#[component]
pub fn ToastZone() -> Element {
    let toast_service = use_coroutine_handle::<ToastCommand>();

    rsx! {
        div {
            class: "toast toast-bottom toast-end items-end absolute bottom-0 right-0",

            for (id, toast) in TOASTS() {
                ToastElement {
                    key: "{id}",
                    message: toast.message.clone(),
                    kind: toast.kind,
                    timeout: toast.timeout,
                    on_close: move |_| {
                        toast_service.send(ToastCommand::Close(id))
                    }
                }
            }
        }
    }
}

#[component]
fn ToastElement(
    message: ReadOnlySignal<String>,
    kind: ReadOnlySignal<ToastKind>,
    timeout: ReadOnlySignal<Option<Option<u128>>>,
    on_undo: Option<EventHandler>,
    on_close: EventHandler,
) -> Element {
    let mut timeout_progress = use_signal(|| 100.0);
    let (alert_style, progress_style) = use_memo(move || {
        let border_style = if timeout().flatten().is_some() {
            "border-0"
        } else {
            "border-t-4"
        };
        match kind() {
            ToastKind::Message => (
                format!("text-info bg-blue-50 border-info {border_style}"),
                "progress-info",
            ),
            ToastKind::Loading => (
                format!("text-info bg-blue-50 border-info {border_style}"),
                "progress-info",
            ),
            ToastKind::Success => (
                format!("text-success bg-green-50 border-success {border_style}"),
                "progress-success",
            ),
            ToastKind::Failure => (
                format!("text-error bg-red-50 border-error {border_style}"),
                "progress-error",
            ),
        }
    })();

    let _ = use_resource(move || async move {
        // Due to a bug in Dioxus 0.5.1 (https://github.com/DioxusLabs/dioxus/issues/2235)
        // reading the `timeout` signal makes the future to be dropped before the end of the loop.
        // So hardcoding the timeout for now.
        let time = 5000;
        // if let Some(time) = timeout().flatten() {
        let time_start = Date::now();
        let time_f = time as f64;
        while Date::now() - time_start < time_f {
            TimeoutFuture::new(10).await;
            let elapsed = Date::now() - time_start;
            *timeout_progress.write() = 100.0 - (elapsed * 100.0 / time_f);
        }
        on_close.call(());
        // }
    });

    rsx! {
        div {
            id: "toast-undo",
            class: "{alert_style} shadow-lg p-0 flex flex-col gap-0 dark:bg-gray-800 w-fit",

            if timeout().flatten().is_some() && (timeout_progress() > 0.0) {
                progress {
                    class: "progress {progress_style} w-full h-1",
                    value: "{timeout_progress}",
                    max: "100"
                }
            }

            div {
                class: "w-full flex items-center px-4 py-2 h-12 gap-4",

                match kind() {
                    ToastKind::Message => rsx! {},
                    ToastKind::Loading => rsx! {
                        Spinner { class: "w-4 h-4" }
                    },
                    ToastKind::Success => rsx! {
                        Icon { class: "w-4 h-4", icon: BsCheckCircle }
                    },
                    ToastKind::Failure => rsx! {
                        Icon { class: "w-4 h-4", icon: BsExclamationTriangle }
                    },
                }

                p {
                    class: "max-w-full whitespace-normal",
                    "{message}"
                }

                if let Some(handler) = on_undo {
                    a { onclick: move |_| handler.call(()), "Undo" }
                }

                button {
                    "type": "button",
                    class: "rounded-sm btn-text p-0 min-h-5 h-5",
                    onclick: move |_| on_close.call(()),
                    span { class: "sr-only", "Close" }
                    Icon { class: "w-5 h-5", icon: BsX }
                }
            }
        }
    }
}
