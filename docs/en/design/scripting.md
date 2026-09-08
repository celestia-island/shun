# Design note: install-time scripting (draft, not implemented)

Status: **requirements captured, one runner proven embeddable, design
pending.** Nothing is wired into the delivery runtime yet; this note
exists so the constraints are not lost between iterations.

## Goal

An installer may reference scripts bundled into the artifact. The shun
CLI compiles/packs them at build time, and the runtime executes them
inside an embedded engine — no system interpreter, no network at
script-run time unless the script asks for it. Two runner candidates:

### Option A — duckscript (embedded, proven)

[`duckscriptsdk`](https://crates.io/crates/duckscriptsdk) (Apache-2.0,
the cargo-make scripting language) embeds as an ordinary dependency:
load the command set into a `Context`, register shun built-ins as custom
commands, run scripts with flow control and std fs/env/net. The
feasibility proof lives in `tests/scripting_duckscript.rs` — variables,
`writefile`/`readfile`, `assert_eq`, and a custom `shun_note` command
recording calls, one script, no unsafe, no C dependencies. Note that
justfile itself is **not** embeddable (the `just` crate is a CLI, no
stable library API) — duckscript is the embeddable member of that
family and the closest to "run these scripts from our repo at install
time".

```toml
[package.metadata.shun.script]
runner = "duckscript"

[[package.metadata.shun.script.hooks]]
phase = "prepare"                 # prepare | post-install | pre-uninstall
script = "installer/prepare.dk"   # packed by `shun build`
```

The shun built-in surface would register as duckscript commands:
`shun_progress`, `shun_emit`, `shun_fetch` (verified downloads), plus
the SDK's own std commands (fs, env, http, process, semver, ...).

### Option B — JavaScript via boa

Richer ecosystem and web familiarity; type support ships as an npm
package (`@celestia-island/shun-script`) with `.d.ts` declarations.
Heavier to build out: sandboxing, async ergonomics on boa, a bundling
step for sources.

## Built-in surface (requirements, runner-agnostic)

- **fs** — read/write/move/remove with streaming copies.
- **hash** — streaming checksums (SHA-256 family at minimum).
- **crypto** — streaming encrypt/decrypt; digital-signature verification
  of downloaded content.
- **net** — HTTP(S) requests with progress, resumable downloads.
- **deps** — dynamically fetch dependencies (scripts or archives) from
  the release feed, verified before use.
- **flow** — emit/consume `FlowEvent`s, add steps, set failure messages.

## Open questions

- Duckscript first (cheap embedding, sufficient for install-time glue)
  with a `runner` field leaving the boa door open — or both from day one?
- Sandboxing model: capability-scoped commands per manifest, or a single
  trusted-script model (the artifact is already signed)?
- Whether `deps` may pull arbitrary URLs or only the declaring manifest's
  release feed.
- Windows-path escaping in duckscript arguments (a backslash starts an
  escape; pass forward-slash paths or quote) — needs a normalization rule
  in the shun command wrappers.
