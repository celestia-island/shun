# Delivery Manifest Reference

The delivery flow is declared in the application's own `Cargo.toml`, under
`[package.metadata.shun]` — the cargo-deb / cargo-wix pattern. Product
identity defaults to `[package]` (`name`, `version`); everything under the
table customizes the flow.

```toml
[package.metadata.shun]
product = "ShunDemo"                       # default: package name
publisher = "celestia-island"              # ARP Publisher field
logo = "docs/logo.webp"                    # shell logo asset
payload = "examples/demo_payload"          # directory packed into artifacts
main-exe = "bin/shun-demo.exe"             # payload-relative entry point

[package.metadata.shun.install]            # install target (default)
local = true                               # registered install (ARP, uninstaller, shortcuts)
portable = true                            # portable mode (.shun-portable marker, no registry)
desktop-shortcut = "ask"                   # always | never | ask (wizard checkbox, default checked)
deep-links = ["shundemo"]                  # URL schemes the app owns (myapp://...)
aumid = "celestia-island.ShunDemo"         # default: generated from publisher + product
icon = "assets/icon.png"                   # payload-relative launcher icon (Linux Icon=)

[[package.metadata.shun.install.verbs]]    # right-click verbs (Explorer verbs / desktop actions)
key = "open-data"                          # stable verb id
display = "Open data folder"               # menu text
target = "data-folder"                     # data-folder | uninstall | app
# arguments = "--safe"                     # app target only: extra CLI arguments

[package.metadata.shun.webview2]           # Windows-only strategy
type = "skip"                              # skip | evergreen-installer | fixed-version
# path = "WebView2Runtime"                 # fixed-version only: extracted runtime folder

[[package.metadata.shun.steps]]            # ordered wizard pipeline (optional)
kind = "mode"                              # mode | scope | license | content | install
align = "center"                           # per-step override: center | start (default from kind)

[[package.metadata.shun.steps]]
kind = "content"
title = "Release notes"                       # content steps carry a title…
markdown = "notes.md"                   # …and a document, inlined at build time

[[package.metadata.shun.steps]]
kind = "install"                           # exactly one install step

[package.metadata.shun.flash]              # flash target (optional)
require-removable = true                   # refuse non-removable devices
```

## Fields

| Field | Type | Default | Meaning |
| --- | --- | --- | --- |
| `product` | string | package `name` | Titles, ARP display name, flash labels |
| `publisher` | string | — | ARP Publisher field |
| `logo` | path | — | Shell logo asset (relative to the manifest) |
| `payload` | path | — | Directory packed into the artifacts |
| `main-exe` | path | — | Payload-relative entry point (shortcut target) |
| `install` | table | both modes on | `local` / `portable` switches, `desktop-shortcut` policy, `verbs`, `deep-links`, `aumid`, `icon` |
| `webview2` | table | `skip` | Windows runtime strategy |
| `msix.logo-background` | color | `transparent` | Plate flattened under a transparent MSIX logo |
| `flash` | table | — | Declares the flash target |

## WebView2 strategies

| `type` | Carries | Requires | Notes |
| --- | --- | --- | --- |
| `skip` | nothing | system WebView2 | Standard artifact |
| `evergreen-installer` | Evergreen offline installer (~127 MB) | elevation at install | Registers a system-wide runtime |
| `fixed-version` | extracted runtime folder | nothing | One private copy shared by the shell and the installed app across install and portable modes |

Measured (2026-09, v151 x64): the fixed-version runtime is **~293 MB
compressed / ~661 MB unpacked** — an accepted cost, in line with other
packagers' output. The artifact embeds exactly **one** copy: the shell
bootstraps itself by staging the payload's runtime subtree into
`%LOCALAPPDATA%\shun\<product>\webview2` and pointing
`WEBVIEW2_BROWSER_EXECUTABLE_FOLDER` there; extraction is hash-aware, so
re-installs and pre-staged files are adopted instead of rewritten
("Reusing" progress events) — no second copy of the same engine.

## MSIX logo plate

Windows plates the logo of a packaged desktop app over a default system
blue (`#0078D7`) on every surface that ignores
`BackgroundColor="transparent"` — the App Installer dialog among them.
`msix.logo-background` takes a `#RRGGBB` color: the shun CLI composites a
transparent logo onto it for the package assets and declares it as the
`BackgroundColor`, so the brand picks the plate color everywhere:

```toml
[package.metadata.shun.msix]
# identity-name / publisher / display-name / executable ...
logo-background = "#0F172A"
```

## Offline fallback shell

The installer shell embeds an egui fallback UI besides the hikari
webview UI. It is driven by the same manifest and payload (one flow, two
renderers), renders no effects, and is selected when WebView2 is missing
on Windows (the banner states the missing environment explicitly) or
forced manually:

```bash
shun-demo-shell --fallback     # same wizard, offline renderer
```

Both UIs support offline screenshots — the window content is captured
with `PrintWindow`, no desktop automation, and the process exits after
saving:

```bash
shun-demo-shell --screenshot=ui.png               # hikari (webview) UI
shun-demo-shell --fallback --screenshot=ui.png    # egui offline UI
```

## Shell UI

`[package.metadata.shun.shell]` (or the `shell` key in a standalone
document) configures the runtime shell:

```toml
[shell]
timeline = "left"          # top (horizontal rail) | left (vertical rail)
log-level = "all"          # all (default) | files | scripts | off
language = "auto"          # auto | en | zh-Hans | zh-Hant | ja | ko | fr | ru | es

[shell.theme]
mode = "system"            # system | light | dark
accent = [34, 211, 238]    # RGB channels — overrides --color-primary
```

## Source

`[package.metadata.shun.source]` picks where the payload comes from at
install time:

```toml
[source]
type = "embedded"          # the payload archive is embedded in the installer
```

```toml
[source]
type = "online"            # the installer downloads the payload
url = "https://github.com/<org>/<repo>/releases/latest/download/ShunDemo.shun"
```

An online installer streams **download → extract → verify in one pass**:
bytes are verified against the manifest as they arrive, and progress events
report the download and extract phases concurrently (multi-layer progress).
Point `url` at your release feed (GitHub Releases or any HTTP host) and the
installer is updated by simply publishing a new package.

## License and custom steps

```toml
license = "docs/LICENSE.md"                # markdown, rendered on the license step

[license-locales]                          # per-locale license overrides
zh-Hans = "docs/LICENSE.zh-Hans.md"
ja = "docs/LICENSE.ja.md"

[[custom-steps]]                           # inject a markdown content step
key = "whats-new"
after = "license"
title = "What's New"
markdown = "docs/whats-new.md"
```

The UI ships eight locales (`en`, `zh-Hans`, `zh-Hant`, `ja`, `ko`, `fr`,
`ru`, `es`) with default texts; `shell.language = "auto"` follows the
system, a fixed locale pins it, and per-locale license overrides keep
localized agreements working.
