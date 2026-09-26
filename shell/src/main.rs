//! Shun installer shell — one wizard, four faces.
//!
//! Everything shown here is generated from the delivery configuration
//! (`SHUN_MANIFEST`, set by the `shun build` CLI; a plain cargo build
//! falls back to the sibling demo app): product identity, payload, and
//! the interface faces the manifest allows. The wizard itself — the
//! step rail, the location/license/install progression, the shortcuts
//! and launch behavior — lives in `shun::wizard`, and every face
//! renders that one model:
//!
//! - **webview GUI** (tauri2, the default when the runtime exists),
//! - **egui GUI** (`--no-webview`, or automatic when WebView2 is
//!   missing — no native webview needed at all),
//! - **TUI** (`--no-gui` with a TTY: the same left step rail in the
//!   terminal; without a TTY the flag resolves to the help text),
//! - **headless CLI** (`--silent`, and the ARP `UninstallString`) —
//!   the substrate every build carries.
//!
//! The manifest's `[shun.ui] faces` list can disable the webview, egui
//! and TUI faces per product; the CLI face cannot be disabled.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod fallback;
#[cfg(windows)]
mod screenshot;
mod terminal;
mod tui;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use clap::{CommandFactory, Parser};
use serde::Serialize;
use shun::config::{ResolvedLicenseDoc, ShunConfig, TargetConfig, UiFace};
use shun::env_probe::UiCapabilities;
use shun::flow::FlowEvent;
use shun::payload::ArchivePayload;
use shun::targets::install::WizardAnswers;
use shun::wizard::{InstallRequest, WizardCore};
use tauri::{Emitter, Manager, State};

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
/// the language step changes the wizard language.
const SHUN_LICENSE_DOCS_JSON: &str =
    include_str!(concat!(env!("OUT_DIR"), "/shun-license-docs.json"));
/// The product logo embedded by build.rs (kind file + bytes) — both GUIs
/// render it in their caption bars.
const LOGO_KIND: &str = include_str!(concat!(env!("OUT_DIR"), "/shun-logo-kind.txt"));
const LOGO_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/shun-logo.bin"));
/// Build flavor marker (the `shun build --variant` env channel), embedded
/// at compile time; "full" when unset.
const SHUN_FLAVOR: &str = match option_env!("SHUN_FLAVOR") {
    Some(flavor) => flavor,
    None => "full",
};

/// The per-locale license documents, keyed by locale (`en`, `zh-Hans`,
/// ...). Empty when the config declares no license at all.
type LicenseDocs = BTreeMap<String, Vec<ResolvedLicenseDoc>>;

/// Parses the embedded per-locale license document map.
fn license_docs() -> LicenseDocs {
    serde_json::from_str(SHUN_LICENSE_DOCS_JSON).expect("embedded license docs parse")
}

/// The wizard-model license documents (`title` flattened), as the TUI
/// and the shared core consume them.
fn wizard_license_docs() -> BTreeMap<String, Vec<shun::wizard::LicenseDoc>> {
    license_docs()
        .into_iter()
        .map(|(locale, docs)| {
            (
                locale,
                docs.into_iter()
                    .map(|doc| shun::wizard::LicenseDoc {
                        title: doc.title.unwrap_or_default(),
                        body: doc.body,
                    })
                    .collect(),
            )
        })
        .collect()
}

// ── Argument face ────────────────────────────────────────────────────────

/// The installer's command line. Every face is manually reachable:
/// `--silent` renders nothing (non-interactive CLI), `--no-gui` picks
/// the TUI (or the help text without a TTY), `--no-webview` pins the
/// egui GUI. Without any of these the shell prefers the tauri2 webview
/// GUI and degrades automatically (webview2 missing → egui; no desktop
/// at all → TUI when a TTY exists, else the help text).
#[derive(Parser, Debug)]
#[command(
    name = "shun-installer",
    about = "shun delivery wizard — install, uninstall, repair",
    version,
    disable_help_subcommand = true
)]
struct Cli {
    /// Non-interactive install/uninstall: no UI at all, progress on
    /// stdout. The face the ARP `UninstallString` and fleet tooling use.
    #[arg(long)]
    silent: bool,

    /// Terminal wizard instead of a GUI. Without a TTY this resolves to
    /// the help text (nothing to render the TUI on).
    #[arg(long)]
    no_gui: bool,

    /// egui GUI, skipping the webview entirely (machines without
    /// WebView2, or forcing the fallback for testing).
    #[arg(long)]
    no_webview: bool,

    /// Legacy spelling of `--no-webview`.
    #[arg(long, hide = true)]
    fallback: bool,

    /// Legacy spelling of `--no-webview`.
    #[arg(long, hide = true)]
    egui: bool,

    /// Drive the uninstall flow (what the ARP entry passes).
    #[arg(long)]
    uninstall: bool,

    /// Install directory (headless installs; the GUIs ask instead).
    #[arg(long)]
    dir: Option<String>,

    /// Delivery mode: `local` (per-user registration).
    #[arg(long, default_value = "local")]
    mode: String,

    /// Install scope: `user` (default) or `machine` (self-elevating).
    #[arg(long)]
    scope: Option<String>,

    /// Skip the conventional desktop shortcut (headless installs).
    #[arg(long)]
    no_desktop: bool,

    /// The wizard language (carried through elevation, into scripts and
    /// the on-disk manifest).
    #[arg(long)]
    language: Option<String>,

    /// Offline UI capture: render, wait, save the window as PNG, exit.
    #[arg(long)]
    screenshot: Option<PathBuf>,

    /// Settle time for `--screenshot`, in milliseconds.
    #[arg(long)]
    screenshot_delay: Option<u64>,
}

/// Maps the legacy switch spellings onto the flag form before parsing:
/// `/S` (NSIS convention) and `/uninstall` (the ARP `UninstallString`
/// passes exactly that).
fn normalize_switches<I: IntoIterator<Item = String>>(args: I) -> Vec<String> {
    args.into_iter()
        .map(|arg| match arg.as_str() {
            "/S" | "/silent" => "--silent".into(),
            "/uninstall" => "--uninstall".into(),
            other => other.to_string(),
        })
        .collect()
}

/// The face this run renders.
enum Face {
    /// The tauri2 webview GUI.
    Tauri,
    /// The egui GUI.
    Egui,
    /// The ratatui terminal wizard.
    Tui,
    /// The headless CLI (`--silent`, ARP).
    Silent,
    /// Print the help text (with `message` as the deciding reason) and
    /// exit. `err` picks the exit code: a graceful `--no-gui` without a
    /// TTY exits 0, an unsatisfiable request exits 2.
    Help { message: String, err: bool },
}

/// Resolves the face: explicit flags first, then the environment, then
/// the manifest's face list. The CLI face is the substrate and never
/// consults the list.
fn resolve_face(cli: &Cli, caps: &UiCapabilities, faces: &[UiFace]) -> Face {
    use shun::config::UiFace::{Egui, Tui, Webview};
    let allows = |face: UiFace| faces.contains(&face);

    if cli.silent {
        return Face::Silent;
    }
    if cli.no_gui {
        if !allows(Tui) {
            return Face::Help {
                message: "the TUI face is disabled in this product's manifest".into(),
                err: true,
            };
        }
        if caps.tty {
            return Face::Tui;
        }
        // The spec'd resolution: no terminal, nothing to render the TUI
        // on — behave exactly like `--help`.
        return Face::Help {
            message: "--no-gui without a terminal — showing the help text".into(),
            err: false,
        };
    }
    if cli.no_webview || cli.fallback || cli.egui {
        if allows(Egui) {
            return Face::Egui;
        }
        return Face::Help {
            message: "the egui face is disabled in this product's manifest".into(),
            err: true,
        };
    }
    // Default: prefer the richest face the environment supports.
    if caps.gui {
        if caps.webview2 && allows(Webview) {
            return Face::Tauri;
        }
        if allows(Egui) {
            return Face::Egui;
        }
    }
    if caps.tty && allows(Tui) {
        return Face::Tui;
    }
    Face::Help {
        message: "no interface face is available here (no desktop, no terminal) — \
                  pass --silent for the headless install"
            .into(),
        err: true,
    }
}

/// The manifest's face list (absent = all three GUI/TUI faces).
fn faces_of(config: &ShunConfig) -> Vec<UiFace> {
    config
        .shell
        .as_ref()
        .and_then(|shell| shell.faces.clone())
        .unwrap_or_else(|| vec![UiFace::Webview, UiFace::Egui, UiFace::Tui])
}

// ── Tauri face ───────────────────────────────────────────────────────────

/// State shared by the commands: the wizard model (behind a mutex —
/// commands mutate it from worker threads), the payload, and whether
/// this run drives the uninstall page instead of the install wizard.
struct AppState {
    core: std::sync::Mutex<WizardCore>,
    payload: ArchivePayload,
    uninstall_mode: bool,
}

impl AppState {
    fn config(&self) -> ShunConfig {
        self.core.lock().expect("wizard core").config.clone()
    }
}

/// The shell surface the webview UI boots from (product identity, the
/// resolved pipeline, face knobs).
#[derive(Serialize)]
struct ShellView {
    product: shun::config::ProductIdentity,
    /// Delivery-mode ids the UI should offer, in order.
    modes: Vec<String>,
    /// The resolved wizard pipeline (ordered steps with inlined bodies).
    steps: Vec<shun::config::ResolvedStep>,
    /// License documents resolved per locale.
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
    /// Optional attachments resolved against the payload.
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
    let config = state.config();
    let mut modes = Vec::new();
    for target in &config.targets {
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
    let flash = config
        .targets
        .iter()
        .any(|t| matches!(t, TargetConfig::Flash(_)));
    let shell = config.shell.clone().unwrap_or_default();
    ShellView {
        product: config.product.clone(),
        modes,
        steps: serde_json::from_str(SHUN_STEPS_JSON).expect("embedded wizard pipeline parses"),
        license_docs: license_docs(),
        timeline: shell.timeline,
        theme: shell.theme,
        language: shell.language,
        log_level: shell.log_level.unwrap_or_default(),
        flash,
        attachments: shun::attachments::resolve(&config, &state.payload)
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
struct DirCandidateView {
    kind: String,
    path: String,
    writable: bool,
}

#[derive(Serialize)]
struct DirDefaults {
    dir: String,
    /// True when `dir` points at a removable drive (portable mode).
    removable: bool,
    candidates: Vec<DirCandidateView>,
}

#[tauri::command]
fn default_dir(state: State<'_, AppState>, mode: String) -> DirDefaults {
    let _ = mode;
    let product = state
        .core
        .lock()
        .expect("wizard core")
        .config
        .product
        .name
        .clone();
    let candidates = shun::wizard::location_defaults(&product);
    let dir = candidates
        .iter()
        .find(|candidate| candidate.writable)
        .map(|candidate| candidate.path.clone())
        .unwrap_or_else(|| shun::wizard::default_location(&product));
    DirDefaults {
        dir,
        removable: false,
        candidates: candidates
            .into_iter()
            .map(|candidate| DirCandidateView {
                kind: candidate.kind.to_string(),
                path: candidate.path,
                writable: candidate.writable,
            })
            .collect(),
    }
}

/// Pads one folder level under a bare filesystem root target so the
/// payload never lands directly on the root — the wizard rewrites the
/// path box with the result. The install re-applies the same guard.
#[tauri::command]
fn nest_root_dir(state: State<'_, AppState>, dir: String) -> String {
    shun::wizard::pad_root_dir(&state.config(), &dir)
}

/// One enumerated drive for the wizard's prefix picker.
#[derive(Serialize)]
struct DriveView {
    mount: String,
    /// "removable" | "fixed" | "network" | "cdrom" | "ramdisk" | "unknown".
    kind: String,
    label: Option<String>,
}

#[tauri::command]
fn list_drives() -> Vec<DriveView> {
    use shun::fs_probe::DriveKind;

    shun::fs_probe::list_drives()
        .into_iter()
        .map(|drive| DriveView {
            mount: drive.mount.to_string_lossy().into_owned(),
            kind: match drive.kind {
                DriveKind::Removable => "removable",
                DriveKind::Fixed => "fixed",
                DriveKind::Network => "network",
                DriveKind::CdRom => "cdrom",
                DriveKind::RamDisk => "ramdisk",
                DriveKind::Unknown => "unknown",
            }
            .to_string(),
            label: drive.label,
        })
        .collect()
}

/// Live writability probe for the path the wizard currently shows.
#[tauri::command]
fn check_dir_writable(dir: String) -> bool {
    shun::fs_probe::is_dir_writable(Path::new(dir.trim()))
}

/// The installer's identity: product version plus the build flavor,
/// shown under the install location.
#[derive(Serialize)]
struct Identity {
    version: String,
    flavor: String,
}

#[tauri::command]
fn get_identity(state: State<'_, AppState>) -> Identity {
    Identity {
        version: state.config().product.version,
        flavor: SHUN_FLAVOR.trim().to_string(),
    }
}

/// One document of the license step: a titled markdown block.
#[derive(Serialize, Clone)]
struct LicenseDoc {
    title: String,
    body: String,
}

/// The wizard's `navigator.language` → embedded license-docs key.
fn license_locale_key(locale: &str) -> &'static str {
    let lower = locale.to_lowercase();
    if lower.starts_with("zh-hant") || lower.starts_with("zh-tw") || lower.starts_with("zh-hk") {
        "zh-Hant"
    } else if lower.starts_with("zh") {
        "zh-Hans"
    } else if lower.starts_with("ja") {
        "ja"
    } else if lower.starts_with("ko") {
        "ko"
    } else if lower.starts_with("ru") {
        "ru"
    } else if lower.starts_with("fr") {
        "fr"
    } else if lower.starts_with("es") {
        "es"
    } else {
        "en"
    }
}

/// The license documents for the requested locale, resolved at build
/// time. An unknown locale falls back to the English set.
#[tauri::command]
fn get_license_docs(locale: String) -> Vec<LicenseDoc> {
    let key = license_locale_key(&locale);
    let docs = license_docs();
    docs.get(key)
        .or_else(|| docs.get("en"))
        .map(|set| {
            set.iter()
                .map(|doc| LicenseDoc {
                    title: doc.title.clone().unwrap_or_default(),
                    body: doc.body.clone(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Install-pane preferences from the manifest's `[shun.shell]` table.
#[derive(Serialize)]
struct ShellPrefs {
    log_level: String,
    log_order: String,
}

#[tauri::command]
fn get_shell_prefs(state: State<'_, AppState>) -> ShellPrefs {
    let shell = state.config().shell.unwrap_or_default();
    ShellPrefs {
        log_level: match shell.log_level {
            Some(shun::config::LogVerbosity::Files) => "files".into(),
            Some(shun::config::LogVerbosity::Scripts) => "scripts".into(),
            Some(shun::config::LogVerbosity::Off) => "off".into(),
            _ => "all".into(),
        },
        log_order: match shell.log_order {
            Some(shun::config::LogOrder::Oldest) => "oldest".into(),
            _ => "newest".into(),
        },
    }
}

// ── Installer preferences (the wizard's remembered language) ────────────
//
// `<data home>/<product>/installer-prefs.json` — per-user, per-product.
// Portable runs NEVER touch it.

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct InstallerPrefs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) language: Option<String>,
}

fn local_appdata() -> PathBuf {
    shun::wizard::local_appdata()
}

fn prefs_path(root: &Path, product: &str) -> PathBuf {
    root.join(product).join("installer-prefs.json")
}

pub(crate) fn load_prefs(root: &Path, product: &str) -> InstallerPrefs {
    std::fs::read(prefs_path(root, product))
        .ok()
        .and_then(|raw| serde_json::from_slice(&raw).ok())
        .unwrap_or_default()
}

pub(crate) fn save_prefs(root: &Path, product: &str, prefs: &InstallerPrefs) -> Result<(), String> {
    let path = prefs_path(root, product);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_vec_pretty(prefs).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

/// Remembers the wizard language for the next run; a portable run keeps
/// the choice in memory only.
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

/// The wizard language remembered from a previous run, if any.
#[tauri::command]
fn get_saved_language(state: State<'_, AppState>) -> Option<String> {
    let product = state.config().product.name;
    load_prefs(&local_appdata(), &product).language
}

/// Remembers the wizard language for the next run.
#[tauri::command]
fn save_language(
    state: State<'_, AppState>,
    language: String,
    portable: Option<bool>,
) -> Result<(), String> {
    let product = state.config().product.name;
    remember_language(
        &local_appdata(),
        &product,
        language,
        portable.unwrap_or(false),
    )
    .map(|_| ())
}

fn emit_progress(app: &tauri::AppHandle, event: &FlowEvent) {
    let _ = app.emit("install-progress", event);
}

/// Whether this run drives the uninstall page (`/uninstall` without
/// `--silent`): the webview renders it instead of the install wizard.
#[tauri::command]
fn is_uninstall_mode(state: State<'_, AppState>) -> bool {
    state.uninstall_mode
}

/// The install dir this uninstaller lives in — the target the uninstall
/// page's repair action re-installs over.
#[tauri::command]
fn current_install_dir() -> Result<String, String> {
    shun::wizard::current_exe_dir()
        .map(|dir| dir.to_string_lossy().into_owned())
        .ok_or_else(|| "cannot locate the uninstaller directory".into())
}

/// Runs the shun uninstall for the install dir this uninstaller lives
/// in, on a blocking thread — the UI must not freeze while the registry
/// and filesystem work proceeds.
#[tauri::command]
async fn perform_uninstall(state: State<'_, AppState>) -> Result<(), String> {
    let core = WizardCore::new(state.config(), BTreeMap::new());
    tauri::async_runtime::spawn_blocking(move || shun::wizard::run_uninstall(&core))
        .await
        .map_err(|e| format!("the uninstall task exited abnormally: {e}"))?
}

/// Applies the done-page confirmation in one shot: creates or removes
/// the install's shortcuts per the checkboxes, on a blocking thread.
#[tauri::command]
async fn set_shortcuts(
    state: State<'_, AppState>,
    desktop: Option<bool>,
    menu: Option<bool>,
    dir: String,
) -> Result<(), String> {
    let config = state.config();
    let main_exe = state.core.lock().expect("wizard core").main_exe();
    let padded = shun::wizard::pad_root_dir(&config, &dir);
    tauri::async_runtime::spawn_blocking(move || {
        shun::targets::shortcuts::apply_shortcut_choices(
            &shun::wizard::shortcut_aumid_for(&config),
            &main_exe,
            desktop,
            menu,
            &padded,
        )
    })
    .await
    .map_err(|e| format!("the shortcut task exited abnormally: {e}"))?
}

/// Launches the freshly installed application (done-page option).
#[tauri::command]
fn launch_app(state: State<'_, AppState>, dir: String) -> Result<(), String> {
    let core = WizardCore::new(state.config(), BTreeMap::new());
    shun::wizard::launch_installed(&core, &dir)
}

/// Runs the install flow for the wizard UI: the shared driver on a
/// blocking thread (a sync command would stall the event loop for the
/// whole extract), events streaming to the frontend.
#[tauri::command]
async fn start_install(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    mode: String,
    dir: String,
    language: Option<String>,
) -> Result<(), String> {
    let config = state.config();
    let payload = state.payload.clone();
    let request = InstallRequest {
        mode,
        dir,
        language,
    };
    tauri::async_runtime::spawn_blocking(move || {
        let core = WizardCore::new(config, BTreeMap::new());
        shun::wizard::run_install(&core, &payload, &request, &mut |event| {
            emit_progress(&app, event)
        })
    })
    .await
    .map_err(|e| format!("the install task exited abnormally: {e}"))?
}

/// Streams a declared (and not already bundled) attachment into the
/// install directory.
#[tauri::command]
fn download_attachment(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    key: String,
    dir: String,
) -> Result<(), String> {
    let config = state.config();
    let resolved = shun::attachments::resolve(&config, &state.payload);
    let attachment = resolved
        .iter()
        .find(|a| a.config.key == key)
        .ok_or_else(|| format!("unknown attachment: {key}"))?;
    if attachment.included {
        return Ok(());
    }
    let dir = dir.trim().trim_end_matches('\\').to_string();
    if dir.is_empty() {
        return Err("the install directory must not be empty".into());
    }
    let dir = shun::wizard::pad_root_dir(&config, &dir);
    shun::attachments::download(&attachment.config, Path::new(&dir), &mut |event| {
        emit_progress(&app, &event)
    })
    .map_err(|e| e.to_string())
}

// ── Headless face ────────────────────────────────────────────────────────

/// The non-interactive install/uninstall: the same shared driver, the
/// events printed to the attached console. Machine scope re-launches
/// under UAC with the resolved answers before anything runs.
fn run_headless(cli: &Cli, config: &ShunConfig, payload: &ArchivePayload) -> Result<(), String> {
    let dir = cli
        .dir
        .clone()
        .unwrap_or_else(|| shun::wizard::default_location(&config.product.name));
    let dir = shun::wizard::pad_root_dir(config, &dir);
    let portable = cli.mode == "portable";
    let machine = cli.scope.as_deref() == Some("machine");

    // The elevation gate for machine scope happens before any flow work;
    // it needs an InstallContext, so build the same one the flow would.
    if !cli.uninstall && machine {
        let install = config
            .targets
            .iter()
            .find_map(|t| match t {
                TargetConfig::Install(install) => Some(install.clone()),
                _ => None,
            })
            .ok_or("this configuration declares no install target")?;
        let mut ctx = shun::targets::install::InstallContext::new(
            config.product.name.clone(),
            config.product.version.clone(),
            PathBuf::from(&dir),
            portable,
        );
        ctx.publisher = config.product.publisher.clone();
        ctx.main_exe = install.main_exe.clone();
        ctx.apply_config(
            &install,
            WizardAnswers {
                desktop_shortcut: !cli.no_desktop,
                start_menu_shortcut: true,
                launch_after_install: false,
                machine: true,
            },
        );
        ctx.language = cli.language.clone();
        ensure_elevated_for(&ctx, &cli.mode, &dir, !cli.no_desktop, false)?;
    }

    let core = WizardCore::new(config.clone(), BTreeMap::new());
    let mut print_event = |event: &FlowEvent| match event {
        shun::flow::FlowEvent::Log { record } => {
            if let Some(line) = headless_log_line(record) {
                println!("{line}");
            }
        }
        shun::flow::FlowEvent::Progress { step, .. } if !step.is_empty() => {
            println!("… {step}");
        }
        _ => {}
    };

    if cli.uninstall {
        shun::wizard::run_uninstall(&core)?;
        println!(
            "shun: uninstalled {}",
            shun::wizard::current_exe_dir()
                .map(|d| d.display().to_string())
                .unwrap_or_default()
        );
        return Ok(());
    }

    let request = InstallRequest {
        mode: cli.mode.clone(),
        dir,
        language: cli.language.clone(),
    };
    shun::wizard::run_install(&core, payload, &request, &mut print_event)?;
    // Headless runs never see the done page: apply the conventional
    // shortcut defaults (real installs only — portable and uninstalls
    // create none).
    if !portable {
        let _ = shun::targets::shortcuts::apply_shortcut_choices(
            &shun::wizard::shortcut_aumid_for(config),
            &core.main_exe(),
            Some(!cli.no_desktop),
            Some(true),
            &core.nested_path(&request.dir),
        );
    }
    println!("shun: install complete");
    Ok(())
}

/// Renders one structured log record for a headless console (English —
/// the console has no locale negotiation).
fn headless_log_line(record: &shun::flow::FlowLog) -> Option<String> {
    use shun::flow::FlowLog;
    match record {
        FlowLog::FileWrite { path } => Some(format!("write {}", path.display())),
        FlowLog::FileReuse { path } => Some(format!("reuse {}", path.display())),
        FlowLog::ScriptBegin { name } => Some(format!("» running script {name}")),
        FlowLog::ScriptLine { line, .. } => Some(format!("  {line}")),
        FlowLog::CommandDone { command } => Some(format!("done {command}")),
        FlowLog::Warning { code, detail } => Some(format!("warning {code}: {detail}")),
        _ => None,
    }
}

/// When the resolved install scope is machine-wide and the current
/// process is not elevated, re-launches this executable under UAC with
/// the same choices (headless) and exits.
pub(crate) fn ensure_elevated_for(
    ctx: &shun::targets::install::InstallContext,
    mode: &str,
    dir: &str,
    desktop: bool,
    uninstalling: bool,
) -> Result<(), String> {
    use shun::targets::install::InstallScope;
    if ctx.scope != InstallScope::Machine || shun::targets::elevate::is_elevated() {
        return Ok(());
    }
    let mut args = format!("--silent --mode={mode} --dir=\"{}\"", dir.trim());
    if !desktop {
        args.push_str(" --no-desktop");
    }
    args.push_str(" --scope=machine");
    if let Some(language) = &ctx.language {
        args.push_str(&format!(" --language={language}"));
    }
    if uninstalling {
        args.push_str(" --uninstall");
    }
    match shun::targets::elevate::relaunch_elevated(&args) {
        Ok(true) => {
            std::thread::sleep(std::time::Duration::from_millis(500));
            std::process::exit(0);
        }
        Ok(false) => {
            Err("machine-wide install needs the UAC prompt to be accepted (it was declined)".into())
        }
        Err(e) => Err(e.to_string()),
    }
}

// ── Native frame titles ─────────────────────────────────────────────────

/// OS window titles per wizard locale: `(locale, installer, uninstall)`
/// templates — `{product}` interpolates the manifest's product name.
/// The webview's own AppTitleBar localizes from the frontend string
/// table; this covers the native frame (taskbar / alt-tab) and mirrors
/// `title` / `uninstallTitle` in `web/src/i18n.ts`.
const WINDOW_TITLES: &[(&str, &str, &str)] = &[
    ("zh-Hans", "{product} 安装程序", "卸载 {product}"),
    ("zh-Hant", "{product} 安裝程式", "解除安裝 {product}"),
    ("en", "{product} Installer", "Uninstall {product}"),
    ("ru", "Установщик {product}", "Удалить {product}"),
    (
        "ja",
        "{product} インストーラー",
        "{product} のアンインストール",
    ),
    ("ko", "{product} 설치 관리자", "{product} 제거"),
    (
        "fr",
        "Programme d'installation {product}",
        "Désinstaller {product}",
    ),
    ("es", "Instalador de {product}", "Desinstalar {product}"),
];

/// Whether `value` is one of the wizard locale strings.
#[cfg(windows)]
fn is_wizard_locale(value: &str) -> bool {
    WINDOW_TITLES.iter().any(|(k, _, _)| *k == value)
}

/// The system UI locale as a BCP-47-ish tag. `None` when the API fails.
#[cfg(windows)]
fn system_locale_tag() -> Option<String> {
    use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;

    const LOCALE_NAME_MAX_LENGTH: usize = 85;
    let mut buf = [0u16; LOCALE_NAME_MAX_LENGTH];
    // SAFETY: `buf` is a valid buffer of the documented size.
    let len = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), LOCALE_NAME_MAX_LENGTH as i32) };
    if len <= 0 {
        return None;
    }
    let len = (len as usize).min(buf.len());
    let tag = String::from_utf16_lossy(&buf[..len.saturating_sub(1)]);
    let tag = tag.trim();
    (!tag.is_empty()).then(|| tag.to_string())
}

/// The OS window title for this run: the saved wizard language wins,
/// then the system locale, then the zh-Hans default.
#[cfg(windows)]
fn os_window_title(config: &ShunConfig, uninstall: bool) -> String {
    let saved = load_prefs(&local_appdata(), &config.product.name)
        .language
        .filter(|l| is_wizard_locale(l));
    let tag = saved
        .or_else(system_locale_tag)
        .unwrap_or_else(|| "zh-Hans".into());
    let key = license_locale_key(&tag);
    let (install, uninstall_title) = WINDOW_TITLES
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, i, u)| (*i, *u))
        .unwrap_or(("{product} 安装程序", "卸载 {product}"));
    let template = if uninstall { uninstall_title } else { install };
    template.replace("{product}", &config.product.name)
}

#[cfg(not(windows))]
fn os_window_title(config: &ShunConfig, uninstall: bool) -> String {
    let template = if uninstall {
        "Uninstall {product}"
    } else {
        "{product} Installer"
    };
    template.replace("{product}", &config.product.name)
}

// ── WebView2 bootstrap ───────────────────────────────────────────────────

/// Stages the payload's fixed-version WebView2 runtime into the shun
/// cache and points the loader at it (the one-copy strategy).
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
    // SAFETY: single-threaded bootstrap before any UI thread exists.
    unsafe {
        std::env::set_var(
            "WEBVIEW2_BROWSER_EXECUTABLE_FOLDER",
            cache.join(&runtime_path),
        );
    }
}

// ── Entry ────────────────────────────────────────────────────────────────

fn main() {
    // parse_from takes argv0 first — feed the program name before the
    // (normalized) real arguments, or the first flag would be eaten as
    // the binary name and every face flag silently ignored.
    let cli = Cli::parse_from(
        std::iter::once("shun-installer".to_string())
            .chain(normalize_switches(std::env::args().skip(1))),
    );
    let config: ShunConfig =
        serde_json::from_str(SHUN_CONFIG_JSON).expect("embedded config decodes");
    let payload = ArchivePayload::from_bytes(EMBEDDED_PAYLOAD).expect("embedded payload decodes");
    let faces = faces_of(&config);
    let caps = UiCapabilities::probe();
    let face = resolve_face(&cli, &caps, &faces);

    match face {
        Face::Silent => {
            // GUI-subsystem binaries inherit no console; attach the
            // parent's so the progress lines reach the invoking terminal.
            let _ = shun::env_probe::attach_parent_console();
            if let Err(err) = run_headless(&cli, &config, &payload) {
                eprintln!("shun: {err}");
                std::process::exit(1);
            }
        }
        Face::Help { message, err } => {
            let _ = shun::env_probe::attach_parent_console();
            println!("{message}");
            println!();
            print!("{}", Cli::command().render_help());
            std::process::exit(i32::from(err));
        }
        Face::Tui => {
            if let Err(err) = tui::run(config, payload, cli.uninstall) {
                let _ = shun::env_probe::attach_parent_console();
                eprintln!("shun: {err}");
                std::process::exit(1);
            }
        }
        Face::Egui => {
            let reason = if cli.no_webview || cli.fallback || cli.egui {
                fallback::FallbackReason::ManualOverride
            } else {
                fallback::FallbackReason::MissingWebview2
            };
            #[cfg(windows)]
            if let Some(path) = &cli.screenshot {
                let title = fallback::window_title(&config);
                screenshot::schedule_by_title(
                    title,
                    path.clone(),
                    cli.screenshot_delay.unwrap_or(2500),
                );
            }
            // The egui face is the last GUI resort: if even it cannot
            // start (renderer failure on broken drivers), say so in a
            // native message box instead of dying silently.
            let result = std::panic::catch_unwind(|| {
                fallback::run(
                    config,
                    payload,
                    reason,
                    LOGO_KIND.trim(),
                    LOGO_BYTES,
                    serde_json::from_str(SHUN_STEPS_JSON).expect("embedded wizard pipeline parses"),
                    license_docs(),
                );
            });
            if result.is_err() {
                native_fatal_box(
                    "无法启动安装界面 / the installer UI could not start",
                    "The egui fallback failed to initialize (graphics driver?). \
                     Run with --silent for the headless install.",
                );
                std::process::exit(1);
            }
        }
        Face::Tauri => {
            let uninstall = cli.uninstall;
            run_tauri(cli, config, payload, uninstall)
        }
    }
}

/// A native, webview-free message box (the zero-dependency floor).
fn native_fatal_box(title: &str, body: &str) {
    let _ = rfd::MessageDialog::new()
        .set_title(title)
        .set_description(body)
        .set_level(rfd::MessageLevel::Error)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

/// Boots the webview face (the richest one): fixed-runtime bootstrap,
/// screenshot capture, the command surface, and the localized native
/// frame title.
fn run_tauri(cli: Cli, config: ShunConfig, payload: ArchivePayload, uninstall_mode: bool) {
    let screenshot = cli.screenshot.clone();
    let delay = cli.screenshot_delay.unwrap_or(4000);
    bootstrap_fixed_webview2(&config, &payload);

    let native_title = os_window_title(&config, uninstall_mode);
    let core = WizardCore::new(config, wizard_license_docs());
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            core: std::sync::Mutex::new(core),
            payload,
            uninstall_mode,
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            default_dir,
            nest_root_dir,
            list_drives,
            check_dir_writable,
            get_identity,
            get_license_docs,
            get_shell_prefs,
            get_saved_language,
            save_language,
            is_uninstall_mode,
            current_install_dir,
            perform_uninstall,
            set_shortcuts,
            launch_app,
            start_install,
            download_attachment
        ])
        .setup(move |app| {
            #[cfg(windows)]
            if let Some(path) = &screenshot {
                screenshot::schedule(app.handle().clone(), path.clone(), delay);
            }
            #[cfg(not(windows))]
            let _ = (&screenshot, delay);
            if let Some(window) = app.get_webview_window("installer") {
                let _ = window.set_title(&native_title);
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running the shun installer shell");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cli(args: &[&str]) -> Cli {
        // parse_from takes argv0 first; feed a placeholder.
        let mut argv = vec!["shun-installer".to_string()];
        argv.extend(normalize_switches(args.iter().map(|s| s.to_string())));
        Cli::parse_from(argv)
    }

    #[test]
    fn legacy_switch_spellings_map_onto_flags() {
        let parsed = cli(&["/S", "/uninstall"]);
        assert!(parsed.silent);
        assert!(parsed.uninstall);
    }

    #[test]
    fn silent_wins_over_everything() {
        let parsed = cli(&["--silent", "--no-gui", "--no-webview"]);
        let caps = UiCapabilities {
            gui: true,
            webview2: true,
            tty: true,
        };
        let faces = vec![UiFace::Webview, UiFace::Egui, UiFace::Tui];
        assert!(matches!(resolve_face(&parsed, &caps, &faces), Face::Silent));
    }

    #[test]
    fn no_gui_without_a_tty_resolves_to_help() {
        let parsed = cli(&["--no-gui"]);
        let caps = UiCapabilities {
            gui: true,
            webview2: true,
            tty: false,
        };
        let faces = vec![UiFace::Webview, UiFace::Egui, UiFace::Tui];
        match resolve_face(&parsed, &caps, &faces) {
            Face::Help { err, .. } => assert!(!err, "graceful --no-gui exits 0"),
            _ => panic!("expected Help"),
        }
    }

    #[test]
    fn disabled_faces_are_never_selected() {
        let caps = UiCapabilities {
            gui: true,
            webview2: false,
            tty: true,
        };
        // egui disabled: webview missing → falls to the TUI (tty exists).
        let faces = vec![UiFace::Webview, UiFace::Tui];
        assert!(matches!(resolve_face(&cli(&[]), &caps, &faces), Face::Tui));
        // requesting the disabled face explicitly is an error-help.
        match resolve_face(&cli(&["--no-webview"]), &caps, &faces) {
            Face::Help { err, .. } => assert!(err),
            _ => panic!("expected erroring Help"),
        }
    }

    #[test]
    fn default_prefers_tauri_then_egui() {
        let good = UiCapabilities {
            gui: true,
            webview2: true,
            tty: true,
        };
        let all = vec![UiFace::Webview, UiFace::Egui, UiFace::Tui];
        assert!(matches!(resolve_face(&cli(&[]), &good, &all), Face::Tauri));
        let no_runtime = UiCapabilities {
            gui: true,
            webview2: false,
            tty: true,
        };
        assert!(matches!(
            resolve_face(&cli(&[]), &no_runtime, &all),
            Face::Egui
        ));
    }

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

        assert_eq!(load_prefs(&root, product).language, None);
        assert!(
            remember_language(&root, product, "zh-Hans".into(), false).expect("prefs saved"),
            "a non-portable run persists the choice"
        );
        assert_eq!(
            load_prefs(&root, product).language.as_deref(),
            Some("zh-Hans")
        );
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

        let path = prefs_path(&root, product);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, [0xFF, 0xFE]).unwrap();
        assert_eq!(load_prefs(&root, product).language, None);

        assert!(remember_language(&root, product, "en".into(), false).is_ok());
        assert_eq!(load_prefs(&root, product).language.as_deref(), Some("en"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn window_titles_interpolate_the_product() {
        let config: ShunConfig = serde_json::from_str(
            r#"{"product":{"name":"Evernight","version":"1.0"},"targets":[]}"#,
        )
        .unwrap();
        assert_eq!(
            os_window_title(&config, false),
            "Evernight 安装程序",
            "unmapped system locales resolve to the zh-Hans default"
        );
    }
}
