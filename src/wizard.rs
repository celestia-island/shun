//! The one wizard model every delivery face renders.
//!
//! A shell can present itself four ways — webview GUI (tauri), egui GUI,
//! TUI (ratatui), and the headless CLI — and the requirement is that they
//! all deliver the *same* wizard: the same left step rail, the same
//! location/license/install/done progression, the same shortcuts and
//! launch behavior. This module is that shared model: [`WizardState`]
//! holds everything a face paints, [`WizardCore`] moves it, and the
//! `run_install` / `run_uninstall` drivers execute the delivery exactly
//! once for all faces (the faces only choose how to render progress).
//!
//! Faces localize labels from their own tables (the model carries step
//! *keys*, not strings); flow-step text arrives as the flow's composed
//! English verbs, which faces may pass through.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::config::{InstallConfig, ShunConfig, TargetConfig};
use crate::flow::{Flow, FlowEvent, FlowLog, FlowPhase};
use crate::payload::{ArchivePayload, MANIFEST_PATH, PayloadEntry};
use crate::targets::install::{
    InstallContext, InstallFlow, UNINSTALLER_NAME, WindowsRegistration, WizardAnswers,
    default_aumid, nest_root_dir, read_manifest, uninstall,
};

/// One wizard step. The rail order is the declaration order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Locale picker — the wizard's opening pane.
    Language,
    /// Install location (path + drive picker + candidates).
    Location,
    /// License agreement (paged documents).
    License,
    /// The running install (progress + log).
    Install,
    /// Done / failed landing page.
    Done,
}

impl Step {
    /// The rail in display order.
    pub const RAIL: [Step; 5] = [
        Step::Language,
        Step::Location,
        Step::License,
        Step::Install,
        Step::Done,
    ];

    /// The step before this one (Language has none).
    pub fn back_from(self) -> Option<Step> {
        Step::RAIL.get(self.index().checked_sub(1)?).copied()
    }

    fn index(self) -> usize {
        match self {
            Step::Language => 0,
            Step::Location => 1,
            Step::License => 2,
            Step::Install => 3,
            Step::Done => 4,
        }
    }
}

/// One line of the install log pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    /// HH:MM:SS stamp.
    pub time: String,
    /// Severity-ish class the faces color by.
    pub kind: LogKind,
    pub text: String,
}

/// Log line classes (echo = plain, ok = success marker, step = phase
/// headline, error = warning/failure record).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogKind {
    Step,
    Echo,
    Ok,
    Error,
}

/// The uninstall page's phase (faces render confirm/running/done from
/// this; the model keeps it so every face offers the same flow).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UninstallPhase {
    Idle,
    Running,
    Done,
    Failed,
}

/// One writability-probed install-location candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirCandidate {
    /// "appdata" | "program-files" | "drive".
    pub kind: &'static str,
    pub path: String,
    pub writable: bool,
}

/// Everything a face paints. Updated only through [`WizardCore`]
/// actions and [`WizardCore::apply_event`], so every face sees the same
/// transitions.
#[derive(Debug, Clone)]
pub struct WizardState {
    /// Current step (the rail highlights it).
    pub step: Step,
    /// Wizard locale (a shell-locale string, e.g. `zh-Hans`).
    pub locale: String,
    /// The install directory shown in the location step.
    pub dir: String,
    /// Writability of `dir`: `None` while a probe is in flight or the
    /// box is empty.
    pub dir_writable: Option<bool>,
    /// Quick-pick candidates under the path field.
    pub candidates: Vec<DirCandidate>,
    /// Index into the license pager.
    pub license_index: usize,
    /// The accept checkbox.
    pub agreed: bool,
    /// Done-page shortcut answers.
    pub desktop_shortcut: bool,
    pub start_menu_shortcut: bool,
    /// Done-page immediate-launch answer.
    pub launch_after: bool,
    /// 0-100 once the flow reports percents.
    pub progress: Option<u8>,
    /// The flow's current step label (English composed verb).
    pub flow_step: String,
    /// The install log pane (bounded — old lines drop).
    pub log: Vec<LogLine>,
    /// Set when the flow failed; lands the done page on its failure
    /// variant.
    pub failure: Option<String>,
    /// The uninstall page's phase.
    pub uninstall_phase: UninstallPhase,
    /// Uninstall error text (failure variant).
    pub uninstall_error: String,
}

/// One document of the license step: a titled markdown block, resolved
/// per locale at build time by the shell.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LicenseDoc {
    #[serde(default)]
    pub title: String,
    pub body: String,
}

/// The shared wizard: config + payload + per-locale license documents
/// plus the renderable [`WizardState`].
pub struct WizardCore {
    /// The resolved delivery configuration.
    pub config: ShunConfig,
    /// License documents keyed by shell locale.
    pub license_docs: BTreeMap<String, Vec<LicenseDoc>>,
    /// The renderable state.
    pub state: WizardState,
}

impl WizardCore {
    /// Assembles the wizard: default locale (the shell's configured
    /// language, else English), the writability-probed location
    /// defaults, and neutral step state.
    pub fn new(config: ShunConfig, license_docs: BTreeMap<String, Vec<LicenseDoc>>) -> Self {
        let locale = config
            .shell
            .as_ref()
            .and_then(|s| s.language.clone())
            .unwrap_or_else(|| "en".into());
        let mut core = Self {
            state: WizardState {
                step: Step::Language,
                locale,
                dir: String::new(),
                dir_writable: None,
                candidates: Vec::new(),
                license_index: 0,
                agreed: false,
                desktop_shortcut: true,
                start_menu_shortcut: true,
                launch_after: true,
                progress: None,
                flow_step: String::new(),
                log: Vec::new(),
                failure: None,
                uninstall_phase: UninstallPhase::Idle,
                uninstall_error: String::new(),
            },
            config,
            license_docs,
        };
        core.refresh_dir_defaults();
        core
    }

    /// The install-target configuration (single install target per
    /// product).
    pub fn install_target(&self) -> Result<InstallConfig, String> {
        self.config
            .targets
            .iter()
            .find_map(|t| match t {
                TargetConfig::Install(install) => Some(install.clone()),
                _ => None,
            })
            .ok_or_else(|| "this configuration declares no install target".into())
    }

    /// The main executable name: the manifest's `main-exe`, else the
    /// product name with an `.exe` suffix.
    pub fn main_exe(&self) -> String {
        self.install_target()
            .ok()
            .and_then(|i| i.main_exe)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("{}.exe", self.config.product.name))
    }

    /// Locale change: re-keys the license pager (documents arrive from
    /// the build-time map; unknown locales keep the previous set — the
    /// face's picker only offers locales with entries).
    pub fn set_locale(&mut self, locale: impl Into<String>) {
        self.state.locale = locale.into();
        self.state.license_index = 0;
    }

    /// The license documents for a locale (English fallback, then none
    /// — the license step always renders).
    pub fn license_docs_for(&self, locale: &str) -> &[LicenseDoc] {
        let docs = self
            .license_docs
            .get(locale)
            .or_else(|| self.license_docs.get("en"));
        docs.map(|d| d.as_slice()).unwrap_or(&[])
    }

    /// Recomputes the location defaults: the candidate chain (per-user
    /// appdata → Program Files → one folder per fixed drive), the
    /// default being the first writable candidate.
    pub fn refresh_dir_defaults(&mut self) {
        let product = self.config.product.name.clone();
        let candidates = dir_candidates_for(&product);
        let paths: Vec<PathBuf> = candidates.iter().map(|(_, p)| p.clone()).collect();
        let dir = crate::fs_probe::first_writable(&paths)
            .or_else(|| paths.first().cloned())
            .unwrap_or_else(|| local_appdata().join(&product));
        self.state.dir = dir.to_string_lossy().into_owned();
        self.state.dir_writable = Some(true);
        self.state.candidates = candidates
            .into_iter()
            .map(|(kind, path)| DirCandidate {
                kind,
                writable: crate::fs_probe::is_dir_writable(&path),
                path: path.to_string_lossy().into_owned(),
            })
            .collect();
    }

    /// Sets the shown path and probes its writability (the same test
    /// `run_install` gates on).
    pub fn set_dir(&mut self, dir: impl Into<String>) {
        let dir = dir.into();
        let target = dir.trim().to_string();
        self.state.dir_writable = if target.is_empty() {
            None
        } else {
            Some(crate::fs_probe::is_dir_writable(Path::new(&target)))
        };
        self.state.dir = dir;
    }

    /// Pads one folder level under a bare filesystem root (the root-drive
    /// guard); updates the shown path when the guard rewrote it.
    /// Returns whether the path changed.
    pub fn nest_dir(&mut self, raw: &str) -> bool {
        let nested = self.nested_path(raw);
        if nested != raw.trim() {
            self.state.dir = nested;
            true
        } else {
            false
        }
    }

    /// The padded form of a path per the root-drive guard, without
    /// touching state.
    pub fn nested_path(&self, raw: &str) -> String {
        let folder = self
            .install_target()
            .ok()
            .and_then(|i| i.root_dir_folder.clone());
        nest_root_dir(
            Path::new(raw.trim()),
            &self.config.product.name,
            folder.as_deref(),
        )
        .to_string_lossy()
        .into_owned()
    }

    /// Advances one step (no-op past Done; entering Install resets the
    /// run state — callers drive the flow themselves).
    pub fn go(&mut self, step: Step) {
        if step == Step::Install {
            self.state.progress = None;
            self.state.flow_step.clear();
            self.state.log.clear();
            self.state.failure = None;
        }
        if step == Step::License {
            self.state.license_index = 0;
        }
        self.state.step = step;
    }

    /// The step before the current one (Language has none).
    pub fn back_from(&self) -> Option<Step> {
        self.state.step.back_from()
    }

    /// Folds one flow event into the renderable state: percent, current
    /// step label, structured log lines, and the failure marker. Every
    /// face pumps its events through here so their panes stay identical.
    pub fn apply_event(&mut self, event: &FlowEvent) {
        match event {
            FlowEvent::Progress {
                phase,
                step,
                percent,
                ..
            } => {
                let _ = phase;
                if !step.is_empty() {
                    self.state.flow_step = step.clone();
                }
                if let Some(p) = percent {
                    self.state.progress = Some((*p).min(100));
                }
            }
            FlowEvent::Log { record } => {
                if let Some((kind, text)) = log_line_of(record) {
                    self.push_log(kind, text);
                }
            }
            FlowEvent::Failed { message } => {
                self.state.failure = Some(message.clone());
                self.push_log(LogKind::Error, message.clone());
            }
            _ => {}
        }
    }

    /// Appends one log line (bounded: the pane keeps the last 500).
    pub fn push_log(&mut self, kind: LogKind, text: String) {
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        // HH:MM:SS of the wall clock (local offset is a face concern;
        // the model keeps UTC-of-epoch, stable across threads).
        let (h, m, s) = ((time / 3600) % 24, (time / 60) % 60, time % 60);
        self.state.log.push(LogLine {
            time: format!("{h:02}:{m:02}:{s:02}"),
            kind,
            text,
        });
        if self.state.log.len() > 500 {
            self.state.log.remove(0);
        }
    }

    /// The install context for a wizard run (registration semantics per
    /// the config; the flow itself creates no shortcuts).
    fn install_context(&self, dir: &str, portable: bool) -> Result<InstallContext, String> {
        let install = self.install_target()?;
        let mut ctx = InstallContext::new(
            self.config.product.name.clone(),
            self.config.product.version.clone(),
            PathBuf::from(dir),
            portable,
        );
        ctx.publisher = self.config.product.publisher.clone();
        ctx.main_exe = install.main_exe.clone();
        ctx.apply_config(
            &install,
            WizardAnswers {
                desktop_shortcut: false,
                start_menu_shortcut: false,
                launch_after_install: false,
                machine: false,
            },
        );
        Ok(ctx)
    }

    /// The uninstall context for the copied uninstaller: it sits inside
    /// the install dir, so the running executable's directory is the
    /// install target. Local-install semantics only.
    pub fn uninstall_context(&self) -> Result<InstallContext, String> {
        let exe_dir = std::env::current_exe()
            .map_err(|e| format!("cannot locate the uninstaller: {e}"))?
            .parent()
            .ok_or_else(|| "cannot locate the uninstaller directory".to_string())?
            .to_path_buf();
        let install = self.install_target()?;
        let mut ctx = InstallContext::new(
            self.config.product.name.clone(),
            self.config.product.version.clone(),
            exe_dir,
            false,
        );
        ctx.publisher = self.config.product.publisher.clone();
        ctx.main_exe = install.main_exe.clone();
        ctx.apply_config(
            &install,
            WizardAnswers {
                desktop_shortcut: false,
                start_menu_shortcut: true,
                launch_after_install: false,
                machine: false,
            },
        );
        Ok(ctx)
    }
}

/// An install request a face files after the done-page confirmation
/// logic resolves (or a headless caller composes from flags).
pub struct InstallRequest {
    /// "local" (today; portable carries the marker flow when enabled).
    pub mode: String,
    /// The padded install directory.
    pub dir: String,
    /// The wizard language (reaches scripts and the on-disk manifest).
    pub language: Option<String>,
}

/// Drives one delivery through the shared path every face uses:
/// writability gate → overwrite hygiene (stop the running app, capture
/// the previous manifest) → the install flow → the stale-file sweep.
/// Faces render the emitted events (and fold them into their wizard
/// state via [`WizardCore::apply_event`]); this function never touches
/// UI state itself.
pub fn run_install(
    core: &WizardCore,
    payload: &ArchivePayload,
    request: &InstallRequest,
    on_event: &mut dyn FnMut(&FlowEvent),
) -> Result<(), String> {
    let dir = request.dir.trim().trim_end_matches('\\').to_string();
    if dir.is_empty() {
        return Err("the install directory must not be empty".into());
    }
    if !crate::fs_probe::is_dir_writable(Path::new(&dir)) {
        return Err("the target directory is not writable — pick another location".into());
    }
    let portable = request.mode != "local";
    let mut ctx = core
        .install_context(&dir, portable)
        .map_err(|e| e.to_string())?;
    ctx.language = match request.language.as_deref() {
        Some(l) if !l.trim().is_empty() => Some(l.to_string()),
        // Repair path (no wizard answer): keep the language the
        // previous install recorded.
        _ => current_exe_dir().as_deref().and_then(manifest_language_of),
    };

    let install_dir = ctx.install_dir.clone();
    let main_exe = core.main_exe();
    let previous = read_installed_manifest(&install_dir).unwrap_or_default();
    if install_dir.join(&main_exe).is_file() {
        on_event(&FlowEvent::Progress {
            phase: FlowPhase::Prepare,
            step: format!("Stopping {main_exe}"),
            percent: None,
        });
    }
    stop_running_app(&install_dir, &main_exe);
    let flow = InstallFlow {
        payload,
        registration: &WindowsRegistration,
        ctx,
    };
    let mut forwarding = |event: FlowEvent| on_event(&event);
    flow.run(&mut forwarding).map_err(|e| e.to_string())?;
    let stale = remove_stale_payload_files(&install_dir, &previous);
    if stale > 0 {
        on_event(&FlowEvent::Progress {
            phase: FlowPhase::Extract,
            step: format!("Removed {stale} stale file(s)"),
            percent: None,
        });
    }
    Ok(())
}

/// Drives the uninstall for the install dir this process sits in (the
/// uninstall page and the headless ARP path both land here).
pub fn run_uninstall(core: &WizardCore) -> Result<(), String> {
    let ctx = core.uninstall_context()?;
    uninstall(&ctx, &WindowsRegistration).map_err(|e| e.to_string())
}

/// Applies the done-page answers: creates or removes the shortcuts and
/// optionally launches the installed app. A shortcut failure never
/// invalidates the completed install (errors combine and return).
pub fn apply_finish(
    core: &WizardCore,
    dir: &str,
    desktop: Option<bool>,
    menu: Option<bool>,
    launch: bool,
) -> Result<(), String> {
    crate::targets::shortcuts::apply_shortcut_choices(
        &shortcut_aumid_for(&core.config),
        &core.main_exe(),
        desktop,
        menu,
        &core.nested_path(dir),
    )?;
    if launch {
        launch_installed(core, dir)?;
    }
    Ok(())
}

/// Launches the freshly installed main executable through the delivery
/// helper (best-effort detached spawn).
pub fn launch_installed(core: &WizardCore, dir: &str) -> Result<(), String> {
    let install = core.install_target()?;
    let dir = nest_root_dir(
        Path::new(dir.trim().trim_end_matches('\\')),
        &core.config.product.name,
        install.root_dir_folder.as_deref(),
    );
    let mut ctx = InstallContext::new(
        core.config.product.name.clone(),
        core.config.product.version.clone(),
        dir,
        false,
    );
    ctx.main_exe = install.main_exe.clone();
    ctx.launch_after_install = true;
    crate::targets::install::launch(&ctx).map_err(|e| e.to_string())
}

/// The shortcut grouping identity: the manifest's `aumid` when declared,
/// else shun's `{publisher}.{product}` default.
pub fn shortcut_aumid_for(config: &ShunConfig) -> String {
    let configured = config.targets.iter().find_map(|t| match t {
        TargetConfig::Install(install) => install.aumid.clone(),
        _ => None,
    });
    configured
        .unwrap_or_else(|| default_aumid(config.product.publisher.as_deref(), &config.product.name))
}

/// The per-user data home (`%LOCALAPPDATA%` / XDG data home).
pub fn local_appdata() -> PathBuf {
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
    }
    #[cfg(not(windows))]
    {
        if let Some(home) = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
            return PathBuf::from(home);
        }
        match std::env::var_os("HOME") {
            Some(home) if !home.is_empty() => PathBuf::from(home).join(".local").join("share"),
            _ => std::env::current_dir().unwrap_or_default(),
        }
    }
}

/// The directory holding the running executable.
pub fn current_exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
}

/// The writability-probed location candidates for a product (the
/// command layer's thin entry — faces that hold a [`WizardCore`] get the
/// same list through [`WizardCore::refresh_dir_defaults`]).
pub fn location_defaults(product: &str) -> Vec<DirCandidate> {
    dir_candidates_for(product)
        .into_iter()
        .map(|(kind, path)| DirCandidate {
            kind,
            writable: crate::fs_probe::is_dir_writable(&path),
            path: path.to_string_lossy().into_owned(),
        })
        .collect()
}

/// The default install location: the first writable candidate, else the
/// per-user appdata folder.
pub fn default_location(product: &str) -> String {
    let candidates = dir_candidates_for(product);
    let paths: Vec<PathBuf> = candidates.iter().map(|(_, p)| p.clone()).collect();
    crate::fs_probe::first_writable(&paths)
        .or_else(|| paths.first().cloned())
        .unwrap_or_else(|| local_appdata().join(product))
        .to_string_lossy()
        .into_owned()
}

/// The root-drive guard for a bare path, config-aware (free-function
/// form for the command layer).
pub fn pad_root_dir(config: &ShunConfig, raw: &str) -> String {
    let folder = config.targets.iter().find_map(|t| match t {
        TargetConfig::Install(install) => install.root_dir_folder.clone(),
        _ => None,
    });
    nest_root_dir(
        Path::new(raw.trim()),
        &config.product.name,
        folder.as_deref(),
    )
    .to_string_lossy()
    .into_owned()
}

/// The candidate chain behind the location defaults, in pick priority
/// order: per-user appdata, Program Files, one folder per fixed drive.
fn dir_candidates_for(product: &str) -> Vec<(&'static str, PathBuf)> {
    use crate::fs_probe::DriveKind;

    let appdata = ("appdata", local_appdata().join(product));
    let program_files = (
        "program-files",
        std::env::var_os("ProgramFiles")
            .filter(|v| !v.is_empty())
            .map_or_else(|| PathBuf::from(r"C:\Program Files"), PathBuf::from)
            .join(product),
    );
    let fixed = crate::fs_probe::list_drives()
        .into_iter()
        .filter(|drive| drive.kind == DriveKind::Fixed)
        .map(|drive| ("drive", drive.mount.join(product)));
    std::iter::once(appdata)
        .chain(std::iter::once(program_files))
        .chain(fixed)
        .collect()
}

/// The wizard language a previous install recorded in its on-disk
/// manifest; `None` when absent or unparsable.
fn manifest_language_of(install_dir: &Path) -> Option<String> {
    read_manifest(install_dir)
        .ok()
        .and_then(|m| m.language)
        .filter(|l| !l.trim().is_empty())
}

/// Stops a running main executable before an overwrite install replaces
/// its payload: Windows locks a running image, so the extraction would
/// fail with os error 5. Returns whether a previous install was found.
#[cfg(windows)]
pub fn stop_running_app(install_dir: &Path, main_exe: &str) -> bool {
    use std::os::windows::process::CommandExt;

    if !install_dir.join(main_exe).is_file() {
        return false;
    }
    let _ = std::process::Command::new("taskkill")
        .args(["/F", "/IM", main_exe])
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .status();
    std::thread::sleep(std::time::Duration::from_millis(800));
    true
}

/// Non-Windows: no image-name file lock to fight.
#[cfg(not(windows))]
pub fn stop_running_app(_install_dir: &Path, _main_exe: &str) -> bool {
    false
}

/// The installer-owned files that live in the install dir but are never
/// payload entries (compared case-insensitively — Windows resolves
/// paths that way).
fn is_installer_artifact(path: &Path) -> bool {
    let name = path.to_string_lossy().to_lowercase();
    name == MANIFEST_PATH || name == UNINSTALLER_NAME || name == ".shun-portable"
}

/// Whether a manifest entry path may be joined under the install dir
/// for deletion: archive-relative and free of `..`.
fn entry_path_is_unsafe(path: &Path) -> bool {
    path.components().any(|c| {
        matches!(
            c,
            std::path::Component::Prefix(_)
                | std::path::Component::RootDir
                | std::path::Component::ParentDir
        )
    })
}

/// The on-disk payload manifest a previous install left behind.
fn read_installed_manifest(install_dir: &Path) -> Option<Vec<PayloadEntry>> {
    let bytes = std::fs::read(install_dir.join(MANIFEST_PATH)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Deletes the files the PREVIOUS payload delivered that the new one no
/// longer carries; best-effort, run only after a successful flow.
fn remove_stale_payload_files(install_dir: &Path, previous: &[PayloadEntry]) -> usize {
    if previous.is_empty() {
        return 0;
    }
    let mut removed = 0usize;
    for entry in previous {
        let path = Path::new(&entry.path);
        if entry_path_is_unsafe(path) || is_installer_artifact(path) {
            continue;
        }
        if std::fs::remove_file(install_dir.join(path)).is_ok() {
            removed += 1;
        }
    }
    if removed > 0 {
        prune_empty_dirs_below(install_dir);
    }
    removed
}

/// Removes empty directories under `root`, deepest first.
fn prune_empty_dirs_below(root: &Path) {
    fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let path = entry.path();
                collect(&path, out);
                out.push(path);
            }
        }
    }
    let mut dirs = Vec::new();
    collect(root, &mut dirs);
    for dir in dirs {
        let _ = std::fs::remove_dir(dir);
    }
}

/// Composes one log pane line from a structured record (English verbs —
/// faces may pass through or re-localize by kind).
fn log_line_of(record: &FlowLog) -> Option<(LogKind, String)> {
    match record {
        FlowLog::FileWrite { path } => Some((LogKind::Echo, format!("write {}", path.display()))),
        FlowLog::FileReuse { path } => Some((LogKind::Echo, format!("reuse {}", path.display()))),
        FlowLog::Warning { code, detail } => {
            let text = if code.is_empty() {
                detail.clone()
            } else if detail.is_empty() {
                code.clone()
            } else {
                format!("{code}: {detail}")
            };
            (!text.is_empty()).then_some((LogKind::Error, text))
        }
        FlowLog::ScriptBegin { name, .. } => {
            Some((LogKind::Step, format!("running script {name}")))
        }
        FlowLog::ScriptLine { line, .. } => Some((LogKind::Echo, line.clone())),
        FlowLog::CommandDone { command, .. } => Some((LogKind::Ok, format!("done {command}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core_with(config_json: &str) -> WizardCore {
        let config: crate::config::ShunConfig = serde_json::from_str(config_json).unwrap();
        WizardCore::new(config, BTreeMap::new())
    }

    #[test]
    fn rail_order_is_stable() {
        assert_eq!(Step::RAIL[0], Step::Language);
        assert_eq!(Step::RAIL[4], Step::Done);
        assert_eq!(Step::Language.back_from(), None);
        let mut core = core_with(BASIC);
        core.go(Step::License);
        assert_eq!(core.back_from(), Some(Step::Location));
    }

    #[test]
    fn entering_install_resets_the_run_state() {
        let mut core = core_with(BASIC);
        core.state.progress = Some(42);
        core.state.failure = Some("old".into());
        core.push_log(LogKind::Echo, "old line".into());
        core.go(Step::Install);
        assert_eq!(core.state.progress, None);
        assert_eq!(core.state.failure, None);
        assert!(core.state.log.is_empty());
    }

    #[test]
    fn apply_event_folds_percent_label_and_log() {
        let mut core = core_with(BASIC);
        core.apply_event(&FlowEvent::Progress {
            phase: FlowPhase::Extract,
            step: "Extracting app.exe".into(),
            percent: Some(42),
        });
        core.apply_event(&FlowEvent::Log {
            record: FlowLog::FileWrite {
                path: PathBuf::from("app.exe"),
            },
        });
        assert_eq!(core.state.progress, Some(42));
        assert_eq!(core.state.flow_step, "Extracting app.exe");
        assert_eq!(core.state.log.len(), 1);
        assert_eq!(core.state.log[0].kind, LogKind::Echo);
    }

    #[test]
    fn main_exe_falls_back_to_the_product_name() {
        let core = core_with(BASIC);
        assert_eq!(core.main_exe(), "app.exe");
    }

    const BASIC: &str =
        r#"{"product":{"name":"app","version":"1.0"},"targets":[{"kind":"install","local":true}]}"#;
}
