#![allow(non_snake_case)]

use std::collections::HashSet;

use chrono::{Local, NaiveDate};
use dioxus::prelude::*;
use log::debug;
use uuid::Uuid;

use universal_inbox::{
    PageToken,
    audit::{AuditAction, AuditActorKind, AuditChangeKind, AuditEntityKind, AuditEvent},
};

use crate::{
    components::{
        audit::{
            ActorChip, AuditEventDetails, AuditSourceIcon, ChangeSummary, actor_label, entity_route,
        },
        list::ListPaginationButtons,
        loading::Loading,
        ui::{
            PageHeader,
            select::{POP_INPUT_CLASSES, POP_INPUT_FIELD_CLASSES},
        },
    },
    config::APP_CONFIG,
    route::Route,
    services::audit_service::{AuditEventsQuery, fetch_audit_events},
    utils::markdown_to_plain_text,
};

/// A row of the timeline: one event, or the events of one bulk action. Never empty.
#[derive(Clone, PartialEq)]
struct TimelineRow(Vec<AuditEvent>);

impl TimelineRow {
    fn key(&self) -> String {
        self.0[0].id.to_string()
    }
}

/// Events of one bulk action share their transaction, actor and change.
fn is_same_bulk_action(event: &AuditEvent, other: &AuditEvent) -> bool {
    event.action == AuditAction::Updated
        && other.action == AuditAction::Updated
        && event.transaction_id == other.transaction_id
        && event.entity_kind == other.entity_kind
        && event.actor == other.actor
        && event.changes == other.changes
}

fn group_by_day(events: &[AuditEvent]) -> Vec<(NaiveDate, Vec<TimelineRow>)> {
    let mut days: Vec<(NaiveDate, Vec<TimelineRow>)> = vec![];
    for event in events {
        let day = event.created_at.with_timezone(&Local).date_naive();
        if days.last().is_none_or(|(last_day, _)| *last_day != day) {
            days.push((day, vec![]));
        }
        let rows = &mut days.last_mut().unwrap().1;
        match rows.last_mut() {
            Some(TimelineRow(bulk)) if is_same_bulk_action(&bulk[0], event) => {
                bulk.push(event.clone())
            }
            _ => rows.push(TimelineRow(vec![event.clone()])),
        }
    }
    days
}

fn day_label(day: NaiveDate) -> String {
    let today = Local::now().date_naive();
    if day == today {
        "Today".to_string()
    } else if Some(day) == today.pred_opt() {
        "Yesterday".to_string()
    } else {
        day.format("%A, %B %-d").to_string()
    }
}

fn entity_kind_label(entity_kind: AuditEntityKind, count: usize) -> String {
    let label = match entity_kind {
        AuditEntityKind::Notification => "notification",
        AuditEntityKind::Task => "task",
    };
    if count > 1 {
        format!("{count} {label}s")
    } else {
        label.to_string()
    }
}

const CARD: &str = "bg-ui-surface border border-ui-border rounded-ui-lg overflow-hidden";
const EMPTY_STATE: &str =
    "flex flex-col items-center gap-1.5 px-6 py-10 text-center text-[length:var(--ui-text-base)]";
const ROW_TITLE: &str = "min-w-0 max-w-full truncate text-[length:var(--ui-text-base)] font-medium text-ui-base-content";

const ACTOR_FILTERS: [(AuditActorKind, &str); 8] = [
    (AuditActorKind::User, "You (web)"),
    (AuditActorKind::Extension, "Browser extension"),
    (AuditActorKind::ApiKey, "API key"),
    (AuditActorKind::Mcp, "MCP client"),
    (AuditActorKind::System, "Sync and jobs"),
    (AuditActorKind::Webhook, "Webhook"),
    (AuditActorKind::Agent, "Agent"),
    (AuditActorKind::Unknown, "Unknown"),
];

const CHANGE_FILTERS: [(AuditChangeKind, &str); 6] = [
    (AuditChangeKind::Status, "Status"),
    (AuditChangeKind::Created, "Created"),
    (AuditChangeKind::Snoozed, "Snoozed"),
    (AuditChangeKind::SetAside, "Set aside"),
    (AuditChangeKind::TaskLink, "Task link"),
    (AuditChangeKind::Removed, "Removed"),
];

#[component]
pub fn AuditLogPage(entity_id: String) -> Element {
    debug!("Rendering audit log page");
    let retention_days = APP_CONFIG
        .read()
        .as_ref()
        .map(|config| config.audit_log_retention_days)
        .unwrap_or(7);

    let mut search = use_signal(String::new);
    let mut entity_kind = use_signal(|| None::<AuditEntityKind>);
    let mut actor_kind = use_signal(|| None::<AuditActorKind>);
    let mut change = use_signal(|| None::<AuditChangeKind>);
    let mut page_token = use_signal(|| None::<PageToken>);
    let mut current_page = use_signal(|| 1usize);
    let mut expanded = use_signal(HashSet::<String>::new);
    let mut entity_filter = use_signal(|| entity_id.parse::<Uuid>().ok());
    let navigator = use_navigator();

    let events = use_resource(move || async move {
        fetch_audit_events(AuditEventsQuery {
            entity_kind: entity_kind(),
            entity_id: entity_filter(),
            actor_kind: actor_kind(),
            change: change(),
            query: search(),
            page_token: page_token(),
        })
        .await
    });
    let mut reset_page = move || {
        page_token.set(None);
        current_page.set(1);
    };

    rsx! {
        div {
            class: "flex-1 overflow-y-auto bg-ui-base-200",

            div {
                class: "px-5 pt-4 pb-10 max-w-7xl mx-auto max-md:px-3 flex flex-col gap-3.5 animate-detail-fade",

                PageHeader {
                    title: "Audit log".to_string(),
                    subtitle: Some(format!(
                        "Every change to your notifications and tasks, and who made it. Events are kept for {retention_days} days."
                    )),
                }

                div { class: "flex flex-wrap items-center gap-2 px-3 py-2.5 bg-ui-surface border border-ui-border rounded-ui-lg",
                    label { class: "{POP_INPUT_CLASSES} flex-[1_1_220px] min-w-44",
                        span { class: "icon-[lucide--search] size-3.5 text-ui-base-muted shrink-0", "aria-hidden": "true" }
                        input {
                            class: POP_INPUT_FIELD_CLASSES,
                            "type": "search",
                            placeholder: "Search by title",
                            "aria-label": "Search by title",
                            value: "{search}",
                            oninput: move |evt| {
                                search.set(evt.value());
                                reset_page();
                            },
                        }
                    }

                    div { class: "inline-flex p-0.5 gap-0.5 bg-ui-surface-alt rounded-ui-md", role: "group", "aria-label": "Entity",
                        for (value, label) in [(None, "All"), (Some(AuditEntityKind::Notification), "Notifications"), (Some(AuditEntityKind::Task), "Tasks")] {
                            button {
                                class: "px-2.5 py-[3px] rounded-ui-sm text-[length:var(--ui-text-sm)] font-medium text-ui-base-muted aria-pressed:bg-ui-surface aria-pressed:text-ui-base-content aria-pressed:shadow-ui-sm",
                                "aria-pressed": "{entity_kind() == value}",
                                onclick: move |_| {
                                    entity_kind.set(value);
                                    reset_page();
                                },
                                "{label}"
                            }
                        }
                    }

                    select {
                        class: "select select-sm w-auto",
                        "aria-label": "Actor",
                        onchange: move |evt| {
                            actor_kind.set(evt.value().parse::<AuditActorKind>().ok());
                            reset_page();
                        },
                        option { value: "", selected: actor_kind().is_none(), "All actors" }
                        for (kind, label) in ACTOR_FILTERS {
                            option { value: "{kind}", selected: actor_kind() == Some(kind), "{label}" }
                        }
                    }

                    select {
                        class: "select select-sm w-auto",
                        "aria-label": "Change",
                        onchange: move |evt| {
                            change.set(evt.value().parse::<AuditChangeKind>().ok());
                            reset_page();
                        },
                        option { value: "", selected: change().is_none(), "All changes" }
                        for (kind, label) in CHANGE_FILTERS {
                            option { value: "{kind}", selected: change() == Some(kind), "{label}" }
                        }
                    }
                }

                if entity_filter().is_some() {
                    div { class: "flex items-center gap-2 text-[12px] text-ui-base-muted",
                        span { class: "icon-[lucide--filter] size-3.5" }
                        "Showing the history of one notification or task."
                        button {
                            class: "text-ui-primary font-medium",
                            onclick: move |_| {
                                entity_filter.set(None);
                                reset_page();
                                navigator.replace(Route::AuditLogPage { entity_id: String::new() });
                            },
                            "Show all events"
                        }
                    }
                }

                match &*events.read() {
                    None => rsx! { div { class: CARD, Loading { label: "Loading the audit log..." } } },
                    Some(Err(error)) => rsx! {
                        div { class: "{CARD} {EMPTY_STATE}",
                            span { class: "icon-[lucide--triangle-alert] size-6 text-ui-error" }
                            div { class: "font-semibold", "Failed to load the audit log" }
                            div { class: "text-ui-base-muted", "{error}" }
                        }
                    },
                    Some(Ok(page)) if page.content.is_empty() => rsx! {
                        div { class: "{CARD} {EMPTY_STATE}",
                            span { class: "icon-[lucide--history] size-7 text-ui-base-muted" }
                            div { class: "font-semibold", "No change recorded" }
                            div { class: "text-ui-base-muted",
                                "Changes to your notifications and tasks from the last {retention_days} days appear here, whoever makes them."
                            }
                        }
                    },
                    Some(Ok(page)) => {
                        let page = page.clone();
                        rsx! {
                            div { class: CARD,
                                for (day, rows) in group_by_day(&page.content) {
                                    div { key: "{day}",
                                        div { class: "sticky top-0 z-[1] px-3.5 pt-2.5 pb-1.5 bg-ui-surface border-b border-ui-border-light text-[10.5px] font-semibold tracking-[0.06em] uppercase text-ui-base-muted",
                                            span { "{day_label(day)}" }
                                        }
                                        for row in rows {
                                            TimelineRowView {
                                                key: "{row.key()}",
                                                row: row.clone(),
                                                is_expanded: expanded.read().contains(&row.key()),
                                                on_toggle: move |key: String| {
                                                    let mut expanded = expanded.write();
                                                    if !expanded.remove(&key) {
                                                        expanded.insert(key);
                                                    }
                                                },
                                            }
                                        }
                                    }
                                }

                                div { class: "flex justify-between items-center px-3.5 py-2 border-t border-ui-border text-[length:var(--ui-text-sm)] text-ui-base-muted",
                                    span { "{page.total} events" }
                                    ListPaginationButtons {
                                        current_page,
                                        page: page.clone(),
                                        on_select: move |token: PageToken| page_token.set(Some(token)),
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn TimelineRowView(
    row: TimelineRow,
    is_expanded: bool,
    on_toggle: EventHandler<String>,
) -> Element {
    let key = row.key();
    let TimelineRow(events) = &row;
    let event = events[0].clone();
    let count = events.len();
    let time = event
        .created_at
        .with_timezone(&Local)
        .format("%H:%M")
        .to_string();
    let actor_title = actor_label(&event.actor, event.actor_name.as_deref()).0;
    let title = markdown_to_plain_text(&event.title);
    let row_background = if is_expanded {
        "bg-[color-mix(in_srgb,var(--ui-primary)_4%,var(--ui-surface))]"
    } else {
        "hover:bg-ui-surface-hover"
    };

    rsx! {
        // One line on desktop. On mobile, a second line under the title holds the change, the
        // actor and the entity kind.
        div { class: "grid grid-cols-[40px_26px_minmax(0,1fr)_auto] max-md:grid-cols-[40px_26px_minmax(0,1fr)] gap-x-2.5 max-md:gap-x-2 items-center px-3.5 py-[7px] border-b border-ui-border-light transition-colors {row_background}",
            div { class: "text-[length:var(--ui-text-sm)] text-ui-base-muted tabular-nums", "{time}" }
            AuditSourceIcon { entity_kind: event.entity_kind, source_kind: event.source_kind.clone() }
            // A single line: the title is truncated so the change and the actor stay on the right
            div { class: "min-w-0 flex",
                if count > 1 {
                    span { class: "{ROW_TITLE} font-semibold", "{entity_kind_label(event.entity_kind, count)}" }
                } else {
                    Link {
                        class: "{ROW_TITLE} hover:text-ui-primary hover:underline underline-offset-2",
                        to: entity_route(event.entity_kind, event.entity_id),
                        "{title}"
                    }
                }
            }
            div { class: "flex items-center gap-2 shrink-0 max-md:col-start-1 max-md:col-span-3 max-md:flex-wrap max-md:gap-1.5 max-md:mt-1",
                ChangeSummary { event: event.clone() }
                ActorChip { actor: event.actor.clone(), actor_name: event.actor_name.clone() }
                span { class: "w-[86px] max-md:hidden text-right text-[length:var(--ui-text-xs)] font-semibold tracking-[0.06em] uppercase text-ui-base-muted",
                    if count > 1 { "Bulk" } else { "{event.entity_kind}" }
                }
                button {
                    class: "btn btn-text btn-circle btn-xs text-ui-base-muted max-md:ml-auto",
                    "aria-label": if is_expanded { "Hide details" } else { "Show details" },
                    "aria-expanded": "{is_expanded}",
                    title: "Details of the change by {actor_title}",
                    onclick: move |_| on_toggle.call(key.clone()),
                    span { class: if is_expanded { "icon-[lucide--chevron-down] size-4" } else { "icon-[lucide--chevron-right] size-4" } }
                }
            }
            if is_expanded {
                div { class: "col-start-3 col-span-2 max-md:col-start-1 max-md:col-span-3 mt-2 mb-1",
                    if count > 1 {
                        div { class: "flex flex-col gap-0.5",
                            for event in events.iter() {
                                Link {
                                    key: "{event.id}",
                                    class: "flex items-center gap-1.5 min-w-0 text-[length:var(--ui-text-sm)] text-ui-base-content hover:text-ui-primary",
                                    to: entity_route(event.entity_kind, event.entity_id),
                                    AuditSourceIcon { entity_kind: event.entity_kind, source_kind: event.source_kind.clone() }
                                    span { class: "truncate", "{markdown_to_plain_text(&event.title)}" }
                                }
                            }
                        }
                    } else {
                        AuditEventDetails { event: event.clone() }
                    }
                }
            }
        }
    }
}
