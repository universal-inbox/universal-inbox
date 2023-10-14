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

#[inline_props]
pub fn ToastZone(cx: Scope) -> Element {
    let toasts_ref = use_atom_ref(cx, &TOASTS);
    let toast_service = use_coroutine_handle::<ToastCommand>(cx).unwrap();

    render! {
        div {
            class: "toast",

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

#[inline_props]
fn Toast<'a>(
    cx: Scope,
    message: String,
    kind: ToastKind,
    timeout: Option<Option<u128>>,
    on_undo: Option<EventHandler<'a>>,
    on_close: EventHandler<'a>,
) -> Element {
    let timeout_progress = use_state(cx, || 50.0);
    let alert_style = use_memo(cx, kind, |kind| match kind {
        ToastKind::Message => "alert-info",
        ToastKind::Loading => "alert-info",
        ToastKind::Success => "alert-success",
        ToastKind::Failure => "alert-error",
    });

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

    let has_callback = on_undo.is_some();
    render! {
        div {
            id: "toast-undo",
            class: "alert {alert_style} shadow-lg p-0 flex flex-col gap-0",
            role: "alert",

            (timeout.flatten().is_some() && (**timeout_progress > 0.0)).then(|| render! {
                progress {
                    class: "progress progress-accent w-full h-1",
                    value: "{timeout_progress}",
                    max: "100"
                }
            })
            div {
                class: "w-full flex items-center divide-x p-2",

                match kind {
                    ToastKind::Message => None,
                    ToastKind::Loading => render! { Spinner {} },
                    ToastKind::Success => render! {
                        Icon {
                            class: "w-12 h-12 px-4", icon: BsCheckCircle
                        }
                    },
                    ToastKind::Failure => render! {
                        Icon {
                            class: "w-12 h-12 px-4", icon: BsExclamationTriangle
                        }
                    },
                }
                div {
                    class: "py-1.5 grow px-2",
                    "{message}"
                }
                div {
                    class: "flex items-center",

                    if has_callback {
                        render! {
                            a {
                                class: "px-2 py-1.5",
                                onclick: move |_| {
                                    if let Some(handler) = on_undo.as_ref() {
                                        handler.call(())
                                    };
                                },
                                "Undo"
                            }
                        }
                    }

                    button {
                        "type": "button",
                        class: "btn btn-ghost",
                        onclick: |_| on_close.call(()),
                        span { class: "sr-only", "Close" }
                        Icon { class: "w-5 h-5", icon: BsX }
                    }
                }
            }
        }
    }
}
