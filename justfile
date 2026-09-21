# shun — flow-driven payload delivery runtime (celestia-island).

set shell := ["bash", "-c"]
# Windows: PowerShell (the 5.1 floor ships with every Windows; pwsh 7 is
# NOT assumed). Linewise recipes must stay PS-5.1-safe: no `&&` chains,
# `cd X; cmd` instead of `cd X && cmd`. Bash-only recipes use
# [script('bash')] and need Git Bash (or WSL) when actually run.
set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command", "[Console]::OutputEncoding=[System.Text.Encoding]::UTF8; $PSDefaultParameterValues['*:Encoding']='utf8';"]
set unstable
set lists

# Repo definitions override the shared template's (imported above).
set allow-duplicate-recipes
set allow-duplicate-variables

# Shared celestia-devtools recipes — NOT in git. This justfile references shared
# variables, so the import is REQUIRED. Bootstrap once: celestia-devtools init
# (or `just fetch` if already staged). Refresh after upgrades.
python_cmd := "python3"
import? "./.just/git-bash-interop.just"
import? "./.just/celestia-devtools.just"

# Stage shared celestia-devtools recipes into .just/ (gitignored).
# Source order: explicit URL arg → local pip bundle (offline) → GitHub raw.
# curl honors HTTP_PROXY/HTTPS_PROXY/ALL_PROXY env vars automatically.
fetch URL='':
    {{ if os_family() == "windows" { "python" } else { "python3" } }} -c "import os; os.makedirs('.just', exist_ok=True)"
    {{ if URL != "" { "curl -fsSL " + URL + " -o .just/celestia-devtools.just" } else if which("celestia-devtools") != "" { "celestia-devtools fetch-just" } else { "curl -fsSL https://raw.githubusercontent.com/celestia-island/celestia-devtools/dev/src/celestia_devtools/common.just -o .just/celestia-devtools.just" } }}

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
