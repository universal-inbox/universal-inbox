//! `BrandTile` — a neutral tile with a brand-colored Iconify glyph for
//! integration provider icons.
//!
//! Built entirely from Tailwind v4 utilities and design tokens exposed via
//! `@theme inline` (`--ui-*`, `--brand-*`).
//!
//! The icon glyph itself is dispatched through the existing
//! [`IntegrationProviderIcon`] helper, which centralizes the
//! `IntegrationProviderKind` → Iconify class mapping for the whole app.
//! `BrandTile` only adds the tile chrome, the size variants, and a
//! brand-colored `currentColor` so monochrome glyphs (e.g. TickTick) pick
//! up the correct accent. Multi-color `logos:*` glyphs ignore the `color`
//! rule and keep their native palette.
//!
//! # Usage
//!
//! ```ignore
//! use universal_inbox::integration_connection::provider::IntegrationProviderKind;
//! use crate::components::ui::{BrandTile, BrandTileSize};
//!
//! rsx! {
//!     // Default size (Md, 26×26 with 16px glyph) — most card / row contexts.
//!     BrandTile { provider: IntegrationProviderKind::Github }
//!
//!     // Compact size for dense list items.
//!     BrandTile { provider: IntegrationProviderKind::Slack, size: BrandTileSize::Sm }
//!
//!     // Hero size for empty states / preview headers.
//!     BrandTile { provider: IntegrationProviderKind::Linear, size: BrandTileSize::Lg }
//! }
//! ```
//!
//! # Provider → icon / brand-color mapping
//!
//! | Provider kind         | Iconify class (via `IntegrationProviderIcon`) | Brand color (`text-*`)        |
//! |-----------------------|-----------------------------------------------|-------------------------------|
//! | `Github`              | `logos:github-icon`                           | `text-brand-github`           |
//! | `Linear`              | `logos:linear-icon`                           | `text-brand-linear`           |
//! | `Slack`               | `logos:slack-icon`                            | `text-brand-slack`            |
//! | `GoogleMail`          | `logos:google-gmail`                          | `text-brand-google`           |
//! | `GoogleCalendar`      | `logos:google-calendar`                       | `text-brand-gcal`             |
//! | `GoogleDrive`         | `logos:google-drive`                          | `text-brand-google`           |
//! | `Todoist`             | `logos:todoist-icon`                          | `text-brand-todoist`          |
//! | `TickTick`            | inline SVG (`currentColor`)                   | `text-brand-ticktick`         |
//! | `Notion`              | `logos:notion-icon`                           | `text-brand-notion`           |
//! | `API`                 | `UILogo`                                      | `text-ui-base-content` (neutral) |
//!
//! # Size variants
//!
//! | Variant            | Tile         | Glyph         |
//! |--------------------|--------------|---------------|
//! | `BrandTileSize::Sm`| 20×20 px     | 14×14 px      |
//! | `BrandTileSize::Md`| 26×26 px     | 16×16 px      |
//! | `BrandTileSize::Lg`| 40×40 px     | 24×24 px      |

#![allow(non_snake_case)]
#![allow(dead_code)]

use dioxus::prelude::*;

use universal_inbox::integration_connection::provider::IntegrationProviderKind;

use crate::components::integrations::icons::IntegrationProviderIcon;

/// Size variant for [`BrandTile`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BrandTileSize {
    /// 20×20 tile with a 14px glyph — dense lists / row leading icons.
    Sm,
    /// 26×26 tile with a 16px glyph — default card / panel header.
    #[default]
    Md,
    /// 40×40 tile with a 24px glyph — hero / preview pane header.
    Lg,
}

impl BrandTileSize {
    /// Tailwind utility classes for the tile container.
    fn tile_classes(self) -> &'static str {
        match self {
            BrandTileSize::Sm => "size-5",
            BrandTileSize::Md => "size-[26px]",
            BrandTileSize::Lg => "size-10",
        }
    }

    /// Tailwind utility classes for the inner glyph.
    fn glyph_classes(self) -> &'static str {
        match self {
            BrandTileSize::Sm => "size-[14px]",
            BrandTileSize::Md => "size-4",
            BrandTileSize::Lg => "size-6",
        }
    }
}

/// Maps a provider kind to the Tailwind `text-brand-*` class that drives the
/// glyph's `currentColor`. Multi-color `logos:*` glyphs ignore this and keep
/// their native palette — only monochrome glyphs (`TickTick`, the in-house
/// `UILogo` for `API`) actually use the resolved color.
fn provider_brand_text_class(kind: IntegrationProviderKind) -> &'static str {
    // tag: New notification integration
    match kind {
        IntegrationProviderKind::Github => "text-brand-github",
        IntegrationProviderKind::Linear => "text-brand-linear",
        IntegrationProviderKind::Slack => "text-brand-slack",
        IntegrationProviderKind::GoogleMail => "text-brand-google",
        IntegrationProviderKind::GoogleCalendar => "text-brand-gcal",
        IntegrationProviderKind::GoogleDrive => "text-brand-google",
        IntegrationProviderKind::Todoist => "text-brand-todoist",
        IntegrationProviderKind::TickTick => "text-brand-ticktick",
        IntegrationProviderKind::Notion => "text-brand-notion",
        // No brand color exists for the in-house API source; fall back to
        // the neutral foreground token so the UILogo stays legible on the
        // white tile in both light and dark themes.
        IntegrationProviderKind::API => "text-ui-base-content",
    }
}

/// A neutral white tile (1px border, 6px radius) with a brand-colored
/// Iconify glyph centered inside.
///
/// See the module-level documentation for the provider→icon/color mapping
/// and the size-variant table.
#[component]
pub fn BrandTile(
    /// Integration provider whose icon and brand color should render.
    provider: IntegrationProviderKind,
    /// Optional size variant. Defaults to [`BrandTileSize::Md`].
    #[props(default)]
    size: BrandTileSize,
) -> Element {
    let tile_size_classes = size.tile_classes();
    let brand_color_class = provider_brand_text_class(provider);
    let glyph_class = format!("{} {brand_color_class}", size.glyph_classes());

    rsx! {
        div {
            // Transparent background — multi-color `logos:*` glyphs stay legible
            // on the modal/card surface in both themes without the white-on-dark
            // contrast pop that `bg-white` introduced.
            class: "flex items-center justify-center shrink-0 bg-transparent border border-ui-border rounded-ui-sm {tile_size_classes}",
            IntegrationProviderIcon { class: glyph_class, provider_kind: provider }
        }
    }
}
