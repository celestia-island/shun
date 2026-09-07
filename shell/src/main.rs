//! Shun demo shell — a hikari UI over the shun install flow.
//!
//! The binary embeds the demo payload at build time (single-file installer
//! pattern) and drives [`shun::targets::install`] through a Tauri command:
//! local mode performs the NSIS-like registration, portable mode drops the
//! `.shun-portable` marker. Progress events stream straight from the flow.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use serde::Serialize;
use shun::flow::{Flow, FlowEvent};
use shun::payload::ArchivePayload;
use shun::targets::install::{InstallContext, InstallFlow, WindowsRegistration, uninstall};
use tauri::{Emitter, State};

const PRODUCT: &str = "ShunDemo";

/// ArchivePayload is not Sync (HashMap in a Mutex-managed state is), so the
/// payload is stored once and cloned per install run.
struct PayloadState(ArchivePayload);

#[derive(Serialize)]
struct DirDefaults {
    dir: String,
}

fn local_appdata() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
}

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()?
        .parent()
        .map(|d| d.to_path_buf())
}

fn install_dir_for(mode: &str) -> PathBuf {
    if mode == "portable" {
        exe_dir()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
            .join(format!("{PRODUCT}-portable"))
    } else {
        local_appdata().join(PRODUCT)
    }
}

#[tauri::command]
fn default_dir(mode: String) -> DirDefaults {
    DirDefaults {
        dir: install_dir_for(&mode).to_string_lossy().into_owned(),
    }
}

fn emit_progress(app: &tauri::AppHandle, event: &FlowEvent) {
    let _ = app.emit("install-progress", event);
}

#[tauri::command]
fn start_install(
    app: tauri::AppHandle,
    state: State<'_, PayloadState>,
    mode: String,
    dir: String,
) -> Result<(), String> {
    let dir = dir.trim().trim_end_matches('\\').to_string();
    if dir.is_empty() {
        return Err("安装目录不能为空".into());
    }
    let ctx = InstallContext {
        product: PRODUCT.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        publisher: Some("celestia-island".to_string()),
        install_dir: PathBuf::from(&dir),
        main_exe: Some(PathBuf::from("bin/shun-demo.cmd")),
        portable: mode == "portable",
        estimated_size_kb: 0,
    };

    let payload = state.0.clone();
    let flow = InstallFlow {
        payload: &payload,
        registration: &WindowsRegistration,
        ctx,
    };
    flow.run(&mut |event| emit_progress(&app, &event))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn uninstall_demo(mode: String, dir: String) -> Result<(), String> {
    let dir = dir.trim().trim_end_matches('\\').to_string();
    if dir.is_empty() {
        return Err("安装目录不能为空".into());
    }
    let ctx = InstallContext {
        product: PRODUCT.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        publisher: None,
        install_dir: PathBuf::from(&dir),
        main_exe: Some(PathBuf::from("bin/shun-demo.cmd")),
        portable: mode == "portable",
        estimated_size_kb: 0,
    };
    uninstall(&ctx, &WindowsRegistration).map_err(|e| e.to_string())
}

fn main() {
    let embedded = include_bytes!(concat!(env!("OUT_DIR"), "/shun-demo-payload.shun"));
    let payload = ArchivePayload::from_bytes(embedded).expect("embedded payload decodes");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(PayloadState(payload))
        .invoke_handler(tauri::generate_handler![
            default_dir,
            start_install,
            uninstall_demo
        ])
        .run(tauri::generate_context!())
        .expect("error while running shun demo shell");
}
