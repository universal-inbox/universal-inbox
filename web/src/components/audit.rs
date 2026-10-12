#![allow(non_snake_case)]

//! Building blocks of the audit log page and of the History drawer.

use chrono::{DateTime, Local, NaiveDateTime, Utc};
use dioxus::prelude::*;
use serde_json::Value;
use uuid::Uuid;

use universal_inbox::{
    audit::{AuditAction, AuditActor, AuditChange, AuditEntityKind, AuditEvent},
    integration_connection::provider::IntegrationProviderKind,
    notification::NotificationId,
    task::{DueDate, TaskId},
};

use crate::{
    components::{integrations::icons::IntegrationProviderIcon, loading::Loading},
    route::Route,
    services::audit_service::{AuditEventsQuery, fetch_audit_events},
    utils::markdown_to_plain_text,
};

// Shared class strings, Tailwind utilities on the design tokens (see the ui-design-system skill).
pub const PILL: &str = "inline-flex items-center gap-1 px-[7px] py-px rounded-ui-pill text-[length:var(--ui-text-xs)] font-semibold whitespace-nowrap";
const PILL_NEW: &str =
    "bg-transparent text-ui-base-muted border border-dashed border-ui-base-300 font-medium";
const PILL_PRIMARY: &str = "bg-ui-primary-subtle text-ui-primary";
const PILL_NEUTRAL: &str = "bg-ui-surface-alt text-ui-secondary";
const PILL_MUTED: &str = "bg-ui-surface-alt text-ui-base-muted";
const PILL_ERROR: &str = "bg-ui-error-subtle text-[#b4495f] dark:text-ui-error";
const PILL_WARNING: &str = "bg-ui-warning-subtle text-[#9a7420] dark:text-ui-warning";
const PILL_INFO: &str = "bg-ui-info-subtle text-[#3a92b6] dark:text-ui-info";
pub const PILL_SUCCESS: &str = "bg-ui-success-subtle text-[#3e9271] dark:text-ui-success";
const ACTOR_CHIP: &str = "inline-flex items-center gap-[5px] py-px pr-2 pl-[3px] rounded-ui-pill border border-ui-border bg-ui-surface text-[length:var(--ui-text-sm)] font-medium text-ui-base-content whitespace-nowrap";
const ACTOR_AVATAR: &str = "size-4 rounded-full inline-flex items-center justify-center";
/// Key/value grid of the event details: `div`s alternating a `KV_KEY` and a `KV_VALUE`
const KV_GRID: &str = "grid grid-cols-[110px_minmax(0,1fr)] max-md:grid-cols-[88px_minmax(0,1fr)] px-3 py-1.5 bg-ui-base-200 border border-ui-border-light rounded-ui-md text-[length:var(--ui-text-sm)] [&>div]:py-1 [&>div]:border-b [&>div]:border-ui-border-light [&>div]:[overflow-wrap:anywhere] [&>div:nth-last-child(-n+2)]:border-b-0";
const KV_KEY: &str =
    "pt-[5px] text-[10px] font-semibold tracking-[0.06em] uppercase text-ui-base-muted";
const KV_VALUE: &str = "font-medium text-ui-base-content";
const KV_CODE: &str = "font-mono text-[10.5px] font-normal text-ui-base-muted";
pub const SOURCE_TILE: &str = "size-[26px] rounded-ui-sm flex items-center justify-center shrink-0";

/// Label and secondary text of an actor, eg. ("MCP", "Claude Desktop").
pub fn actor_label(actor: &AuditActor, actor_name: Option<&str>) -> (String, Option<String>) {
    match actor {
        AuditActor::User { .. } => ("You".to_string(), Some("web".to_string())),
        AuditActor::ApiKey => ("API key".to_string(), None),
        AuditActor::Extension { version } => {
            ("Browser extension".to_string(), Some(format!("v{version}")))
        }
        AuditActor::Mcp { client_id } => (
            "MCP".to_string(),
            Some(actor_name.unwrap_or(client_id.as_str()).to_string()),
        ),
        AuditActor::System { job, source, .. } => {
            let label = match job.as_str() {
                "SyncNotifications" | "SyncTasks" => "Sync".to_string(),
                "ProcessNotificationSideEffects" => "Side effects".to_string(),
                job => job.to_string(),
            };
            (
                label,
                source.map(|source| provider_name(source).to_string()),
            )
        }
        AuditActor::Webhook { provider } => (
            "Webhook".to_string(),
            Some(format!("{} events", provider_name(*provider))),
        ),
        AuditActor::Agent { name, .. } => ("Agent".to_string(), Some(name.clone())),
        AuditActor::Cli { command } => ("CLI".to_string(), Some(command.clone())),
        AuditActor::Unknown => ("Unknown actor".to_string(), None),
    }
}

/// Avatar colors and icon of an actor chip
fn actor_style(actor: &AuditActor) -> (&'static str, &'static str) {
    const SYSTEM: &str = "bg-ui-surface-alt text-ui-base-muted";
    match actor {
        AuditActor::User { .. } => ("bg-ui-primary text-white", "icon-[lucide--user]"),
        AuditActor::ApiKey => (
            "bg-ui-secondary-subtle text-ui-secondary",
            "icon-[lucide--key-round]",
        ),
        AuditActor::Extension { .. } => (
            "bg-ui-purple-subtle text-ui-purple",
            "icon-[lucide--puzzle]",
        ),
        AuditActor::Mcp { .. } => (
            "bg-ui-info-subtle text-[#3a92b6] dark:text-ui-info",
            "icon-[lucide--plug-zap]",
        ),
        AuditActor::System { .. } => (SYSTEM, "icon-[lucide--refresh-cw]"),
        AuditActor::Webhook { .. } => (SYSTEM, "icon-[lucide--webhook]"),
        AuditActor::Agent { .. } => (
            "bg-ui-accent-subtle text-ui-accent-content dark:text-ui-accent",
            "icon-[lucide--sparkles]",
        ),
        AuditActor::Cli { .. } => (SYSTEM, "icon-[lucide--terminal]"),
        AuditActor::Unknown => (
            "bg-ui-error-subtle text-[#b4495f] dark:text-ui-error",
            "icon-[lucide--circle-help]",
        ),
    }
}

pub fn provider_name(provider: IntegrationProviderKind) -> &'static str {
    match provider {
        IntegrationProviderKind::Github => "GitHub",
        IntegrationProviderKind::Linear => "Linear",
        IntegrationProviderKind::GoogleCalendar => "Google Calendar",
        IntegrationProviderKind::GoogleDrive => "Google Drive",
        IntegrationProviderKind::GoogleMail => "Gmail",
        IntegrationProviderKind::Notion => "Notion",
        IntegrationProviderKind::Slack => "Slack",
        IntegrationProviderKind::Todoist => "Todoist",
        IntegrationProviderKind::TickTick => "TickTick",
        IntegrationProviderKind::API => "API",
    }
}

#[component]
pub fn ActorChip(actor: AuditActor, actor_name: Option<String>) -> Element {
    let (label, meta) = actor_label(&actor, actor_name.as_deref());
    let (avatar_class, icon_class) = actor_style(&actor);
    let (title, unknown_class) = if actor == AuditActor::Unknown {
        (
            "No actor was recorded for this change: it is a bug to report",
            "border-dashed border-ui-error",
        )
    } else {
        ("", "")
    };

    rsx! {
        span { class: "{ACTOR_CHIP} {unknown_class}", title,
            span { class: "{ACTOR_AVATAR} {avatar_class}", span { class: "{icon_class} size-2.5" } }
            "{label}"
            if let Some(meta) = meta {
                span { class: "text-ui-base-muted font-normal", "· {meta}" }
            }
        }
    }
}

/// Source tile, drawn like the list rows: a framed icon for the GitHub, Linear, Slack and
/// Google Calendar notifications, a bare one otherwise.
#[component]
pub fn AuditSourceIcon(entity_kind: AuditEntityKind, source_kind: Option<String>) -> Element {
    let Some(provider) = source_kind
        .as_deref()
        .and_then(|source_kind| source_kind.parse::<IntegrationProviderKind>().ok())
    else {
        return rsx! { span { class: SOURCE_TILE } };
    };
    let is_framed = entity_kind == AuditEntityKind::Notification
        && matches!(
            provider,
            IntegrationProviderKind::Github
                | IntegrationProviderKind::Linear
                | IntegrationProviderKind::Slack
                | IntegrationProviderKind::GoogleCalendar
        );

    rsx! {
        span { class: SOURCE_TILE, title: provider_name(provider),
            if is_framed {
                span {
                    class: "w-full h-full flex items-center justify-center rounded-[inherit] bg-ui-surface border border-ui-border",
                    IntegrationProviderIcon { class: "size-4", provider_kind: provider }
                }
            } else {
                IntegrationProviderIcon { class: "size-5", provider_kind: provider }
            }
        }
    }
}

fn status_pill_class(status: &str) -> &'static str {
    match status {
        "Unread" | "Active" => PILL_PRIMARY,
        "Deleted" => PILL_ERROR,
        "Unsubscribed" => PILL_WARNING,
        "Done" => PILL_SUCCESS,
        _ => PILL_NEUTRAL,
    }
}

#[component]
fn StatusPill(status: Option<String>) -> Element {
    match status {
        Some(status) => rsx! {
            span { class: "{PILL} {status_pill_class(&status)}", "{status}" }
        },
        None => rsx! { span { class: "{PILL} {PILL_NEW}", "New" } },
    }
}

fn json_string(value: &Option<Value>) -> Option<String> {
    value
        .as_ref()
        .and_then(|value| value.as_str())
        .map(str::to_string)
}

/// Format a stored timestamp, a naive UTC (`snoozed_until`) or RFC 3339 one (`set_aside_at`).
fn format_timestamp(value: &Option<Value>) -> Option<String> {
    let text = value.as_ref()?.as_str()?;
    let datetime = DateTime::parse_from_rfc3339(text)
        .map(|datetime| datetime.with_timezone(&Utc))
        .or_else(|_| {
            NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%.f").map(|naive| naive.and_utc())
        })
        .ok()?;
    Some(
        datetime
            .with_timezone(&Local)
            .format("%a %b %-d, %H:%M")
            .to_string(),
    )
}

/// The changes of an event as pills: `Unread → Deleted`, `Snoozed until …`, `Created`.
#[component]
pub fn ChangeSummary(event: AuditEvent) -> Element {
    if event.action == AuditAction::Created && event.entity_kind == AuditEntityKind::Task {
        return rsx! {
            span { class: "{PILL} {PILL_SUCCESS}",
                span { class: "icon-[lucide--plus] size-3" }
                "Created"
            }
        };
    }
    if event.action == AuditAction::Removed {
        return rsx! { span { class: "{PILL} {PILL_ERROR}", "Removed" } };
    }

    let status = event.changes.get("status").cloned();
    let snoozed_until = event.changes.get("snoozed_until").cloned();
    let set_aside_at = event.changes.get("set_aside_at").cloned();
    let task_link = event.changes.get("task_id").cloned();

    rsx! {
        span { class: "inline-flex items-center gap-1 flex-nowrap",
            if let Some(AuditChange { from, to }) = status {
                span { class: "inline-flex items-center gap-1",
                    StatusPill { status: json_string(&from) }
                    span { class: "icon-[lucide--arrow-right] size-3 text-ui-base-muted" }
                    StatusPill { status: json_string(&to) }
                }
            }
            if let Some(AuditChange { to, .. }) = snoozed_until {
                match format_timestamp(&to) {
                    Some(until) => rsx! {
                        span { class: "{PILL} {PILL_INFO}",
                            span { class: "icon-[lucide--clock] size-3" }
                            "Snoozed until {until}"
                        }
                    },
                    None => rsx! { span { class: "{PILL} {PILL_NEUTRAL}", "Unsnoozed" } },
                }
            }
            if let Some(AuditChange { to, .. }) = set_aside_at {
                if to.as_ref().is_some_and(|to| !to.is_null()) {
                    span { class: "{PILL} {PILL_MUTED}", "Set aside" }
                } else {
                    span { class: "{PILL} {PILL_NEUTRAL}", "Restored" }
                }
            }
            if let Some(AuditChange { to, .. }) = task_link {
                if to.as_ref().is_some_and(|to| !to.is_null()) {
                    span { class: "{PILL} {PILL_SUCCESS}",
                        span { class: "icon-[lucide--link-2] size-3" }
                        "Linked to a task"
                    }
                } else {
                    span { class: "{PILL} {PILL_NEUTRAL}", "Unlinked from its task" }
                }
            }
        }
    }
}

pub fn entity_route(entity_kind: AuditEntityKind, entity_id: Uuid) -> Route {
    match entity_kind {
        AuditEntityKind::Notification => Route::NotificationPage {
            notification_id: NotificationId(entity_id),
        },
        AuditEntityKind::Task => Route::SyncedTaskPage {
            task_id: TaskId(entity_id),
        },
    }
}

fn param_text(params: &Value, key: &str) -> Option<String> {
    match params.get(key)? {
        Value::Null => None,
        Value::String(text) if text.is_empty() => None,
        Value::String(text) => Some(text.clone()),
        value => Some(value.to_string()),
    }
}

/// Details shown when a row is expanded: task creation parameters, why an agent acted, who
/// triggered a job.
#[component]
pub fn AuditEventDetails(event: AuditEvent) -> Element {
    let params = event.params.clone().unwrap_or(Value::Null);
    let due = params
        .get("due_at")
        .cloned()
        .and_then(|due_at| serde_json::from_value::<DueDate>(due_at).ok())
        .map(|due_at| due_at.display_date());
    let priority = params
        .get("priority")
        .and_then(Value::as_u64)
        .map(|priority| format!("P{priority}"));
    let body_present = params.get("body_present").and_then(Value::as_bool);
    let triggered_by = match &event.actor {
        AuditActor::System {
            triggered_by: Some(triggered_by),
            ..
        } => Some(actor_label(triggered_by, None)),
        _ => None,
    };
    let reason = match &event.actor {
        AuditActor::Agent {
            reason: Some(reason),
            ..
        } => Some(
            reason
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| reason.to_string()),
        ),
        _ => None,
    };

    rsx! {
        if let Some(reason) = reason {
            div { class: "flex gap-2 mb-2 px-3 py-2 rounded-ui-md bg-ui-accent-subtle text-ui-base-content text-[length:var(--ui-text-base)]",
                span { class: "icon-[lucide--message-square-quote] size-3.5 shrink-0 mt-0.5" }
                span { "{reason}" }
            }
        }
        div { class: KV_GRID,
            if event.params.is_some() {
                if let Some(kind) = param_text(&params, "kind") {
                    div { class: KV_KEY, "Task manager" }
                    div { class: KV_VALUE, "{kind}" }
                }
                if let Some(project) = param_text(&params, "project") {
                    div { class: KV_KEY, "Project" }
                    div { class: KV_VALUE, "{project}" }
                }
                if let Some(title) = param_text(&params, "title").map(|title| markdown_to_plain_text(&title)) {
                    div { class: KV_KEY, "Title" }
                    div { class: KV_VALUE, "{title}" }
                }
                if let Some(due) = due {
                    div { class: KV_KEY, "Due" }
                    div { class: KV_VALUE, "{due}" }
                }
                if let Some(priority) = priority {
                    div { class: KV_KEY, "Priority" }
                    div { class: KV_VALUE, "{priority}" }
                }
                if let Some(body_present) = body_present {
                    div { class: KV_KEY, "Body" }
                    div { class: KV_VALUE,
                        if body_present {
                            span { class: "inline-flex items-center gap-1 font-normal text-ui-base-muted",
                                span { class: "icon-[lucide--lock] size-3" }
                                "Set, encrypted (not copied into the audit log)"
                            }
                        } else {
                            "Empty"
                        }
                    }
                }
            }
            if let Some((label, meta)) = triggered_by {
                div { class: KV_KEY, "Triggered by" }
                div { class: KV_VALUE,
                    "{label}"
                    if let Some(meta) = meta { " · {meta}" }
                }
            }
            if let AuditActor::Mcp { client_id } = &event.actor {
                div { class: KV_KEY, "MCP client" }
                div { class: KV_VALUE, code { class: KV_CODE, "{client_id}" } }
            }
            div { class: KV_KEY, "{event.entity_kind} id" }
            div { class: KV_VALUE, code { class: KV_CODE, "{event.entity_id}" } }
        }
    }
}

fn timeline_dot_class(event: &AuditEvent) -> &'static str {
    if matches!(event.actor, AuditActor::Agent { .. }) {
        return "border-ui-accent";
    }
    if event.action == AuditAction::Created {
        return "border-ui-success";
    }
    let to_status = event
        .changes
        .get("status")
        .and_then(|change| json_string(&change.to));
    match to_status.as_deref() {
        Some("Deleted") | Some("Unsubscribed") => "border-ui-error",
        Some("Done") => "border-ui-success",
        _ if event.changes.contains_key("snoozed_until") => "border-ui-info",
        _ => "border-ui-primary",
    }
}

pub fn format_event_time(created_at: DateTime<Utc>) -> String {
    let local = created_at.with_timezone(&Local);
    let today = Local::now().date_naive();
    if local.date_naive() == today {
        format!("Today, {}", local.format("%H:%M"))
    } else if Some(local.date_naive()) == today.pred_opt() {
        format!("Yesterday, {}", local.format("%H:%M"))
    } else {
        local.format("%b %-d, %H:%M").to_string()
    }
}

/// History of one notification or task, opened next to its detail with `i`.
#[component]
pub fn HistoryDrawer(
    entity_kind: AuditEntityKind,
    entity_id: Uuid,
    retention_days: u32,
    on_close: EventHandler<()>,
) -> Element {
    let mut entity = use_signal(|| (entity_kind, entity_id));
    if *entity.peek() != (entity_kind, entity_id) {
        entity.set((entity_kind, entity_id));
    }
    let events = use_resource(move || async move {
        let (entity_kind, entity_id) = entity();
        fetch_audit_events(AuditEventsQuery {
            entity_kind: Some(entity_kind),
            entity_id: Some(entity_id),
            ..Default::default()
        })
        .await
    });
    let entity_label = match entity_kind {
        AuditEntityKind::Notification => "notification",
        AuditEntityKind::Task => "task",
    };

    rsx! {
        aside {
            id: "history-drawer",
            // Full-width overlay on mobile, a 320px column next to the detail otherwise
            class: "w-80 shrink-0 min-w-0 flex flex-col bg-ui-surface border-l border-ui-border animate-detail-fade max-md:fixed max-md:inset-0 max-md:z-40 max-md:w-full max-md:border-l-0",
            "aria-label": "History",

            div { class: "flex items-center gap-2 px-3.5 py-2.5 border-b border-ui-border",
                h3 { class: "flex-1 flex items-center gap-1.5 text-[length:var(--ui-text-md)] font-bold tracking-tight",
                    span { class: "icon-[lucide--history] size-4 text-ui-base-muted" }
                    "History"
                }
                button {
                    class: "btn btn-text btn-circle btn-xs text-ui-base-muted",
                    "aria-label": "Close history",
                    title: "Close (i)",
                    onclick: move |_| on_close.call(()),
                    span { class: "icon-[lucide--x] size-4" }
                }
            }

            div { class: "flex-1 overflow-y-auto px-3.5 pt-3 pb-5",
                div { class: "mb-3 text-[length:var(--ui-text-sm)] text-ui-base-muted",
                    "Changes to this {entity_label} in the last {retention_days} days, newest first."
                }
                match &*events.read() {
                    None => rsx! { Loading { label: "Loading history..." } },
                    Some(Err(error)) => rsx! {
                        div { class: "text-[length:var(--ui-text-sm)] text-ui-error", "Failed to load the history: {error}" }
                    },
                    Some(Ok(page)) if page.content.is_empty() => rsx! {
                        div { class: "text-[length:var(--ui-text-sm)] text-ui-base-muted", "No change recorded." }
                    },
                    Some(Ok(page)) => rsx! {
                        // The vertical line linking the dots of the timeline
                        div { class: "relative pl-5 before:content-[''] before:absolute before:left-[5px] before:top-1.5 before:bottom-1.5 before:w-px before:bg-ui-border",
                            for event in page.content.iter().cloned() {
                                div { key: "{event.id}", class: "relative pb-3.5 last:pb-0",
                                    span { class: "absolute -left-5 top-[3px] size-[11px] rounded-full bg-ui-surface border-2 {timeline_dot_class(&event)}" }
                                    div { class: "text-[length:var(--ui-text-sm)] text-ui-base-muted tabular-nums", "{format_event_time(event.created_at)}" }
                                    // A task history includes the notification it was created from
                                    if event.entity_kind != entity_kind {
                                        Link {
                                            class: "inline-flex items-center gap-1 mt-0.5 text-[length:var(--ui-text-sm)] text-ui-primary",
                                            to: entity_route(event.entity_kind, event.entity_id),
                                            span { class: "icon-[lucide--arrow-up-right] size-3" }
                                            "From {event.entity_kind.to_string().to_lowercase()} \"{markdown_to_plain_text(&event.title)}\""
                                        }
                                    }
                                    div { class: "mt-[3px]",
                                        ChangeSummary { event: event.clone() }
                                    }
                                    div { class: "mt-1",
                                        ActorChip { actor: event.actor.clone(), actor_name: event.actor_name.clone() }
                                    }
                                    if event.params.is_some() || matches!(event.actor, AuditActor::Agent { reason: Some(_), .. }) {
                                        div { class: "mt-1.5",
                                            AuditEventDetails { event: event.clone() }
                                        }
                                    }
                                }
                            }
                        }
                    },
                }
            }

            div { class: "flex justify-between px-3.5 py-2 border-t border-ui-border text-[length:var(--ui-text-sm)] text-ui-base-muted [&_a]:text-ui-primary",
                span {
                    if let Some(Ok(page)) = &*events.read() {
                        "{page.total} events"
                    }
                }
                Link {
                    to: Route::AuditLogPage { entity_id: entity_id.to_string() },
                    "Open in audit log"
                }
            }
        }
    }
}
