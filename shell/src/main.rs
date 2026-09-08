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
    fn install_target(&self) -> Result<shun::config::InstallConfig, String> {
        self.config
            .targets
            .iter()
            .find_map(|t| match t {
                TargetConfig::Install(install) => Some(install.clone()),
                _ => None,
            })
            .ok_or_else(|| "此配置未声明安装目标".to_string())
    }

    /// The wizard's answers to the `ask` policies (desktop checkbox,
    /// install scope).
    fn install_context(
        &self,
        mode: &str,
        dir: &str,
        answers: shun::targets::install::WizardAnswers,
    ) -> Result<InstallContext, String> {
        let install = self.install_target()?;
        let mut ctx = InstallContext::new(
            self.config.product.name.clone(),
            self.config.product.version.clone(),
            PathBuf::from(dir),
            mode == "portable",
        );
        ctx.publisher = self.config.product.publisher.clone();
        ctx.main_exe = install.main_exe.clone();
        ctx.apply_config(&install, answers);
        Ok(ctx)
    }
}

#[derive(Serialize)]
struct ShellView {
    product: shun::config::ProductIdentity,
    /// Delivery-mode ids the UI should offer, in order.
    modes: Vec<String>,
    /// The resolved wizard pipeline (ordered steps with inlined bodies).
    steps: Vec<shun::config::ResolvedStep>,
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
        // The wizard pipeline, resolved at build time (markdown bodies
        // inlined into shun-steps.json next to the config).
        steps: serde_json::from_str(include_str!(concat!(env!("OUT_DIR"), "/shun-steps.json")))
            .expect("embedded wizard pipeline parses"),
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
    // The wizard's answers to the `ask` policies; absent values (older
    // front-ends) resolve to the defaults (desktop on, per-user).
    desktop: Option<bool>,
    machine: Option<bool>,
) -> Result<(), String> {
    let dir = dir.trim().trim_end_matches('\\').to_string();
    if dir.is_empty() {
        return Err("安装目录不能为空".into());
    }
    let answers = shun::targets::install::WizardAnswers {
        desktop_shortcut: desktop.unwrap_or(true),
        machine: machine.unwrap_or(false),
    };
    let ctx = state.install_context(&mode, &dir, answers)?;

    // Machine scope needs an elevated token; re-launch this binary under
    // UAC carrying the resolved answers, headlessly.
    ensure_elevated_for(&ctx, &mode, &dir, answers, false)?;
    let _ = &app;

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
    let answers = shun::targets::install::WizardAnswers::defaults();
    let ctx = state.install_context(&mode, &dir, answers)?;
    ensure_elevated_for(&ctx, &mode, &dir, answers, true)?;
    uninstall(&ctx, &WindowsRegistration).map_err(|e| e.to_string())
}

/// When the resolved install scope is machine-wide and the current
/// process is not elevated, re-launches this executable under UAC with
/// the same choices (headless) and exits. A declined UAC prompt surfaces
/// as an error the wizard can show. No-op for per-user installs.
pub(crate) fn ensure_elevated_for(
    ctx: &InstallContext,
    mode: &str,
    dir: &str,
    answers: shun::targets::install::WizardAnswers,
    uninstalling: bool,
) -> Result<(), String> {
    use shun::targets::install::InstallScope;
    if ctx.scope != InstallScope::Machine || shun::targets::elevate::is_elevated() {
        return Ok(());
    }
    let mut args = format!("--silent --mode={mode} --dir=\"{}\"", dir.trim());
    if !answers.desktop_shortcut {
        args.push_str(" --no-desktop");
    }
    args.push_str(" --scope=machine");
    if uninstalling {
        args.push_str(" --uninstall");
    }
    match shun::targets::elevate::relaunch_elevated(&args) {
        Ok(true) => {
            // The elevated copy carries on; this (unelevated) instance is
            // done. Give the front-end a beat to flush, then exit.
            std::thread::sleep(std::time::Duration::from_millis(500));
            std::process::exit(0);
        }
        Ok(false) => Err("需要管理员权限才能进行机器级安装（UAC 被拒绝）/ \
                          machine-wide install needs the UAC prompt to be accepted"
            .into()),
        Err(e) => Err(e.to_string()),
    }
}

/// Automated-install arguments (headless mode): `--silent` skips the
/// UI and runs the flow headlessly with `--mode=local|portable`,
/// `--dir=<path>`, `--scope=user|machine`, `--desktop`/`--no-desktop`
/// and an optional `--uninstall`. The host application is
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
    let mut desktop: Option<bool> = None;
    let mut machine: Option<bool> = None;
    for arg in args {
        if let Some(value) = arg.strip_prefix("--mode=") {
            mode = value.to_string();
        } else if let Some(value) = arg.strip_prefix("--dir=") {
            dir = Some(PathBuf::from(value.trim_matches('"')));
        } else if arg == "--uninstall" || arg == "/uninstall" {
            // `/uninstall` is what the ARP UninstallString passes; the
            // double dash spelling stays for script symmetry.
            uninstall_mode = true;
        } else if arg == "--desktop" {
            desktop = Some(true);
        } else if arg == "--no-desktop" {
            desktop = Some(false);
        } else if arg == "--scope=machine" {
            machine = Some(true);
        } else if arg == "--scope=user" {
            machine = Some(false);
        }
    }
    let product = config.product.name.clone();
    let dir = dir.unwrap_or_else(|| match mode.as_str() {
        "portable" => exe_dir()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
            .join(format!("{product}-portable")),
        _ => match machine {
            // Machine-wide installs land in Program Files by default.
            Some(true) => std::env::var_os("ProgramFiles")
                .map(|root| PathBuf::from(root).join(&product))
                .unwrap_or_else(|| local_appdata().join(&product)),
            _ => local_appdata().join(&product),
        },
    });
    let mut ctx = InstallContext::new(
        product,
        config.product.version.clone(),
        dir,
        mode == "portable",
    );
    ctx.publisher = config.product.publisher.clone();
    ctx.main_exe = config.targets.iter().find_map(|t| match t {
        TargetConfig::Install(install) => install.main_exe.clone(),
        _ => None,
    });
    let answers = shun::targets::install::WizardAnswers {
        desktop_shortcut: desktop.unwrap_or(true),
        machine: machine.unwrap_or(false),
    };
    if let Some(install) = config.targets.iter().find_map(|t| match t {
        TargetConfig::Install(install) => Some(install),
        _ => None,
    }) {
        ctx.apply_config(install, answers);
    }
    // The elevation gate for machine scope: re-launch under UAC when the
    // answers (or a `machine` policy) resolved machine-wide and this
    // process is not elevated.
    let dir_display = ctx.install_dir.display().to_string();
    ensure_elevated_for(&ctx, &mode, &dir_display, answers, uninstall_mode)?;
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

/// Stages the payload's fixed-version WebView2 runtime into the shun
/// cache and points the WebView2 loader at it. Idempotent: hash-aware
/// staging reuses an already-cached copy, so this costs nothing after
/// the first run. On any failure the caller's normal detection applies
/// (system runtime, or the egui fallback).
fn bootstrap_fixed_webview2(config: &ShunConfig, payload: &ArchivePayload) {
    let runtime_path = match config.webview2.as_ref() {
        Some(shun::config::Webview2Strategy::FixedVersion { path }) => PathBuf::from(path),
        _ => return,
    };
    let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) else {
        eprintln!("shun: no LOCALAPPDATA to cache the fixed-version runtime in");
        return;
    };
    let cache = local
        .join("shun")
        .join(&config.product.name)
        .join("webview2");
    if let Err(err) = payload.extract_prefix(&cache, &runtime_path, &mut |_| {}) {
        eprintln!("shun: staging the fixed-version runtime failed ({err}); falling back");
        return;
    }
    // Safety: single-threaded bootstrap before any UI or worker thread
    // exists, and the loader must find the variable before WebView2 is
    // first initialized.
    unsafe {
        std::env::set_var(
            "WEBVIEW2_BROWSER_EXECUTABLE_FOLDER",
            cache.join(&runtime_path),
        );
    }
}

/// The resolved wizard pipeline, embedded at build time (markdown
/// bodies inlined).
const SHUN_STEPS_JSON: &str = include_str!(concat!(env!("OUT_DIR"), "/shun-steps.json"));

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

    // One-copy WebView2 bootstrap: when the manifest ships a
    // fixed-version runtime INSIDE the payload, stage just that subtree
    // into a cache dir and run on it. The installer shell and the
    // installed app then share a single embedded copy (hash-aware
    // extraction adopts the staged files instead of rewriting them).
    bootstrap_fixed_webview2(&config, &payload);

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
        fallback::run(
            config,
            payload,
            reason,
            LOGO_KIND.trim(),
            LOGO_BYTES,
            serde_json::from_str(SHUN_STEPS_JSON).expect("embedded wizard pipeline parses"),
        );
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
