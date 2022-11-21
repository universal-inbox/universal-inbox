#![allow(clippy::derive_partial_eq_without_eq)]

use dioxus::core::to_owned;
use dioxus::prelude::*;
use dioxus_free_icons::icons::bs_icons::{BsCheck2, BsX};
use dioxus_free_icons::Icon;
use gloo_timers::future::TimeoutFuture;
use js_sys::Date;
use uuid::Uuid;

use crate::components::spinner::spinner;
use crate::services::toast_service::{ToastCommand, TOASTS};

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
}

#[inline_props]
pub fn toast_zone(cx: Scope) -> Element {
    let toasts = use_atom_ref(&cx, TOASTS);
    let toast_service = use_coroutine_handle::<ToastCommand>(&cx).unwrap();

    cx.render(rsx! {
        div {
            class: "absolute bottom-5 right-5 flex flex-col gap-2",

            toasts.read().clone().into_iter().map(move |(id, toast)| {
                rsx!(
                    self::toast {
                        key: "{id}",
                        message: toast.message.clone(),
                        kind: toast.kind.clone(),
                        timeout: toast.timeout,
                        on_close: move |_| {
                            toast_service.send(ToastCommand::Close(id))
                        }
                    }
                )
            })
        }
    })
}

#[inline_props]
fn toast<'a>(
    cx: Scope,
    message: String,
    kind: ToastKind,
    timeout: Option<Option<u128>>,
    on_undo: Option<EventHandler<'a>>,
    on_close: EventHandler<'a>,
) -> Element {
    let timeout_progress = use_state(&cx, || 100.0);

    let timeout_future = use_future(&cx, (timeout,), |(timeout,)| {
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
    cx.render(rsx! {
        div {
            id: "toast-undo",
            class: "w-full max-w-sm text-sm shadow bg-light-200/90 dark:bg-dark-500/90",
            role: "alert",

            (timeout.flatten().is_some() && (**timeout_progress > 0.0)).then(|| rsx!(
                div {
                    class: "w-full rounded-full h-0.5",
                    div { class: "bg-light-500 dark:bg-dark-600 h-0.5 rounded-full", style: "width: {timeout_progress}%" }
                }
            ))
            div {
                class: "flex items-center p-2 divide-x divide-light-400 dark:divide-dark-500",

                match kind {
                    ToastKind::Message => None,
                    ToastKind::Loading => cx.render(rsx!(self::spinner {})),
                    ToastKind::Success => cx.render(rsx!(Icon { class: "w-8 h-8 px-2", icon: BsCheck2 }))
                }
                div {
                    class: "py-1.5 grow px-2",
                    "{message}"
                }
                div {
                    class: "flex items-center",

                    has_callback.then(|| rsx!(
                        a {
                            class: "px-2 py-1.5 rounded-lg text-light-500 dark:text-dark-700 hover:bg-light-400 hover:dark:bg-dark-600 hover:dark:text-white",
                            onclick: move |_| {
                                if let Some(handler) = on_undo.as_ref() {
                                    handler.call(())
                                };
                            },
                            "Undo"
                        }
                    ))

                    button {
                        "type": "button",
                        class: "h-8 w-8 px-2 py-1.5 rounded-lg inline-flex hover:shadow-md hover:bg-light-400 hover:dark:bg-dark-600",
                        onclick: |_| on_close.call(()),
                        span { class: "sr-only", "Close" }
                        Icon { class: "w-5 h-5", icon: BsX }
                    }
                }
            }
        }
    })
}
