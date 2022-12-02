use log::debug;

use dioxus::core::to_owned;
use dioxus::events::MouseEvent;
use dioxus::fermi::UseAtomRef;
use dioxus::prelude::*;
use dioxus_free_icons::icons::bs_icons::{
    BsBellSlash, BsBookmark, BsCheck2, BsClockHistory, BsTrash,
};
use dioxus_free_icons::Icon;

use universal_inbox::notification::{Notification, NotificationMetadata};

use super::icons::{github, todoist};
use crate::services::notification_service::UniversalInboxUIModel;

#[inline_props]
pub fn notifications_list<'a>(
    cx: Scope,
    notifications: Vec<Notification>,
    ui_model: &'a UseAtomRef<UniversalInboxUIModel>,
    on_delete: EventHandler<'a, &'a Notification>,
    on_unsubscribe: EventHandler<'a, &'a Notification>,
    on_snooze: EventHandler<'a, &'a Notification>,
    on_mark_as_done: EventHandler<'a, &'a Notification>,
) -> Element {
    cx.render(rsx!(table {
        class: "w-full",

        thead {
            tr { class: "border-b border-light-200 dark:border-dark-300" }
        }
        tbody {
            notifications.iter().enumerate().map(|(i, notif)| {
                rsx!{
                    tr {
                        class: "border-b border-light-200 dark:border-dark-300",
                        key: "{notif.id}",

                        self::notification {
                            notif: notif,
                            selected: i == ui_model.read().selected_notification_index,
                            on_delete: |n| on_delete.call(n)
                            on_unsubscribe: |n| on_unsubscribe.call(n)
                            on_snooze: |n| on_snooze.call(n)
                            on_mark_as_done: |n| on_mark_as_done.call(n)
                        }
                    }
                }
            })
        }
    }))
}

#[inline_props]
fn notification<'a>(
    cx: Scope,
    notif: &'a Notification,
    selected: bool,
    on_delete: EventHandler<'a, &'a Notification>,
    on_unsubscribe: EventHandler<'a, &'a Notification>,
    on_snooze: EventHandler<'a, &'a Notification>,
    on_mark_as_done: EventHandler<'a, &'a Notification>,
) -> Element {
    let is_hovered = use_state(&cx, || false);
    let style = use_state(&cx, || "");
    let is_task = use_state(&cx, || false);
    let notif_label = use_state(&cx, || "notification");

    use_effect(&cx, (selected,), |(selected,)| {
        to_owned![style];
        async move {
            style.set(if selected {
                "dark:bg-dark-500 bg-light-200 drop-shadow-lg"
            } else {
                "dark:bg-dark-200 bg-light-0 hover:drop-shadow-lg"
            });
        }
    });

    use_effect(&cx, (&notif.metadata,), |(metadata,)| {
        to_owned![is_task];
        to_owned![notif_label];
        async move {
            match metadata {
                NotificationMetadata::Todoist(_) => {
                    debug!("Setting as task");
                    is_task.set(true);
                    notif_label.set("task");
                }
                _ => {
                    debug!("Setting as notification");
                    is_task.set(false);
                    notif_label.set("notification");
                }
            }
        }
    });

    cx.render(rsx!(
        td {
            class: "flex gap-2 h-10 items-center px-3 py-1 {style}",
            // Buggy as of Dioxus 0.2
            onmouseenter: |_| { is_hovered.set(true); },
            onmouseleave: |_| { is_hovered.set(false); },

            if let Some(link) = &notif.source_html_url {
                rsx!(a {
                    class: "flex grow gap-2",
                    href: "{link}",
                    target: "_blank",

                    self::notification_display { notif: notif }
                })
            } else {
                rsx!(self::notification_display { notif: notif })
            },

            (*selected || *is_hovered.get()).then(|| rsx!(
                self::notification_button {
                    title: "Delete {notif_label}",
                    onclick: |_| on_delete.call(notif),
                    Icon { class: "w-5 h-5" icon: BsTrash }
                },
                (!*is_task.get()).then(|| rsx!(
                    self::notification_button {
                        title: "Unsubscribe from the notification",
                        onclick: |_| on_unsubscribe.call(notif),
                        Icon { class: "w-5 h-5" icon: BsBellSlash }
                    }
                )),
                (*is_task.get()).then(|| rsx!(
                    self::notification_button {
                        title: "Mark task as done",
                        onclick: |_| on_mark_as_done.call(notif),
                        Icon { class: "w-5 h-5" icon: BsCheck2 }
                    }
                )),
                self::notification_button {
                    title: "Snooze notification",
                    onclick: |_| on_snooze.call(notif),
                    Icon { class: "w-5 h-5" icon: BsClockHistory }
                },
                self::notification_button { title: "not yet implemented", Icon { class: "w-5 h-5" icon: BsBookmark } },
            ))
        }
    ))
}

#[inline_props]
fn notification_display<'a>(cx: Scope, notif: &'a Notification) -> Element {
    let icon = match notif.metadata {
        NotificationMetadata::Github(_) => cx.render(rsx!(self::github {})),
        NotificationMetadata::Todoist(_) => cx.render(rsx!(self::todoist {})),
    };

    cx.render(rsx!(
        div {
            class: "flex flex-none h-6 items-center",
            div { class: "h-5 w-5", icon }
        }
        div {
            class: "flex grow text-sm justify-left",
            "{notif.title}"
        }
    ))
}

#[derive(Props)]
struct NotificationButtonProps<'a> {
    children: Element<'a>,
    title: &'a str,
    #[props(optional)]
    onclick: Option<EventHandler<'a, MouseEvent>>,
}

fn notification_button<'a>(cx: Scope<'a, NotificationButtonProps<'a>>) -> Element {
    cx.render(rsx!(
        div {
            class: "flex flex-none justify-center bg-light-200 dark:bg-dark-500 h-8 w-8 hover:shadow-md hover:bg-light-400 hover:dark:bg-dark-600",
            onclick: move |evt| {
                if let Some(handler) = &cx.props.onclick {
                    handler.call(evt)
                }
            },

            button {
                class: "text-sm",
                title: "{cx.props.title}",

                &cx.props.children
            }
        }
    ))
}
