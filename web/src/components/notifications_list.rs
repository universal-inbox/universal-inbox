use dioxus::{events::MouseEvent, prelude::*};
use dioxus_free_icons::{
    icons::bs_icons::{BsBellSlash, BsBookmark, BsCheck2, BsClockHistory, BsTrash},
    Icon,
};
use fermi::UseAtomRef;

use universal_inbox::notification::{NotificationMetadata, NotificationWithTask};

use crate::{
    components::icons::{github, todoist},
    services::notification_service::UniversalInboxUIModel,
};

#[inline_props]
pub fn notifications_list<'a>(
    cx: Scope,
    notifications: Vec<NotificationWithTask>,
    ui_model_ref: UseAtomRef<UniversalInboxUIModel>,
    on_delete: EventHandler<'a, &'a NotificationWithTask>,
    on_unsubscribe: EventHandler<'a, &'a NotificationWithTask>,
    on_snooze: EventHandler<'a, &'a NotificationWithTask>,
    on_complete_task: EventHandler<'a, &'a NotificationWithTask>,
    on_plan: EventHandler<'a, &'a NotificationWithTask>,
) -> Element {
    let selected_notification_index = ui_model_ref.read().selected_notification_index;

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

                        (!notif.is_built_from_task()).then(|| rsx!(
                            self::notification {
                                notif: notif,
                                selected: i == selected_notification_index,
                                ui_model_ref: ui_model_ref,

                                self::notification_button {
                                    title: "Delete notification",
                                    onclick: |_| on_delete.call(notif),
                                    Icon { class: "w-5 h-5" icon: BsTrash }
                                },
                                self::notification_button {
                                    title: "Unsubscribe from the notification",
                                    onclick: |_| on_unsubscribe.call(notif),
                                    Icon { class: "w-5 h-5" icon: BsBellSlash }
                                }
                                self::notification_button {
                                    title: "Snooze notification",
                                    onclick: |_| on_snooze.call(notif),
                                    Icon { class: "w-5 h-5" icon: BsClockHistory }
                                },
                                self::notification_button {
                                    title: "Create task",
                                    onclick: |_| on_plan.call(notif),
                                    Icon { class: "w-5 h-5" icon: BsBookmark }
                                },
                            }
                        )),

                        (notif.is_built_from_task()).then(|| rsx!(
                            self::notification {
                                notif: notif,
                                selected: i == selected_notification_index,
                                ui_model_ref: ui_model_ref,

                                self::notification_button {
                                    title: "Delete task",
                                    onclick: |_| on_delete.call(notif),
                                    Icon { class: "w-5 h-5" icon: BsTrash }
                                },
                                self::notification_button {
                                    title: "Complete task",
                                    onclick: |_| on_complete_task.call(notif),
                                    Icon { class: "w-5 h-5" icon: BsCheck2 }
                                }
                                self::notification_button {
                                    title: "Snooze notification",
                                    onclick: |_| on_snooze.call(notif),
                                    Icon { class: "w-5 h-5" icon: BsClockHistory }
                                },
                                self::notification_button {
                                    title: "Plan task",
                                    onclick: |_| on_plan.call(notif),
                                    Icon { class: "w-5 h-5" icon: BsBookmark }
                                }
                            }
                        ))
                    }
                }
            })
        }
    }))
}

#[inline_props]
fn notification<'a>(
    cx: Scope,
    notif: &'a NotificationWithTask,
    selected: bool,
    ui_model_ref: &'a UseAtomRef<UniversalInboxUIModel>,
    children: Element<'a>,
) -> Element {
    let is_hovered = use_state(cx, || false);
    let style = use_state(cx, || "");
    let unhover_element = ui_model_ref.read().unhover_element;

    use_memo(cx, (selected,), |(selected,)| {
        style.set(if selected {
            "dark:bg-dark-500 bg-light-200 drop-shadow-lg"
        } else {
            "dark:bg-dark-200 bg-light-0 hover:drop-shadow-lg"
        });
    });

    cx.render(rsx!(
        td {
            class: "flex gap-2 h-10 items-center px-3 py-1 {style}",
            onmousemove: |_| {
                if ui_model_ref.write_silent().set_unhover_element(false) {
                    cx.needs_update();
                }
            },
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

            (*selected || (!unhover_element && *is_hovered.get())).then(|| rsx!(
                children
            ))
        }
    ))
}

#[inline_props]
fn notification_display<'a>(cx: Scope, notif: &'a NotificationWithTask) -> Element {
    let icon = match notif.metadata {
        NotificationMetadata::Github(_) => cx.render(rsx!(self::github {})),
        NotificationMetadata::Todoist => cx.render(rsx!(self::todoist {})),
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
