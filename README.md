<p align="center"><!-- <img src="https://raw.githubusercontent.com/celestia-island/docs.celestia.world/master/res/logo/shun.webp" alt="Shun" width="240" /> --></p>

<h1 align="center">Shun</h1>

<p align="center"><strong>Flow-driven payload delivery runtime — installers, flashers, and portable modes</strong></p>

<div align="center">

[![License: SySL-1.0](https://img.shields.io/badge/License-SySL--1.0-blue.svg)](https://sysl.celestia.world)
[![GitHub](https://img.shields.io/badge/github-celestia--island%2Fshun-blue.svg)](https://github.com/celestia-island/shun)
[![Checks](https://img.shields.io/github/actions/workflow/status/celestia-island/shun/checks.yml)](https://github.com/celestia-island/shun/actions/workflows/checks.yml)
[![docs.rs](https://docs.rs/shun/badge.svg)](https://docs.rs/shun)

</div>

---

Shun packages the **delivery** half of shipping desktop software. One config
document drives both the build CLI and the runtime shell:

- a **payload** — the app directory packed once, embedded into a single-file
  installer or carried as a sidecar;
- a **flow** — choose a mode, choose a target, stream real progress events;
- pluggable **targets**:
  - `install` — NSIS-like registration (ARP entry, uninstaller, shortcuts,
    deep links) *and* a portable mode that writes no registry at all;
  - `flash` — write an image onto a block device with post-write
    verification.

On Windows, a dual-variant WebView2 strategy covers clean machines: a
standard artifact that requires the system runtime, and a fully
self-contained artifact that carries a **fixed-version WebView2 runtime
privately** — one copy shared by the installer shell and the installed app
across install and portable modes, no admin, no system writes.

## Status

Pre-release; the crate is settling against three real consumers in the
celestia ecosystem — the WoWSP installer shell, shittim-chest local, and the
evernight image flasher. Active development happens on the `dev` branch;
`master` will receive the initial release once the first delivery flow is
complete. APIs are unstable until `0.1`.

## Structure

| Path | Role |
| --- | --- |
| `src/config.rs` | Config schema — one document for the build CLI and the shell |
| `src/flow.rs` | Flow model — progress events the shell renders |
| `src/payload.rs` | Payload manifest + streamed extraction |
| `src/targets/install.rs` | Install target: registration backends, portable mode |
| `src/targets/flash.rs` | Flash target: block-device write + verify |

The Tauri runtime shell (hikari-based UI) and the build CLI land on top of
this contract in later iterations.

## Development

```bash
just fetch   # stage shared celestia-devtools recipes (once)
just ci      # fmt-check + clippy + test
```

Workflow: rapid preparation happens on `dev`; `master` receives the initial
release commit, after which everything lands through PRs.

## License

SySL-1.0 — see [LICENSE](./LICENSE).
