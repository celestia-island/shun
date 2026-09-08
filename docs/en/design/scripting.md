# Design note: install-time scripting

Status: **runner decided — duckscript. Python researched and proven
embeddable; adoption pending.** duckscript is the only scripting runner;
the JavaScript option is dropped (the cargo-make toolset around
duckscript is complete, and where it is not, calling Python is the
escape hatch). This note records the decision and the Python embedding
research.

## duckscript (the runner)

[`duckscriptsdk`](https://crates.io/crates/duckscriptsdk) (Apache-2.0,
the cargo-make scripting language) embeds as an ordinary dependency:
load the command set into a `Context`, register shun built-ins as custom
commands, run scripts with flow control and std fs/env/net. The
feasibility proof lives in `tests/scripting_duckscript.rs`. justfile
itself is not embeddable (the `just` crate is a CLI, no stable library
API) — duckscript is the embeddable member of that family.

```toml
[package.metadata.shun.script]
runner = "duckscript"

[[package.metadata.shun.script.hooks]]
phase = "prepare"                 # prepare | post-install | pre-uninstall
script = "installer/prepare.dk"   # packed by `shun build`
```

The shun built-in surface registers as duckscript commands:
`shun_progress`, `shun_emit`, `shun_fetch` (verified downloads), plus
the SDK's own std commands (fs, env, http, process, semver, ...).

Gotchas to normalize in the shun wrappers: Windows backslash paths are
escape characters in duckscript arguments (pass forward-slash paths),
and assignment is output-capture syntax (`x = cmd args`).

## Embedded Python — measured (probe run 2026-09)

Everything below ran for real, twice: on a host CPython 3.13.5 and on a
**carried embeddable runtime** (`python-3.13.5-embed-amd64.zip`
unpacked, with the PyO3 `pyembed_runner` example placed beside it so
`python313.dll`/stdlib load from the carried folder — `sys.prefix`
confirmed the carried dir):

| Capability | Result |
| --- | --- |
| Real HTTPS (urllib + TLS) | ok (direct pypi.org was network-blocked locally; example.com/tencent mirror fine) |
| Streaming SHA-256 + HMAC | ok |
| AES-CTR roundtrip, RSA-2048 sign/verify | ok — via the `cryptography` wheel pre-installed into the carried runtime (`pip --target runtime/Lib/site-packages` + enable `import site` in `python313._pth`) |
| Machine identity | MachineGuid (winreg), MAC (`uuid.getnode`), C: volume serial (ctypes `GetVolumeInformationW`) — all ok |
| TPM | `tbs.dll` via ctypes reached correctly; the probe machine's firmware has TPM disabled, so `Tbsi_Context_Create` returns `TBS_E_TPM_NOT_FOUND` (0x8028400F — note: NOT 0x80284002, which is `TBS_E_BAD_PARAMETER` from a NULL params struct). The call path is validated; on TPM-enabled hardware the same code reads `TPM_PT_MANUFACTURER` |

Measured sizes: embeddable zip **10.9 MB** / unpacked **20.4 MB** /
+cryptography wheel **32.4 MB**. The PyO3 runner binary itself is
~0.2 MB. Third-party wheels with native `.pyd`s (like cryptography)
work unchanged — ship them inside the carried runtime.

Gotchas recorded for the real integration: the embedded interpreter
does not finalize on drop — flush stdio explicitly after running
scripts (see the runner example); `eval` takes expressions only; pip
against the carried runtime needs `--target` plus the `._pth` tweak
(or a python-build-standalone runtime, which ships pip).

## WebView2 fixed-version embedding — measured

Question: can the installer carry the WebView2 engine itself, powering
both its own UI and the deployed app? **Mechanically yes — proven end
to end**; the cost is the payload.

- v151.0.4129.101 x64 fixed-version cab: **307,241,094 bytes ≈ 293 MB**
  compressed, **661.1 MB unpacked**.
- The demo shell ran against the unpacked carried runtime
  (`WEBVIEW2_BROWSER_EXECUTABLE_FOLDER`, already the first probe in
  `webview2_available`): UI rendered (offline screenshot verified) and
  **all six renderer processes came from the carried folder**, not the
  system Evergreen install.
- Verdict: feasible but heavy. A single-file installer grows by ~300 MB
  compressed; compare `evergreen-installer` (~127 MB offline installer,
  system-wide, needs elevation once). Fixed-version makes sense only
  for air-gapped/locked-down fleets or strict version pinning — the
  existing `fixed-version` strategy in the manifest describes exactly
  this deployment; the egui fallback remains the zero-cost floor for
  machines with nothing at all.


## Embedded Python (researched, feasible)

**Verdict: yes — Rust can embed a small CPython, cleanly.** The proof is
`tests/scripting_python.rs` behind the opt-in `python-probe` feature:
[PyO3](https://pyo3.rs) with `auto-initialize` embeds the interpreter
into the process, evaluates real Python with the stdlib, calls custom
Rust functions, and maps Python exceptions to Rust errors. The feature
is never in the default build; on CI only the Windows leg (which
resolves `--all-features` against a preinstalled CPython) exercises it.

Shipping options for a self-contained installer, mirroring the WebView2
strategy table:

| Option | Carries | Notes |
| --- | --- | --- |
| `system` (default) | nothing | duckscript `process` commands may invoke an installed python; degrades gracefully when absent |
| `embeddable` | Windows embeddable package (~12–16 MB) | Official `python-3.x.x-embed-amd64.zip`: `python3xx.dll` + stdlib zip + `._pth`, no admin, no registry — a private runtime exactly like fixed-version WebView2 |
| `standalone` | python-build-standalone (~30–60 MB) | [Astral-stewarded](https://astral.sh/blog/python-build-standalone) distributions (what `uv` ships); cross-platform, pinned, full-featured; overkill unless pip/native deps are needed |

Sketch:

```toml
[package.metadata.shun.script.python]    # optional heavy escape hatch
type = "embeddable"                      # system | embeddable | standalone
```

Rejected/deferred alternatives:

- **RustPython** (MIT, pure Rust) — self-declared not production-ready,
  stdlib gaps, no C-extension modules; attractive someday, not for
  installers today.
- **PyOxidizer / `pyembed`** — higher-level embedding, but the project
  is in maintenance mode; PyO3 alone is enough here.

Open questions before wiring it in:

- Size budget: is +12–16 MB on the artifact acceptable for the products
  that opt in? (Embedding is opt-in per manifest, so the default
  artifact stays small.)
- Version coupling: PyO3 links the build host's CPython; the shipped
  runtime must match. Pin by building against the very distribution we
  ship (`PYO3_PYTHON` → unpacked embeddable/standalone dir).
- Isolation: embedded interpreter in-process (single-file installer UX)
  vs subprocess (simpler crash isolation) — or both, chosen per hook.
- Which hooks may escalate to Python at all (prepare only, or also
  post-install repair paths?).
