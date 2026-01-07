#![allow(non_snake_case, clippy::derive_partial_eq_without_eq)]

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use js_sys::Date;
use uuid::Uuid;

use crate::{
    components::spinner::Spinner,
    services::toast_service::{ToastCommand, TOASTS},
};

#[derive(Clone, PartialEq, Debug)]
pub struct Toast {
    pub id: Uuid,
    pub message: String,
    pub kind: ToastKind,
    pub timeout: Option<u128>,
    pub created_at: f64,  // Timestamp from js_sys::Date::now()
    pub dismissing: bool, // Flag to trigger dismissal animation
}

impl Default for Toast {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            message: Default::default(),
            kind: Default::default(),
            timeout: Default::default(),
            created_at: Date::now(),
            dismissing: false,
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
            class: "notyf max-lg:justify-start! lg:justify-end!",

            for (id, toast) in TOASTS() {
                ToastElement {
                    key: "{id}",
                    message: toast.message.clone(),
                    kind: toast.kind,
                    timeout: toast.timeout,
                    dismissing: toast.dismissing,
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
    dismissing: ReadOnlySignal<bool>,
    on_undo: Option<EventHandler>,
    on_close: EventHandler,
) -> Element {
    let mut timeout_progress = use_signal(|| 50.0);
    let mut dismiss = use_signal(|| "");
    let toast_style = use_memo(move || match kind() {
        ToastKind::Message => "notyf__toast--info bg-info",
        ToastKind::Loading => "notyf__toast--info bg-info",
        ToastKind::Success => "notyf__toast--success bg-success",
        ToastKind::Failure => "notyf__toast--error bg-error",
    })();

    let _ = use_resource(move || async move {
        // BUG: Due to a bug in Dioxus 0.5.1 (https://github.com/DioxusLabs/dioxus/issues/2235)
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
        *dismiss.write() = "notyf__toast--disappear";
        TimeoutFuture::new(300).await;
        on_close.call(());
        // }
    });

    // Handle auto-dismiss when dismissing flag is set
    let _ = use_resource(move || async move {
        if dismissing() {
            *dismiss.write() = "notyf__toast--disappear";
            TimeoutFuture::new(300).await;
            on_close.call(());
        }
    });

    rsx! {
        div {
            id: "toast-element",
            class: "notyf__toast notyf__toast--dismissible notyf__toast--lower max-w-md! {dismiss} {toast_style}",

            div {
                class: "notyf__wrapper",

                match kind() {
                    ToastKind::Message => rsx! {},
                    ToastKind::Loading => rsx! {
                        div {
                            class: "notyf__icon",
                            Spinner { class: "text-info" }
                        }
                    },
                    ToastKind::Success => rsx! {
                        div {
                            class: "notyf__icon",
                            i { class: "notyf__icon--success text-success" }
                        }
                    },
                    ToastKind::Failure => rsx! {
                        div {
                            class: "notyf__icon",
                            i { class: "icon-[tabler--alert-triangle] text-error" }
                        }
                    },
                }

                p { class: "notyf__message text-sm!", "{message}" }

                if let Some(handler) = on_undo {
                    a { onclick: move |_| handler.call(()), "Undo" }
                }

                div {
                    class: "notyf__dismiss",
                    button {
                        "type": "button",
                        class: "notyf__dismiss-btn",
                        onclick: move |_| {
                            spawn({
                                async move {
                                    *dismiss.write() = "notyf__toast--disappear";
                                    TimeoutFuture::new(300).await;
                                    on_close.call(());
                                }
                            });
                        }
                    }
                }
            }
            div { class: "notyf__ripple" }
        }
    }
}
