//! Boolean toggle switch — a single typed Dioxus component that emits the
//! `.toggle-switch` (settings) or `.ui-toggle` (sidebar theme) class hook
//! used by the stylesheet's `:checked + .toggle-track + .toggle-thumb` and
//! `.ui-toggle.on::after` selectors.
//!
//! ## Design system note: the CSS class hook hybrid
//!
//! [`ToggleSwitch`] emits the CSS class names
//! (`toggle-switch`, `toggle-track`, `toggle-thumb`, `ui-toggle`, `ui-toggle on`)
//! that the stylesheet binds the checked-state animations to via a precise
//! selector chain depending on markup *and* sibling order, e.g.
//!
//! ```css
//! .toggle-switch input:checked + .toggle-track            { background: var(--ui-success); }
//! .toggle-switch input:checked + .toggle-track + .toggle-thumb { transform: translateX(16px); }
//! .ui-toggle.on                                            { background: var(--ui-primary); }
//! .ui-toggle.on::after                                     { transform: translateX(14px); }
//! ```
//!
//! The `Md` variant renders
//! `<label.toggle-switch><input/><span.toggle-track/><span.toggle-thumb/></label>`
//! with the input as a direct sibling of `.toggle-track` so the
//! `input:checked + .toggle-track + .toggle-thumb` selector fires.
//! The `Sm` variant renders a single `<label.ui-toggle[.on]>` with a hidden
//! input inside, since the sidebar CSS uses an `::after` pseudo-element on
//! the outer span and the modifier class drives the visual state.
//!
//! Same pattern as [`crate::components::ui::badges::Tag`] — the class string
//! is the contract between the Rust component and the stylesheet. Keep it
//! intact.
//!
//! ## When to use which size
//!
//! - [`ToggleSize::Sm`] — sidebar theme switch (16px tall, `.ui-toggle`).
//!   Compact and used in tight chrome contexts.
//! - [`ToggleSize::Md`] — settings rows (18px tall, `.toggle-switch`).
//!   Used inside [`crate::components::settings_controls::SettingRow`].
//!
//! ## Usage
//!
//! ```ignore
//! use crate::components::ui::toggle::{ToggleSwitch, ToggleSize};
//!
//! ToggleSwitch {
//!     size: ToggleSize::Sm,
//!     checked: dark_mode_signal(),
//!     onchange: move |new| dark_mode_signal.set(new),
//! }
//! ```

#![allow(non_snake_case)]
#![allow(dead_code)]

use dioxus::prelude::*;

/// Visual size variant for [`ToggleSwitch`]. Drives both the markup shape and
/// the CSS class hook so the stylesheet bindings fire correctly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToggleSize {
    /// Compact 16px-tall toggle, used in the sidebar theme switch.
    /// Renders as `<label class="ui-toggle[ on]">` with a hidden input. The
    /// visible track + thumb come from the `.ui-toggle` and `.ui-toggle::after`
    /// CSS rules, with the `on` modifier class driving the checked state.
    Sm,
    /// Standard 18px-tall toggle, used in settings rows. Renders as
    /// `<label class="toggle-switch"><input/><span class="toggle-track"/>
    /// <span class="toggle-thumb"/></label>` so the
    /// `.toggle-switch input:checked + .toggle-track + .toggle-thumb`
    /// selector chain keeps animating the thumb on toggle.
    Md,
}

/// Accessible boolean toggle emitting the `.toggle-switch` or `.ui-toggle`
/// class hook. Wraps a real `<input type="checkbox" role="switch">` so
/// keyboard users, screen readers, and the browser's form semantics all work
/// for free — no custom ARIA juggling required.
///
/// **Hybrid pattern**: emits CSS class names so the `:checked + .toggle-track`
/// and `.ui-toggle.on::after` rules bind correctly. Do not collapse the class
/// hooks into utility-only styling — the checked transition would disappear.
/// See module docs for the rationale.
///
/// ```ignore
/// // Sidebar theme switch (compact)
/// ToggleSwitch {
///     size: ToggleSize::Sm,
///     checked: is_dark,
///     onchange: move |new| set_dark(new),
/// }
///
/// // Settings row (standard)
/// ToggleSwitch {
///     size: ToggleSize::Md,
///     checked: config().sync_enabled,
///     label: Some("Sync notifications".into()),
///     onchange: move |new| update_config(new),
/// }
/// ```
#[component]
pub fn ToggleSwitch(
    /// Which size/style variant to render. Drives the outer class and markup
    /// shape so the right CSS selector chain binds.
    size: ToggleSize,
    /// Current checked state. Drives both the input's `checked` attribute
    /// and the `.on` modifier class on `Sm`-variant toggles.
    checked: bool,
    /// Optional accessible label. Rendered as the input's `aria-label` so
    /// screen readers announce the toggle's purpose. Required for
    /// stand-alone toggles that lack an adjacent visible label
    /// (e.g. inside a `SettingRow`).
    #[props(default)]
    label: Option<String>,
    /// Disables the toggle. The hidden input gets `disabled`, so the click
    /// handler and keyboard activation are both blocked by the browser.
    #[props(default = false)]
    disabled: bool,
    /// Fired with the new checked state whenever the user toggles. Hooked
    /// to the input's `oninput` so it fires on every change (keyboard,
    /// click, programmatic).
    onchange: EventHandler<bool>,
) -> Element {
    let aria_label = label.clone();

    match size {
        ToggleSize::Sm => {
            // Sidebar theme switch — outer `<label>` carries the `.ui-toggle`
            // class (and `.on` modifier when checked) so the CSS
            // `.ui-toggle::after` thumb decoration animates via the modifier.
            // The hidden input lives inside for accessibility + form semantics.
            let class = if checked { "ui-toggle on" } else { "ui-toggle" };
            rsx! {
                label {
                    class: "{class}",
                    input {
                        r#type: "checkbox",
                        role: "switch",
                        checked,
                        disabled,
                        aria_label: aria_label,
                        aria_checked: "{checked}",
                        // Visually hidden but still focusable / clickable —
                        // a11y best practice for custom-styled toggles.
                        style: "position:absolute;opacity:0;width:100%;height:100%;margin:0;cursor:pointer;",
                        oninput: move |event| onchange.call(event.value() == "true"),
                    }
                }
            }
        }
        ToggleSize::Md => {
            // Settings toggle — the input MUST be a direct sibling of
            // `.toggle-track` (and `.toggle-track` a direct sibling of
            // `.toggle-thumb`) for the
            // `.toggle-switch input:checked + .toggle-track + .toggle-thumb`
            // selector chain to fire. Do not reorder these spans.
            rsx! {
                label {
                    class: "toggle-switch",
                    input {
                        r#type: "checkbox",
                        role: "switch",
                        checked,
                        disabled,
                        aria_label: aria_label,
                        aria_checked: "{checked}",
                        oninput: move |event| onchange.call(event.value() == "true"),
                    }
                    span { class: "toggle-track" }
                    span { class: "toggle-thumb" }
                }
            }
        }
    }
}
