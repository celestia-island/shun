//! Shun demo shell — a hikari UI over the shun install flow.
//!
//! Everything shown here is generated from the shun configuration declared
//! in this crate's `Cargo.toml` (`[package.metadata.shun]`, resolved at
//! build time and embedded as JSON): product identity, which delivery
//! modes exist, and the payload. The binary embeds the payload at build
//! time (single-file installer pattern) and drives
//! `shun::targets::install` through Tauri commands: local mode performs
//! the NSIS-like registration, portable mode drops the `.shun-portable`
//! marker. Progress events stream straight from the flow.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use serde::Serialize;
use shun::config::{ShunConfig, TargetConfig};
use shun::flow::{Flow, FlowEvent};
use shun::payload::ArchivePayload;
use shun::targets::install::{InstallContext, InstallFlow, WindowsRegistration, uninstall};
use tauri::{Emitter, State};

/// The resolved configuration, embedded by build.rs.
const SHUN_CONFIG_JSON: &str = include_str!(concat!(env!("OUT_DIR"), "/shun-config.json"));
/// The payload archive packed by build.rs from `metadata.shun.payload`.
const EMBEDDED_PAYLOAD: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/shun-demo-payload.shun"));

/// State shared by the commands: the resolved config and the payload
/// (cloned per install run).
struct AppState {
    config: ShunConfig,
    payload: ArchivePayload,
}

impl AppState {
    fn install_context(&self, mode: &str, dir: &str) -> Result<InstallContext, String> {
        let install = self
            .config
            .targets
            .iter()
            .find_map(|t| match t {
                TargetConfig::Install(install) => Some(install.clone()),
                _ => None,
            })
            .ok_or_else(|| "此配置未声明安装目标".to_string())?;

        Ok(InstallContext {
            product: self.config.product.name.clone(),
            version: self.config.product.version.clone(),
            publisher: self.config.product.publisher.clone(),
            install_dir: PathBuf::from(dir),
            main_exe: install.main_exe.clone(),
            portable: mode == "portable",
            estimated_size_kb: 0,
        })
    }
}

#[derive(Serialize)]
struct ShellView {
    product: shun::config::ProductIdentity,
    /// Delivery-mode ids the UI should offer, in order.
    modes: Vec<String>,
    /// Step indicator placement for the wizard layout.
    timeline: Option<shun::config::TimelineOrientation>,
    /// Theme knobs for the frontend (mode pin + accent override).
    theme: Option<shun::config::ThemeConfig>,
    /// Configured UI language, `None` = follow the system.
    language: Option<String>,
    /// Whether a flash target is declared (the UI shows it as pending).
    flash: bool,
}

#[tauri::command]
fn get_config(state: State<'_, AppState>) -> ShellView {
    let mut modes = Vec::new();
    for target in &state.config.targets {
        match target {
            TargetConfig::Install(install) => {
                if install.local {
                    modes.push("local".into());
                }
                if install.portable {
                    modes.push("portable".into());
                }
            }
            TargetConfig::Flash(_) => {}
        }
    }
    let flash = state
        .config
        .targets
        .iter()
        .any(|t| matches!(t, TargetConfig::Flash(_)));
    let shell = state.config.shell.clone().unwrap_or_default();
    ShellView {
        product: state.config.product.clone(),
        modes,
        timeline: shell.timeline,
        theme: shell.theme,
        language: shell.language,
        flash,
    }
}

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

fn install_dir_for(state: &AppState, mode: &str) -> PathBuf {
    let product = &state.config.product.name;
    if mode == "portable" {
        exe_dir()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
            .join(format!("{product}-portable"))
    } else {
        std::env::var_os("LOCALAPPDATA")
            .map(|local| PathBuf::from(local).join(product))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default().join(product))
    }
}

#[tauri::command]
fn default_dir(state: State<'_, AppState>, mode: String) -> DirDefaults {
    DirDefaults {
        dir: install_dir_for(&state, &mode)
            .to_string_lossy()
            .into_owned(),
    }
}

fn emit_progress(app: &tauri::AppHandle, event: &FlowEvent) {
    let _ = app.emit("install-progress", event);
}

#[tauri::command]
fn start_install(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    mode: String,
    dir: String,
) -> Result<(), String> {
    let dir = dir.trim().trim_end_matches('\\').to_string();
    if dir.is_empty() {
        return Err("安装目录不能为空".into());
    }
    let ctx = state.install_context(&mode, &dir)?;

    let payload = state.payload.clone();
    let flow = InstallFlow {
        payload: &payload,
        registration: &WindowsRegistration,
        ctx,
    };
    flow.run(&mut |event| emit_progress(&app, &event))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn uninstall_demo(state: State<'_, AppState>, mode: String, dir: String) -> Result<(), String> {
    let dir = dir.trim().trim_end_matches('\\').to_string();
    if dir.is_empty() {
        return Err("安装目录不能为空".into());
    }
    let ctx = state.install_context(&mode, &dir)?;
    uninstall(&ctx, &WindowsRegistration).map_err(|e| e.to_string())
}

/// Automated-install arguments (the NSIS `/S` analog): `--silent` skips the
/// UI and runs the flow headlessly with `--mode=local|portable`,
/// `--dir=<path>` and an optional `--uninstall`. The host application is
/// expected to exit cleanly BEFORE invoking the installer with these flags
/// during an update.
fn run_headless(
    args: &[String],
    config: &ShunConfig,
    payload: &ArchivePayload,
) -> Result<(), String> {
    let mut mode = "local".to_string();
    let mut dir: Option<PathBuf> = None;
    let mut uninstall_mode = false;
    for arg in args {
        if let Some(value) = arg.strip_prefix("--mode=") {
            mode = value.to_string();
        } else if let Some(value) = arg.strip_prefix("--dir=") {
            dir = Some(PathBuf::from(value));
        } else if arg == "--uninstall" {
            uninstall_mode = true;
        }
    }
    let product = config.product.name.clone();
    let dir = dir.unwrap_or_else(|| match mode.as_str() {
        "portable" => exe_dir()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
            .join(format!("{product}-portable")),
        _ => local_appdata().join(&product),
    });
    let ctx = InstallContext {
        product,
        version: config.product.version.clone(),
        publisher: config.product.publisher.clone(),
        install_dir: dir,
        main_exe: config.targets.iter().find_map(|t| match t {
            TargetConfig::Install(install) => install.main_exe.clone(),
            _ => None,
        }),
        portable: mode == "portable",
        estimated_size_kb: 0,
    };
    if uninstall_mode {
        uninstall(&ctx, &WindowsRegistration).map_err(|e| e.to_string())?;
        println!("shun: uninstalled {}", ctx.install_dir.display());
        return Ok(());
    }
    let flow = InstallFlow {
        payload,
        registration: &WindowsRegistration,
        ctx,
    };
    flow.run(&mut |event| println!("{event:?}"))
        .map_err(|e| e.to_string())?;
    println!("shun: install complete");
    Ok(())
}

fn main() {
    let config: ShunConfig =
        serde_json::from_str(SHUN_CONFIG_JSON).expect("embedded config decodes");
    let payload = ArchivePayload::from_bytes(EMBEDDED_PAYLOAD).expect("embedded payload decodes");

    let args: Vec<String> = std::env::args().skip(1).collect();
    let silent = args.iter().any(|a| a == "--silent" || a == "/S");
    if silent {
        if let Err(err) = run_headless(&args, &config, &payload) {
            eprintln!("shun: {err}");
            std::process::exit(1);
        }
        return;
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState { config, payload })
        .invoke_handler(tauri::generate_handler![
            get_config,
            default_dir,
            start_install,
            uninstall_demo
        ])
        .run(tauri::generate_context!())
        .expect("error while running shun demo shell");
}
