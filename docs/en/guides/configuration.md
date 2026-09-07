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
main-exe = "bin/shun-demo.cmd"             # payload-relative entry point

[package.metadata.shun.install]            # install target (default)
local = true                               # registered install (ARP, uninstaller, shortcuts)
portable = true                            # portable mode (.shun-portable marker, no registry)

[package.metadata.shun.webview2]           # Windows-only strategy
type = "skip"                              # skip | evergreen-installer | fixed-version
# path = "WebView2Runtime"                 # fixed-version only: extracted runtime folder

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
| `install` | table | both modes on | `local` / `portable` switches |
| `webview2` | table | `skip` | Windows runtime strategy |
| `flash` | table | — | Declares the flash target |

## WebView2 strategies

| `type` | Carries | Requires | Notes |
| --- | --- | --- | --- |
| `skip` | nothing | system WebView2 | Standard artifact |
| `evergreen-installer` | Evergreen offline installer (~127 MB) | elevation at install | Registers a system-wide runtime |
| `fixed-version` | extracted runtime folder | nothing | One private copy shared by the shell and the installed app across install and portable modes |
