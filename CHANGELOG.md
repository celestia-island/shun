# Changelog

## 0.5.2 (2026-10-01)

The ghost-hover and path-row polish release.

- **Two hover modes, cleanly split**: solid buttons (primary, danger)
  keep their fill, brighten ~10% and cast the baked gaussian glow;
  transparent ghosts get hikari's flat `--c-primary-light` wash with
  the label shifting to the primary — no glow. `mix()` is alpha-aware
  so the ghost's resting plate stays transparent (the wash previously
  lerped RGB through dark mid-tones, reading as a gray flash before
  the pale color landed).
- **The path field splits display/editing**: at rest the text is
  painter-drawn at the measured baseline (egui's TextEdit row
  centering shifted glyphs by DPI-dependent points no static margin
  tracked); clicking swaps in the TextEdit with requested focus;
  Enter or blur returns to the painter draw.
- The browse button sits at an explicit rect — the row's layout
  cursor parks at the drive chip, so a cursor-based allocate landed
  it mid-field over the path text.
- Footer buttons share one size on both faces (md metrics: 40px tall,
  16px lateral); the web face's lg override drops the lopsided 24px
  lateral padding.
- The install progress bar is the web face's thin track (6pt, no
  in-bar text); the percent rides the live-step line.
- License documents dedent at ingest — the BSL's hard-wrapped
  indentation rendered as markdown code blocks on both faces.
- Done titles drop the leading check glyph at the i18n source (the
  pane draws its own hero; the text duplicate read as a second check).

## 0.5.1 (2026-09-30)

## 0.5.1 (2026-09-30)

The known-gaps release — every item from 0.5.0's list now ships.

- **Optional attachments render on both faces**: the location pane
  carries the optional-components block (per-component checkbox, size,
  a bundled badge for full builds); picked non-bundled components
  stream in right after the payload through the same progress channel,
  on the web face via `download_attachment` and in the egui worker
  inline. A failed attachment fails the run. The demo declares a
  bundled `sample-models` attachment so the block demos offline.
- **Custom content steps render on both faces**, slotted at their
  declaration position around the license (both faces now walk a
  dynamic flow — the web timeline and footer step through one array,
  the egui face swaps its hardcoded three-page assumption for a
  `Page` model). The web pane renders the markdown through the shared
  rich-text engine; the egui pane ships a minimal markdown renderer
  (headings, lists, quotes; emphasis marks strip). The demo declares a
  发行说明 step between the scope and the license.
- **Flash-target notice**: a declared Flash target reads as a pending
  post-install step line on the location pane (both faces). The flash
  runner itself stays a later milestone.
- **The egui wallpaper backdrop**: the manifest's first IMAGE source
  fetches off-thread after boot and paints cover-style behind the
  wizard at ~35% strength; video/pipeline sources stand down (no egui
  renderer) and a failed fetch paints nothing — the degraded-face
  line, now honest instead of absent.

## 0.5.0 (2026-09-30)

## 0.5.0 (2026-09-30)

The parity release: the egui fallback face and the tauri2 webview face
render the same wizard — one flow, two engines — and the delivery flow
grows phase script hooks.

### Faces
- **Unified fixed-origin layout** — every pane on both faces renders on
  one left-aligned column at fixed insets (48px from the rail edge,
  the heading 84px under the window top); the step rails top out at
  the heading's first line. No per-page centering, no
  content-dependent positioning.
- **Theme follows the machine**: `system` mode reads the OS app theme
  (Windows 11 personalization / `prefers-color-scheme`), undetectable
  resolving light. The solar-clock resolution and its WinRT
  geolocation dependency are retired.
- **The standalone uninstaller page** renders on every GUI face
  (`/uninstall`): confirm (cancel · repair · danger uninstall) →
  indeterminate progress → done/failed, repair included; the egui face
  previously ignored the flag entirely and showed the install wizard.
- **The install log strip** is the web LogPane, egui-side: frameless
  activity strip, HH:MM:SS stamps, per-kind colors, fresh-end fade,
  header preview + fold chevron, no scrollbar; newest-first by
  default (`shell.log-order` pins oldest-first), folded by default
  with errors forcing it open.
- The done page carries the full answer set (start-menu shortcut,
  desktop shortcut, launch-after) on both faces, applied through the
  shared finish path; the log card appears only on the failure variant.
- The drive picker's search box is gone (the `searchable` prop — the
  popup teleports, so CSS could never reach it); AppTitleBar's theme
  toggle is actually wired (`emit` was undefined — it has been
  silently throwing since the customActions refactor).
- Content-sized footer buttons; the path input's text centers on its
  row; i18n is one source (wizard-strings.json, 10 locales) shared by
  both faces.

### Flow
- **Phase script hooks** (docs/en/design/scripting.md, minimal mount):
  `[package.metadata.shun.script]` declares a duckscript runner and
  per-phase hooks — `prepare` (pre-extraction), `post-install` (after
  the flow completes, before the done page), `pre-uninstall` (before
  removal, read from the installed copy). Scripts ride the payload;
  `SHUN_LANGUAGE` is exported; a failing hook fails the flow. The
  demo ships all three as screenshot windows (~5s/2.5s/1.5s holds).
- The silent uninstall lane prints its event stream like the install's.
- Manifest parity round: `shell.log-order`, `theme.rail-background`,
  `theme.pane-background` apply on both faces; a theme flip re-applies
  the manifest background layers (the tint previously vanished on the
  first side flip).

### Known gaps (both faces alike, deliberate)
- `wallpaper` renders webview-only (no egui renderer — the documented
  degraded-face line); `attachments`, flash targets and custom content
  steps have no pane on either face yet.

## 0.4.2

The maintenance line before the parity push.
