# Quick Start

Shun ships two halves: the **build side** (pack a payload, resolve the
delivery manifest) and the **runtime side** (a shell that drives the flow).

## Try the demo

```bash
cargo run --example demo_flash                        # enumerate flash-candidate devices
cargo run --example demo_install                      # generate ShunDemo.shun + local install
cargo run --example demo_install -- --portable        # portable install (no registry)
cargo run --example demo_install -- --uninstall       # remove the install (all traces)
```

`demo_install` generates the installer package `ShunDemo.shun`, extracts it
with streamed progress, and — in local mode — performs NSIS-like
registration: a per-user ARP entry (Settings → Apps), a start-menu
shortcut, and a self-copying `uninstall.exe`. Portable mode writes a
`.shun-portable` marker and never touches the registry.

## Run the demo shell

```bash
pnpm --dir shell/web install
cargo run -p shun_demo_shell
```

The shell embeds the demo payload at build time (single-file installer
pattern) and renders the delivery modes declared in
`shell/Cargo.toml` → `[package.metadata.shun]`.
