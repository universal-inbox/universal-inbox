#![allow(non_snake_case)]
//! Inline popover editor for a task's [`TaskTimeConfig`] (time-of-day, optional
//! duration, IANA timezone). Renders a compact summary chip + trigger button;
//! clicking opens a small anchored popover with the three fields (Option A from
//! the design mockups). Control only — callers supply the surrounding layout
//! (a `SettingRow` in integration settings, a plain row in the planning modal).

use chrono::NaiveTime;
use chrono_tz::TZ_VARIANTS;
use dioxus::prelude::*;
use js_sys::{Array, Object, Reflect};
use wasm_bindgen::JsValue;

use universal_inbox::integration_connection::integrations::task_time_config::TaskTimeConfig;

/// Duration quick-pick presets, in minutes.
const DURATION_PRESETS: &[(u32, &str)] = &[
    (15, "15 min"),
    (30, "30 min"),
    (45, "45 min"),
    (60, "1h"),
    (90, "1h30"),
    (120, "2h"),
];

/// Browser/runtime IANA timezone (e.g. `"Europe/Paris"`), falling back to UTC.
/// Reads `Intl.DateTimeFormat().resolvedOptions().timeZone` via typed js-sys
/// bindings (no `eval`).
fn browser_timezone() -> String {
    let dtf = js_sys::Intl::DateTimeFormat::new(&Array::new(), &Object::new());
    Reflect::get(&dtf.resolved_options(), &JsValue::from_str("timeZone"))
        .ok()
        .and_then(|v| v.as_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "UTC".to_string())
}

/// Human label for a duration in minutes ("30 min", "1h", "1h30").
fn format_duration(minutes: u32) -> String {
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h{m:02}"),
    }
}

/// One-line summary shown on the trigger ("09:00 · 30 min · Europe/Paris").
fn format_summary(cfg: &TaskTimeConfig) -> String {
    let mut parts = vec![cfg.time.format("%H:%M").to_string()];
    if let Some(duration) = cfg.duration_minutes {
        parts.push(format_duration(duration));
    }
    parts.push(cfg.timezone.clone());
    parts.join(" · ")
}

#[component]
pub fn TaskTimeConfigRow(
    value: ReadSignal<Option<TaskTimeConfig>>,
    on_change: EventHandler<Option<TaskTimeConfig>>,
    #[props(default)] disabled: bool,
) -> Element {
    let mut open = use_signal(|| false);
    // Draft fields, seeded from `value` (or defaults) each time the popover opens.
    let mut draft_time = use_signal(|| "09:00".to_string());
    let mut draft_duration = use_signal(|| None::<u32>);
    let mut draft_tz = use_signal(browser_timezone);

    let seed_and_open = move |_| {
        let current = value();
        *draft_time.write() = current
            .as_ref()
            .map(|c| c.time.format("%H:%M").to_string())
            .unwrap_or_else(|| "09:00".to_string());
        *draft_duration.write() = current.as_ref().and_then(|c| c.duration_minutes);
        *draft_tz.write() = current
            .as_ref()
            .map(|c| c.timezone.clone())
            .unwrap_or_else(browser_timezone);
        open.set(true);
    };

    let commit = move |_| {
        let Ok(time) = NaiveTime::parse_from_str(&draft_time.read(), "%H:%M") else {
            return;
        };
        on_change.call(Some(TaskTimeConfig {
            time,
            duration_minutes: draft_duration(),
            timezone: draft_tz(),
        }));
        open.set(false);
    };

    let clear = move |_| {
        on_change.call(None);
        open.set(false);
    };

    rsx! {
        div { class: "relative flex items-center gap-2",
            // Summary chip + trigger
            match value() {
                Some(cfg) => rsx! {
                    span {
                        class: "inline-flex items-center gap-1 text-[12px] text-ui-base-content \
                                whitespace-nowrap",
                        span { class: "icon-[lucide--clock] size-3.5 text-ui-primary" }
                        "{format_summary(&cfg)}"
                    }
                    button {
                        r#type: "button",
                        class: "btn btn-text btn-circle btn-xs",
                        disabled,
                        aria_label: "Edit scheduled time",
                        onclick: seed_and_open,
                        span { class: "icon-[lucide--pencil] size-3.5" }
                    }
                },
                None => rsx! {
                    span { class: "text-[12px] text-ui-base-muted", "Not set" }
                    button {
                        r#type: "button",
                        class: "btn btn-text btn-xs gap-1 text-ui-primary",
                        disabled,
                        onclick: seed_and_open,
                        span { class: "icon-[lucide--plus] size-3.5" }
                        "Set time"
                    }
                },
            }

            // Anchored popover
            if open() {
                // Transparent backdrop: a click anywhere outside the popover
                // dismisses it without committing changes.
                button {
                    r#type: "button",
                    class: "fixed inset-0 z-40 cursor-default",
                    aria_label: "Close",
                    tabindex: "-1",
                    onclick: move |_| open.set(false),
                }
                div {
                    class: "absolute right-0 top-full mt-1 z-50 w-[340px] p-3 \
                            bg-ui-surface border border-ui-border rounded-ui-md shadow-ui-md \
                            animate-ss-pop-in",
                    div { class: "grid grid-cols-3 gap-2",
                        // Time
                        label { class: "flex flex-col gap-1",
                            span {
                                class: "text-[10px] font-semibold uppercase tracking-[0.06em] \
                                        text-ui-base-muted",
                                "Time"
                            }
                            input {
                                r#type: "time",
                                class: "h-9 px-2 bg-ui-surface-alt border border-ui-border \
                                        rounded-ui-sm text-[12px] text-ui-base-content \
                                        focus:border-ui-primary outline-none",
                                value: "{draft_time}",
                                oninput: move |evt| draft_time.set(evt.value()),
                            }
                        }
                        // Duration
                        label { class: "flex flex-col gap-1",
                            span {
                                class: "text-[10px] font-semibold uppercase tracking-[0.06em] \
                                        text-ui-base-muted",
                                "Duration"
                            }
                            select {
                                class: "h-9 px-2 bg-ui-surface-alt border border-ui-border \
                                        rounded-ui-sm text-[12px] text-ui-base-content \
                                        focus:border-ui-primary outline-none",
                                value: draft_duration().map(|d| d.to_string()).unwrap_or_default(),
                                onchange: move |evt| {
                                    let v = evt.value();
                                    draft_duration.set(if v.is_empty() { None } else { v.parse().ok() });
                                },
                                option { value: "", "None" }
                                for (minutes, label) in DURATION_PRESETS.iter() {
                                    option {
                                        value: "{minutes}",
                                        selected: draft_duration() == Some(*minutes),
                                        "{label}"
                                    }
                                }
                            }
                        }
                        // Timezone
                        label { class: "flex flex-col gap-1",
                            span {
                                class: "text-[10px] font-semibold uppercase tracking-[0.06em] \
                                        text-ui-base-muted",
                                "Timezone"
                            }
                            select {
                                class: "h-9 px-2 bg-ui-surface-alt border border-ui-border \
                                        rounded-ui-sm text-[12px] text-ui-base-content \
                                        focus:border-ui-primary outline-none max-w-full",
                                value: "{draft_tz}",
                                onchange: move |evt| draft_tz.set(evt.value()),
                                for tz in TZ_VARIANTS.iter() {
                                    option {
                                        value: tz.name(),
                                        selected: draft_tz() == tz.name(),
                                        "{tz.name()}"
                                    }
                                }
                            }
                        }
                    }
                    div { class: "flex items-center justify-end gap-2 mt-3",
                        button {
                            r#type: "button",
                            class: "btn btn-text btn-xs text-ui-base-muted",
                            onclick: clear,
                            "Clear"
                        }
                        button {
                            r#type: "button",
                            class: "btn btn-primary btn-xs",
                            onclick: commit,
                            "OK"
                        }
                    }
                }
            }
        }
    }
}
