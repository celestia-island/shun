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
// The collapsible install-output terminal (same-origin with the hikari
// web terminal component).
mod terminal;
// Offline UI capture (PrintWindow) — Windows-only; the flag is parsed
// everywhere but ignored where the API does not exist.
#[cfg(windows)]
mod screenshot;

use std::path::{Path, PathBuf};

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
/// The wizard pipeline resolved once without a locale (`shun-steps.json`,
/// written by build.rs) — the first-paint steps and their back-compat
/// license documents.
const SHUN_STEPS_JSON: &str = include_str!(concat!(env!("OUT_DIR"), "/shun-steps.json"));
/// The license documents resolved per shell locale (`shun-license-docs.json`,
/// written by build.rs) — the license step re-picks from this map whenever
/// the first-step language selector changes the wizard language.
const SHUN_LICENSE_DOCS_JSON: &str =
    include_str!(concat!(env!("OUT_DIR"), "/shun-license-docs.json"));
/// The product logo embedded by build.rs (kind file + bytes) — both UIs
/// render it in their caption bars.
const LOGO_KIND: &str = include_str!(concat!(env!("OUT_DIR"), "/shun-logo-kind.txt"));
const LOGO_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/shun-logo.bin"));

/// The per-locale license documents, keyed by locale (`en`, `zh-Hans`,
/// ...). Empty when the config declares no license at all.
type LicenseDocs = std::collections::BTreeMap<String, Vec<shun::config::ResolvedLicenseDoc>>;

/// Parses the embedded per-locale license document map.
fn license_docs() -> LicenseDocs {
    serde_json::from_str(SHUN_LICENSE_DOCS_JSON).expect("embedded license docs parse")
}

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
    /// License documents resolved per locale — the wizard re-picks from
    /// this map when the first-step language selector changes the UI
    /// language, falling back to `steps[].licenses` for unknown keys.
    license_docs: LicenseDocs,
    /// Step indicator placement for the wizard layout.
    timeline: Option<shun::config::TimelineOrientation>,
    /// Theme knobs for the frontend (mode pin + accent override).
    theme: Option<shun::config::ThemeConfig>,
    /// Configured UI language, `None` = follow the system.
    language: Option<String>,
    /// Terminal log verbosity for the install pane.
    log_level: shun::config::LogVerbosity,
    /// Whether a flash target is declared (the UI shows it as pending).
    flash: bool,
    /// Optional attachments resolved against the payload: entries marked
    /// `included` shipped inside the payload, the rest offer a download.
    attachments: Vec<AttachmentView>,
}

#[derive(Serialize)]
struct AttachmentView {
    key: String,
    title: String,
    included: bool,
    size: Option<u64>,
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
        steps: serde_json::from_str(SHUN_STEPS_JSON).expect("embedded wizard pipeline parses"),
        license_docs: license_docs(),
        timeline: shell.timeline,
        theme: shell.theme,
        language: shell.language,
        log_level: shell.log_level.unwrap_or_default(),
        flash,
        attachments: shun::attachments::resolve(&state.config, &state.payload)
            .into_iter()
            .map(|a| AttachmentView {
                key: a.config.key,
                title: a.config.title,
                included: a.included,
                size: a.config.size,
            })
            .collect(),
    }
}

#[derive(Serialize)]
struct DirDefaults {
    dir: String,
}

/// The per-user application-data root preference state lives under:
/// `%LOCALAPPDATA%` on Windows, the XDG data home on unix
/// (`$XDG_DATA_HOME` when set to a non-empty value, else
/// `$HOME/.local/share`). Both fall back to the working directory when
/// nothing is resolvable — a last resort, never the normal path.
#[cfg(windows)]
pub(crate) fn local_appdata() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
}

/// Non-Windows branch of [`local_appdata`]: the XDG data home (a
/// set-but-empty value counts as unset, per the XDG base-directory
/// spec).
#[cfg(not(windows))]
pub(crate) fn local_appdata() -> PathBuf {
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(data_home);
    }
    match std::env::var_os("HOME") {
        Some(home) if !home.is_empty() => PathBuf::from(home).join(".local").join("share"),
        _ => std::env::current_dir().unwrap_or_default(),
    }
}

// ── Installer preferences (the wizard's remembered language) ────────────
//
// `<data home>/<product>/installer-prefs.json` — per-user, per-product,
// one key today. Portable runs NEVER touch it (the repo promise: a
// portable copy writes no system state), so the choice stays in memory
// for that run. The helpers take the appdata root as a parameter so
// tests can point them at a scratch directory.

/// The on-disk shape of `installer-prefs.json`.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct InstallerPrefs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) language: Option<String>,
}

/// The prefs file location for a product, under [`local_appdata`].
pub(crate) fn prefs_path(root: &Path, product: &str) -> PathBuf {
    root.join(product).join("installer-prefs.json")
}

/// Reads the prefs file; a missing or unreadable file means defaults —
/// preference state is best-effort and never blocks the wizard.
pub(crate) fn load_prefs(root: &Path, product: &str) -> InstallerPrefs {
    std::fs::read(prefs_path(root, product))
        .ok()
        .and_then(|raw| serde_json::from_slice(&raw).ok())
        .unwrap_or_default()
}

/// Writes the prefs file (creating the product folder).
pub(crate) fn save_prefs(root: &Path, product: &str, prefs: &InstallerPrefs) -> Result<(), String> {
    let path = prefs_path(root, product);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_vec_pretty(prefs).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

/// Remembers the wizard language for the next run. A portable run keeps
/// the choice in memory only — nothing is written, and the return value
/// says whether the preference was persisted.
pub(crate) fn remember_language(
    root: &Path,
    product: &str,
    language: String,
    portable: bool,
) -> Result<bool, String> {
    if portable {
        return Ok(false);
    }
    save_prefs(
        root,
        product,
        &InstallerPrefs {
            language: Some(language),
        },
    )?;
    Ok(true)
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

/// The wizard language remembered from a previous run (the
/// `installer-prefs.json` per-user file), if any. The frontend seeds the
/// first-step language selector with it, falling back to its own
/// system-locale resolution.
#[tauri::command]
fn get_saved_language(state: State<'_, AppState>) -> Option<String> {
    load_prefs(&local_appdata(), &state.config.product.name).language
}

/// Remembers the wizard language for the next run. Skipped when the
/// wizard runs portable — a portable copy writes no system state, so
/// the choice stays in memory for that run only.
#[tauri::command]
fn save_language(
    state: State<'_, AppState>,
    language: String,
    // The selected delivery mode decides portability; absent (older
    // front-ends) means not portable.
    portable: Option<bool>,
) -> Result<(), String> {
    remember_language(
        &local_appdata(),
        &state.config.product.name,
        language,
        portable.unwrap_or(false),
    )
    .map(|_| ())
}

/// Pads one folder level under a bare filesystem root target (a picked
/// drive like `D:\`) so the payload never lands directly on the root —
/// the wizard rewrites the path box with the result the moment a root
/// is picked or typed. The install itself re-applies the same guard.
#[tauri::command]
fn nest_root_dir(state: State<'_, AppState>, dir: String) -> DirDefaults {
    let install = state.install_target().ok();
    DirDefaults {
        dir: shun::targets::install::nest_root_dir(
            Path::new(dir.trim()),
            &state.config.product.name,
            install.as_ref().and_then(|i| i.root_dir_folder.as_deref()),
        )
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
    // The wizard language picked on the first step; reaches payload
    // scripts as `SHUN_LANGUAGE` and the on-disk install manifest.
    language: Option<String>,
) -> Result<(), String> {
    let dir = dir.trim().trim_end_matches('\\').to_string();
    if dir.is_empty() {
        return Err("安装目录不能为空".into());
    }
    let answers = shun::targets::install::WizardAnswers {
        desktop_shortcut: desktop.unwrap_or(true),
        start_menu_shortcut: true,
        machine: machine.unwrap_or(false),
        // No done-page launch toggle in the demo shell yet: the answer is
        // the default-checked one, and nothing calls
        // `shun::targets::install::launch` here yet.
        launch_after_install: true,
    };
    let mut ctx = state.install_context(&mode, &dir, answers)?;
    ctx.language = language;

    // Machine scope needs an elevated token; re-launch this binary under
    // UAC carrying the resolved answers, headlessly.
    ensure_elevated_for(&ctx, &mode, &dir, answers, false)?;

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

/// Streams a declared (and not already bundled) attachment into the
/// install directory, streaming progress through `install-progress`.
#[tauri::command]
fn download_attachment(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    key: String,
    dir: String,
) -> Result<(), String> {
    let resolved = shun::attachments::resolve(&state.config, &state.payload);
    let attachment = resolved
        .iter()
        .find(|a| a.config.key == key)
        .ok_or_else(|| format!("未知附件：{key}"))?;
    if attachment.included {
        return Ok(());
    }
    let dir = dir.trim().trim_end_matches('\\').to_string();
    if dir.is_empty() {
        return Err("安装目录不能为空".into());
    }
    // Same root-drive guard as the install itself: an attachment never
    // streams onto a bare drive root either.
    let install = state.install_target().ok();
    let dir = shun::targets::install::nest_root_dir(
        Path::new(&dir),
        &state.config.product.name,
        install.as_ref().and_then(|i| i.root_dir_folder.as_deref()),
    );
    shun::attachments::download(&attachment.config, &dir, &mut |event| {
        emit_progress(&app, &event)
    })
    .map_err(|e| e.to_string())
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
    // Carry the wizard language through the elevation relaunch so the
    // headless copy still exports it to scripts and records it.
    if let Some(language) = &ctx.language {
        args.push_str(&format!(" --language={language}"));
    }
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
/// `--dir=<path>`, `--scope=user|machine`, `--desktop`/`--no-desktop`,
/// an optional `--language=<locale>` (the wizard language, carried
/// through the elevation relaunch) and an optional `--uninstall`. The
/// host application is expected to exit cleanly BEFORE invoking the
/// installer with these flags during an update.
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
    let mut language: Option<String> = None;
    for arg in args {
        if let Some(value) = arg.strip_prefix("--mode=") {
            mode = value.to_string();
        } else if let Some(value) = arg.strip_prefix("--dir=") {
            dir = Some(PathBuf::from(value.trim_matches('"')));
        } else if let Some(value) = arg.strip_prefix("--language=") {
            language = Some(value.trim_matches('"').to_string());
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
    let dir = match dir {
        Some(dir) => Some(dir),
        // The ARP UninstallString (`"...uninstall.exe" /uninstall`) carries
        // no --dir: the uninstaller lives inside the install directory, so
        // self-locate from the running executable instead of falling back
        // to the default install location (which a custom --dir install
        // never populated — uninstalling it would be a no-op that leaves
        // the real installation behind).
        None if uninstall_mode => exe_dir(),
        None => Some(match mode.as_str() {
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
        }),
    };
    let dir = dir.unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let mut ctx = InstallContext::new(
        product,
        config.product.version.clone(),
        dir,
        mode == "portable",
    );
    ctx.publisher = config.product.publisher.clone();
    ctx.language = language;
    ctx.main_exe = config.targets.iter().find_map(|t| match t {
        TargetConfig::Install(install) => install.main_exe.clone(),
        _ => None,
    });
    let answers = shun::targets::install::WizardAnswers {
        desktop_shortcut: desktop.unwrap_or(true),
        start_menu_shortcut: true,
        machine: machine.unwrap_or(false),
        // No done-page launch toggle in the demo shell yet (the answer is
        // the default-checked one; nothing calls `launch` here).
        launch_after_install: true,
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
    flow.run(&mut |event| match &event {
        // Structured logs render as terminal lines; the remaining events
        // keep their compact debug form.
        FlowEvent::Log { record } => {
            if let Some(line) = headless_log_line(record) {
                println!("{line}");
            }
        }
        _ => println!("{event:?}"),
    })
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

/// Renders one structured log record for a headless console (English —
/// the console has no locale negotiation).
fn headless_log_line(record: &shun::flow::FlowLog) -> Option<String> {
    use shun::flow::FlowLog;
    let line = match record {
        FlowLog::FileWrite { path } => format!("write {}", path.display()),
        FlowLog::FileReuse { path } => format!("reuse {}", path.display()),
        FlowLog::ScriptBegin { name } => format!("» running script {name}"),
        FlowLog::ScriptLine { line, .. } => format!("  {line}"),
        FlowLog::CommandDone { command } => format!("✓ {command}"),
        _ => return None,
    };
    Some(line)
}

fn main() {
    let config: ShunConfig =
        serde_json::from_str(SHUN_CONFIG_JSON).expect("embedded config decodes");
    let payload = ArchivePayload::from_bytes(EMBEDDED_PAYLOAD).expect("embedded payload decodes");

    let args: Vec<String> = std::env::args().skip(1).collect();
    // Headless entry points: explicit `--silent` runs, and the ARP
    // `UninstallString` (`...uninstall.exe" /uninstall`) which must
    // uninstall without opening the wizard.
    let silent = args.iter().any(|a| a == "--silent" || a == "/S");
    let uninstalling = args.iter().any(|a| a == "--uninstall" || a == "/uninstall");
    if silent || uninstalling {
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
            #[cfg(windows)]
            {
                let title = fallback::window_title(&config);
                screenshot::schedule_by_title(title, path, screenshot_delay.unwrap_or(2500));
            }
            #[cfg(not(windows))]
            {
                let _ = path;
                eprintln!("shun: --screenshot is windows-only; ignoring");
            }
        }
        fallback::run(
            config,
            payload,
            reason,
            LOGO_KIND.trim(),
            LOGO_BYTES,
            serde_json::from_str(SHUN_STEPS_JSON).expect("embedded wizard pipeline parses"),
            license_docs(),
        );
        return;
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState { config, payload })
        .invoke_handler(tauri::generate_handler![
            get_config,
            default_dir,
            nest_root_dir,
            get_saved_language,
            save_language,
            start_install,
            uninstall_demo,
            download_attachment
        ])
        .setup(move |app| {
            #[cfg(windows)]
            if let Some(path) = &screenshot {
                screenshot::schedule(app.handle().clone(), path.clone(), delay);
            }
            #[cfg(not(windows))]
            let _ = (app, &screenshot, delay);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch appdata root: the prefs helpers take the root as a
    /// parameter, so tests never touch the real `%LOCALAPPDATA%`.
    fn scratch_root(tag: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("shun-prefs-test-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    #[test]
    fn installer_prefs_roundtrip_and_portable_skip() {
        let root = scratch_root("roundtrip");
        let product = "ShunDemo";

        // Nothing saved yet: the defaults carry no language.
        assert_eq!(load_prefs(&root, product).language, None);

        // A remembered choice reads back from the same file the wizard
        // writes (`installer-prefs.json` under the product folder).
        assert!(
            remember_language(&root, product, "zh-Hans".into(), false).expect("prefs saved"),
            "a non-portable run persists the choice"
        );
        assert_eq!(
            load_prefs(&root, product).language.as_deref(),
            Some("zh-Hans")
        );

        // A portable run keeps the choice in memory only: nothing is
        // written, and an earlier choice survives untouched.
        assert!(
            !remember_language(&root, product, "en".into(), true).expect("portable is a no-op"),
            "a portable run does not persist"
        );
        assert_eq!(
            load_prefs(&root, product).language.as_deref(),
            Some("zh-Hans"),
            "the portable run did not overwrite the saved choice"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn corrupt_prefs_file_degrades_to_defaults() {
        let root = scratch_root("corrupt");
        let product = "ShunDemo";

        // Junk bytes (not JSON): preference state is best-effort, so a
        // corrupt file degrades to the defaults instead of failing the
        // wizard (or panicking the parser).
        let path = prefs_path(&root, product);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, [0xFF, 0xFE]).unwrap();
        assert_eq!(load_prefs(&root, product).language, None);

        // A following save repairs the file and the choice reads back.
        remember_language(&root, product, "ja".into(), false).expect("prefs saved");
        assert_eq!(load_prefs(&root, product).language.as_deref(), Some("ja"));

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Pulls the quoted literals out of the bracketed list following
    /// `marker` — a lenient scan, no TS parser: slice from `= [` to the
    /// closing `]`, then take the odd split segments.
    fn quoted_list(marker: &str, text: &str) -> Vec<String> {
        let marker_at = text.find(marker).expect("marker present");
        let open = text[marker_at..].find("= [").expect("list opens") + marker_at + 2;
        let close = text[open..].find(']').expect("list closes") + open;
        text[open + 1..close]
            .split('"')
            .enumerate()
            .filter(|(index, _)| index % 2 == 1)
            .map(|(_, part)| part.to_string())
            .collect()
    }

    #[test]
    fn shell_locale_lists_match_i18n_ts_and_build_rs() {
        // The wizard's locale list is written out in two places: the
        // i18n tables (`shell/web/src/i18n.ts`, the language selector's
        // options) and build.rs's `SHELL_LOCALES` (the per-locale
        // license documents). They must not drift silently.
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let i18n = std::fs::read_to_string(manifest_dir.join("web/src/i18n.ts"))
            .expect("the web i18n tables sit inside the shell crate");
        let build = std::fs::read_to_string(manifest_dir.join("build.rs"))
            .expect("build.rs sits in the shell crate");

        let locales = quoted_list("export const LOCALES", &i18n);
        let shell_locales = quoted_list("const SHELL_LOCALES", &build);

        assert_eq!(locales.len(), 8, "i18n.ts carries the eight locales");
        assert_eq!(
            shell_locales, locales,
            "build.rs SHELL_LOCALES must mirror the i18n.ts locale list"
        );
    }
}
