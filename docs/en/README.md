<p align="center"><img src="../logo.webp" alt="Shun" width="240" /></p>

<h1 align="center">Shun</h1>

<p align="center"><strong>Flow-driven payload delivery runtime — installers, flashers, and portable modes</strong></p>

<div align="center">

[![License: SySL-1.0](https://img.shields.io/badge/License-SySL--1.0-blue.svg)](https://sysl.celestia.world)
[![Crates.io](https://img.shields.io/crates/v/shun)](https://crates.io/crates/shun)
[![docs.rs](https://docs.rs/shun/badge.svg)](https://docs.rs/shun)
[![Checks](https://img.shields.io/github/actions/workflow/status/celestia-island/shun/checks.yml)](https://github.com/celestia-island/shun/actions/workflows/checks.yml)

</div>

<div align="center">

**English** ·
[简体中文](../zh-Hans/README.md) ·
[繁體中文](../zh-Hant/README.md) ·
[日本語](../ja/README.md) ·
[한국어](../ko/README.md) ·
[Français](../fr/README.md) ·
[Русский](../ru/README.md) ·
[Español](../es/README.md)

</div>

---

Shun packages the **delivery** half of shipping desktop software. One
config document — the app's own `Cargo.toml` (`[package.metadata.shun]`,
the cargo-deb / cargo-wix pattern) — drives the build CLI and the
runtime shell:

- a **payload** packed once, embedded into a single-file installer or
  carried as a sidecar;
- a **flow** — pick a mode, pick a target, stream real progress;
- pluggable **targets**:
  - `install` — NSIS-like registration, per platform: Windows ARP
    entries, shortcuts (AUMID-stamped), Explorer context-menu verbs,
    deep links, per-user or machine-wide (self-elevating); Linux
    `.desktop` launchers with desktop actions; macOS `.app` completion
    plus Launch Services — and a portable mode that writes no system
    state anywhere;
  - `flash` — write an image to a block device with post-write
    verification.

The wizard itself is a **declarative pipeline**
(`mode | scope | license | content | install`, freely ordered), its
install pane shows a real phase-weighted progress bar and a collapsible
terminal logging every file operation — verbosity configurable via
`shell.log-level`.

On Windows the shell has two faces: a hikari WebView UI and an embedded
**egui fallback** that needs no WebView2 at all — same flow, same
manifest (`--fallback` forces it). A fixed-version WebView2 runtime can
ride inside the payload, one copy shared by the shell and the installed
app.

## Example

One demo covers delivery end to end — a real Tauri 2 payload
(`demo-app/`), an installer shell built on
[@celestia-island/hikari](https://github.com/celestia-island/hikari)
(`shell/`), one manifest:

```bash
just demo                                        # stage → build → run the installer shell
just demo -- --fallback                          # force the offline egui shell
cargo run --example demo_install                 # generate a .shun package + local install
cargo run --example demo_install -- --portable   # portable install (no system state)
cargo run --example demo_flash                   # enumerate flash-candidate devices
```

Full field reference: the
[configuration guide](./guides/configuration.md)
([简体中文](../zh-Hans/guides/configuration.md)).

## Status

Current release: **0.2.0**. The crate settles against three real
consumers in the celestia ecosystem — the WoWSP installer shell,
shittim-chest local, and the evernight image flasher. APIs track the
three consumers between minor versions — expect additive changes from
their integration feedback.

## Structure

| Path | Role |
| --- | --- |
| `src/config.rs` | Config schema + `[package.metadata.shun]` loader |
| `src/flow.rs` | Flow model — progress and log events the shell renders |
| `src/payload.rs` | Payload pack / manifest / streamed extraction |
| `src/targets/` | Install (Windows/Linux/macOS registration) and flash targets |
| `demo-app/` | ShunDemo — the Tauri 2 payload app (sample UI, delivery manifest) |
| `shell/` | Installer shell: hikari UI (Tauri) + egui offline fallback |
| `docs/` | Guides and design notes, per locale |

## Development

```bash
just fetch   # stage shared celestia-devtools recipes (once)
just ci      # fmt-check + clippy + test
```

Work lands on `master` through squash-merged PRs from `feat/*` / `fix/*`
branches. See [AGENTS.md](../../AGENTS.md) for the full conventions.

## License

SySL-1.0 — see [LICENSE](../../LICENSE).
