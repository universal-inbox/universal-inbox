use ::universal_inbox::Notification;
use dioxus::core::to_owned;
use dioxus::fermi::UseAtomRef;
use dioxus::prelude::*;
use dioxus_free_icons::icons::bs_icons::{
    BsBellSlash, BsBookmark, BsCheck2, BsClockHistory, BsGithub,
};
use dioxus_free_icons::Icon;

#[inline_props]
pub fn notifications_list<'a>(
    cx: Scope,
    notifications: Vec<Notification>,
    selected_notification_index: &'a UseAtomRef<usize>,
) -> Element {
    cx.render(rsx!(ul {
        class: "flex flex-col gap-2",

        notifications.iter().enumerate().map(|(i, notif)| {
            rsx!{
                li {
                    key: "{notif.id}",

                    self::notification {
                        notif: notif,
                        selected: i == *(selected_notification_index.read())
                    }
                }
            }
        })
    }))
}

#[inline_props]
fn notification<'a>(cx: Scope, notif: &'a Notification, selected: bool) -> Element {
    let is_hovered = use_state(&cx, || false);
    let style = use_state(&cx, || "");

    use_effect(&cx, (selected,), |(selected,)| {
        to_owned![style];
        async move {
            style.set(if selected {
                "dark:bg-dark-200 bg-light-200 border-solid border shadow-lg"
            } else {
                "dark:bg-dark-500 bg-light-0 hover:border-solid hover:border"
            });
        }
    });

    cx.render(rsx!(
        div {
            class: "flex gap-2 rounded-lg h-14 items-center p-3 dark:border-white border-black {style}",
            onmouseenter: |_| { is_hovered.set(true); },
            onmouseleave: |_| { is_hovered.set(false); },

            div {
                class: "flex flex-none h-6 items-center",
                Icon { class: "w-5 h-5" icon: BsGithub }
            },
            div {
                class: "flex grow text-sm justify-left",
                "{notif.title}"
            },
            (*selected || *is_hovered.get()).then(|| rsx!(
                self::notification_button { Icon { class: "w-5 h-5" icon: BsCheck2 } },
                self::notification_button { Icon { class: "w-5 h-5" icon: BsBellSlash } },
                self::notification_button { Icon { class: "w-5 h-5" icon: BsClockHistory } },
                self::notification_button { Icon { class: "w-5 h-5" icon: BsBookmark } },
            ))
        }
    ))
}

#[derive(Props)]
struct NotificationButtonProps<'a> {
    children: Element<'a>,
}

fn notification_button<'a>(cx: Scope<'a, NotificationButtonProps<'a>>) -> Element {
    cx.render(rsx!(
        div {
            class: "flex flex-none justify-center rounded-lg bg-light-300 dark:bg-dark-300 border-black dark:border-white h-8 w-8 hover:border-solid hover:border",
            button {
                class: "text-sm",
                &cx.props.children
            }
        }
    ))
}
