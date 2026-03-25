#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus::web::WebEventExt;

use universal_inbox::{
    Page,
    notification::{NotificationListOrder, NotificationStatus, NotificationWithTask},
    task::{TaskId, TaskPlanning},
    third_party::item::ThirdPartyItemData,
};

use crate::{
    components::{
        delete_all_confirmation_modal::DeleteAllConfirmationModal,
        flyonui::tooltip::Tooltip,
        integrations::{
            api::web_page::notification_list_item::WebPageNotificationListItem,
            github::notification_list_item::GithubNotificationListItem,
            google_calendar::notification_list_item::GoogleCalendarEventListItem,
            google_drive::notification_list_item::GoogleDriveCommentListItem,
            google_mail::notification_list_item::GoogleMailThreadListItem,
            icons::IntegrationProviderIcon,
            linear::notification_list_item::LinearNotificationListItem,
            slack::notification_list_item::{
                SlackReactionNotificationListItem, SlackThreadNotificationListItem,
            },
            ticktick::notification_list_item::TickTickNotificationListItem,
            todoist::notification_list_item::TodoistNotificationListItem,
        },
        list::{List, ListPaginationButtons},
        task_link_modal::TaskLinkModal,
        task_planning_modal::TaskPlanningModal,
        ui::{Badge, BadgeVariant, Button, ButtonVariant, use_outside_close},
    },
    config::get_api_base_url,
    icons::UILogo,
    model::UI_MODEL,
    services::{
        flyonui::open_flyonui_modal,
        integration_connection_service::{
            TASK_SERVICE_INTEGRATION_CONNECTION, TASK_SERVICE_INTEGRATION_CONNECTIONS,
        },
        notification_service::{
            NotificationCommand, NotificationFilters, NotificationSourceKindFilter,
        },
    },
};

#[derive(Clone, PartialEq)]
pub struct NotificationListContext {
    pub is_task_actions_enabled: bool,
    pub notification_service: Coroutine<NotificationCommand>,
}

#[component]
pub fn NotificationsList(
    notifications: ReadSignal<Page<NotificationWithTask>>,
    notification_filters: Signal<NotificationFilters>,
) -> Element {
    let api_base_url = use_memo(move || get_api_base_url().unwrap());
    let notification_service = use_coroutine_handle::<NotificationCommand>();
    let context = use_memo(move || NotificationListContext {
        is_task_actions_enabled: UI_MODEL.read().is_task_actions_enabled,
        notification_service,
    });
    use_context_provider(move || context);
    let current_notification = UI_MODEL
        .read()
        .selected_notification_index
        .and_then(|index| {
            notifications()
                .content
                .get(index)
                .map(|notification| Signal::new(notification.clone()))
        });
    let current_page = use_signal(|| 1);
    let filters_str = notification_filters()
        .selected()
        .iter()
        .map(|f| f.to_string())
        .collect::<Vec<_>>()
        .join(", ");

    rsx! {
        div {
            id: "notifications-list",
            // `.list-panel` class hook is preserved because the shell properties
            // (background, border-right, flex column, overflow:hidden) still
            // live in `web/css/universal-inbox.css`. Responsive width is now
            // expressed via Tailwind `max-*` variants on this element. The
            // `!` (`!important` in Tailwind v4) is required to win against the
            // `.list-panel { width: 380px; ... }` rule declared later in the
            // cascade with equal specificity.
            class: "list-panel max-xl:w-[360px]! max-xl:min-w-[300px]! max-lg:w-[320px]! max-lg:min-w-[280px]! max-md:w-full! max-md:min-w-0! max-md:[.app-layout.show-detail_&]:hidden!",

            // ── List header ──
            div {
                class: "flex items-center justify-between py-1.5 px-5 border-b border-ui-border bg-ui-surface",
                div {
                    class: "flex flex-col flex-1 min-w-0",
                    div {
                        class: "flex items-center justify-between gap-2",
                        h1 {
                            class: "flex items-center gap-2 text-[15px] font-bold tracking-tight",
                            "Inbox"
                            if notifications().total > 0 {
                                Badge { variant: BadgeVariant::Count, "{notifications().total}" }
                            }
                        }
                        div {
                            class: "flex items-center gap-1",

                            Button {
                                variant: ButtonVariant::Ghost,
                                title: "Refresh notifications".to_string(),
                                aria_label: "Refresh notifications".to_string(),
                                onclick: move |_| notification_service.send(NotificationCommand::Refresh),
                                icon_class: "icon-[lucide--refresh-cw]".to_string(),
                                enable_tooltip: true
                            }
                            if !notifications().content.is_empty() {
                                Button {
                                    variant: ButtonVariant::Danger,
                                    title: "Delete all notifications".to_string(),
                                    aria_label: "Delete all notifications".to_string(),
                                    onclick: move |_| open_flyonui_modal("#delete-all-confirmation-modal"),
                                    icon_class: "icon-[lucide--trash-2]".to_string(),
                                    enable_tooltip: true
                                }
                            }
                        }
                    }
                }
            }

            // ── Filter section ──
            div {
                class: "py-1.5 px-5 border-b border-ui-border",
                // `filter-row` stays as a class hook so the `::-webkit-scrollbar`
                // hide rule keeps applying — it's a last-resort form-control
                // internal that doesn't have a clean utility equivalent.
                div {
                    class: "filter-row flex items-center gap-1 overflow-x-auto overflow-y-hidden",
                    SourceFilter {
                        notification_source_kind_filters: notification_filters().notification_source_kind_filters,
                        on_select: move |filter| {
                            notification_filters.write().select(filter);
                            notification_service.send(NotificationCommand::Refresh);
                        },
                        on_clear: move |_| {
                            notification_filters.write().reset();
                            notification_service.send(NotificationCommand::Refresh);
                        },
                    }

                    div {
                        class: "flex-1 flex items-center justify-center min-w-0",
                        if notifications().pages_count > 1 {
                            ListPaginationButtons {
                                current_page,
                                page: notifications,
                                on_select: move |selected_page_token| {
                                    notification_filters.write().current_page_token = selected_page_token;
                                    notification_service.send(NotificationCommand::Refresh);
                                }
                            }
                        }
                    }

                    NotificationListOrdering {
                        notification_list_order: notification_filters().sort_by,
                        on_change: move |new_order| {
                            notification_filters.write().sort_by = new_order;
                            notification_service.send(NotificationCommand::Refresh);
                        }
                    }
                }
            }

            // ── Notification list ──
            if notifications().content.is_empty() && notification_filters().is_filtered() {
                div {
                    class: "relative w-full h-full flex justify-center items-center",
                    UILogo {
                        class: "opacity-30 dark:opacity-10 w-96 h-96",
                        alt: "No notifications"
                    }
                    div {
                        class: "flex flex-col items-center absolute object-center top-2/3 transform translate-y-1/4",
                        p {
                            class: "text-base-content/50",
                            "There's no new {filters_str} notifications"
                        }
                    }
                }
            } else {
                div {
                    class: "notification-list snap-y snap-mandatory",
                    List {
                        id: "notifications_list",

                        for (i, notification) in notifications().content.into_iter().map(Signal::new).enumerate() {
                            NotificationListItem {
                                notification,
                                is_selected: Some(i) == UI_MODEL.read().selected_notification_index,
                                on_select: move |_| {
                                    UI_MODEL.write().selected_notification_index = Some(i);
                                    let notif = notification();
                                    if notif.status == NotificationStatus::Unread {
                                        notification_service.send(NotificationCommand::MarkAsRead(notif.id));
                                    }
                                },
                            }
                        }
                    }
                }
            }
        }

        if let Some(notification) = current_notification {
                TaskPlanningModal {
                    api_base_url: api_base_url(),
                    notification_to_plan: notification(),
                    task_service_integration_connection: TASK_SERVICE_INTEGRATION_CONNECTION.signal(),
                    task_service_integration_connections: TASK_SERVICE_INTEGRATION_CONNECTIONS.signal(),
                    ui_model: UI_MODEL.signal(),
                    on_task_planning: move |(params, task_id): (TaskPlanning, TaskId)| {
                        notification_service.send(NotificationCommand::PlanTask(
                            notification(),
                            task_id,
                            params
                        ));
                    },
                    on_task_creation: move |params| {
                        notification_service.send(NotificationCommand::CreateTaskFromNotification(
                            notification(),
                            params
                        ));
                    },
                }
            }

        if let Some(notification) = current_notification {
                TaskLinkModal {
                    api_base_url: api_base_url(),
                    notification_to_link: notification(),
                    task_service_integration_connections: TASK_SERVICE_INTEGRATION_CONNECTIONS.signal(),
                    ui_model: UI_MODEL.signal(),
                    on_task_link: move |task_id| {
                        notification_service.send(NotificationCommand::LinkNotificationWithTask(
                            notification().id,
                            task_id,
                        ));
                    },
                }
            }

        DeleteAllConfirmationModal {
            on_confirm: move |_| {
                notification_service.send(NotificationCommand::DeleteAll);
            }
        }
    }
}

#[component]
fn NotificationListItem(
    notification: ReadSignal<NotificationWithTask>,
    is_selected: ReadSignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    match notification().source_item.data {
        ThirdPartyItemData::GithubNotification(github_notification) => rsx! {
            GithubNotificationListItem {
                notification,
                github_notification: *github_notification,
                is_selected,
                on_select
            }
        },
        ThirdPartyItemData::LinearNotification(linear_notification) => rsx! {
            LinearNotificationListItem {
                notification,
                linear_notification: *linear_notification,
                is_selected,
                on_select,
            }
        },
        ThirdPartyItemData::GoogleCalendarEvent(google_calendar_event) => rsx! {
            GoogleCalendarEventListItem {
                notification,
                google_calendar_event: *google_calendar_event,
                is_selected,
                on_select,
            }
        },
        ThirdPartyItemData::GoogleDriveComment(google_drive_comment) => rsx! {
            GoogleDriveCommentListItem {
                notification,
                google_drive_comment: *google_drive_comment,
                is_selected,
                on_select,
            }
        },
        ThirdPartyItemData::GoogleMailThread(google_mail_thread) => rsx! {
            GoogleMailThreadListItem {
                notification,
                google_mail_thread: *google_mail_thread,
                is_selected,
                on_select,
            }
        },
        ThirdPartyItemData::SlackReaction(slack_reaction) => rsx! {
            SlackReactionNotificationListItem {
                notification,
                slack_reaction: *slack_reaction,
                is_selected,
                on_select,
            },
        },
        ThirdPartyItemData::SlackThread(_) => rsx! {
            SlackThreadNotificationListItem { notification, is_selected, on_select },
        },
        ThirdPartyItemData::TodoistItem(todoist_item) => rsx! {
            TodoistNotificationListItem {
                notification,
                todoist_item: *todoist_item,
                is_selected,
                on_select,
            }
        },
        ThirdPartyItemData::TickTickItem(ticktick_item) => rsx! {
            TickTickNotificationListItem {
                notification,
                ticktick_item: *ticktick_item,
                is_selected,
                on_select,
            }
        },
        ThirdPartyItemData::LinearIssue(_) => rsx! {},
        ThirdPartyItemData::WebPage(web_page) => rsx! {
            WebPageNotificationListItem {
                notification,
                web_page: *web_page,
                is_selected,
                on_select
            }
        },
    }
}

#[component]
pub fn SourceFilter(
    notification_source_kind_filters: ReadSignal<Vec<NotificationSourceKindFilter>>,
    on_select: EventHandler<NotificationSourceKindFilter>,
    on_clear: EventHandler<()>,
) -> Element {
    let mut is_open = use_signal(|| false);
    let mut wrapper_el: Signal<Option<web_sys::Element>> = use_signal(|| None);
    // Initialize popover_style with `position: fixed` (out-of-flow) anchored
    // far off-screen so the popover does NOT inflate the wrapper's bounding
    // rect on the first render after open (before `use_effect` computes the
    // real anchor coords). Without this, the popover would render in flow
    // inside the `relative inline-flex` wrapper, the wrapper's `rect.bottom`
    // would include the popover, and the `top` computed below would be wrong.
    let mut popover_style =
        use_signal(|| "position: fixed; top: -9999px; left: -9999px;".to_string());

    let close = move || {
        if *is_open.peek() {
            is_open.set(false);
        }
    };
    use_outside_close(wrapper_el, is_open, close);

    // Position the popover with `position: fixed` anchored to the trigger's
    // bounding rect. Using `position: absolute` would clip the popover behind
    // the inbox panel's `overflow: hidden` ancestors (`.list-panel`,
    // `.main-content`, `.app-layout`); fixed escapes all clipping ancestors.
    use_effect(move || {
        if is_open()
            && let Some(el) = wrapper_el()
        {
            let rect = el.get_bounding_client_rect();
            let top = rect.bottom() + 6.0;
            let left = rect.left();
            popover_style.set(format!("position: fixed; top: {top}px; left: {left}px;"));
        }
    });

    let filters = notification_source_kind_filters();
    let is_filtered = filters.iter().any(|f| !f.selected);
    let selected: Vec<NotificationSourceKindFilter> = filters
        .iter()
        .filter(|f| f.selected && is_filtered)
        .cloned()
        .collect();
    let visible: Vec<NotificationSourceKindFilter> = selected.iter().take(2).cloned().collect();
    let overflow = selected.len().saturating_sub(2);
    let button_class = if is_filtered {
        "border-ui-primary bg-ui-primary-subtle text-ui-base-content hover:text-ui-base-content hover:border-ui-primary-hover hover:bg-ui-primary-subtle"
    } else {
        "border-ui-border bg-ui-surface text-ui-base-muted hover:text-ui-base-content hover:border-ui-base-300 hover:bg-ui-surface-hover"
    };

    rsx! {
        div {
            class: "relative inline-flex",
            onmounted: move |element| {
                wrapper_el.set(Some(element.as_web_event()));
            },

            Tooltip {
                class: "flex justify-center",
                text: "Filter sources",

                button {
                    r#type: "button",
                    class: "btn btn-sm rounded-ui-sm border font-semibold {button_class}",
                    "aria-haspopup": "menu",
                    "aria-expanded": is_open(),
                    "aria-label": "Filter sources",
                    tabindex: 0,
                    onclick: move |_| is_open.set(!is_open()),

                    if is_filtered {
                        span { class: "inline-flex items-center pl-1.5",
                            for f in visible.iter() {
                                span {
                                    class: "w-[18px] h-[18px] rounded-full bg-ui-surface border-[1.5px] border-ui-surface shadow-[0_0_0_1px_var(--ui-border)] inline-flex items-center justify-center [&:not(:first-child)]:-ml-1.5",
                                    key: "{f.kind}",
                                    IntegrationProviderIcon { class: "w-3 h-3", provider_kind: f.kind.into() }
                                }
                            }
                            if overflow > 0 {
                                span { class: "source-filter-stack-more", "+{overflow}" }
                            }
                        }
                        span {
                            class: "source-filter-clear",
                            role: "button",
                            "aria-label": "Clear source filter",
                            onclick: move |evt: Event<MouseData>| {
                                evt.stop_propagation();
                                on_clear.call(());
                            },
                            span { class: "icon-[lucide--x] size-3" }
                        }
                    } else {
                        span { class: "icon-[lucide--filter] size-3" }
                        span { class: "icon-[tabler--chevron-down] size-3 opacity-60" }
                    }
                }
            }

            if is_open() {
                div {
                    class: "w-60 z-[80] bg-ui-surface border border-ui-border rounded-ui-md shadow-ui-md p-1.5 flex flex-col",
                    style: "{popover_style}",
                    role: "menu",
                    tabindex: 0,

                    div { class: "source-filter-list",
                        for f in filters.iter() {
                            button {
                                r#type: "button",
                                key: "{f.kind}",
                                class: {
                                    let base = "flex items-center gap-2 w-full px-2 py-1.5 rounded-ui-sm text-[length:var(--ui-text-base)] text-ui-base-content text-left bg-transparent border-0 cursor-pointer hover:bg-ui-surface-hover";
                                    if f.selected && is_filtered { format!("{base} bg-ui-primary-subtle") } else { base.to_string() }
                                },
                                onclick: {
                                    let f = f.clone();
                                    move |_| on_select.call(f.clone())
                                },
                                span { class: "w-5 h-5 rounded-ui-xs inline-flex items-center justify-center bg-ui-surface border border-ui-border shrink-0",
                                    IntegrationProviderIcon { class: "w-4 h-4", provider_kind: f.kind.into() }
                                }
                                span { class: "flex-1 min-w-0", "{f.kind}" }
                                if f.selected && is_filtered {
                                    span { class: "source-filter-check icon-[lucide--check] size-3" }
                                }
                            }
                        }
                    }

                    if is_filtered {
                        div { class: "border-t border-ui-border-light mt-1 pt-1 flex",
                            button {
                                r#type: "button",
                                class: "flex-1 px-2 py-1.5 rounded-ui-sm text-[length:var(--ui-text-sm)] font-medium text-ui-base-muted text-left bg-transparent border-0 cursor-pointer hover:bg-ui-surface-hover hover:text-ui-base-content",
                                onclick: move |_| {
                                    on_clear.call(());
                                    is_open.set(false);
                                },
                                "Clear ({selected.len()})"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn NotificationListOrdering(
    notification_list_order: ReadSignal<NotificationListOrder>,
    on_change: EventHandler<NotificationListOrder>,
) -> Element {
    let icon_class = use_memo(move || match notification_list_order() {
        NotificationListOrder::UpdatedAtDesc => "icon-[tabler--chevron-down] size-3",
        NotificationListOrder::UpdatedAtAsc => "icon-[tabler--chevron-up] size-3",
    });

    rsx! {
        Button {
            variant: ButtonVariant::Ghost,
            aria_label: "Sort by updated date".to_string(),
            title: "Sort by updated date".to_string(),
            onclick: move |_| {
                let new_order = match notification_list_order() {
                    NotificationListOrder::UpdatedAtAsc => NotificationListOrder::UpdatedAtDesc,
                    NotificationListOrder::UpdatedAtDesc => NotificationListOrder::UpdatedAtAsc,
                };
                on_change.call(new_order);
            },
            icon_class: "icon-[lucide--calendar] size-3".to_string(),
            enable_tooltip: true,
            span { class: "{icon_class}" }
        }
    }
}
