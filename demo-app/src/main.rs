//! ShunDemo — the application delivered by the shun demo flow.
//!
//! The UI is deliberately small but real: a Tauri 2 window with a sample
//! interface that proves the delivery end to end. The interesting part is
//! [`delivery_mode`]: it reads the `.shun-portable` marker the installer
//! drops next to the payload, so the app can tell which delivery mode put
//! it there — that is the whole point of the demo.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

#[tauri::command]
fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Which delivery mode installed this copy: `portable` when the installer
/// dropped a `.shun-portable` marker beside the payload, `local` for a
/// registered install (ARP entry, shortcuts, uninstaller).
#[tauri::command]
fn delivery_mode() -> String {
    let marker = exe_dir().join(".shun-portable");
    if marker.is_file() {
        "portable".into()
    } else {
        "local".into()
    }
}

/// Where this executable lives — the directory the installer extracted.
#[tauri::command]
fn install_root() -> String {
    exe_dir().to_string_lossy().into_owned()
}

fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|parent| parent.to_path_buf()))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            app_version,
            delivery_mode,
            install_root
        ])
        .run(tauri::generate_context!())
        .expect("error while running ShunDemo");
}
