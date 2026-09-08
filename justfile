# shun — flow-driven payload delivery runtime (celestia-island).

set shell := ["bash", "-c"]
set windows-shell := ["bash.exe", "-c"]
set unstable
set lists

# Shared celestia-devtools recipes — NOT in git. This justfile references shared
# variables, so the import is REQUIRED. Bootstrap once: celestia-devtools init
# (or `just fetch` if already staged). Refresh after upgrades.
python_cmd := "python3"
import? "./.just/git-bash-interop.just"
import? "./.just/celestia-devtools.just"

# Stage shared celestia-devtools recipes into .just/ (gitignored).
# Source order: explicit URL arg → local pip bundle (offline) → GitHub raw.
# curl honors HTTP_PROXY/HTTPS_PROXY/ALL_PROXY env vars automatically.
[script('bash')]
fetch URL='':
    #!/usr/bin/env bash
    set -euo pipefail
    out=.just/celestia-devtools.just
    mkdir -p .just
    if [ -n "{{URL}}" ]; then
      echo "[fetch] {{URL}} -> $out"
      curl -fsSL "{{URL}}" -o "$out"
    elif command -v celestia-devtools >/dev/null 2>&1; then
      src=$(celestia-devtools include-path)
      echo "[fetch] local bundle ($src) -> $out"
      cp "$src" "$out"
    else
      echo "[fetch] github raw -> $out"
      curl -fsSL "https://raw.githubusercontent.com/celestia-island/celestia-devtools/dev/src/celestia_devtools/common.just" -o "$out"
    fi
    echo "[fetch] wrote $out"

default:
    @just --list

# Format all sources.
fmt:
    just fmt-toml
    cargo fmt --all

# Check formatting without writing.
fmt-check:
    cargo fmt --all -- --check

# Type-check all targets and features (format gate included).
check: fmt-check
    cargo check --all-targets --all-features

# Clippy with -D warnings.
clippy:
    cargo clippy --all-targets --all-features -- -D warnings

# Run the Rust unit/integration test suite.
test:
    cargo test --all-features

# Build all features.
build:
    cargo build --all-features

# One-shot local gate: fmt-check + clippy + cargo tests.
ci:
    just fmt-check
    just clippy
    just test

# Demo: build the ShunDemo application and stage it into the demo
# payload (manifest-driven; `shun stage` derives everything from
# demo-app/Cargo.toml).
demo-payload:
    cargo run --quiet -p shun -- stage --manifest demo-app/Cargo.toml

# Demo: the comprehensive one — stage the demo app, build and run the
# installer shell over it (hikari UI; --fallback forces the egui shell).
demo ARGS='':
    just demo-payload
    cargo run --release -p shun_demo_shell -- {{ARGS}}

# Demo (CLI): generate <Product>.shun and run a local install.
demo-install ARGS='':
    cargo run --example demo_install -- {{ARGS}}

# Demo (CLI): uninstall the demo install.
demo-uninstall ARGS='':
    cargo run --example demo_install -- --uninstall {{ARGS}}

# Demo: enumerate flash-candidate devices.
demo-flash:
    cargo run --example demo_flash
