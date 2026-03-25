//! Compact 44px-wide task-app picker.
//!
//! Used by the task-planning and task-link modals to choose which task
//! manager (Todoist, TickTick, …) a task targets. The trigger is intentionally
//! tile-only — a single 20×20 `BrandTile` with the chevron — so the picker
//! sits flush next to a larger sibling field (task title or task search) in a
//! `grid-cols-[44px_1fr]` row. When opened, the popover widens to ≥180px so
//! the provider labels stay legible.
//!
//! The tight trigger geometry (44px × 34px, padding 0 6px, gap 4px) is
//! composed from Tailwind arbitrary-variant descendants targeting the stable
//! `data-ui-select-trigger`, `data-ui-select-value`, and
//! `data-ui-select-popover` attributes published by `UISelect` (see
//! `web/src/components/ui/select.rs`).

#![allow(non_snake_case)]
#![allow(dead_code)]

use dioxus::prelude::*;

use universal_inbox::integration_connection::provider::IntegrationProviderKind;

use crate::components::ui::{
    brand_tile::{BrandTile, BrandTileSize},
    select::{UISelect, UISelectOption},
    select_renderers::TaskMgrOption,
};

/// Compact 44px-wide task-app picker built on top of `UISelect`.
///
/// The closed trigger renders a 20×20 [`BrandTile`] (no label). The open
/// popover renders one `TaskMgrOption` per option (tile + label). When the
/// user picks a value, `on_change` fires with the new
/// `Option<IntegrationProviderKind>`.
#[component]
pub fn TaskAppTileSelect(
    value: Signal<Option<IntegrationProviderKind>>,
    options: Vec<UISelectOption<IntegrationProviderKind>>,
    on_change: EventHandler<Option<IntegrationProviderKind>>,
    #[props(default = "App".to_string())] placeholder: String,
    #[props(default = "task-app".to_string())] name: String,
) -> Element {
    rsx! {
        // Arbitrary-variant descendants give the `.pop-tile-slot` geometry:
        // 44px-wide / 34px-tall select trigger with tight padding and gap,
        // value cell that doesn't flex-grow, and a popover wide enough
        // (≥180px) for the provider labels to remain readable. The selectors
        // anchor on the stable data-attributes published by `UISelect`.
        //
        // Geometry: pl 4 + tile 20 + gap 4 + chev 12 + pr 4 = 44 px exactly.
        // The chevron is shrunk to `size-3` (12 px) here so it visibly sits
        // inside the button instead of bleeding past the right edge — the
        // default `size-3.5` (14 px) chevron overflows in a 44 px trigger.
        div {
            class: "[&_[data-ui-select-trigger]]:h-[34px] [&_[data-ui-select-trigger]]:w-11 [&_[data-ui-select-trigger]]:px-1 [&_[data-ui-select-trigger]]:gap-1 [&_[data-ui-select-value]]:flex-none [&_[data-ui-select-chev]]:size-3 [&_[data-ui-select-popover]]:right-auto [&_[data-ui-select-popover]]:min-w-[180px]",
            UISelect::<IntegrationProviderKind> {
                value,
                options,
                on_change,
                placeholder,
                name,
                width: "44px".to_string(),
                render_value: use_callback(
                    |opt: UISelectOption<IntegrationProviderKind>| {
                        rsx! { BrandTile { provider: opt.value, size: BrandTileSize::Sm } }
                    },
                ),
                render_option: use_callback(
                    |opt: UISelectOption<IntegrationProviderKind>| {
                        rsx! {
                            TaskMgrOption {
                                logo: rsx! { BrandTile { provider: opt.value, size: BrandTileSize::Sm } },
                                label: opt.label,
                            }
                        }
                    },
                ),
            }
        }
    }
}
