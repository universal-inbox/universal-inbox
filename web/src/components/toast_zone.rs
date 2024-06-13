#![allow(non_snake_case, clippy::derive_partial_eq_without_eq)]

use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::bs_icons::{BsCheckCircle, BsExclamationTriangle, BsX},
    Icon,
};
use fermi::use_atom_ref;
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

#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub enum ToastKind {
    #[default]
    Message,
    Loading,
    Success,
    Failure,
}

#[component]
pub fn ToastZone(cx: Scope) -> Element {
    let toasts_ref = use_atom_ref(cx, &TOASTS);
    let toast_service = use_coroutine_handle::<ToastCommand>(cx).unwrap();

    render! {
        div {
            class: "toast toast-bottom toast-end items-end absolute bottom-0 right-0",

            toasts_ref.read().clone().into_iter().map(move |(id, toast)| {
                render! {
                    Toast {
                        key: "{id}",
                        message: toast.message.clone(),
                        kind: toast.kind.clone(),
                        timeout: toast.timeout,
                        on_close: move |_| {
                            toast_service.send(ToastCommand::Close(id))
                        }
                    }
                }
            })
        }
    }
}

#[component]
fn Toast<'a>(
    cx: Scope,
    message: String,
    kind: ToastKind,
    timeout: Option<Option<u128>>,
    on_undo: Option<EventHandler<'a>>,
    on_close: EventHandler<'a>,
) -> Element {
    let timeout_progress = use_state(cx, || 50.0);
    let (alert_style, progress_style) = use_memo(
        cx,
        (kind, &timeout.flatten().is_some()),
        |(kind, has_timeout)| {
            let border_style = if has_timeout {
                "border-0"
            } else {
                "border-t-4"
            };
            match kind {
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
        },
    );

    let timeout_future = use_future(cx, (timeout,), |(timeout,)| {
        to_owned![timeout_progress];
        async move {
            if let Some(time) = timeout.flatten() {
                let time_start = Date::now();
                let time_f = time as f64;
                while Date::now() - time_start < time_f {
                    TimeoutFuture::new(10).await;
                    let elapsed = Date::now() - time_start;
                    timeout_progress.set(100.0 - (elapsed * 100.0 / time_f));
                }
                return true;
            }
            false
        }
    });
    if let Some(should_close_toast) = timeout_future.value() {
        if *should_close_toast {
            on_close.call(());
        }
    }

    render! {
        div {
            id: "toast-undo",
            class: "{alert_style} shadow-lg p-0 flex flex-col gap-0 dark:bg-gray-800 w-fit",

            if timeout.flatten().is_some() && (**timeout_progress > 0.0) {
                render! {
                    progress {
                        class: "progress {progress_style} w-full h-1",
                        value: "{timeout_progress}",
                        max: "100"
                    }
                }
            }

            div {
                class: "w-full flex items-center px-4 py-2 h-12 gap-4",

                match kind {
                    ToastKind::Message => None,
                    ToastKind::Loading => render! {
                        Spinner { class: "w-4 h-4" }
                    },
                    ToastKind::Success => render! {
                        Icon { class: "w-4 h-4", icon: BsCheckCircle }
                    },
                    ToastKind::Failure => render! {
                        Icon { class: "w-4 h-4", icon: BsExclamationTriangle }
                    },
                }

                p {
                    class: "max-w-full whitespace-normal",
                    "{message}"
                }

                if let Some(handler) = on_undo.as_ref() {
                    render! {
                        a { onclick: move |_| handler.call(()), "Undo" }
                    }
                }

                button {
                    "type": "button",
                    class: "rounded btn-ghost p-0 min-h-5 h-5",
                    onclick: |_| on_close.call(()),
                    span { class: "sr-only", "Close" }
                    Icon { class: "w-5 h-5", icon: BsX }
                }
            }
        }
    }
}
