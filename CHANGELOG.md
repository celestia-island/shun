# Changelog

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
