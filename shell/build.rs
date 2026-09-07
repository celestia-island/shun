//! Build-time inputs for the demo shell, resolved from a delivery
//! manifest:
//!
//! - the shun configuration (loaded from `[package.metadata.shun]` via
//!   `ShunConfig::from_any` — or from the path in `SHUN_MANIFEST` when the
//!   `shun build` CLI drives an external application) is written to
//!   `OUT_DIR/shun-config.json` and embedded by the runtime; and
//! - the payload directory declared there is packed into
//!   `OUT_DIR/shun-demo-payload.shun` and embedded via `include_bytes!` —
//!   the single-file installer pattern, exercised for real.
//!
//! The application manifest (comctl32 v6 + per-monitor DPI) is also
//! embedded here: without it, Windows loads comctl32 v5 and the process
//! dies at startup with STATUS_ENTRYPOINT_NOT_FOUND (tao imports
//! TaskDialogIndirect).

use std::path::{Path, PathBuf};

fn main() {
    let own_manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by cargo");

    // Delivery manifest resolution: the `shun build` CLI points
    // SHUN_MANIFEST at the target application's manifest; a plain cargo
    // build of the shell falls back to its own Cargo.toml.
    let manifest_path = std::env::var("SHUN_MANIFEST")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(&own_manifest_dir).join("Cargo.toml"));
    let manifest_dir = manifest_path
        .parent()
        .map(|d| d.to_path_buf())
        .unwrap_or_else(|| Path::new(&own_manifest_dir).to_path_buf());
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR is set by cargo");
    let out_dir = Path::new(&out_dir);

    // 1. Resolve the shun configuration (metadata.shun or standalone doc).
    let config = shun::config::ShunConfig::from_any(&manifest_path)
        .expect("shun metadata in the delivery manifest parses");
    let config_json = serde_json::to_vec_pretty(&config).expect("config serializes");
    std::fs::write(out_dir.join("shun-config.json"), config_json).expect("write embedded config");
    println!("cargo:rerun-if-changed={}", manifest_path.display());

    // 2. Pack the payload directory declared in the configuration (paths in
    //    the manifest are relative to it).
    let payload_relative = config
        .payload
        .clone()
        .expect("metadata.shun.payload declares the payload directory");
    let payload_dir = manifest_dir.join(payload_relative);
    let archive = shun::payload::pack_directory(&payload_dir).expect("demo payload packs cleanly");
    std::fs::write(out_dir.join("shun-demo-payload.shun"), archive)
        .expect("write embedded payload");
    println!("cargo:rerun-if-changed={}", payload_dir.display());

    // 3. Embed the application manifest (comctl32 v6 + per-monitor DPI).
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(
        tauri_build::WindowsAttributes::new().app_manifest(include_str!("app.manifest")),
    ))
    .expect("tauri-build failed");
}
