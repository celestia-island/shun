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

- **install** — NSIS-like registration (per-user ARP entry, self-copying
  uninstaller, start-menu shortcut, deep links) plus a portable mode that
  touches no registry. Uninstall removes every trace per the manifest.
- **flash** — block-device writes with post-write verification (image
  flashing). Backend lands with the evernight flasher; the trait surface
  and device enumeration ship today.

## WebView2 (Windows)

The shell is itself a Tauri app, so the WebView2 runtime is a hard
prerequisite for its own UI. The delivery manifest chooses the strategy:
require the system runtime, embed the Evergreen offline installer, or carry
a fixed-version runtime privately — one copy shared by the shell and the
installed app across install and portable modes.
