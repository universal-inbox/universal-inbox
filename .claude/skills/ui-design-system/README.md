# Universal Inbox Design System

> **Universal Inbox: The single source of truth for all your notifications. One unified inbox.**

Universal Inbox pulls notifications from developer tools (GitHub, Linear, Slack, Google Mail, Google Calendar, Google Drive, Todoist, Notion) into a single keyboard-driven triage app. The product is a three-pane desktop-first web app: dark sidebar on the left, notification list in the middle, detail preview on the right. The product is built for focus, speed, and high information density — it is a power-user tool, not a mainstream consumer app.

## Sources

This design system was built from the following provided materials:
- `uploads/design-proposal-notifications-page.html` — a ~4,200-line design proposal / working prototype of the Notifications page, containing the full token set, component CSS, interaction patterns, and seed data. This is the primary source of truth.
- `uploads/ui-logo-transparent.png` — the Universal Inbox logo (a blue gradient "U" with an inset "i" / vertical bar).
- `uploads/DMSans-Regular.woff2` + latin-ext — DM Sans webfont (the product's UI typeface). Both weights + range variants copied to `fonts/`.

There is **no codebase or Figma file** attached. All patterns here are derived from the HTML proposal.

---

## Product surfaces

From the sources, Universal Inbox has one primary product:

1. **Universal Inbox web app** — three-pane desktop app with
   - Sidebar (dark, always): branding + nav (Inbox · Tasks · Manage) + user profile footer
   - List panel: filter bar, search, notification items grouped by time
   - Detail panel: full preview + action dock
   - Settings page: integration cards with expandable accounts and config toggles

Secondary surfaces referenced in copy: a marketing/landing site (not in the sources, out of scope for this kit).

---

## Content Fundamentals

**Voice.** Crisp, professional, utility-first. Never chirpy, never marketing-y. Short sentences that describe what the thing does. No exclamation marks in UI chrome — the only exclamation allowed is in the "You're all caught up" inbox-zero reward.

**Person.** Second person ("you") in help text and empty states. Never "we". Never addresses the user by name in UI.

**Casing.**
- **Sentence case** for buttons, nav items, titles, section headers, menu items: "Delete all notifications", "Open in browser", "Stop notifications", "Create task". Never Title Case.
- **UPPERCASE with wide tracking** for overline labels / section dividers only: "INBOX", "TASKS", "MANAGE", "TODAY", "YESTERDAY".
- **lowercase** for keyboard hints and single-letter keycap labels: `d`, `s`, `p`.

**Precision over fluff.** Copy tells the user the mechanism. Examples from the source:
- "Last synced 3 min ago" — not "recently updated"
- "Auth token expired — Reconnect" — not "Something went wrong"
- "Syncing notifications..." — explicit state
- "This will permanently delete all notifications matching the current filters. You won't be able to undo this." — describes consequence

**Keyboard is first-class.** Copy references shortcut keys inline: "Select a notification and press `s` to snooze it." Every action in help overlays lists the key.

**No emoji in UI chrome.** The only place emoji appears in the proposal is inside *user-generated content* (a Slack message containing `📥 Universal Inbox new release 📥`). Emoji is never used as a UI icon, empty-state illustration, or decoration.

**Tone examples (verbatim from source):**
- Inbox zero: "You're all caught up. Nothing needs your attention right now. New notifications will appear here as they arrive."
- Empty snoozed: "Not ready to deal with something? Select a notification and press s to snooze it. It will reappear in your inbox at the time you choose."
- Settings warning: "1 integration needs attention — Slack connection has expired"
- Task hint: "Task synced from notification. Complete to mark both as done."

---

## Visual Foundations

### Color
- **Single primary: `#388FEF` azure blue.** Used for active states, primary actions, focus rings (15% alpha), unread tint, and the logo gradient. There is no secondary brand color.
- **Slate neutrals** run the surfaces: `#FEFEFE` → `#F7F9FC` → `#E1E6EE` (base 100/200/300), `#0F172A` text, `#64748B` muted text. A cool, graphite palette — no warm grays.
- **Soft, desaturated semantics.** Success `#6BC5A0`, warning `#E8C46A`, error `#E8899A`, info `#6BB8D9`, purple `#8B5CF6`. Each has a `-subtle` paired tint (`#EDF9F3` etc.) used as background for tags/alerts. Colors are noticeably **muted** — they sit quietly next to UI, they don't shout.
- **Integration brand colors** are reserved for the source badge on a notification: `#1B1F23` GitHub, `#5E6AD2` Linear, `#4A154B` Slack, `#EA4335` Google, `#4285F4` GCal, `#E44332` Todoist, `#191919` Notion. Each is used only to color a 26×26 icon tile — never applied to larger surfaces.
- **The sidebar is always dark** (`#0C1525`) regardless of light/dark mode on the rest of the app. This is an intentional asymmetric pattern — the sidebar is a "frame" around the content.

### Type
- **DM Sans** for all UI text. The product uses weights 300/400/500/600/700, but 400/500/600/700 cover 99% of use.
- **Dense scale.** Body text is `12.5px`. Titles are `15px`. Panel headers are `13.5px`. Caption/time stamps `11px`. Section overlines `10.5–11px` UPPERCASE with `0.06–0.08em` tracking. **This is a dense, power-user UI** — the scale is deliberately small.
- **Tight letter-spacing on titles** (`-0.02em`) for a crisp, modern feel.
- **Monospace for keycaps and code**: SF Mono → Fira Code → JetBrains Mono fallback chain. Keys render in a small pill (`.kbd`).

### Spacing & layout
- **4px base grid.** Core spacings: 4 / 6 / 8 / 10 / 12 / 14 / 16 / 20.
- **Three panels, fixed widths.** Sidebar 224px (collapses to 56px at `≤1024px`), list panel 380px (340 at `≤1280px`, 320 at `≤1024px`), detail fills remaining. Stacks at `≤768px`.
- **Content padding is 14px** inside list/header/row regions — not 16px. This is deliberate, part of the density.

### Radii
- `3px` xs — for tiny chips, keycap backgrounds
- `6px` sm — default on buttons, inputs, small surfaces
- `8px` md — cards, menu items, toggles
- `12px` lg — integration cards, modals
- `9999px` pill — for filter chips, nav badges, status leaves

### Shadows
Three-tier system, all based on `rgba(15, 23, 42, X)` slate:
- `sm` – `0 1px 2px 0 rgba(15,23,42,0.05)` — subtle lift (buttons, inputs)
- `md` – stacked `0 4px 6px -1px + 0 2px 4px -2px` — cards, menus
- `lg` – `0 20px 40px rgba(15,23,42,0.15)` — modals, overlays
- **Focus ring** is a 2px outer glow at `rgba(56,143,239,0.15)`

### Borders
- `1px solid var(--ui-border)` (`#E2E8F0`) is the default border everywhere.
- `var(--ui-border-light)` (`#F1F5F9`) for secondary/nested dividers inside cards.
- **Status signaling uses badges, not left-border stripes.** An inline `.leaf` chip ("Connected" / "Error") in the card header carries the state. Don't use colored left-borders as a decorative status indicator.

### Backgrounds, imagery
- **No imagery.** No illustrations. No photos. No patterns. No gradients as backgrounds (exception: the logo mark itself uses a blue gradient).
- Surfaces are flat colors. The only subtle tint is `unread` notifications which mix 8% primary into the surface via `color-mix()`.
- Icons are used instead of imagery in empty states — `lucide:inbox`, `lucide:check-circle`, `lucide:clock` — rendered at 40px, 20% opacity.

### Motion
- **Standard ease** `cubic-bezier(0.25, 0.1, 0.25, 1)` for generic transitions.
- **Expressive ease-out** `cubic-bezier(0.16, 1, 0.3, 1)` for entrances, modal/menu openings.
- Durations: hover/press `0.12s`; standard `0.15–0.2s`; entrance `0.25–0.3s`.
- **List items stagger in** at 30ms per item (capped at 15 items).
- **Dismissal animation** is two-phase: slide-right + fade (0.18s), then row-collapse via `grid-template-rows: 0fr` (0.15s). Very refined.
- Subtle hand-drawn feel on the inbox-zero checkmark — SVG stroke draw-on animation (`stroke-dashoffset`) over 0.5s.
- Respects `prefers-reduced-motion`.

### Hover & press
- **Hover** on subtle buttons: color change to `var(--ui-base-content)` + border → `var(--ui-primary)`. On interactive rows: background shifts to `var(--ui-surface-hover)` (`#F8FAFC`). Never opacity-based.
- **Active/pressed**: No explicit scale or shrink. Primary buttons darken to `--ui-primary-hover` (`#2B7BD6`). Minimal — the product doesn't do bouncy interactions.
- **Focus-visible** always draws `2px solid var(--ui-primary)` with 1–2px offset. Keyboard users get a visible ring.

### Transparency & blur
- Used sparingly. The sidebar uses `rgba(255,255,255,0.05)` for hover states on dark. `color-mix(in srgb, ...)` is used throughout to build tinted surfaces without new tokens.
- No `backdrop-filter` blurs in the source — the product is flat.

### Component personality
- **Cards**: white surface, 1px border, radius `lg` (12px), no drop shadow by default (only on hover/expanded state of integration cards, where a soft primary-tinted shadow appears).
- **Buttons**: bordered-ghost by default (`1px` border, surface background, muted text). Primary = filled solid. Secondary = filled slate subtle. Ghost = transparent until hover.
- **Chips/tags**: pill-radius, `--*-subtle` bg + `--*` text. Action-required tags (review, urgent, mention) bold + preceded by a 5px colored dot.
- **Inputs**: border + `surface-alt` background. Focus adds the primary focus ring. No large shadows.
- **Toggles**: iOS-style 34×18 pill, success green when on.

### Layout rules
- App shell is `100vh`, fixed, no page scroll. Inner panels scroll independently.
- Three-pane layout with hard 1px dividers between panes — no gutters, no shadows between panes.
- Footer strip at the bottom with integration status dots.

---

## Iconography

**Icon system: [Lucide](https://lucide.dev) via [Iconify Web Component](https://iconify.design/docs/iconify-icon/)**. Every icon in the proposal uses `<iconify-icon icon="lucide:..."></iconify-icon>`. Icons are linework, 2px stroke, rounded joins.

**Brand icons** (for integration sources) come from **Logos** via the Iconify CDN — every glyph keeps its real multicolor brand palette. Sit each glyph on a neutral tile (white, 1px border, 6px radius) so the authentic colors read cleanly.

- `logos:github-icon`, `logos:linear-icon`, `logos:slack-icon`
- `logos:google-gmail`, `logos:google-calendar`, `logos:google-drive`
- `logos:todoist-icon`, `logos:notion-icon`

Do not force integration logos into a single flat hex — a monochrome Slack or Gmail misrepresents the brand. If you need a monochrome silhouette (e.g. footer dots, inverted UI), use `simple-icons:*` with an intentional single color.

**Brand icon presentation:** render each integration icon on a **neutral tile** (white surface, 1px border, 6px radius) with the **brand color applied to the icon itself**. Avoid filling the tile with brand color — a grid of saturated GitHub-black/Slack-aubergine/Gmail-red tiles reads as a sticker sheet and fights visual hierarchy. Exception: the integration logo on its own settings card header may use the filled tile, as a hero moment.

All icons rendered inline via CDN — no sprite, no font file, no SVGs checked in. Sizing:
- `11–13px` in tight rows (filter chips, meta badges)
- `14px` default in buttons and list rows
- `16–18px` in card headers and integration tiles
- `20–40px` in empty/hero states

**Emoji:** not used as UI. Appears only in user-generated text (a Slack message sample).

**Unicode symbols:** used in kbd labels only — `&uarr; &darr; &larr; &rarr;` for arrow keys, `⌘` for the meta key. That's it.

**The logo** is the one custom visual asset — a vertically stacked "U" with an inset "i" bar. Both paths share a vertical **linear gradient `#4481eb` → `#05befe`** (bottom → top) which gives the mark its signature blue glow. Stored as `assets/logo.svg` (vector, natural viewBox ~76×91, aspect 0.83:1). A legacy raster `assets/ui-logo.png` is kept for contexts that cannot accept SVG. Use the SVG everywhere possible.

**The wordmark** ("Universal Inbox" text beside the mark, in horizontal lockups) uses the same gradient language as the logo — a vertical `#12B1FA` → `#4481eb` linear gradient (top → bottom), clipped to text via `background-clip: text`. Weight **800**, letter-spacing -0.02em. Matches the treatment on universal-inbox.com. This is the only place text receives a gradient fill — body copy, nav labels, and headings remain solid `--ui-base-content`.

---

## Index

| File | What's in it |
|---|---|
| `README.md` | This document |
| `SKILL.md` | Claude Code skill manifest |
| `colors_and_type.css` | All tokens + semantic type classes |
| `fonts/` | DM Sans woff2 (regular + latin-ext) |
| `assets/` | Logo, any shipped raster assets |
| `preview/` | HTML cards rendered in the Design System tab (Type, Colors, Spacing, Components, Brand) |
| `ui_kits/app/` | Universal Inbox web app UI kit — JSX components + interactive `index.html` |

---

## CAVEATS

- **No Figma, no codebase.** Everything here is reverse-engineered from one HTML proposal. Real production tokens may differ.
- **Logo is now SVG** (`assets/logo.svg`) — scalable and crisp at all sizes. A legacy raster PNG (`assets/ui-logo.png`) is kept for contexts that can't accept SVG.
- **Only DM Sans Regular** was supplied. The proposal specifies weights 300/400/500/600/700 — DM Sans ships with all of them. The single variable woff2 covers the full range, so this works, but shipping each static weight may give better fallback behavior.
- **No marketing / landing site styles exist** in the sources. This kit is for the in-app product only.
- **Dark mode tokens are defined** but were not exhaustively verified — some `-subtle` colors remap to alpha channels, worth reviewing before shipping.
