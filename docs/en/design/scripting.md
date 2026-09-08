# Design note: install-time scripting (draft, not implemented)

Status: **requirements captured, design pending.** Nothing here is built
yet; the note exists so the constraints are not lost between iterations.

## Goal

An installer may reference JavaScript bundled into the artifact. The shun
CLI compiles/minifies the script at build time, and the runtime executes
it inside an embedded [boa](https://boajs.dev/) engine — no system Node,
no network at script-run time unless the script asks for it. Type support
ships as an npm package (`@celestia-island/shun-script` or similar) with
`.d.ts` declarations for the built-in surface.

## Shape (sketch)

```toml
[package.metadata.shun.script]
entry = "installer/main.js"        # bundled, compiled and minified by `shun build`
```

The script runs alongside the flow: it can subscribe to flow events,
inject wizard steps, and veto or customize phases (custom pages, extra
registration work, product-specific cleanup on uninstall).

## Built-in surface (requirements)

- **fs** — read/write/move/remove with streaming copies.
- **hash** — streaming checksums (SHA-256 family at minimum).
- **crypto** — streaming encrypt/decrypt; digital-signature verification
  of downloaded content.
- **net** — HTTP(S) requests with progress, resumable downloads.
- **deps** — dynamically fetch dependencies (scripts or archives) from
  the release feed, verified before use.
- **flow** — emit/consume `FlowEvent`s, add steps, set failure messages.

## Open questions

- Sandboxing model: capability-scoped APIs per manifest, or a single
  trusted-script model (the artifact is already signed)?
- Sync vs async ergonomics on boa; backpressure for streaming ops.
- Whether `deps` may pull arbitrary URLs or only the declaring manifest's
  release feed.
- Bundle format for the compiled script (single minified source vs a
  small container with source maps for diagnostics).
