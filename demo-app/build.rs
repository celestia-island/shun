//! Plain tauri-build entry for the demo application, with the same
//! comctl32 v6 + per-monitor-DPI application manifest the installer shell
//! embeds: without it Windows loads comctl32 v5 and the process dies at
//! startup with STATUS_ENTRYPOINT_NOT_FOUND (tao imports
//! TaskDialogIndirect).

fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(
        tauri_build::WindowsAttributes::new().app_manifest(include_str!("app.manifest")),
    ))
    .expect("tauri-build failed");
}
