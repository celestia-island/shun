<p align="center"><img src="./docs/logo.webp" alt="Shun" width="240" /></p>

<h1 align="center">Shun</h1>

<p align="center"><strong>Flow-driven payload delivery runtime — installers, flashers, and portable modes</strong></p>

<div align="center">

[![License: SySL-1.0](https://img.shields.io/badge/License-SySL--1.0-blue.svg)](https://sysl.celestia.world)
[![GitHub](https://img.shields.io/badge/github-celestia--island%2Fshun-blue.svg)](https://github.com/celestia-island/shun)
[![Checks](https://img.shields.io/github/actions/workflow/status/celestia-island/shun/checks.yml)](https://github.com/celestia-island/shun/actions/workflows/checks.yml)
[![docs.rs](https://docs.rs/shun/badge.svg)](https://docs.rs/shun)

</div>

<div align="center">

**English** ·
[简体中文](./docs/zh-Hans/README.md) ·
[繁體中文](./docs/zh-Hant/README.md) ·
[日本語](./docs/ja/README.md) ·
[한국어](./docs/ko/README.md) ·
[Français](./docs/fr/README.md) ·
[Русский](./docs/ru/README.md) ·
[Español](./docs/es/README.md)

</div>

---

Shun packages the **delivery** half of shipping desktop software. The
delivery flow is declared in the application's own `Cargo.toml`
(`[package.metadata.shun]` — the cargo-deb / cargo-wix pattern) and one
config document drives both the build CLI and the runtime shell:

- a **payload** — the app directory packed once, embedded into a single-file
  installer or carried as a sidecar;
- a **flow** — choose a mode, choose a target, stream real progress events;
- pluggable **targets**:
  - `install` — NSIS-like registration (per-user ARP entry, self-copying
    uninstaller, start-menu shortcut, deep links) *and* a portable mode that
    writes no registry at all;
  - `flash` — write an image onto a block device with post-write
    verification.

On Windows, a dual-variant WebView2 strategy covers clean machines: a
standard artifact that requires the system runtime, and a fully
self-contained artifact that carries a **fixed-version WebView2 runtime
privately** — one copy shared by the installer shell and the installed app
across install and portable modes, no admin, no system writes. For
machines where even that is not available, the installer shell embeds an
**egui fallback UI**: the same flow, the same manifest, no WebView2 at all
— selected automatically when the runtime is missing (the banner says so)
or manually via `--fallback`.

## Example

One comprehensive demo covers delivery end to end. The payload is a real
Tauri 2 application (`demo-app/`) with a sample UI, the installer shell
(`shell/`, built on
[@celestia-island/hikari](https://github.com/celestia-island/hikari))
embeds it at build time, and the whole thing is declared by a single
delivery manifest:

```bash
just demo                                               # stage demo app → build → run the installer shell
just demo -- --fallback                                 # force the offline egui shell (no WebView2)
cargo run --example demo_flash                          # enumerate flash-candidate devices
cargo run --example demo_install                        # CLI integration check (generate .shun + local install)
cargo run --example demo_install -- --portable          # portable install (no registry)
cargo run --example demo_install -- --uninstall         # remove the install (all traces)
```

The delivery manifest lives in the demo application's own `Cargo.toml`
(the cargo-deb / cargo-wix pattern — the shell resolves it at build time,
so installer and app describe one product):

```toml
[package.metadata.shun]
product = "ShunDemo"
publisher = "celestia-island"
logo = "../docs/logo.webp"
payload = "../examples/demo_payload"
main-exe = "bin/shun-demo.exe"

[package.metadata.shun.install]
local = true
portable = true
```

The payload root keeps small committed data files; the application
binary is a build product staged into `bin/` by `just demo-payload`
(never committed). `demo_install` generates the installer package
`ShunDemo.shun` (zstd tar + SHA-256 manifest) in the working directory,
extracts it with streamed progress, and — in local mode — performs the
NSIS-like registration described above.

See [docs/en/guides/configuration.md](./docs/en/guides/configuration.md)
([简体中文](./docs/zh-Hans/guides/configuration.md)) for the full reference,
including the WebView2 strategy matrix.

## Status

`0.1` cut. The crate is settling against three real consumers in the
celestia ecosystem — the WoWSP installer shell, shittim-chest local, and the
evernight image flasher (which lands the flash target's block-device write
backend). Active development happens on the `dev` branch; `master` carries
the release history. APIs track the three consumers between minor
versions — expect additive changes and upstream proposals from their
integration feedback (e.g. a flash-specific `FlowPhase` variant and a
`Backend(String)` error kind, both raised by the evernight flasher
integration).

## Structure

| Path | Role |
| --- | --- |
| `src/config.rs` | Config schema + `[package.metadata.shun]` loader |
| `src/flow.rs` | Flow model — progress events the shell renders |
| `src/payload.rs` | Payload pack / manifest / streamed extraction |
| `src/targets/install.rs` | Install target: registration backends, portable mode |
| `src/targets/flash.rs` | Flash target: block-device write + verify |
| `demo-app/` | ShunDemo — the Tauri 2 payload app (sample UI, delivery manifest) |
| `shell/` | Installer shell: hikari UI (Tauri) + egui offline fallback |
| `docs/` | Guides and design notes, per locale |

## Development

```bash
just fetch   # stage shared celestia-devtools recipes (once)
just ci      # fmt-check + clippy + test
```

Workflow: rapid preparation happens on `dev`; `master` receives the initial
release commit, after which everything lands through PRs.

## License

SySL-1.0 — see [LICENSE](./LICENSE).
