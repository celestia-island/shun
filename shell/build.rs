//! Packs the demo payload (`../examples/demo_payload`) at build time so the
//! shell binary embeds it via `include_bytes!` — the single-file installer
//! pattern, exercised for real — and embeds the application manifest
//! (comctl32 v6 + per-monitor DPI). Without the manifest, Windows loads
//! comctl32 v5 and the process dies at startup with
//! STATUS_ENTRYPOINT_NOT_FOUND (tao imports TaskDialogIndirect).

use std::path::Path;

fn main() {
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by cargo");

    let payload_dir = Path::new(&manifest_dir).join("../examples/demo_payload");
    let archive = shun::payload::pack_directory(&payload_dir).expect("demo payload packs cleanly");
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR is set by cargo");
    let out = Path::new(&out_dir).join("shun-demo-payload.shun");
    std::fs::write(&out, archive).expect("write embedded payload");
    println!("cargo:rerun-if-changed=../examples/demo_payload");

    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(
        tauri_build::WindowsAttributes::new().app_manifest(include_str!("app.manifest")),
    ))
    .expect("tauri-build failed");
}
