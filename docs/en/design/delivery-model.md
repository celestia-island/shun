# Delivery Model

Shun splits delivery into three orthogonal pieces:

## Payload

An application directory packed into a zstd-compressed tar with a SHA-256
manifest (`shun-manifest.json`). The archive is embedded into the installer
binary (`include_bytes!`, the single-file installer pattern) or carried as
a sidecar. Extraction verifies every entry against the manifest and streams
progress events.

## Flow

A delivery run is a sequence of steps streaming `FlowEvent`s — `started`,
`progress { step, percent }`, `completed`, `failed` — that the shell UI
renders directly. The install flow extracts the payload, persists the
on-disk manifest (consumed by uninstall), then either registers (local
mode) or drops the portable marker (portable mode).

## Targets

- **install** — direct registration, per platform: on Windows a per-user
  ARP entry, a self-copying uninstaller, start-menu/desktop shortcuts
  (AUMID-stamped), and optional Explorer context-menu verbs; on Linux a
  per-user `.desktop` launcher with desktop actions (including
  Uninstall); on macOS `.app`-bundle completion plus Launch Services
  registration. A portable mode touches no system state anywhere.
  Uninstall removes every trace per the manifest.
- **flash** — block-device writes with post-write verification (image
  flashing). Backend lands with the evernight flasher; the trait surface
  and device enumeration ship today.

## WebView2 (Windows)

The shell is itself a Tauri app, so the WebView2 runtime is a hard
prerequisite for its own UI. The delivery manifest chooses the strategy:
require the system runtime, embed the Evergreen offline installer, or carry
a fixed-version runtime privately — one copy shared by the shell and the
installed app across install and portable modes.

## Directory field

The install-target row reads `[folder badge | path input | browse]` —
badge at the left inside the field, browse pinned at the row's right end
(the shittim-chest file-picker look), and the hint below.

The browse action is a *picker seam*, not a hardcoded dialog. The field
is a candidate hikari file-picker component whose backend is selectable:

1. **browser native** — `<input type="file">` / `showDirectoryPicker()`.
   Fine for content (uploads); unusable for install targets, because the
   browser deliberately hides absolute paths — a directory handle only
   exposes its leaf name.
2. **hikari in-app picker** — a unified modal browser inside the app
   window (theming, keyboard navigation, remote roots). Future hikari
   component work.
3. **app hook** — the host supplies the picker. A Tauri2 app lands here:
   the dialog plugin opens a real OS window *outside* the webview
   (Tauri2 has no in-app dialog), which neither of the other backends
   covers. The shun shell resolves to this backend via the global
   Tauri API; the egui fallback calls the OS dialog directly (`rfd`).

The auto chain is hook → native → manual typing; the component keeps
all three addressable so embedders can force one.
