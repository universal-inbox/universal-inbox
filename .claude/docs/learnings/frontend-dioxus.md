# Frontend — Dioxus 0.7 + Tailwind v4

Frontend is **Dioxus 0.7 compiled to `wasm32-unknown-unknown`** (WASM), pinned in `web/Cargo.toml`.
Module doc-comments in `web/src/components/ui/*.rs` carry the per-component charter.

## `web/src` layout

```
web/src/
├── components/        # feature components (sidebar, notification_preview, integrations_panel, ...)
│   └── ui/            # design-system primitives (see inventory below)
├── pages/             # page-level components
├── services/          # API client services (talk to api/ backend)
├── layouts/           # shell / page layouts
├── model.rs           # app state model
├── route.rs           # router definition
├── theme.rs           # light/dark theme switching (data-theme)
├── keyboard_manager.rs# global keyboard shortcuts (triage is keystroke-first)
├── auth.rs · form.rs · icons.rs · settings.rs · config.rs
```

## Styling decision order (stop at the first that fits)

1. **Tailwind v4 utilities backed by `@theme` tokens** — `bg-ui-primary`, `text-ui-error`,
   `border-ui-border`, `rounded-ui-md`, `shadow-ui-sm`, `font-ui`, `bg-avatar-hue-7`,
   `text-brand-github`. Dark mode resolves automatically via the `@custom-variant dark` rule +
   `[data-theme="dark"]` token overrides — never duplicate values for dark. If a token is missing,
   add it to the `@theme inline` block at the top of `web/css/universal-inbox.css`, don't inline a literal.
2. **FlyonUI native components** — `btn`, `badge`, `alert`, `modal`, `dropdown`, `toggle`, `tabs`,
   `accordion`, `tooltip`, `input`. The theme plugin maps the brand palette to FlyonUI's `--color-*`.
3. **An existing Dioxus component in `web/src/components/ui/`** (inventory below). Repeated 3+ times = it belongs here.
4. **A new component in `web/src/components/ui/`** — variant-driven, token-aware, doc-comment with usage,
   register in `web/src/components/ui/mod.rs` with a matching `pub use` (flat-imported via `use crate::components::ui::*;`).
   Style inline with utility classes on the RSX element; reuse existing class constants
   (e.g. `POP_INPUT_CLASSES` in `ui/select.rs`) before writing new ones.
5. **Custom CSS in `web/css/universal-inbox.css`** — LAST resort, only for: `::before`/`::after`
   decorations, `@keyframes`, sibling/parent-state cascades, scrollbar internals, `appearance:none`
   form-control internals, responsive overrides that don't fit `sm:`/`md:`/`lg:`.

## `web/src/components/ui/` inventory

Check this before styling from scratch — each is variant-driven and token-aware:

| Component | Variants / sizes |
|---|---|
| `Button` | `Primary`/`Ghost`/`Danger`/`Warning`/`Icon` × `Xs`/`Sm`/`Md`/`Lg` (module doc table lists older variants; the enum is authoritative) |
| `Badge` | `Primary`/`Muted`/`Count`/`Email`/`Method` × optional `BadgeTone` |
| `Tag` | `Open`/`Review`/`Error`/`Warning`/`Info`/`Success`/`Muted`/`Urgent`/`Mention` |
| `StatusLeaf` | `Connected`/`Disconnected`/`Error`/`SyncIssue`/`Syncing` |
| `Card` family | `Card`, `CardHeader`, `CardMeta`, `CardBody`, `CardRight`, `CardEmptyState` (`CardVariant::{Default,ApiKeys,Integration}`) |
| `ToggleSwitch` | `ToggleSize::{Sm,Md}` |
| `UISelect` / `UISearchSelect` / `UIMultiSelect` | `ui/select.rs`; presets in `option_presets.rs` |
| `ModalHeader` / `ModalSourceRow` / `ModalFooter` | modal chrome |
| `StatusSection` / `StatusRow` / `StatusDot`, `MetadataGrid` / `MetadataItem` | preview panels |
| `ThreadedMessage`, `ParticipantStack`, `ReactionChip` | thread/comment rendering |
| `NavItem` / `NavSection`, `PageHeader` / `Overline` | — |
| `Kbd` / `KeyboardHint` | `KbdSize::{Xs,Sm}` |
| `BrandTile` | `BrandTileSize::{Sm,Md,Lg}` + `provider: IntegrationProviderKind` |

Need a missing variant or size? **Extend the existing component** (add an enum variant + class mapping). Don't fork a parallel one.

## The "CSS class hook" hybrid rule

Some components (`Tag`, `StatusLeaf`, `Card` Integration variant, the
notification row `.ui-nrow*`) emit **legacy CSS class names** alongside their utility composition.
This is deliberate: pseudo-element dots, keyframe animations (`card-body-in`, `notif-enter`,
`notif-dismiss-*`, `inbox-zero-pop`), and `:checked +` sibling cascades are anchored to those names.
Keep the class string intact — it's the contract between the component and the CSS layer. Document
load-bearing class names in the module doc-comment.

## What NOT to do

- No new `.foo-bar` custom CSS classes unless a "last resort" criterion applies.
- No arbitrary-value syntax `bg-[var(--ui-primary)]` / `bg-[#388fef]` for tokens that exist — use `bg-ui-primary`.
- No hardcoded hex colors, font stacks, pixel radii, or shadows in Rust strings — they belong in the token layer.
- No duplicated dark-mode rules — utilities/tokens already react to `[data-theme="dark"]`.
- Settings integration cards keep their header on ONE row on mobile (tile, `CardMeta` flex-1 min-w-0, `CardRight`):
  no `max-md:flex-wrap`; make wide buttons icon-only below `sm` (`span { class: "hidden sm:inline" }`).
- Never store secrets/OIDC codes in local storage or log them (`web/src/services/local_storage.rs`).
- No `style="..."` inline overrides except for genuinely dynamic values (e.g. `style="--source-accent: {color}"` per row).

## Building / running web

```bash
cd web && just test            # frontend tests (builds assets first)
cd web && just check           # clippy -D warnings (also rebuilds web/public assets)
just start ui-web              # via process-compose (after `just run-detached`)
just web run false             # dev server (dx serve, non-interactive); rewrites web/Dioxus.toml
                               # backend port from $API_PORT — revert before committing
```

`ui-web` builds Tailwind once (no `--watch`) and the `dx serve` watcher is flaky in worktrees:
after adding classes or if edits don't show, `process-compose process restart ui-web -p $PROCESS_COMPOSE_PORT`.
After editing `web/js/index.js`: `cd web && just bundle-js`, touch a `web/src` file, restart `ui-web`.

Verify health: `curl -s http://localhost:${API_PORT:-8000}/ping` then
`curl -o /dev/null -w "%{http_code}" http://localhost:${DX_SERVE_PORT:-8080}/`.

See also: [common pitfalls](common-pitfalls.md) · [API design](api-design.md).

_Last updated: 2026-10-15_
