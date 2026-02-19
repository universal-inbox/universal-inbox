#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{Icon, icons::bs_icons::BsCardChecklist};

use universal_inbox::{
    HasHtmlUrl,
    notification::NotificationWithTask,
    third_party::integrations::ticktick::{TickTickItem, TickTickItemPriority},
};

use crate::{
    components::{
        Tag, TagDisplay,
        integrations::{icons::TickTick, ticktick::list_item::TickTickListItemSubtitle},
        list::{ListContext, ListItem},
        notifications_list::{TaskHint, get_notification_list_item_action_buttons},
    },
    utils::format_elapsed_time,
};

#[component]
pub fn TickTickNotificationListItem(
    notification: ReadSignal<NotificationWithTask>,
    ticktick_item: ReadSignal<TickTickItem>,
    is_selected: ReadSignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    let notification_updated_at = use_memo(move || format_elapsed_time(notification().updated_at));
    let list_context = use_context::<Memo<ListContext>>();
    let task_icon_style = match ticktick_item().priority {
        TickTickItemPriority::High => "",
        TickTickItemPriority::Medium => "text-yellow-500",
        TickTickItemPriority::Low => "text-orange-500",
        TickTickItemPriority::None => "",
    };
    let link = notification().get_html_url();

    rsx! {
        ListItem {
            key: "{notification().id}",
            title: "{notification().title}",
            subtitle: rsx! { TickTickListItemSubtitle { ticktick_item } },
            link,
            icon: rsx! {
                TickTick { class: "h-5 w-5" },
                TaskHint { task: notification().task }
            },
            subicon: rsx! {
                Icon { class: "h-5 w-5 min-w-5 {task_icon_style}", icon: BsCardChecklist }
            },
            action_buttons: get_notification_list_item_action_buttons(
                notification,
                list_context().show_shortcut,
                None,
                None
            ),
            is_selected,
            on_select,

            for tag in ticktick_item()
                .tags
                .unwrap_or_default()
                .into_iter()
                .map(Into::<Tag>::into) {
                    TagDisplay { tag }
                }

            span { class: "text-base-content/50 whitespace-nowrap text-xs font-mono", "{notification_updated_at}" }
        }
    }
}
