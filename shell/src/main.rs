//! Shun demo shell — a hikari UI over the shun install flow.
//!
//! Everything shown here is generated from the shun configuration declared
//! in this crate's `Cargo.toml` (`[package.metadata.shun]`, resolved at
//! build time and embedded as JSON): product identity, which delivery
//! modes exist, and the payload. The binary embeds the payload at build
//! time (single-file installer pattern) and drives
//! `shun::targets::install` through Tauri commands: local mode performs
//! the direct Windows registration, portable mode drops the `.shun-portable`
//! marker. Progress events stream straight from the flow.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod fallback;
mod screenshot;

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
/// The product logo embedded by build.rs (kind file + bytes) — both UIs
/// render it in their caption bars.
const LOGO_KIND: &str = include_str!(concat!(env!("OUT_DIR"), "/shun-logo-kind.txt"));
const LOGO_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/shun-logo.bin"));

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

/// Automated-install arguments (headless mode): `--silent` skips the
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

    // Offline UI capture: render the UI, save the window content as a
    // PNG, exit. Works for both renderers (webview and egui) via
    // PrintWindow — no desktop automation involved. `--screenshot-delay`
    // overrides the settle time (defaults: 4s webview, 2.5s egui).
    let screenshot = args.iter().find_map(|arg| {
        arg.strip_prefix("--screenshot=")
            .map(std::path::PathBuf::from)
    });
    let screenshot_delay: Option<u64> = args.iter().find_map(|arg| {
        arg.strip_prefix("--screenshot-delay=")
            .and_then(|v| v.parse().ok())
    });
    let delay = screenshot_delay.unwrap_or(4000);

    // UI engine selection: Tauri renders through WebView2 on Windows and
    // there is no alternative engine inside Tauri — when the runtime is
    // missing (or the operator forces it with `--fallback`/`--egui`) the
    // same flow runs through the embedded egui fallback wizard instead.
    // Same embedded config, same payload: one manifest, two renderers.
    let manual_fallback = args.iter().any(|a| a == "--fallback" || a == "--egui");
    if manual_fallback || !webview2_available() {
        let reason = if manual_fallback {
            fallback::FallbackReason::ManualOverride
        } else {
            fallback::FallbackReason::MissingWebview2
        };
        if let Some(path) = screenshot {
            let title = fallback::window_title(&config);
            screenshot::schedule_by_title(title, path, screenshot_delay.unwrap_or(2500));
        }
        fallback::run(config, payload, reason, LOGO_KIND.trim(), LOGO_BYTES);
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
        .setup(move |app| {
            if let Some(path) = &screenshot {
                screenshot::schedule(app.handle().clone(), path.clone(), delay);
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running shun demo shell");
}

/// Detects a usable WebView2 runtime the way the WebView2 loader does:
/// an explicit `WEBVIEW2_BROWSER_EXECUTABLE_FOLDER` (the fixed-version
/// strategy) wins, then the Evergreen EdgeUpdate registry entries
/// (per-machine, per-user). `false` means the Tauri window cannot come
/// up and the egui fallback must take over.
#[cfg(windows)]
fn webview2_available() -> bool {
    if std::env::var_os("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER").is_some() {
        return true;
    }
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

    const CLIENT: &str = r"Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
    let probes = [
        (
            HKEY_LOCAL_MACHINE,
            format!(r"SOFTWARE\WOW6432Node\{CLIENT}"),
        ),
        (HKEY_CURRENT_USER, format!(r"SOFTWARE\{CLIENT}")),
    ];
    probes.iter().any(|(root, path)| {
        RegKey::predef(*root)
            .open_subkey(path)
            .and_then(|key| key.get_value::<String, _>("pv"))
            .is_ok_and(|version| !version.is_empty() && version != "0.0.0.0")
    })
}

/// WebView2 is a Windows-only concern; other platforms always have their
/// system webview available and never auto-fall back (`--fallback` still
/// works manually).
#[cfg(not(windows))]
fn webview2_available() -> bool {
    true
}
