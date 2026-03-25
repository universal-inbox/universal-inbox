#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::{
    HasHtmlUrl,
    notification::{NotificationId, NotificationSourceKind, NotificationWithTask},
    third_party::{
        integrations::{github::GithubNotificationItem, slack::SlackReactionItem},
        item::ThirdPartyItemData,
    },
};

use crate::{
    components::{
        integrations::{
            api::web_page::preview::WebPagePreview,
            github::preview::{
                GithubNotificationDefaultPreview, discussion::GithubDiscussionPreview,
                pull_request::GithubPullRequestPreview,
            },
            google_calendar::preview::GoogleCalendarEventPreview,
            google_drive::preview::GoogleDriveCommentPreview,
            google_mail::preview::GoogleMailThreadPreview,
            icons::{NotificationIcon, TaskIcon},
            linear::preview::LinearNotificationPreview,
            slack::preview::{
                file::SlackFilePreview, message::SlackMessagePreview, thread::SlackThreadPreview,
            },
        },
        notifications_list::NotificationListContext,
        task_preview::{TaskDetailsPreview, task_source_display_name, task_sub_type},
        ui::{ActionButton, Button, ButtonVariant},
    },
    model::{PreviewPane, UniversalInboxUIModel},
    services::notification_service::NotificationCommand,
    utils::reset_scroll_top,
};

#[component]
pub fn NotificationPreview(
    ui_model: Signal<UniversalInboxUIModel>,
    notification: ReadSignal<NotificationWithTask>,
    notifications_count: ReadSignal<usize>,
) -> Element {
    let notification_service = use_coroutine_handle::<NotificationCommand>();
    let context = use_memo(move || NotificationListContext {
        is_task_actions_enabled: ui_model.read().is_task_actions_enabled,
        notification_service,
    });
    use_context_provider(move || context);
    let has_notification_details_preview = !notification().is_built_from_task();
    let has_task_details_preview = notification().task.is_some();
    let shortcut_visibility_style = use_memo(move || {
        if ui_model.read().is_help_enabled {
            "visible"
        } else {
            "invisible"
        }
    });

    let mut latest_shown_notification_id = use_signal(|| None::<NotificationId>);
    use_effect(move || {
        // reset selected_preview_pane, preview_cards_expanded and scroll position when showing another notification
        let mut latest_shown_notification_id = latest_shown_notification_id.write();
        if *latest_shown_notification_id != Some(notification().id) {
            let mut ui_model = ui_model.write();
            ui_model.selected_preview_pane = if has_notification_details_preview {
                PreviewPane::Notification
            } else {
                PreviewPane::Task
            };
            ui_model.preview_cards_expanded = false;
            *latest_shown_notification_id = Some(notification().id);
            let _ = reset_scroll_top("notification-preview-details");
        }
    });

    let previous_button_style = if ui_model
        .read()
        .selected_notification_index
        .unwrap_or_default()
        == 0
    {
        "disabled"
    } else {
        ""
    };
    let next_button_style = if ui_model
        .read()
        .selected_notification_index
        .unwrap_or_default()
        == notifications_count() - 1
    {
        "disabled"
    } else {
        ""
    };

    let (notification_tab_style, task_tab_style) =
        if ui_model.read().selected_preview_pane == PreviewPane::Notification {
            ("active", "")
        } else {
            ("", "active")
        };

    rsx! {
        // Detail header: back button (mobile) + tabs on the left, actions on the right
        div {
            class: "detail-header",

                // Back button for mobile — first on the left, only visible
                // on mobile in detail view (md:hidden hides it on desktop;
                // on mobile the host detail panel only renders when the list
                // is hidden, so the button only appears in that state).
                Button {
                    variant: ButtonVariant::Ghost,
                    // `.detail-back-btn` keeps `display: none` in CSS so the
                    // button stays hidden everywhere except mobile detail
                    // view. The `max-md:[.app-layout.show-detail_&]:inline-flex!`
                    // variant reveals it when the parent layout enters
                    // show-detail at ≤768px. The `!` (`!important` in
                    // Tailwind v4) is required to win against the cascade-
                    // ordered `.detail-back-btn { display: none }` rule.
                    class: "detail-back-btn max-md:[.app-layout.show-detail_&]:inline-flex!".to_string(),
                    aria_label: "Back to list".to_string(),
                    title: "Back to list".to_string(),
                    onclick: move |_| ui_model.write().selected_notification_index = None,
                    icon_class: "icon-[tabler--arrow-left]".to_string(),
                }

                div {
                    class: "detail-tabs",

                    if has_notification_details_preview {
                        button {
                            class: "ui-detail-source {notification_tab_style}",
                            role: "tab",
                            "aria-label": "Show notification preview",
                            "aria-pressed": "{notification_tab_style == \"active\"}",
                            onclick: move |_| { ui_model.write().selected_preview_pane = PreviewPane::Notification },
                            span { class: "ui-detail-source-tile",
                                NotificationIcon { kind: notification().kind }
                            }
                            span { "{source_display_name(notification().kind)}" }
                            if let Some(sub) = notification_sub_type(&notification().source_item.data) {
                                span { class: "sub", "· {sub}" }
                            }
                        }
                    }
                    if has_notification_details_preview && has_task_details_preview {
                        span {
                            class: "detail-tabs-link",
                            "aria-hidden": "true",
                            title: "Linked task",
                            span { class: "icon-[lucide--link-2]" }
                        }
                    }
                    if has_task_details_preview {
                        if let Some(task) = notification().task {
                            button {
                                class: "ui-detail-source {task_tab_style}",
                                role: "tab",
                                "aria-label": "Show linked task preview",
                                "aria-pressed": "{task_tab_style == \"active\"}",
                                onclick: move |_| { ui_model.write().selected_preview_pane = PreviewPane::Task },
                                span { class: "ui-detail-source-tile",
                                    TaskIcon { class: "h-3 w-3".to_string(), kind: task.kind }
                                }
                                span { "{task_source_display_name(task.kind)}" }
                                span { class: "sub", "· {task_sub_type(&task)}" }
                            }
                        }
                    }
                }

                div {
                    class: "detail-actions",

                    if shortcut_visibility_style == "visible" {
                        if has_task_details_preview {
                            span {
                                class: "detail-kbd",
                                "tab"
                            }
                        }
                        span {
                            class: "detail-kbd",
                            "e"
                        }
                    }

                    // Open in source button — common to every preview kind
                    Button {
                        variant: ButtonVariant::Ghost,
                        href: notification().get_html_url().to_string(),
                        aria_label: format!("Open in {}", source_display_name(notification().kind)),
                        title: format!("Open in {}", source_display_name(notification().kind)),
                        icon_class: "icon-[lucide--external-link]".to_string(),
                        enable_tooltip: true,
                    }
                }
            }

            div {
                class: "detail-body",

                match ui_model.read().selected_preview_pane {
                    PreviewPane::Notification => rsx! {
                        div {
                            id: "notification-tab",
                            NotificationDetailsPreview {
                                notification,
                                expand_details: ui_model.read().preview_cards_expanded
                            }
                        }
                    },
                    PreviewPane::Task => rsx! {
                        if let Some(task) = notification().task {
                            div {
                                id: "task-tab",
                                TaskDetailsPreview {
                                    task,
                                    expand_details: ui_model.read().preview_cards_expanded
                                }
                            }
                        }
                    },
                }
            }

            // Detail dock: bottom action bar
            div {
                class: "detail-dock",

                div {
                    class: "inline-flex items-center gap-1 text-ui-base-muted",
                    Button {
                        variant: ButtonVariant::Icon,
                        disabled: previous_button_style == "disabled",
                        aria_label: "Previous notification".to_string(),
                        onclick: move |_| {
                            let mut model = ui_model.write();
                            model.selected_notification_index = Some(model.selected_notification_index.unwrap_or_default() - 1);
                        },
                        icon_class: "icon-[tabler--chevron-left]".to_string(),
                    }

                    span { class: "text-[11px] font-medium text-ui-base-muted tabular-nums", "{ui_model.read().selected_notification_index.unwrap_or_default() + 1} / {notifications_count()}" }

                    Button {
                        variant: ButtonVariant::Icon,
                        disabled: next_button_style == "disabled",
                        aria_label: "Next notification".to_string(),
                        onclick: move |_| {
                            let mut model = ui_model.write();
                            model.selected_notification_index = Some(model.selected_notification_index.unwrap_or_default() + 1);
                        },
                        icon_class: "icon-[tabler--chevron-right]".to_string(),
                    }
                }

                div {
                    class: "flex items-center gap-1.5 min-w-0",
                    for btn in get_notification_action_buttons(
                        notification,
                        shortcut_visibility_style == "visible") {
                        { btn }
                    }
                }
            }
    }
}

#[component]
fn NotificationDetailsPreview(
    notification: ReadSignal<NotificationWithTask>,
    expand_details: ReadSignal<bool>,
) -> Element {
    match notification().source_item.data {
        ThirdPartyItemData::GithubNotification(github_notification) => {
            match github_notification.item {
                Some(GithubNotificationItem::GithubPullRequest(github_pull_request)) => rsx! {
                    GithubPullRequestPreview { github_pull_request, expand_details }
                },
                Some(GithubNotificationItem::GithubDiscussion(github_discussion)) => rsx! {
                    GithubDiscussionPreview { github_discussion, expand_details }
                },
                _ => rsx! {
                    GithubNotificationDefaultPreview {
                        notification,
                        github_notification: *github_notification
                    }
                },
            }
        }
        ThirdPartyItemData::SlackReaction(slack_reaction) => match slack_reaction.item {
            SlackReactionItem::SlackMessage(slack_message) => rsx! {
                SlackMessagePreview { slack_message, title: notification().title }
            },
            SlackReactionItem::SlackFile(slack_file) => rsx! {
                SlackFilePreview { slack_file, title: notification().title }
            },
        },
        ThirdPartyItemData::SlackThread(slack_thread) => rsx! {
            SlackThreadPreview {
                slack_thread: *slack_thread,
                title: notification().title,
                expand_details
            }
        },
        ThirdPartyItemData::LinearNotification(linear_notification) => rsx! {
            LinearNotificationPreview {
                linear_notification: *linear_notification,
                expand_details
            }
        },
        ThirdPartyItemData::GoogleMailThread(google_mail_thread) => rsx! {
            GoogleMailThreadPreview {
                notification,
                google_mail_thread: *google_mail_thread,
                expand_details,
            }
        },
        ThirdPartyItemData::GoogleCalendarEvent(google_calendar_event) => rsx! {
            GoogleCalendarEventPreview {
                notification,
                google_calendar_event: *google_calendar_event,
                expand_details
            }
        },
        ThirdPartyItemData::GoogleDriveComment(google_drive_comment) => rsx! {
            GoogleDriveCommentPreview {
                notification,
                google_drive_comment: *google_drive_comment,
                expand_details
            }
        },
        ThirdPartyItemData::WebPage(web_page) => rsx! {
            WebPagePreview { notification, web_page: *web_page }
        },
        ThirdPartyItemData::LinearIssue(_)
        | ThirdPartyItemData::TodoistItem(_)
        | ThirdPartyItemData::TickTickItem(_) => rsx! {},
    }
}

fn source_display_name(kind: NotificationSourceKind) -> &'static str {
    // tag: New notification integration
    match kind {
        NotificationSourceKind::Github => "GitHub",
        NotificationSourceKind::Linear => "Linear",
        NotificationSourceKind::GoogleMail => "Gmail",
        NotificationSourceKind::GoogleCalendar => "Google Calendar",
        NotificationSourceKind::GoogleDrive => "Google Drive",
        NotificationSourceKind::Slack => "Slack",
        NotificationSourceKind::Todoist => "Todoist",
        NotificationSourceKind::TickTick => "TickTick",
        NotificationSourceKind::API => "Universal Inbox",
    }
}

fn notification_sub_type(data: &ThirdPartyItemData) -> Option<&'static str> {
    match data {
        ThirdPartyItemData::GithubNotification(n) => match &n.item {
            Some(GithubNotificationItem::GithubPullRequest(_)) => Some("Pull request"),
            Some(GithubNotificationItem::GithubDiscussion(_)) => Some("Discussion"),
            _ => Some("Notification"),
        },
        ThirdPartyItemData::LinearNotification(_) => Some("Notification"),
        ThirdPartyItemData::GoogleMailThread(_) => Some("Email"),
        ThirdPartyItemData::GoogleCalendarEvent(_) => Some("Event"),
        ThirdPartyItemData::GoogleDriveComment(_) => Some("Comment"),
        ThirdPartyItemData::SlackReaction(_) => Some("Reaction"),
        ThirdPartyItemData::SlackThread(_) => Some("Thread"),
        ThirdPartyItemData::TodoistItem(_) | ThirdPartyItemData::TickTickItem(_) => Some("Task"),
        ThirdPartyItemData::WebPage(_) => Some("Web page"),
        ThirdPartyItemData::LinearIssue(_) => None,
    }
}

pub fn get_notification_action_buttons(
    notification: ReadSignal<NotificationWithTask>,
    show_shortcut: bool,
) -> Vec<Element> {
    let context = use_context::<Memo<NotificationListContext>>();

    if !notification().is_built_from_task() {
        let mut buttons = vec![rsx! {
            ActionButton {
                title: "Delete notification",
                shortcut: "d",
                show_shortcut,
                onclick: move |_| {
                    context().notification_service
                        .send(NotificationCommand::DeleteFromNotification(notification()));
                },
                icon_class: "icon-[lucide--trash-2]",
            }
        }];

        if notification().task.is_some() {
            buttons.push(rsx! {
                ActionButton {
                    title: "Complete task",
                    shortcut: "c",
                    disabled_label: (!context().is_task_actions_enabled)
                        .then_some("No task management service connected".to_string()),
                    show_shortcut,
                    onclick: move |_| {
                        context().notification_service
                            .send(NotificationCommand::CompleteTaskFromNotification(notification()));
                    },
                    icon_class: "icon-[lucide--check-circle]"
                }
            });
        }

        buttons.push(rsx! {
            ActionButton {
                title: "Unsubscribe from the notification",
                shortcut: "u",
                show_shortcut,
                onclick: move |_| {
                    context().notification_service.send(NotificationCommand::Unsubscribe(notification().id));
                },
                icon_class: "icon-[lucide--bell-off]"
            }
        });

        buttons.push(rsx! {
            ActionButton {
                title: "Snooze notification",
                shortcut: "s",
                show_shortcut,
                onclick: move |_| {
                    context().notification_service.send(NotificationCommand::Snooze(notification().id));
                },
                icon_class: "icon-[lucide--clock]"
            }
        });

        if notification().task.is_none() {
            buttons.push(rsx! {
                ActionButton {
                    title: "Create task",
                    shortcut: "p",
                    disabled_label: (!context().is_task_actions_enabled)
                        .then_some("No task management service connected".to_string()),
                    show_shortcut,
                    data_overlay: "#task-planning-modal",
                    icon_class: "icon-[lucide--list-plus]"
                }
            });

            buttons.push(rsx! {
                ActionButton {
                    title: "Create task with defaults",
                    shortcut: "t",
                    disabled_label: (!context().is_task_actions_enabled)
                        .then_some("No task management service connected".to_string()),
                    show_shortcut,
                    onclick: move |_| {
                        context().notification_service.send(NotificationCommand::CreateTaskWithDetaultsFromNotification(notification()));
                    },
                    icon_class: "icon-[lucide--zap]"
                }
            });

            buttons.push(rsx! {
                ActionButton {
                    title: "Link to task",
                    shortcut: "l",
                    disabled_label: (!context().is_task_actions_enabled)
                        .then_some("No task management service connected".to_string()),
                    show_shortcut,
                    data_overlay: "#task-linking-modal",
                    icon_class: "icon-[lucide--link]"
                }
            });
        }

        buttons
    } else {
        vec![
            rsx! {
                ActionButton {
                    title: "Delete task",
                    shortcut: "d",
                    disabled_label: (!context().is_task_actions_enabled)
                        .then_some("No task management service connected".to_string()),
                    show_shortcut,
                    onclick: move |_| {
                        context().notification_service
                            .send(NotificationCommand::DeleteFromNotification(notification()));
                    },
                    icon_class: "icon-[lucide--trash-2]"
                }
            },
            rsx! {
                ActionButton {
                    title: "Complete task",
                    shortcut: "c",
                    disabled_label: (!context().is_task_actions_enabled)
                        .then_some("No task management service connected".to_string()),
                    show_shortcut,
                    onclick: move |_| {
                        context().notification_service
                            .send(NotificationCommand::CompleteTaskFromNotification(notification()));
                    },
                    icon_class: "icon-[lucide--check-circle]"
                }
            },
            rsx! {
                ActionButton {
                    title: "Snooze notification",
                    shortcut: "s",
                    show_shortcut,
                    onclick: move |_| {
                        context().notification_service.send(NotificationCommand::Snooze(notification().id));
                    },
                    icon_class: "icon-[lucide--clock]"
                }
            },
            rsx! {
                ActionButton {
                    title: "Plan task",
                    shortcut: "p",
                    disabled_label: (!context().is_task_actions_enabled)
                        .then_some("No task management service connected".to_string()),
                    show_shortcut,
                    data_overlay: "#task-planning-modal",
                    icon_class: "icon-[lucide--calendar-check]"
                }
            },
            rsx! {
                ActionButton {
                    title: "Create task with defaults",
                    shortcut: "t",
                    disabled_label: (!context().is_task_actions_enabled)
                        .then_some("No task management service connected".to_string()),
                    show_shortcut,
                    onclick: move |_| {
                        context().notification_service.send(NotificationCommand::CreateTaskWithDetaultsFromNotification(notification()));
                    },
                    icon_class: "icon-[lucide--zap]"
                }
            },
        ]
    }
}
