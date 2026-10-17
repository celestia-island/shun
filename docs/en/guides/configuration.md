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

[[package.metadata.shun.attachments]]
key = "models"
title = "2D/3D model pack"
dest = "models"
[package.metadata.shun.attachments.online]
url = "https://example.test/models.shun"   # optional companion resource (lite builds download it at install time)

[package.metadata.shun.update]             # update watch (optional)
sources = [                                # mirror base URLs, probed in order
  "https://mirror.example.test/shun-demo",
  "https://releases.example.test/shun-demo",
]
files = ["latest", "app-setup.exe"]        # resolved under the winning source

[package.metadata.shun.install]            # install target (default)
local = true                               # registered install (ARP, uninstaller, shortcuts)
portable = true                            # portable mode (.shun-portable marker, no registry)
portable-marker = ".shun-portable"          # marker file name for portable copies (override when the app detects its own)
desktop-shortcut = "ask"                   # always | never | ask (wizard checkbox, default checked; headless resolves ask by launcher presence)
start-menu-shortcut = "always"             # always | never | ask (default: always — the desktop one is the asked-about convenience)
launch-after-install = "ask"               # always | never | ask (done-page checkbox, default checked; pin it for shells without one)
# default-dir = "%LOCALAPPDATA%\\Programs\\Wowsp"  # override the default location (env-expanded %VAR%/${VAR})
deep-links = ["shundemo"]                  # URL schemes the app owns (myapp://...)
aumid = "celestia-island.ShunDemo"         # default: generated from publisher + product
icon = "assets/icon.png"                   # payload-relative launcher icon (Linux Icon=)
root-dir-folder = "ShunDemo"               # folder padded under a bare drive root (D:\ → D:\ShunDemo; default: product name)

[[package.metadata.shun.install.verbs]]    # right-click verbs (Explorer verbs / desktop actions)
key = "open-data"                          # stable verb id
display = "Open data folder"               # menu text
target = "data-folder"                     # data-folder | uninstall | app
# arguments = "--safe"                     # app target only: extra CLI arguments

[package.metadata.shun.webview2]           # Windows-only strategy
type = "skip"                              # skip | evergreen-installer | fixed-version
# path = "WebView2Runtime"                 # fixed-version only: extracted runtime folder
# silent-install = true                    # evergreen-installer: run the carried installer
                                           #   on a runtime-less machine (default — the
                                           #   wizard asks first, headless runs it silently)
# download-url = "https://…"               # evergreen-installer: when nothing is carried,
                                           #   fetch the Evergreen installer at install
                                           #   time (same consent as the carried one)
# warn-missing = true                      # skip / evergreen-installer: the egui fallback
                                           #   warns + links to Microsoft's download page
                                           #   when the machine has no runtime (default)

[[package.metadata.shun.steps]]            # ordered wizard pipeline (optional)
columns = 2             # optional mode-grid columns; defaults to one column per mode
kind = "mode"                              # mode | scope | license | content | pairing | install
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
| `install` | table | both modes on | `local` / `portable` switches, `portable-marker` file name, `desktop-shortcut` / `start-menu-shortcut` / `launch-after-install` policies, `default-dir` (env-expanded default location override), `verbs`, `deep-links`, `aumid`, `icon`, `root-dir-folder` (folder padded under a bare drive-root target, default: the product name) |
| `attachments` | array of tables | none | optional companion resources (asset packs): `key` / `title` / `dest` / `online.url`; lite builds download them at install time |
| `update` | table | none | update watch: `sources` (mirror base URLs, probed in order) + `files` resolved under the first reachable source |
| `webview2` | table | `skip` | Windows runtime strategy |
| `steps` | array of tables | default pipeline | Ordered wizard pipeline (`mode`/`scope`/`license`/`content`/`pairing`/`install`, freely ordered; exactly one `install`) |
| `script` | table | none | Payload script hooks: `runner` + `hooks[]` (`phase`/`script`) — see the scripting design note |
| `variant` | array of tables | none | Named artifact configurations the `shun build --variant` flag selects |
| `signing` | table | unsigned | Authenticode / codesign profile applied to built artifacts |
| `license-sysl` | table | none | Bundle the SySL license + official translations from its repository |
| `msix.logo-background` | color | `transparent` | Plate flattened under a transparent MSIX logo |
| `flash` | table | — | Declares the flash target |

## Shortcut policies

`install.desktop-shortcut` and `install.start-menu-shortcut` resolve per
lane. The wizard faces show checkboxes (default checked — the NSIS
convention); the headless lane (`--silent`) resolves `ask` by
**presence**: a launcher that already exists is refreshed in place, a
missing one is not invented. A silent update must never spray icons the
user did not ask for — AV/EDR heuristics flag exactly that, and a
launcher the user deleted must stay deleted. `always` / `never` pin the
answer in every lane; the explicit `--shortcut-desktop` /
`--shortcut-menu` flags (the updater contract) win over everything.
Uninstall removes launchers unconditionally.

## Launch after install

`install.launch-after-install` resolves the done-page convention: `ask`
(the default) follows the wizard's checkbox — the NSIS "launch after
install" toggle, checked in headless runs — while `always` / `never` pin
it for shells that offer no such toggle.

The install flow itself never launches anything. The shell calls
`shun::targets::install::launch` once the run succeeded: it spawns the
configured `main-exe` inside the install directory detached — no waiting,
so the installer process can exit right after — and is a no-op when the
resolved `InstallContext::launch_after_install` is false or no entry
point is declared. A declared but missing entry point surfaces as the
usual `MissingEntry` error.

## Root-drive guard

A target that is a bare filesystem root — a picked drive like `D:\`
(`D:` and a UNC share root `\\server\share` count too, as does the
POSIX `/`) — never receives the payload directly:
`InstallContext::apply_config` pads one folder level under it, the
product name by default, customized via `install.root-dir-folder`. The
wizard shells rewrite the path box the moment a root is picked or
typed, so the displayed destination is always the real one; headless
`--dir=D:\` runs get the same guard inside the flow.

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

### Runtime-less machines

The face ladder never dies on the missing runtime — it degrades. Before
the ladder resolves, the shell probes the runtime with the loader's own
verdict (`wry::webview_version()`, the exact call tauri gates webview
creation on), so a broken registry state can no longer send the webview
face into tauri's English "Could not find the WebView2 Runtime" box:

- `evergreen-installer` + `silent-install` (default `true`): the runtime
  acquire lives in the install step — initialization only decodes and
  stages the carried installer (discovered under the payload's
  `webview2/` prefix) into the per-user cache, and nothing installs
  before the face ladder resolves. A wizard run pauses on a consent
  callout first (`Install the runtime` / `Skip, main program only`) and
  runs the acquire only on the granted answer — visibly, with the
  installer's own window and elevation through UAC when its manifest
  asks. The headless track keeps `silent-install` as its consent and
  runs the installer with `/silent /install`. A successful acquire
  upgrades the machine in place — the loader reports a usable runtime
  once it settles.
- A build that carries no installer but declares `download-url` fetches
  the Evergreen installer from that URL inside the same install-step
  acquire, behind the same consent (headless runs keep the
  `silent-install` knob) — a lite flavor upgrades the machine from the
  network instead of its own payload. A slow or absent network never
  blocks the first UI: the fetch happens inside the install flow, and
  the degrade banner says the runtime is coming.
- Whatever the outcome, a machine still without a runtime takes the
  egui face. With `warn-missing` (default `true`) and no carried
  installer, that face warns at the start and the end of the wizard and
  embeds a link to Microsoft's official WebView2 download page. Turn
  both knobs off per product:

```toml
[package.metadata.shun.webview2]
type = "evergreen-installer"
silent-install = false                  # never auto-run the carried installer
warn-missing = false                    # silent fallback, no warning banners
```

One upstream limitation worth knowing: tauri's runtime gate ignores
`WEBVIEW2_BROWSER_EXECUTABLE_FOLDER`, so a fixed-version-only machine
(no installed Evergreen runtime) takes the egui face even though webview
creation could otherwise succeed — the shell prefers the face that is
guaranteed to render.

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
webview UI. It is driven by the same manifest and payload (one flow,
three renderers), renders no effects, and is selected when WebView2 is
missing on Windows (the banner states the missing environment
explicitly) or forced manually:

```bash
shun-installer --no-webview   # same wizard, offline renderer (--fallback is the legacy spelling)
```

Both UIs support offline screenshots — the window content is captured
with `PrintWindow`, no desktop automation, and the process exits after
saving:

```bash
shun-installer --screenshot=ui.png                 # hikari (webview) UI
shun-installer --no-webview --screenshot=ui.png    # egui offline UI
```

## Shell UI

`[package.metadata.shun.shell]` (or the `shell` key in a standalone
document) configures the runtime shell:

```toml
[shell]
timeline = "left"          # top (horizontal rail) | left (vertical rail)
log-level = "all"          # all (default) | files | scripts | off
log-order = "newest"    # newest (default, latest on top) | oldest (append at tail)
language = "auto"          # auto | en | zh-Hans | zh-Hant | ja | ko | fr | ru | es | de | pt
faces = ["webview", "egui", "tui"]  # faces this product may present (default: all three)

[shell.theme]
mode = "system"            # system | light | dark
accent = [34, 211, 238]    # RGB channels — overrides --color-primary
```

`shell.faces` restricts the interface faces this product's shell may
present — `webview` / `egui` / `tui`; absent means all three. The
headless CLI face (`--silent`, the ARP uninstall string) is the
substrate and cannot be disabled. Requesting a disabled face is an
error; auto-selection skips disabled faces.

### Wizard language

The wizard asks for its language on the **first step**: a selector
listing the ten supported locales, each labeled in its own language
(English, 简体中文, 日本語, ...). Switching re-renders the UI strings
and the license documents immediately — the agreement follows the
selected locale through `license-locales` / `locale-paths` — and the
choice is remembered for the next run in the per-user preferences file
(`<local app data>/<product>/installer-prefs.json`, e.g.
`%LOCALAPPDATA%\ShunDemo\installer-prefs.json` holding
`{ "language": "zh-Hans" }`). Portable runs keep the choice in memory
only: a portable copy writes no system state. `shell.language` stays
the configuration pin — a remembered choice wins over it, then the
system locale.

The chosen language travels into the install flow: payload scripts
receive it as the `SHUN_LANGUAGE` environment variable, and it is
recorded in the on-disk install manifest (so repair/update passes can
see it). What lands in the installed application's own configuration
is up to the payload's scripts — shun only exports the fact.

## Pairing step

The `pairing` step kind claims a credential for this machine — the
inverted lane: this machine displays a code, an operator types it into
a control panel. One pane, two lanes:

```toml
[[package.metadata.shun.steps]]
kind = "pairing"

[package.metadata.shun.steps.pairing.source]
kind = "gateway"                     # gateway | scripts
official = "https://gateway.example.test"   # preselected gateway base URL
# allow-custom = true                # also offer a custom gateway input (default)

[package.metadata.shun.steps.pairing.identity]
node-id = true                       # the gateway protocol keys on it
name = false                         # optional device name (hostname prefill)
# tier = 1                           # tier provisioned on claim, when the product pins one

# env-file = "pairing.env"           # gateway lane: where the claimed credential lands
                                     #   inside the install dir (.env style)
```

- `gateway` — the built-in JSON-RPC client (`pairing.request` mints
  the code, `pairing.await` long-polls, `pairing.accept` delivers the
  credential). No scripting runtime needed on the target machine; the
  lane requires `node-id` and `env-file`.
- `scripts` — product-owned activation: `request` / `await` / `record`
  payload-relative Python scripts (`python3`, falling back to
  `python`). The pane's answers arrive as one JSON object on the
  script's **stdin** — never argv — and each phase's stdout must be
  exactly one JSON value; `record` persists the claim (an `env-file`
  is carried but unconsumed). Every phase runs under a deadline: a
  hung script is killed, not waited out.

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

## Update watch

`[package.metadata.shun.update]` (or the `update` key in a standalone
document) declares the mirror sources the shell probes for updates, and
the files resolved under them:

```toml
[update]
sources = [                                # mirror base URLs, tried in order
  "https://mirror.example.test/shun-demo",
  "https://releases.example.test/shun-demo",
]
files = ["latest", "app-setup.exe"]
```

At runtime the shell calls `shun::update::resolve`: every source gets a
short probe (a plain GET of the first file, 10 s timeout), the first
source that answers `200` wins for the whole pass, and every declared
file resolves to a URL under the winning base (`<source>/<file>`).
Unreachable or non-200 sources are skipped as warnings; when no source
answers, nothing resolves. The shell then fetches the resolved URLs
itself — a `latest` version marker as text (`fetch_text`), artifact
downloads through the online payload pipeline (`OnlinePayload`:
download → extract → verify).

## Build variants

`[[package.metadata.shun.variant]]` (singular — the table-array idiom)
declares named artifact configurations the `shun build --variant <name>`
flag selects — the axes behind an installer matrix (architecture and
OS come from `--target`, resource packs ride the payload directories):

```toml
[[package.metadata.shun.variant]]
name = "lite"                        # id + artifact-name suffix
payload = "payload-lite"             # payload directory override
faces = ["egui", "tui"]              # face overrides for this variant's shell

[package.metadata.shun.variant.env]  # reaches the shell's cargo build
SHUN_FLAVOR = "lite"                 # (and its build.rs) — flavor markers
```

Applying a variant swaps the payload directory and the shell's face
list; `env` rides the shell build so flavors can embed markers.
Unknown names are an error.

## Code signing

`[package.metadata.shun.signing]` configures signing of built
artifacts — `shun build` signs the installer at the end of its run and
`shun sign` signs standalone files (both skip with `--no-sign`).
Absent = unsigned artifacts, fine for local testing:

```toml
[package.metadata.shun.signing.windows]   # Authenticode via signtool
enabled = true
thumbprint = "<certificate thumbprint>"   # selects a cert from the user/machine store (/sha1)
# timestamp-url = "http://timestamp.digicert.com"   # RFC 3161 (/tr, the default)

[package.metadata.shun.signing.macos]     # codesign
enabled = true
identity = "<signing identity>"
```

## License and custom steps

```toml
license = "docs/LICENSE.md"                # markdown, rendered on the license step

[license-locales]                          # per-locale license overrides
zh-Hans = "docs/LICENSE.zh-Hans.md"
ja = "docs/LICENSE.ja.md"

[[licenses]]                               # additional license documents
title = "Copyright notice"                 # optional heading above the body
path = "NOTICE.md"                         # markdown, relative to the manifest
[licenses.locale-paths]                    # per-locale overrides for this document
zh-Hans = "NOTICE.zh-Hans.md"

[[custom-steps]]                           # inject a markdown content step
key = "whats-new"
after = "license"
title = "What's New"
markdown = "docs/whats-new.md"
```

`license` + `license-locales` are the single-document sugar; the
`licenses` array of tables declares further documents, each with an
optional `title` and its own `locale-paths`. Both combine: the sugar
document comes first, then the array in declaration order. A matching
locale path (`license-locales` or `locale-paths`) wins over the base
document. The license step shows one document at a time — with a
prev/next pager when several resolve — and the single accept checkbox
covers all of them. In the resolved JSON each document travels under
`licenses` (title + body); the legacy `body` string concatenates every
body joined by a divider line, so renderers that only read `body` keep
showing the whole agreement.

The UI ships ten locales (`en`, `zh-Hans`, `zh-Hant`, `ja`, `ko`, `fr`,
`ru`, `es`, `de`, `pt`) with default texts; `shell.language = "auto"`
follows the system, a fixed locale pins it, and per-locale license
overrides keep localized agreements working.

## License from the SySL repository

`[package.metadata.shun.license-sysl]` bundles the Synthetic Source
License and its official translations from the license's own
repository at build time (requires the `online` feature) — wizards
show the agreement in the user's language without vendoring license
files:

```toml
[package.metadata.shun.license-sysl]
repo = "celestia-island/sysl"        # default
branch = "main"                      # default
locales = ["zh-Hans", "ja"]          # translations to bundle (root English is always bundled)
```

Locale tags map onto the sysl repository's i18n directories
(`zh-Hans` → `zhs`, `zh-Hant` → `zht`; `ja`/`ko`/`fr`/`ru`/`es`/`de`/
`pt`/`ar` direct). The bundled documents render on the license step
like any other license document.

## Script hooks

`[package.metadata.shun.script]` declares payload-relative runner
scripts the delivery flow executes at phase boundaries. One runner per
product — only `duckscript` (the embedded interpreter) is mounted
today:

```toml
[package.metadata.shun.script]
runner = "duckscript"

[[package.metadata.shun.script.hooks]]
phase = "post-install"               # prepare | post-install | pre-uninstall
script = "scripts/post_install.ds"   # payload-relative, packed with the payload
```

A failing hook fails the flow. The full contract — phases, the
environment hooks see (`SHUN_LANGUAGE` among them), and examples —
lives in the [scripting design note](../design/scripting.md).
