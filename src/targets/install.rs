//! Install target: NSIS-like registration (ARP, uninstaller, shortcuts,
//! deep links) and a portable mode that writes no registry at all.
//!
//! The Windows backend follows the wowsp NSIS template as its executable
//! spec: HKCU ARP entry, a self-copying `uninstall.exe`, and a start-menu
//! shortcut. Portable mode writes a `.shun-portable` marker and skips all
//! registration — the same convention wowsp apps detect for data placement.

use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::config::InstallConfig;
use crate::error::ShunError;
use crate::flow::{Flow, FlowEvent, FlowPhase};
// Only the Windows registration streams warning records; the import
// would be unused (and fatal under -D warnings) on other targets.
#[cfg(windows)]
use crate::flow::FlowLog;
use crate::payload::{MANIFEST_PATH, PayloadEntry, PayloadSource};

/// Marker file enabling portable (便捷) mode for a delivered copy.
pub const PORTABLE_MARKER: &str = ".shun-portable";

/// The environment variable shun exports to payload script steps (the
/// duckscript / Python runners — see `docs/en/design/scripting.md`):
/// the wizard language, so a payload can write the user's choice into
/// the installed application's own configuration. The flow only
/// exports the fact — what a payload does with it is the payload's own
/// script ([`InstallContext::script_env`]).
pub const LANGUAGE_ENV: &str = "SHUN_LANGUAGE";

/// The install directory as exported to payload script steps — the
/// root-padded path the flow actually delivered into (see
/// [`InstallContext::script_env`]).
pub const INSTALL_DIR_ENV: &str = "SHUN_INSTALL_DIR";

/// Whether the delivery ran in portable mode (`1`/`0`), as exported to
/// payload script steps — post-install relocation hooks branch on it
/// (portable copies keep their data beside the executable).
pub const PORTABLE_ENV: &str = "SHUN_PORTABLE";

/// The veto variable a `check-dir` script hook sets (to a
/// human-readable reason) to refuse the proposed install directory; the
/// wizard shows the value verbatim. Read and cleared by the check
/// wrapper in `crate::wizard` around every check-dir run.
pub const DIR_BLOCKED_ENV: &str = "SHUN_DIR_BLOCKED";

/// Name of the uninstaller binary copied into the install directory.
#[cfg(windows)]
pub const UNINSTALLER_NAME: &str = "uninstall.exe";
/// Name of the uninstaller binary copied into the install directory.
#[cfg(not(windows))]
pub const UNINSTALLER_NAME: &str = "uninstall";

/// A context-menu verb the registration backends expose on the app's
/// launchers (Explorer verbs on Windows, Desktop Actions on Linux).
#[derive(Debug, Clone, PartialEq)]
pub struct VerbSpec {
    /// Stable verb id — the registry segment / desktop-action id.
    pub key: String,

    /// Display string shown in the menu.
    pub display: String,

    /// What the verb invokes.
    pub target: VerbTarget,
}

/// What a context-menu verb invokes.
#[derive(Debug, Clone, PartialEq)]
pub enum VerbTarget {
    /// Open the install directory in the file manager.
    DataFolder,
    /// Run the copied uninstaller.
    Uninstall,
    /// Launch the app entry point with extra arguments.
    App { arguments: String },
}

/// Where the install lands: per-user (the default — HKCU / `~/.local` /
/// `~/Applications`, no elevation) or machine-wide (Windows: HKLM,
/// all-users Start Menu, public desktop; the shell self-elevates).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InstallScope {
    /// Per-user surfaces only (the default).
    #[default]
    User,
    /// Machine-wide surfaces (Windows-only today).
    Machine,
}

/// The wizard's answers to the `ask` policies, consumed by
/// [`InstallContext::apply_config`].
#[derive(Debug, Clone, Copy, Default)]
pub struct WizardAnswers {
    /// The desktop-shortcut checkbox (`ask` policy; default checked).
    pub desktop_shortcut: bool,
    /// The start-menu-shortcut checkbox (`ask` policy; default checked).
    pub start_menu_shortcut: bool,
    /// The machine-scope choice (`ask` policy; default per-user).
    pub machine: bool,
    /// The launch-after-install checkbox (`ask` policy; default checked —
    /// the NSIS done-page convention).
    pub launch_after_install: bool,
}

impl WizardAnswers {
    /// The default-checked answers (the NSIS done-page convention):
    /// desktop and start-menu shortcuts on, per-user scope, launch
    /// after install on.
    pub fn defaults() -> Self {
        Self {
            desktop_shortcut: true,
            start_menu_shortcut: true,
            machine: false,
            launch_after_install: true,
        }
    }
}

impl From<&crate::config::VerbConfig> for VerbSpec {
    fn from(config: &crate::config::VerbConfig) -> Self {
        use crate::config::VerbConfig;
        match config {
            VerbConfig::DataFolder { key, display } => Self {
                key: key.clone(),
                display: display.clone(),
                target: VerbTarget::DataFolder,
            },
            VerbConfig::Uninstall { key, display } => Self {
                key: key.clone(),
                display: display.clone(),
                target: VerbTarget::Uninstall,
            },
            VerbConfig::App {
                key,
                display,
                arguments,
            } => Self {
                key: key.clone(),
                display: display.clone(),
                target: VerbTarget::App {
                    arguments: arguments.clone(),
                },
            },
        }
    }
}

/// Where and how an install-mode delivery lands on this machine.
#[derive(Debug, Clone, PartialEq)]
pub struct InstallContext {
    /// Product name (shortcut titles, ARP display name, registry key).
    pub product: String,

    /// Product version (ARP display version).
    pub version: String,

    /// Publisher shown in ARP (e.g. `celestia-island`).
    pub publisher: Option<String>,

    /// Final directory of the installed payload.
    pub install_dir: PathBuf,

    /// Payload-relative path of the app entry point the shortcut targets.
    pub main_exe: Option<PathBuf>,

    /// Portable mode: skip registry, shortcuts, and uninstaller entirely;
    /// all data stays beside the executable.
    pub portable: bool,

    /// Marker file name written into the install directory for portable
    /// copies (config `portable-marker`, default `.shun-portable`).
    pub portable_marker: String,

    /// Where the install lands (per-user default; machine-wide writes
    /// HKLM and all-users surfaces and needs elevation).
    pub scope: InstallScope,

    /// Create a desktop shortcut beside the start-menu one (local mode).
    pub desktop_shortcut: bool,

    /// Create the start-menu launcher (local mode).
    pub start_menu_shortcut: bool,

    /// Launch the installed application once the install succeeded — see
    /// [`launch`]. Resolved from `install.launch-after-install`.
    pub launch_after_install: bool,

    /// Context-menu verbs to register (local mode).
    pub verbs: Vec<VerbSpec>,

    /// URL schemes the app owns (`myapp://…`), registered as protocol
    /// handlers (local mode).
    pub deep_links: Vec<String>,

    /// Grouping identity stamped on the shortcuts (Windows
    /// `System.AppUserModel.ID`); the app should pass the same value to
    /// `SetCurrentProcessExplicitAppUserModelID`.
    pub aumid: Option<String>,

    /// Payload-relative icon file the Linux launcher references.
    pub icon: Option<PathBuf>,

    /// ARP `EstimatedSize` in KiB; computed by the flow from the manifest.
    pub estimated_size_kb: u32,

    /// The wizard language the install ran under (`en`, `zh-Hans`, ...),
    /// as picked on the wizard's first step. Exported to payload scripts
    /// as [`LANGUAGE_ENV`] and recorded in the on-disk manifest so later
    /// passes (repair/update) can see it; `None` when the shell offered
    /// no choice.
    pub language: Option<String>,
}

impl InstallContext {
    /// A context with the identity fields set and every registration
    /// knob at its no-op default (no desktop shortcut, no launch after
    /// install, no verbs, no icon; the start-menu launcher is on — the
    /// historical behavior for hand-built contexts; the flow fills
    /// `estimated_size_kb`). Chain [`Self::apply_config`] to resolve the
    /// config-declared knobs.
    pub fn new(product: String, version: String, install_dir: PathBuf, portable: bool) -> Self {
        Self {
            product,
            version,
            publisher: None,
            install_dir,
            main_exe: None,
            portable,
            portable_marker: PORTABLE_MARKER.to_string(),
            scope: InstallScope::User,
            desktop_shortcut: false,
            start_menu_shortcut: true,
            launch_after_install: false,
            verbs: Vec::new(),
            deep_links: Vec::new(),
            aumid: None,
            icon: None,
            estimated_size_kb: 0,
            language: None,
        }
    }

    /// The environment shun exports to payload script steps: the wizard
    /// language ([`LANGUAGE_ENV`]) when the context carries one — the
    /// variable is absent rather than blank, so scripts can distinguish
    /// "not chosen" from a value — plus the delivery facts hooks
    /// otherwise cannot know: [`INSTALL_DIR_ENV`] (where the payload
    /// landed, root-padded) and [`PORTABLE_ENV`] (`1`/`0`). Post-install
    /// relocation hooks (moving shipped packs into a cache root, writing
    /// stamps) ride exactly those two.
    pub fn script_env(&self) -> Vec<(String, String)> {
        let mut vars = Vec::new();
        if let Some(language) = &self.language {
            vars.push((LANGUAGE_ENV.to_string(), language.clone()));
        }
        vars.push((
            INSTALL_DIR_ENV.to_string(),
            self.install_dir.display().to_string(),
        ));
        vars.push((
            PORTABLE_ENV.to_string(),
            if self.portable { "1" } else { "0" }.to_string(),
        ));
        vars
    }

    /// Applies the install-target configuration knobs onto a context:
    /// the desktop- and start-menu-shortcut, launch-after-install and
    /// install-scope policies resolved against the wizard answers (`ask`
    /// consults them; the shared flow passes all-false answers and the
    /// shells apply shortcuts past the flow, at their done pages), the
    /// context-menu verbs, the deep-link schemes, the AUMID, and the
    /// launcher icon.
    pub fn apply_config(&mut self, install: &InstallConfig, answers: WizardAnswers) {
        use crate::config::{DesktopShortcutPolicy, ScopePolicy};
        self.desktop_shortcut = match install.desktop_shortcut {
            DesktopShortcutPolicy::Always => true,
            DesktopShortcutPolicy::Never => false,
            DesktopShortcutPolicy::Ask => answers.desktop_shortcut,
        };
        self.start_menu_shortcut = match install.start_menu_shortcut {
            DesktopShortcutPolicy::Always => true,
            DesktopShortcutPolicy::Never => false,
            DesktopShortcutPolicy::Ask => answers.start_menu_shortcut,
        };
        self.launch_after_install = match install.launch_after_install {
            DesktopShortcutPolicy::Always => true,
            DesktopShortcutPolicy::Never => false,
            DesktopShortcutPolicy::Ask => answers.launch_after_install,
        };
        self.scope = match install.scope {
            ScopePolicy::User => InstallScope::User,
            ScopePolicy::Machine => InstallScope::Machine,
            ScopePolicy::Ask => {
                if answers.machine {
                    InstallScope::Machine
                } else {
                    InstallScope::User
                }
            }
        };
        self.portable_marker = install
            .portable_marker
            .clone()
            .unwrap_or_else(|| PORTABLE_MARKER.to_string());
        self.verbs = install.verbs.iter().map(VerbSpec::from).collect();
        self.deep_links = install
            .deep_links
            .iter()
            .map(|scheme| url_scheme(scheme))
            .filter(|s| !s.is_empty())
            .collect();
        self.aumid = Some(
            install
                .aumid
                .clone()
                .unwrap_or_else(|| default_aumid(self.publisher.as_deref(), &self.product)),
        );
        self.icon = install.icon.clone();

        // A bare filesystem root target (a picked drive like `D:\`) never
        // receives the payload directly. The default lane
        // (`forbid-drive-root`, on since 0.6.30) refuses roots outright
        // in the shared directory guards before the flow runs — padding
        // here would mask that refusal; only the opted-out legacy pad
        // lane rewrites the target one folder level down
        // (`root-dir-folder`, the product name by default).
        if !install.forbid_drive_root {
            self.install_dir = nest_root_dir(
                &self.install_dir,
                &self.product,
                install.root_dir_folder.as_deref(),
            );
        }
    }
}

/// Whether a path addresses a bare filesystem root: a drive root (`D:\`,
/// `D:`, verbatim `\\?\C:\`), a UNC share root (`\\server\share`), or the
/// POSIX root (`/`). `.` and `..` segments resolve away first, so `D:\..`
/// and `/foo/..` are roots too. A payload delivered onto such a target
/// would scatter its files across the root (and an uninstall would try
/// to sweep them), so [`nest_root_dir`] pads one folder level under it
/// first.
pub fn is_fs_root(dir: &Path) -> bool {
    // A path is a bare root when it carries a prefix/root lead-in and no
    // normal component survives resolving `.`/`..` against it (`..` above
    // a root cannot climb out of it, hence the saturating subtraction).
    let starts_at_root = matches!(
        dir.components().next(),
        Some(Component::Prefix(_)) | Some(Component::RootDir)
    );
    if !starts_at_root {
        return false;
    }
    let mut depth = 0usize;
    for component in dir.components() {
        match component {
            Component::Normal(_) => depth += 1,
            Component::ParentDir => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    depth == 0
}

/// Pads one folder level under a bare filesystem root target (see
/// [`is_fs_root`]); every other path passes through unchanged. The
/// folder defaults to the product name and is customized via the
/// install target's `root-dir-folder` knob.
pub fn nest_root_dir(dir: &Path, product: &str, folder: Option<&str>) -> PathBuf {
    if !is_fs_root(dir) {
        return dir.to_path_buf();
    }
    let name = folder
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(product);
    // Rebuild the bare root textually — `D:\..` pads as `D:\Wowsp`, not
    // the odd `D:\..\Wowsp` — by keeping only the prefix/root lead-in.
    let mut padded = dir
        .components()
        .take_while(|component| matches!(component, Component::Prefix(_) | Component::RootDir))
        .fold(OsString::new(), |mut acc, component| {
            acc.push(component.as_os_str());
            acc
        });
    // Pin a separator unless the lead-in already ends with one. `D:\`
    // does, but `D:` would swallow the folder as a drive-relative
    // segment (`D:Wowsp`), and a UNC share root parsed without its
    // trailing separator (`\\server\share`) would fuse onto the share
    // name (`\\server\shareWowsp`).
    let ends_with_sep = padded
        .as_encoded_bytes()
        .last()
        .is_some_and(|byte| matches!(byte, b'/' | b'\\'));
    if !ends_with_sep {
        padded.push(if cfg!(windows) { "\\" } else { "/" });
    }
    padded.push(name);
    PathBuf::from(padded)
}

/// Normalizes a configured deep-link scheme to a registry-safe URL
/// scheme name: lowercase, `[a-z0-9+.-]` only, trailing `://` stripped
/// (empty schemes are dropped).
pub fn url_scheme(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches(':').trim_end_matches('/');
    trimmed
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '+' | '.' | '-'))
        .collect()
}

/// The default shortcut grouping identity: `{publisher}.{product}` with
/// each segment reduced to `[A-Za-z0-9-]` runs joined by dots (AUMIDs are
/// `Company.Product` style identifiers, not free text).
pub fn default_aumid(publisher: Option<&str>, product: &str) -> String {
    let segment = |raw: &str| {
        let cleaned: String = raw
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c
                } else {
                    '.'
                }
            })
            .collect();
        let collapsed: Vec<&str> = cleaned.split('.').filter(|s| !s.is_empty()).collect();
        collapsed.join("-")
    };
    match publisher {
        Some(publisher) if !segment(publisher).is_empty() => {
            format!("{}.{}", segment(publisher), segment(product))
        }
        _ => segment(product),
    }
}

/// Filesystem/registry-safe stem for a product name: the characters
/// Windows forbids in file names (`<>:"/\|?*` plus controls) become `_`,
/// and trailing dots/spaces (also illegal) are trimmed.
pub fn shortcut_stem(product: &str) -> String {
    let mut stem: String = product
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                '_'
            } else {
                c
            }
        })
        .collect();
    while stem.ends_with('.') || stem.ends_with(' ') {
        stem.pop();
    }
    if stem.is_empty() {
        stem.push('_');
    }
    stem
}

/// The on-disk install manifest (`shun-manifest.json` in the install
/// directory): the payload entries the uninstall pass consumes, plus
/// the install-time metadata later passes (repair/update) need. The
/// same file name inside the payload archive stays the bare
/// [`PayloadEntry`] list — only the delivered copy carries the wrapper.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstallManifest {
    /// The wizard language the install ran under (the context's
    /// `language`, also exported to scripts as [`LANGUAGE_ENV`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,

    /// The payload entries, per the archive manifest.
    pub entries: Vec<PayloadEntry>,
}

/// Reads the on-disk install manifest written by [`InstallFlow::run`].
/// Manifests delivered by pre-0.4.1 installs carry the bare entry list;
/// both shapes parse.
pub fn read_manifest(install_dir: &Path) -> Result<InstallManifest, ShunError> {
    let raw = std::fs::read(install_dir.join(MANIFEST_PATH))?;
    // The new wrapper first; the legacy bare entry list degrades to a
    // manifest without metadata.
    if let Ok(manifest) = serde_json::from_slice::<InstallManifest>(&raw) {
        return Ok(manifest);
    }
    let entries: Vec<PayloadEntry> = serde_json::from_slice(&raw)
        .map_err(|e| ShunError::Config(format!("manifest parse: {e}")))?;
    Ok(InstallManifest {
        language: None,
        entries,
    })
}

/// Runs the config's phase script hooks (docs/en/design/scripting.md):
/// `prepare` (before any payload work), `post-install` (after the flow
/// completes, before the wizard's done page), `pre-uninstall` (before
/// the removal begins). The scripts ride the payload (`prepare` and
/// `pre-uninstall` read them pre-extraction) and run on the embedded
/// duckscript interpreter with the wizard language exported
/// (`SHUN_LANGUAGE`). A failing hook fails the flow: a silently
/// skipped hook would lie about what ran.
pub fn run_phase_hooks(
    phase: &str,
    config: &crate::config::ShunConfig,
    ctx: &InstallContext,
    on_event: &mut dyn FnMut(FlowEvent),
    read: &dyn Fn(&Path) -> Option<Vec<u8>>,
) -> Result<(), ShunError> {
    let Some(hooks) = &config.script else {
        return Ok(());
    };
    run_script_hooks(phase, hooks, ctx, on_event, read)
}

/// The scripted lane (feature `scripting`, default on): each matching
/// hook runs on the embedded duckscript interpreter with the wizard
/// language exported (`SHUN_LANGUAGE`).
#[cfg(feature = "scripting")]
fn run_script_hooks(
    phase: &str,
    hooks: &crate::config::ScriptHooksConfig,
    ctx: &InstallContext,
    on_event: &mut dyn FnMut(FlowEvent),
    read: &dyn Fn(&Path) -> Option<Vec<u8>>,
) -> Result<(), ShunError> {
    use crate::flow::FlowLog;
    use duckscript::types::runtime::Context as DuckContext;

    if hooks.runner != "duckscript" {
        return Err(ShunError::Config(format!(
            "unsupported script runner: {} (only `duckscript` ships)",
            hooks.runner
        )));
    }
    for hook in hooks.hooks.iter().filter(|h| h.phase == phase) {
        let path = Path::new(&hook.script);
        let bytes = read(path).ok_or_else(|| ShunError::MissingEntry(path.to_path_buf()))?;
        let script = std::str::from_utf8(&bytes)
            .map_err(|e| ShunError::Config(format!("script {} is not utf-8: {e}", hook.script)))?;
        on_event(FlowEvent::Log {
            record: FlowLog::ScriptBegin {
                name: hook.script.clone(),
            },
        });
        let mut context = DuckContext::new();
        duckscriptsdk::load(&mut context.commands)
            .map_err(|e| ShunError::Config(format!("duckscript sdk load: {e}")))?;
        // The wizard language rides into the script's environment.
        for (key, value) in ctx.script_env() {
            // SAFETY: the flow runs single-threaded here, and the vars
            // are shun's own SHUN_* namespace.
            unsafe { std::env::set_var(&key, &value) };
        }
        duckscript::runner::run_script(script, context, None)
            .map_err(|e| ShunError::Config(format!("script {}: {e}", hook.script)))?;
        on_event(FlowEvent::Log {
            record: FlowLog::CommandDone {
                command: hook.script.clone(),
            },
        });
    }
    Ok(())
}

/// The featureless lane: a manifest declaring hooks against a build
/// without the `scripting` feature is a configuration error, never a
/// silent skip — the flow would lie about what ran (see
/// [`run_phase_hooks`]' contract above).
#[cfg(not(feature = "scripting"))]
fn run_script_hooks(
    phase: &str,
    hooks: &crate::config::ScriptHooksConfig,
    _ctx: &InstallContext,
    _on_event: &mut dyn FnMut(FlowEvent),
    _read: &dyn Fn(&Path) -> Option<Vec<u8>>,
) -> Result<(), ShunError> {
    let _ = phase;
    if !hooks.hooks.is_empty() {
        return Err(ShunError::Config(
            "script hooks need the `scripting` cargo feature (default on; embedded \
             builds with default-features = false must enable it)"
                .into(),
        ));
    }
    Ok(())
}

/// The install delivery flow: extract the payload with progress, persist
/// the on-disk manifest (consumed by uninstall), then either register
/// (local mode) or drop the portable marker (portable mode).
pub struct InstallFlow<'a> {
    /// Payload to deliver.
    pub payload: &'a dyn PayloadSource,

    /// Per-OS registration backend.
    pub registration: &'a dyn Registration,

    /// Where and how the payload lands.
    pub ctx: InstallContext,
}

impl Flow for InstallFlow<'_> {
    fn run(&self, on_event: &mut dyn FnMut(FlowEvent)) -> Result<(), ShunError> {
        on_event(FlowEvent::Started);

        // Audit shun-R1-3: the payload used to extract directly into the
        // final install directory — an interruption left a half-written
        // tree with no manifest (uninstallable) or a stale manifest
        // pointing at the OLD file set. Extract into a staging directory
        // and atomically rename into place; an interrupted run leaves
        // only the staging directory (cleanable on the next attempt).
        let staging_dir = self.ctx.install_dir.with_extension("shun-partial");

        // Clean up any leftover staging from a previous interrupted run.
        if staging_dir.exists() {
            let _ = std::fs::remove_dir_all(&staging_dir);
        }
        std::fs::create_dir_all(&staging_dir)?;

        self.payload.extract(&staging_dir, on_event)?;

        let manifest = serde_json::to_vec_pretty(&InstallManifest {
            language: self.ctx.language.clone(),
            entries: self.payload.manifest().to_vec(),
        })
        .map_err(|e| ShunError::Config(format!("manifest serialize: {e}")))?;
        std::fs::write(staging_dir.join(MANIFEST_PATH), manifest)?;

        // Atomic swap: the final directory either has the complete
        // payload + manifest, or it does not exist at all. An existing
        // installation is replaced by moving it aside first so the
        // rename is a single atomic step — unless a survivor of the
        // stopped app still holds the directory (a taskkilled host
        // leaves its WebView2 children alive for a moment; they inherit
        // the install dir as their working directory, and Windows
        // refuses to rename a directory any process is using). The swap
        // retries within a bounded window, then degrades to an in-place
        // merge so an unattended update never strands the install on
        // the old version.
        if self.ctx.install_dir.exists() {
            let backup = self.ctx.install_dir.with_extension("shun-old");
            let _ = std::fs::remove_dir_all(&backup);
            swap_or_merge_into_place(
                &staging_dir,
                &self.ctx.install_dir,
                &backup,
                SWAP_LOCK_RETRY_WINDOW,
                on_event,
            )?;
        } else {
            std::fs::create_dir_all(
                self.ctx
                    .install_dir
                    .parent()
                    .unwrap_or(std::path::Path::new(".")),
            )?;
            std::fs::rename(&staging_dir, &self.ctx.install_dir)?;
        }

        let mut ctx = self.ctx.clone();
        let total_bytes = PayloadEntry::total_bytes(self.payload.manifest());
        ctx.estimated_size_kb = u32::try_from(total_bytes / 1024).unwrap_or(u32::MAX);

        if ctx.portable {
            std::fs::write(ctx.install_dir.join(&ctx.portable_marker), b"")?;
        } else {
            self.registration.register_logged(&ctx, on_event)?;
        }

        on_event(FlowEvent::Completed);
        Ok(())
    }
}

/// How long the atomic directory swap keeps retrying while a survivor
/// of the stopped app (a WebView2 child that inherited the install dir
/// as its working directory, say) still holds the rename back.
const SWAP_LOCK_RETRY_WINDOW: std::time::Duration = std::time::Duration::from_secs(30);
/// Poll cadence inside [`SWAP_LOCK_RETRY_WINDOW`].
const SWAP_LOCK_RETRY_POLL: std::time::Duration = std::time::Duration::from_millis(250);

/// Moves `staging_dir` over `install_dir` as one atomic rename when
/// nothing holds the directory; when a survivor keeps the directory
/// rename-locked past `retry_window`, degrades to merging the staged
/// files in place instead. Returns `true` when the atomic swap happened,
/// `false` when the in-place merge did. The caller owns the `backup`
/// slot's pre-clean; on the swap path this function clears it again.
fn swap_or_merge_into_place(
    staging_dir: &Path,
    install_dir: &Path,
    backup: &Path,
    retry_window: std::time::Duration,
    on_event: &mut dyn FnMut(FlowEvent),
) -> Result<bool, ShunError> {
    let deadline = std::time::Instant::now() + retry_window;
    let mut announced_wait = false;
    loop {
        match std::fs::rename(install_dir, backup) {
            Ok(()) => match std::fs::rename(staging_dir, install_dir) {
                Ok(()) => {
                    let _ = std::fs::remove_dir_all(backup);
                    return Ok(true);
                }
                Err(_) => {
                    // The staged side would not move — put the old tree
                    // back and let the merge lane try file by file. If
                    // even the restore fails (something recreated the
                    // install dir inside the window), fail loudly: the
                    // merge below would otherwise bury the only complete
                    // copy of the old tree in the backup slot.
                    if std::fs::rename(backup, install_dir).is_err() {
                        return Err(ShunError::Io(std::io::Error::other(format!(
                            "the previous install could not be restored from {}",
                            backup.display()
                        ))));
                    }
                    break;
                }
            },
            // A lock collision is the expected shape of "a survivor
            // still holds the directory": wait and retry. Anything else
            // is real.
            Err(e) if is_lock_collision(&e) => {}
            Err(e) => return Err(e.into()),
        }
        if std::time::Instant::now() >= deadline {
            break;
        }
        if !announced_wait {
            announced_wait = true;
            on_event(FlowEvent::Progress {
                phase: FlowPhase::Extract,
                step: "Waiting for the previous install to unlock".into(),
                percent: None,
            });
        }
        std::thread::sleep(SWAP_LOCK_RETRY_POLL);
    }
    merge_staging_in_place(staging_dir, install_dir)?;
    Ok(false)
}

/// The Windows raw errors that pin a directory rename: access denied
/// (5), sharing violation (32) and lock violation (33). POSIX has no
/// equivalent pin (a cwd-held directory renames fine), so non-Windows
/// builds treat every rename failure as real and never retry.
#[cfg(windows)]
fn is_lock_collision(e: &std::io::Error) -> bool {
    matches!(e.raw_os_error(), Some(5) | Some(32) | Some(33))
}

#[cfg(not(windows))]
fn is_lock_collision(_e: &std::io::Error) -> bool {
    false
}

/// Degraded delivery for a rename-locked install directory: every staged
/// file is moved onto its destination with the park-aside trick the
/// payload lane's `replace_file` uses for a running executable (a held
/// file can be renamed away but not overwritten in place), so the
/// install directory itself is never renamed. This lane trades the
/// atomic swap's all-or-nothing guarantee for completion: files the new
/// payload no longer carries are swept afterwards by the caller's
/// stale-file pass.
fn merge_staging_in_place(staging_dir: &Path, install_dir: &Path) -> Result<(), ShunError> {
    for entry in WalkDir::new(staging_dir) {
        let entry = entry
            .map_err(|e| ShunError::Io(std::io::Error::other(format!("walk staging: {e}"))))?;
        let relative = entry.path().strip_prefix(staging_dir).map_err(|_| {
            // WalkDir builds every path from the root it was given, so
            // this is unreachable — but a foreign path must never become
            // an escape hatch out of the install directory.
            ShunError::Io(std::io::Error::other(format!(
                "staged entry escapes the staging root: {}",
                entry.path().display()
            )))
        })?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        let target = install_dir.join(relative);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target)?;
            continue;
        }
        std::fs::create_dir_all(target.parent().unwrap_or(install_dir))?;
        if target.exists() {
            let parked = park_file_aside(&target)?;
            std::fs::rename(entry.path(), &target).map_err(|e| {
                // Restore the parked original so a failed move does not
                // leave the destination without its previous content.
                let _ = std::fs::rename(&parked, &target);
                ShunError::Io(e)
            })?;
            let _ = std::fs::remove_file(&parked);
        } else {
            std::fs::rename(entry.path(), &target)?;
        }
    }
    let _ = std::fs::remove_dir_all(staging_dir);
    Ok(())
}

/// Parks `target` aside under a `.shun-old-<ts>` sibling name so a staged
/// file can take its place — the same convention `replace_file` parks a
/// running executable under. Parked copies from earlier merges are
/// swept first (same-directory siblings, prefix-matched): a file a
/// survivor kept open through one update must not accumulate one parked
/// corpse per update, and the parked copy's own deletion fails
/// harmlessly while a process still holds it.
fn park_file_aside(target: &Path) -> Result<PathBuf, ShunError> {
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let prefix = format!("{name}.shun-old-");
    if let Some(dir) = target.parent() {
        if let Ok(read_dir) = std::fs::read_dir(dir) {
            for stale in read_dir.flatten() {
                if stale.file_name().to_string_lossy().starts_with(&prefix) {
                    let _ = std::fs::remove_file(stale.path());
                }
            }
        }
    }
    let parked = target.with_file_name(format!(
        "{prefix}{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::rename(target, &parked)?;
    Ok(parked)
}

/// Removes an install delivered by [`InstallFlow`]: registration trace,
/// payload files (per the on-disk manifest), marker, and the directory
/// itself once empty. Safe to call for both modes.
pub fn uninstall(ctx: &InstallContext, registration: &dyn Registration) -> Result<(), ShunError> {
    if !ctx.portable {
        registration.unregister(ctx)?;
    }

    let manifest_path = ctx.install_dir.join(MANIFEST_PATH);
    if manifest_path.exists() {
        // read_manifest accepts both the wrapper and the legacy bare
        // entry list, so copies installed by older shells still uninstall.
        for entry in read_manifest(&ctx.install_dir)?.entries {
            // Audit shun-R1-1: a poisoned manifest must not delete files
            // outside the install directory — skip traversal entries
            // loudly instead of removing them.
            if crate::payload::entry_path_is_unsafe(std::path::Path::new(&entry.path)) {
                // The crate is log-free by design — stderr is the
                // observable channel (mirrors the elevate.rs style).
                eprintln!(
                    "shun: uninstall skipping traversal entry {:?} (poisoned manifest?)",
                    entry.path
                );
                continue;
            }
            let _ = std::fs::remove_file(ctx.install_dir.join(entry.path));
        }
        let _ = std::fs::remove_file(&manifest_path);
        remove_empty_dirs_below(&ctx.install_dir);
    }

    let _ = std::fs::remove_file(ctx.install_dir.join(&ctx.portable_marker));

    let uninstaller = ctx.install_dir.join(UNINSTALLER_NAME);
    if uninstaller.exists() {
        schedule_self_delete(&uninstaller);
    }

    let _ = std::fs::remove_dir(&ctx.install_dir);
    Ok(())
}

/// Sweeps a RELOCATED install's files out of `old_dir` — the
/// manifest-precise half of [`uninstall`] with none of its registration
/// teardown: the relocation's successor install already owns the ARP
/// entry and shortcuts, so unregistering here would tear down the NEW
/// install instead. Deletes exactly what the old directory's on-disk
/// manifest lists (traversal-guarded the same way `uninstall` is),
/// plus the manifest itself, the portable marker and the copied
/// uninstaller, then prunes directories that became empty — foreign
/// files sharing the directory are never touched, and the directory
/// itself is removed only once empty. Returns how many files went.
///
/// A manifest that fails to parse aborts the sweep (an unknown file
/// set must not be half-deleted); individual locked leftovers are
/// skipped and simply not counted.
pub fn relocate_cleanup(old_dir: &Path, portable_marker: &str) -> Result<u32, ShunError> {
    let mut removed = 0u32;

    let manifest_path = old_dir.join(MANIFEST_PATH);
    if manifest_path.exists() {
        // read_manifest accepts both the wrapper and the legacy bare
        // entry list, so installs relocated from older shells sweep
        // their full file set too.
        for entry in read_manifest(old_dir)?.entries {
            // Audit shun-R1-1, relocation edition: a poisoned manifest
            // must not delete files outside the install directory —
            // skip traversal entries loudly instead of removing them.
            if crate::payload::entry_path_is_unsafe(std::path::Path::new(&entry.path)) {
                eprintln!(
                    "shun: relocation skipping traversal entry {:?} (poisoned manifest?)",
                    entry.path
                );
                continue;
            }
            if std::fs::remove_file(old_dir.join(&entry.path)).is_ok() {
                removed += 1;
            }
        }
        if std::fs::remove_file(&manifest_path).is_ok() {
            removed += 1;
        }
        remove_empty_dirs_below(old_dir);
    }

    if std::fs::remove_file(old_dir.join(portable_marker)).is_ok() {
        removed += 1;
    }

    // The self-copied uninstaller is not manifest-listed. It is not
    // running (this process is the NEW shell), so it deletes outright
    // — no self-delete scheduling needed.
    if std::fs::remove_file(old_dir.join(UNINSTALLER_NAME)).is_ok() {
        removed += 1;
    }

    let _ = std::fs::remove_dir(old_dir);
    Ok(removed)
}

/// Launches the installed application (the configured `main_exe` inside
/// the install dir), detached — the installer process may exit right
/// after. No-op when `ctx.launch_after_install` resolved false, and when
/// no entry point is declared at all.
pub fn launch(ctx: &InstallContext) -> Result<(), ShunError> {
    if !ctx.launch_after_install {
        return Ok(());
    }
    let Some(main_exe) = &ctx.main_exe else {
        return Ok(());
    };
    let exe = ctx.install_dir.join(main_exe);
    if !exe.is_file() {
        return Err(ShunError::MissingEntry(exe));
    }
    std::process::Command::new(&exe)
        .spawn()
        .map_err(ShunError::Io)?;
    Ok(())
}

/// Removes directories under `root` deepest-first (payload subdirectories
/// whose files the manifest pass just deleted), then `root` itself.
/// Failures are ignored — a non-empty directory simply stays.
fn remove_empty_dirs_below(root: &Path) {
    let mut dirs: Vec<PathBuf> = WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_dir() && e.path() != root)
        .map(|e| e.path().to_path_buf())
        .collect();
    dirs.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
    for dir in dirs {
        let _ = std::fs::remove_dir(dir);
    }
    let _ = std::fs::remove_dir(root);
}

/// Per-OS registration backend.
///
/// Windows implements ARP entries, uninstaller registration, and start
/// menu shortcuts. Unix backends follow platform conventions (desktop
/// entries, launcher registration) once the macOS and Linux outputs land.
pub trait Registration {
    /// Register the install: ARP entry, uninstaller, shortcuts.
    fn register(&self, ctx: &InstallContext) -> Result<(), ShunError>;

    /// [`Registration::register`] with an event sink: implementations
    /// that degrade non-fatally (a security policy denying a desktop
    /// shortcut, an AUMID stamp) stream [`FlowLog::Warning`] records
    /// here so wizard terminals can show *why* something is missing.
    /// The default wraps [`Registration::register`] silently.
    fn register_logged(
        &self,
        ctx: &InstallContext,
        on_event: &mut dyn FnMut(FlowEvent),
    ) -> Result<(), ShunError> {
        let _ = on_event;
        self.register(ctx)
    }

    /// Remove every trace [`Registration::register`] left behind.
    fn unregister(&self, ctx: &InstallContext) -> Result<(), ShunError>;
}

/// HKCU ARP key path for a product (per-user installs need no elevation).
/// The product name is stem-sanitized: a `\` would nest a subkey. The
/// same path hangs off HKLM for machine-wide installs.
pub fn arp_key_path(product: &str) -> String {
    format!(
        r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{}",
        shortcut_stem(product)
    )
}

/// The directory the product's currently registered install lives in,
/// read back from ARP (`InstallLocation`, HKCU first — the per-user
/// lane — then HKLM for machine-scope installs): the upgrade-in-place
/// target the wizard defaults and the dir-less headless lane lead with.
/// `None` when nothing is registered, the recorded location is not an
/// absolute path, or the directory no longer exists — a deleted install
/// must not pin the default.
#[cfg(windows)]
pub fn registered_install_dir(product: &str) -> Option<PathBuf> {
    use std::ffi::OsString;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};

    let arp = arp_key_path(product);
    [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE]
        .into_iter()
        .find_map(|root| {
            let key = winreg::RegKey::predef(root)
                .open_subkey_with_flags(&arp, KEY_READ)
                .ok()?;
            let location: OsString = key.get_value("InstallLocation").ok()?;
            let path = PathBuf::from(location);
            (path.is_absolute() && path.is_dir()).then_some(path)
        })
}

/// Non-Windows: no ARP registry to read an install location back from.
#[cfg(not(windows))]
pub fn registered_install_dir(_product: &str) -> Option<PathBuf> {
    None
}

/// Windows registration backend (ARP, uninstaller self-copy, start-menu
/// and desktop shortcuts, Explorer verbs, deep links) — per-user by
/// default; `InstallScope::Machine` writes the same surfaces machine-
/// wide (HKLM, all-users Start Menu, public desktop).
#[derive(Debug, Clone, Copy, Default)]
pub struct WindowsRegistration;

#[cfg(windows)]
impl Registration for WindowsRegistration {
    fn register(&self, ctx: &InstallContext) -> Result<(), ShunError> {
        self.register_logged(ctx, &mut |_| {})
    }

    fn register_logged(
        &self,
        ctx: &InstallContext,
        on_event: &mut dyn FnMut(FlowEvent),
    ) -> Result<(), ShunError> {
        // Uninstaller: copy this binary next to the payload (skipped when
        // we are already running from the install directory, i.e. when the
        // uninstaller re-registers itself).
        let uninstaller = ctx.install_dir.join(UNINSTALLER_NAME);
        match std::env::current_exe() {
            Ok(current) if current != uninstaller => {
                std::fs::create_dir_all(&ctx.install_dir)?;
                std::fs::copy(current, &uninstaller)?;
            }
            Ok(_) => {}
            Err(e) => return Err(ShunError::Io(e)),
        }

        let root = registry_root(ctx.scope);
        let (key, _) = root.create_subkey(arp_key_path(&ctx.product))?;
        key.set_value("DisplayName", &ctx.product)?;
        key.set_value("DisplayVersion", &ctx.version)?;
        if let Some(publisher) = &ctx.publisher {
            key.set_value("Publisher", publisher)?;
        }
        key.set_value("InstallLocation", &ctx.install_dir.as_os_str())?;
        // DisplayIcon on the application executable (not the icon-less
        // uninstaller) so the Apps list shows the product icon.
        let display_icon = ctx
            .main_exe
            .as_ref()
            .map(|exe| ctx.install_dir.join(exe))
            .unwrap_or_else(|| uninstaller.clone());
        key.set_value("DisplayIcon", &display_icon.as_os_str())?;
        key.set_value(
            "UninstallString",
            &format!("\"{}\" /uninstall", uninstaller.display()),
        )?;
        // Maintenance entries (修改/修复): both land on the uninstaller
        // UI, which hosts the repair (re-extract) action beside the
        // uninstall one — a bare launch of the wizard would read as a
        // fresh install instead.
        key.set_value(
            "ModifyPath",
            &format!("\"{}\" /uninstall", uninstaller.display()),
        )?;
        key.set_value(
            "RepairString",
            &format!("\"{}\" /uninstall", uninstaller.display()),
        )?;
        key.set_value("EstimatedSize", &ctx.estimated_size_kb)?;

        // Shortcuts: the start-menu one when the context resolved
        // `start-menu-shortcut`, plus the desktop one when the context
        // resolved `desktop-shortcut`. Both carry the AUMID (taskbar
        // grouping / pinning identity). Machine scope writes the
        // all-users Start Menu and the public desktop.
        if let Some(main_exe) = &ctx.main_exe {
            let target = ctx.install_dir.join(main_exe);
            let stem = shortcut_stem(&ctx.product);

            let mut start_menu_link = None;
            if ctx.start_menu_shortcut {
                let programs = start_menu_dir(ctx.scope)?;
                std::fs::create_dir_all(&programs)?;
                let link = programs.join(format!("{stem}.lnk"));
                write_shortcut(ctx, &target, &link, on_event)?;
                start_menu_link = Some(link);
            }

            if ctx.desktop_shortcut {
                // Best-effort: AV/EDR policies commonly deny `.lnk`
                // creation on the desktop specifically (fake-shortcut /
                // ransomware protection) — the desktop shortcut is a
                // convenience and never fails the install.
                if let Some(link) = desktop_dir(ctx.scope)
                    .ok()
                    .map(|d| d.join(format!("{stem}.lnk")))
                {
                    if let Err(e) = write_shortcut(ctx, &target, &link, on_event) {
                        warn(on_event, "desktop-shortcut-blocked", &e.to_string());
                    }
                }
            }

            register_verbs(ctx, main_exe)?;
            register_deep_links(ctx, main_exe)?;

            // Tell the shell the shortcut surfaces changed — without
            // this, a freshly written Start-menu .lnk only appears once
            // Explorer re-indexes on its own schedule.
            let mut changed = Vec::new();
            if let Some(link) = start_menu_link {
                changed.push(link);
            }
            if ctx.desktop_shortcut {
                if let Ok(desktop) = desktop_dir(ctx.scope) {
                    changed.push(desktop.join(format!("{stem}.lnk")));
                }
            }
            notify_shell_change(&changed);
        }

        Ok(())
    }

    fn unregister(&self, ctx: &InstallContext) -> Result<(), ShunError> {
        use winreg::enums::KEY_READ;

        let root = registry_root(ctx.scope);
        let arp = arp_key_path(&ctx.product);
        if root.open_subkey_with_flags(&arp, KEY_READ).is_ok() {
            root.delete_subkey(&arp)?;
        }

        if ctx.main_exe.is_some() {
            let stem = shortcut_stem(&ctx.product);
            if let Ok(programs) = start_menu_dir(ctx.scope) {
                let _ = std::fs::remove_file(programs.join(format!("{stem}.lnk")));
            }
            // The desktop shortcut is attempted unconditionally: the
            // config may have changed between install and uninstall.
            if let Ok(desktop) = desktop_dir(ctx.scope) {
                let _ = std::fs::remove_file(desktop.join(format!("{stem}.lnk")));
            }
            if let Some(main_exe) = &ctx.main_exe {
                unregister_verbs(ctx, main_exe);
            }
        }
        unregister_deep_links(ctx);
        if ctx.main_exe.is_some() {
            let stem = shortcut_stem(&ctx.product);
            let mut changed = Vec::new();
            if let Ok(programs) = start_menu_dir(ctx.scope) {
                changed.push(programs.join(format!("{stem}.lnk")));
            }
            if let Ok(desktop) = desktop_dir(ctx.scope) {
                changed.push(desktop.join(format!("{stem}.lnk")));
            }
            notify_shell_change(&changed);
        }

        // The uninstaller binary deletes itself (it is usually the process
        // being run) — see `schedule_self_delete`.
        Ok(())
    }
}

/// The registry root the install scope writes: HKCU per user, HKLM
/// machine-wide (the caller must hold an elevated token for HKLM).
#[cfg(windows)]
fn registry_root(scope: InstallScope) -> winreg::RegKey {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    match scope {
        InstallScope::User => RegKey::predef(HKEY_CURRENT_USER),
        InstallScope::Machine => RegKey::predef(HKEY_LOCAL_MACHINE),
    }
}

/// The Start Menu `Programs` directory for the scope: the user's
/// `%APPDATA%` folder, or the all-users `%ProgramData%` one.
#[cfg(windows)]
fn start_menu_dir(scope: InstallScope) -> Result<PathBuf, ShunError> {
    let (variable, which) = match scope {
        InstallScope::User => ("APPDATA", "the user start menu"),
        InstallScope::Machine => ("ProgramData", "the all-users start menu"),
    };
    std::env::var(variable)
        .map(|root| Path::new(&root).join(r"Microsoft\Windows\Start Menu\Programs"))
        .map_err(|_| ShunError::Config(format!("%{variable}% is not set ({which})")))
}

/// Streams one non-fatal degradation as a warning record (stderr too —
/// headless consoles see it inline).
#[cfg(windows)]
fn warn(on_event: &mut dyn FnMut(FlowEvent), code: &str, detail: &str) {
    eprintln!("shun: {code}: {detail}");
    on_event(FlowEvent::Log {
        record: FlowLog::Warning {
            code: code.to_string(),
            detail: detail.to_string(),
        },
    });
}

/// Notifies the shell that shortcut surfaces changed
/// (`SHChangeNotify`): per-item updates for the `.lnk` files plus one
/// association-change flush, so the Start menu, desktop, and icon
/// caches refresh immediately instead of on Explorer's own schedule —
/// the standard installer finisher.
#[cfg(windows)]
fn notify_shell_change(paths: &[PathBuf]) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::{
        SHCNE_ASSOCCHANGED, SHCNE_UPDATEITEM, SHCNF_PATH, SHChangeNotify,
    };

    unsafe {
        for path in paths {
            let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            SHChangeNotify(
                SHCNE_UPDATEITEM as i32,
                SHCNF_PATH,
                wide.as_ptr().cast(),
                std::ptr::null(),
            );
        }
        SHChangeNotify(
            SHCNE_ASSOCCHANGED as i32,
            0,
            std::ptr::null(),
            std::ptr::null(),
        );
    }
}

/// Creates a `.lnk` via mslnk, then stamps the AUMID on top through the
/// Shell property store (enhancement-only: a stamping failure never
/// blocks the install).
#[cfg(windows)]
fn write_shortcut(
    ctx: &InstallContext,
    target: &Path,
    link: &Path,
    on_event: &mut dyn FnMut(FlowEvent),
) -> Result<(), ShunError> {
    mslnk::ShellLink::new(target)
        .map_err(|e| ShunError::Config(format!("shortcut creation failed: {e}")))?
        .create_lnk(link)
        .map_err(|e| ShunError::Config(format!("shortcut creation failed: {e}")))?;
    if let Some(aumid) = &ctx.aumid {
        if let Err(e) = crate::targets::aumid::stamp(link, aumid) {
            warn(
                on_event,
                "aumid-stamp-blocked",
                &format!("{}: {e}", link.display()),
            );
        }
    }
    Ok(())
}

/// The desktop directory for the scope via `SHGetKnownFolderPath` — not
/// `%USERPROFILE%\Desktop`, which is wrong whenever the desktop is
/// redirected (OneDrive, domain policies). Machine scope resolves the
/// public desktop instead.
#[cfg(windows)]
fn desktop_dir(scope: InstallScope) -> Result<PathBuf, ShunError> {
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{
        FOLDERID_Desktop, FOLDERID_PublicDesktop, SHGetKnownFolderPath,
    };

    unsafe {
        let folder = match scope {
            InstallScope::User => &FOLDERID_Desktop,
            InstallScope::Machine => &FOLDERID_PublicDesktop,
        };
        let mut pwstr = std::ptr::null_mut();
        if SHGetKnownFolderPath(folder, 0, std::ptr::null_mut(), &mut pwstr) < 0 || pwstr.is_null()
        {
            return Err(ShunError::Config(
                "desktop folder lookup failed (SHGetKnownFolderPath)".into(),
            ));
        }
        let mut len = 0usize;
        while *pwstr.add(len) != 0 {
            len += 1;
        }
        let path = String::from_utf16_lossy(std::slice::from_raw_parts(pwstr as *const u16, len));
        CoTaskMemFree(pwstr.cast());
        Ok(PathBuf::from(path))
    }
}

/// Explorer-verb registry location for the entry executable:
/// `HKCU\Software\Classes\Applications\<exe file name>` — the documented
/// Application Registration surface; its verbs appear on the executable
/// and on shortcuts to it.
#[cfg(windows)]
fn applications_key(main_exe: &Path) -> String {
    let exe = main_exe
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "app.exe".into());
    format!(r"Software\Classes\Applications\{exe}")
}

#[cfg(windows)]
fn verb_command(ctx: &InstallContext, main_exe: &Path, target: &VerbTarget) -> String {
    match target {
        VerbTarget::DataFolder => format!("explorer.exe \"{}\"", ctx.install_dir.display()),
        VerbTarget::Uninstall => format!(
            "\"{}\" /uninstall",
            ctx.install_dir.join(UNINSTALLER_NAME).display()
        ),
        VerbTarget::App { arguments } => {
            let exe = ctx.install_dir.join(main_exe).display().to_string();
            let arguments = arguments.trim();
            if arguments.is_empty() {
                format!("\"{exe}\"")
            } else {
                format!("\"{exe}\" {arguments}")
            }
        }
    }
}

#[cfg(windows)]
fn register_verbs(ctx: &InstallContext, main_exe: &Path) -> Result<(), ShunError> {
    if ctx.verbs.is_empty() {
        return Ok(());
    }
    let root = registry_root(ctx.scope);
    for verb in &ctx.verbs {
        let (menu, _) = root.create_subkey(format!(
            r"{}\shell\{}",
            applications_key(main_exe),
            verb.key
        ))?;
        menu.set_value("", &verb.display)?;
        let (command, _) = root.create_subkey(format!(
            r"{}\shell\{}\command",
            applications_key(main_exe),
            verb.key
        ))?;
        command.set_value("", &verb_command(ctx, main_exe, &verb.target))?;
    }
    Ok(())
}

/// Removes the verb keys this install created, then the `shell` and
/// `Applications\<exe>` containers — but only where they are empty, so a
/// verb someone else registered under the same exe survives.
#[cfg(windows)]
fn unregister_verbs(ctx: &InstallContext, main_exe: &Path) {
    let root = registry_root(ctx.scope);
    for verb in &ctx.verbs {
        let _ = root.delete_subkey_all(format!(
            r"{}\shell\{}",
            applications_key(main_exe),
            verb.key
        ));
    }
    let _ = root.delete_subkey(format!(r"{}\shell", applications_key(main_exe)));
    let _ = root.delete_subkey(applications_key(main_exe));
}

#[cfg(not(windows))]
impl Registration for WindowsRegistration {
    fn register(&self, _ctx: &InstallContext) -> Result<(), ShunError> {
        Err(ShunError::Unsupported(
            "windows registration backend (compile on windows)",
        ))
    }

    fn unregister(&self, _ctx: &InstallContext) -> Result<(), ShunError> {
        Err(ShunError::Unsupported(
            "windows registration backend (compile on windows)",
        ))
    }
}

/// Deletes `exe` after a short delay when it is the currently running
/// process (a running binary cannot delete itself outright): a detached
/// `cmd` waits ~1s then removes the file. Non-running files delete
/// directly.
#[cfg(windows)]
fn schedule_self_delete(exe: &Path) {
    // CREATE_NO_WINDOW — NOT DETACHED_PROCESS: for a console-subsystem
    // child, DETACHED means "don't inherit the parent console", and
    // Windows then allocates a fresh VISIBLE console for cmd.exe — the
    // ping flash users report after every uninstall. NO_WINDOW keeps the
    // child console-less entirely.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    use std::os::windows::process::CommandExt;

    // Canonicalized comparison: the scheduled path and current_exe can
    // disagree in case, 8.3 short form, or the \\?\ prefix while naming
    // the same file — and a false negative lands in the direct-delete
    // branch, which cannot remove a running image, so the uninstaller
    // copy survives its own uninstall.
    let running = std::env::current_exe()
        .ok()
        .and_then(|current| std::fs::canonicalize(current).ok());
    let scheduled = std::fs::canonicalize(exe).ok();
    if scheduled.is_some() && scheduled == running {
        // The uninstaller cannot remove its own image before exiting, and
        // the synchronous rmdir in `uninstall` already ran while the file
        // was still there — so the delayed sweep deletes the file and
        // then the directory too (plain rmdir: a non-empty directory
        // simply fails and stays).
        let dir = exe
            .parent()
            .map(|d| d.display().to_string())
            .unwrap_or_default();
        let script = format!(
            "ping -n 2 127.0.0.1 > nul & del /q \"{}\" & rmdir \"{}\"",
            exe.display(),
            dir,
        );
        // Audit shun-R1-6: cmd.exe metacharacters in the installation
        // path (quotes, percent-variables) escape the surrounding quotes
        // and execute arbitrary commands at the installation privilege
        // (machine scope = elevated). Refuse to schedule when the path
        // contains them — the delayed delete silently no-ops instead.
        let path_str = exe.display().to_string();
        // `dir` is already the resolved String form (the same value the
        // schedule call above consumes) — the Windows-only compile caught
        // the stray to_string_lossy on a String that the ubuntu job's
        // cfg never type-checks (2026-10-15 master red).
        if path_str.contains('"')
            || path_str.contains('%')
            || dir.contains('"')
            || dir.contains('%')
        {
            eprintln!(
                "shun: refusing cmd-based self-delete: installation path \
                 contains cmd metacharacters (quote or percent); delete manually"
            );
            return;
        }
        let _ = std::process::Command::new("cmd")
            .arg("/C")
            .raw_arg(script)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    } else {
        let _ = std::fs::remove_file(exe);
    }
}

#[cfg(not(windows))]
fn schedule_self_delete(exe: &Path) {
    let _ = std::fs::remove_file(exe);
}

/// Registers the configured URL schemes as per-user protocol handlers
/// (`HKCU\Software\Classes\<scheme>` with the empty `URL Protocol`
/// value that marks a protocol class, plus the open command receiving
/// the URL as `%1`). Schemes are config-declared app-owned namespaces,
/// so unregistering deletes the whole key — the NSIS convention.
#[cfg(windows)]
fn register_deep_links(ctx: &InstallContext, main_exe: &Path) -> Result<(), ShunError> {
    if ctx.deep_links.is_empty() {
        return Ok(());
    }
    let root = registry_root(ctx.scope);
    let exe = ctx.install_dir.join(main_exe);
    for scheme in &ctx.deep_links {
        let (key, _) = root.create_subkey(format!(r"Software\Classes\{scheme}"))?;
        key.set_value("", &format!("URL:{} ({scheme})", ctx.product))?;
        key.set_value("URL Protocol", &"")?;
        let (open, _) =
            root.create_subkey(format!(r"Software\Classes\{scheme}\shell\open\command"))?;
        open.set_value("", &format!("\"{}\" \"%1\"", exe.display()))?;
    }
    Ok(())
}

#[cfg(windows)]
fn unregister_deep_links(ctx: &InstallContext) {
    if ctx.deep_links.is_empty() {
        return;
    }
    let root = registry_root(ctx.scope);
    for scheme in &ctx.deep_links {
        let _ = root.delete_subkey_all(format!(r"Software\Classes\{scheme}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stems_replace_filename_illegal_characters() {
        assert_eq!(shortcut_stem("Plain-Name"), "Plain-Name");
        assert_eq!(shortcut_stem("ShunDemo/Test *2"), "ShunDemo_Test _2");
        assert_eq!(shortcut_stem(r"Back\slash"), "Back_slash");
        // Trailing dots and spaces are also illegal in file names.
        assert_eq!(shortcut_stem("App. "), "App");
        assert_eq!(shortcut_stem("///"), "___");
    }

    #[test]
    fn arp_keys_use_the_sanitized_stem() {
        assert_eq!(
            arp_key_path(r"App\Nested"),
            r"Software\Microsoft\Windows\CurrentVersion\Uninstall\App_Nested"
        );
    }

    #[test]
    fn aumids_are_identifier_segments() {
        assert_eq!(
            default_aumid(Some("celestia-island"), "ShunDemo"),
            "celestia-island.ShunDemo"
        );
        assert_eq!(default_aumid(Some("A & B Co"), "My App"), "A-B-Co.My-App");
        assert_eq!(default_aumid(None, "Widget"), "Widget");
        assert_eq!(default_aumid(Some("///"), "Widget"), "Widget");
    }

    #[test]
    fn non_roots_pass_through_untouched() {
        assert!(!is_fs_root(Path::new("")));
        assert!(!is_fs_root(Path::new(".")));
        assert!(!is_fs_root(Path::new("relative/dir")));
        assert!(!is_fs_root(Path::new(r"D:\Apps")));
        assert_eq!(
            nest_root_dir(Path::new(r"D:\Apps"), "Wowsp", Some("Guard")),
            PathBuf::from(r"D:\Apps")
        );
    }

    #[cfg(windows)]
    #[test]
    fn drive_roots_nest_under_the_folder() {
        for root in [
            r"D:\",
            "D:/",
            "D:",
            r"\\?\C:\",
            r"\\server\share",
            r"\\server\share\",
        ] {
            assert!(is_fs_root(Path::new(root)), "{root} is a filesystem root");
        }
        // The product name is the default folder; a customized one wins.
        assert_eq!(
            nest_root_dir(Path::new(r"D:\"), "Wowsp", None),
            PathBuf::from(r"D:\Wowsp")
        );
        assert_eq!(
            nest_root_dir(Path::new("D:/"), "Wowsp", None),
            PathBuf::from("D:/Wowsp")
        );
        // A prefix-only path pins the separator — `D:Wowsp` would be
        // drive-relative (the current directory of D:), not the root.
        assert_eq!(
            nest_root_dir(Path::new("D:"), "Wowsp", None),
            PathBuf::from(r"D:\Wowsp")
        );
        assert_eq!(
            nest_root_dir(Path::new(r"\\server\share"), "Wowsp", Some("Tools")),
            PathBuf::from(r"\\server\share\Tools")
        );
        // Blank customizations fall back to the product name.
        assert_eq!(
            nest_root_dir(Path::new(r"D:\"), "Wowsp", Some("  ")),
            PathBuf::from(r"D:\Wowsp")
        );
        // `..` segments resolve away: these are all bare drive roots,
        // and the padding rebuilds the clean root text.
        for root in [r"D:\..", r"D:\Apps\..", r"D:\Apps\..\.."] {
            assert!(is_fs_root(Path::new(root)), "{root} resolves to a root");
        }
        assert_eq!(
            nest_root_dir(Path::new(r"D:\Apps\.."), "Wowsp", None),
            PathBuf::from(r"D:\Wowsp")
        );
        // Relative paths never count, whatever they resolve to.
        for relative in ["..", "relative/.."] {
            assert!(!is_fs_root(Path::new(relative)));
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn posix_root_nests_under_the_folder() {
        assert!(is_fs_root(Path::new("/")));
        assert_eq!(
            nest_root_dir(Path::new("/"), "Wowsp", None),
            PathBuf::from("/Wowsp")
        );
        // `..` above the root cannot climb out of it.
        assert!(is_fs_root(Path::new("/foo/..")));
        assert_eq!(
            nest_root_dir(Path::new("/foo/.."), "Wowsp", None),
            PathBuf::from("/Wowsp")
        );
    }

    #[test]
    fn apply_config_pads_a_root_install_dir() {
        let root = if cfg!(windows) {
            PathBuf::from(r"D:\")
        } else {
            PathBuf::from("/")
        };
        let padded = if cfg!(windows) {
            PathBuf::from(r"D:\Wowsp")
        } else {
            PathBuf::from("/Wowsp")
        };

        // The default lane refuses roots outright (the shared directory
        // guards reject them before the flow); apply_config must not
        // mask that refusal with a silent rewrite.
        let mut ctx = InstallContext::new("Wowsp".into(), "1.0.0".into(), root.clone(), false);
        ctx.apply_config(
            &crate::config::InstallConfig::default(),
            WizardAnswers::defaults(),
        );
        assert_eq!(
            ctx.install_dir, root,
            "the default (forbid-drive-root) leaves roots to the guards"
        );

        // The opted-out legacy pad lane keeps its rewrite.
        let mut ctx = InstallContext::new("Wowsp".into(), "1.0.0".into(), root.clone(), false);
        ctx.apply_config(
            &crate::config::InstallConfig {
                forbid_drive_root: false,
                ..Default::default()
            },
            WizardAnswers::defaults(),
        );
        assert_eq!(ctx.install_dir, padded, "roots pad under the product name");

        let mut ctx = InstallContext::new("Wowsp".into(), "1.0.0".into(), root, false);
        ctx.apply_config(
            &crate::config::InstallConfig {
                forbid_drive_root: false,
                root_dir_folder: Some("Wowsp CE".into()),
                ..Default::default()
            },
            WizardAnswers::defaults(),
        );
        assert_eq!(
            ctx.install_dir,
            padded.parent().unwrap().join("Wowsp CE"),
            "root-dir-folder customizes the padded name"
        );

        let mut ctx = InstallContext::new(
            "Wowsp".into(),
            "1.0.0".into(),
            PathBuf::from(r"D:\Apps"),
            false,
        );
        ctx.apply_config(
            &crate::config::InstallConfig::default(),
            WizardAnswers::defaults(),
        );
        assert_eq!(
            ctx.install_dir,
            PathBuf::from(r"D:\Apps"),
            "non-root targets pass through"
        );
    }

    #[test]
    fn apply_config_resolves_the_registration_knobs() {
        use crate::config::{DesktopShortcutPolicy, InstallConfig, VerbConfig};

        let mut ctx = InstallContext::new(
            "Shun Demo".into(),
            "0.0.1".into(),
            PathBuf::from("."),
            false,
        );
        ctx.publisher = Some("celestia-island".into());
        assert!(
            !ctx.launch_after_install,
            "the baseline context launches nothing"
        );

        let mut install = InstallConfig {
            desktop_shortcut: DesktopShortcutPolicy::Never,
            start_menu_shortcut: DesktopShortcutPolicy::Never,
            aumid: Some("explicit.aumid".into()),
            verbs: vec![VerbConfig::DataFolder {
                key: "open-data".into(),
                display: "Open data".into(),
            }],
            ..InstallConfig::default()
        };
        ctx.apply_config(
            &install,
            WizardAnswers {
                desktop_shortcut: true,
                start_menu_shortcut: true,
                machine: false,
                launch_after_install: true,
            },
        );
        assert!(!ctx.desktop_shortcut, "never wins over the wizard answer");
        assert!(
            !ctx.start_menu_shortcut,
            "never wins over the wizard answer"
        );
        assert_eq!(ctx.aumid.as_deref(), Some("explicit.aumid"));
        assert_eq!(ctx.verbs.len(), 1);
        assert_eq!(ctx.verbs[0].target, VerbTarget::DataFolder);

        // The portable marker defaults to the shun convention and can be
        // pointed at a product's own marker name.
        assert_eq!(ctx.portable_marker, PORTABLE_MARKER);
        install.portable_marker = Some(".portable".into());
        ctx.apply_config(&install, WizardAnswers::defaults());
        assert_eq!(ctx.portable_marker, ".portable");
        install.portable_marker = None;
        ctx.apply_config(&install, WizardAnswers::defaults());
        assert_eq!(ctx.portable_marker, PORTABLE_MARKER);

        install.desktop_shortcut = DesktopShortcutPolicy::Always;
        ctx.apply_config(&install, WizardAnswers::defaults());
        assert!(ctx.desktop_shortcut, "always wins over the wizard answer");

        install.desktop_shortcut = DesktopShortcutPolicy::Ask;
        ctx.apply_config(
            &install,
            WizardAnswers {
                desktop_shortcut: false,
                start_menu_shortcut: false,
                machine: false,
                launch_after_install: true,
            },
        );
        assert!(!ctx.desktop_shortcut, "ask follows the wizard answer");

        // The start-menu shortcut mirrors the desktop one: always wins,
        // ask follows the wizard answer.
        install.start_menu_shortcut = DesktopShortcutPolicy::Always;
        ctx.apply_config(
            &install,
            WizardAnswers {
                desktop_shortcut: false,
                start_menu_shortcut: false,
                machine: false,
                launch_after_install: true,
            },
        );
        assert!(
            ctx.start_menu_shortcut,
            "always wins over the wizard answer"
        );

        install.start_menu_shortcut = DesktopShortcutPolicy::Ask;
        ctx.apply_config(
            &install,
            WizardAnswers {
                desktop_shortcut: false,
                start_menu_shortcut: true,
                machine: false,
                launch_after_install: true,
            },
        );
        assert!(ctx.start_menu_shortcut, "ask follows the wizard answer");
        ctx.apply_config(
            &install,
            WizardAnswers {
                desktop_shortcut: false,
                start_menu_shortcut: false,
                machine: false,
                launch_after_install: true,
            },
        );
        assert!(!ctx.start_menu_shortcut, "ask follows the wizard answer");

        // The launch-after-install policy mirrors the shortcut ones:
        // `always` / `never` pin it, `ask` follows the wizard answer.
        install.launch_after_install = DesktopShortcutPolicy::Always;
        ctx.apply_config(
            &install,
            WizardAnswers {
                desktop_shortcut: true,
                start_menu_shortcut: true,
                machine: false,
                launch_after_install: false,
            },
        );
        assert!(
            ctx.launch_after_install,
            "always wins over the wizard answer"
        );

        install.launch_after_install = DesktopShortcutPolicy::Never;
        ctx.apply_config(
            &install,
            WizardAnswers {
                desktop_shortcut: true,
                start_menu_shortcut: true,
                machine: false,
                launch_after_install: true,
            },
        );
        assert!(
            !ctx.launch_after_install,
            "never wins over the wizard answer"
        );

        install.launch_after_install = DesktopShortcutPolicy::Ask;
        ctx.apply_config(
            &install,
            WizardAnswers {
                desktop_shortcut: true,
                start_menu_shortcut: true,
                machine: false,
                launch_after_install: true,
            },
        );
        assert!(ctx.launch_after_install, "ask follows the wizard answer");
        ctx.apply_config(
            &install,
            WizardAnswers {
                desktop_shortcut: true,
                start_menu_shortcut: true,
                machine: false,
                launch_after_install: false,
            },
        );
        assert!(!ctx.launch_after_install, "ask follows the wizard answer");

        // Without an explicit AUMID the default is generated from the
        // identity.
        install.aumid = None;
        ctx.apply_config(
            &install,
            WizardAnswers {
                desktop_shortcut: true,
                start_menu_shortcut: true,
                machine: false,
                launch_after_install: true,
            },
        );
        assert_eq!(ctx.aumid.as_deref(), Some("celestia-island.Shun-Demo"));

        // Deep-link schemes normalize to registry-safe form.
        install.deep_links = vec!["MyApp://".into(), " shundemo ".into(), "///".into()];
        ctx.apply_config(
            &install,
            WizardAnswers {
                desktop_shortcut: true,
                start_menu_shortcut: true,
                machine: false,
                launch_after_install: true,
            },
        );
        assert_eq!(
            ctx.deep_links,
            vec!["myapp".to_string(), "shundemo".to_string()]
        );
    }

    #[test]
    fn url_schemes_normalize_to_lowercase_identifiers() {
        assert_eq!(url_scheme("MyApp://"), "myapp");
        assert_eq!(url_scheme(" Web+Extension.1 "), "web+extension.1");
        assert_eq!(url_scheme("has spaces"), "hasspaces");
        assert_eq!(url_scheme("///"), "");
    }

    #[test]
    fn script_env_exports_the_wizard_language() {
        let mut ctx = InstallContext::new(
            "Wowsp".into(),
            "1.0.0".into(),
            PathBuf::from(r"D:\Games\Wowsp"),
            false,
        );
        // The delivery facts are always exported; the language only when
        // the context carries one (absent, never blank).
        assert_eq!(
            ctx.script_env(),
            vec![
                (INSTALL_DIR_ENV.to_string(), r"D:\Games\Wowsp".to_string()),
                (PORTABLE_ENV.to_string(), "0".to_string()),
            ],
            "no language chosen: only the delivery facts"
        );
        ctx.language = Some("zh-Hans".into());
        ctx.portable = true;
        assert_eq!(
            ctx.script_env(),
            vec![
                (LANGUAGE_ENV.to_string(), "zh-Hans".to_string()),
                (INSTALL_DIR_ENV.to_string(), r"D:\Games\Wowsp".to_string()),
                (PORTABLE_ENV.to_string(), "1".to_string()),
            ]
        );
    }

    #[test]
    fn install_manifest_roundtrips_and_reads_the_legacy_shape() {
        use crate::payload::MANIFEST_PATH;

        let dir = tempfile::tempdir().unwrap();
        let manifest_path = dir.path().join(MANIFEST_PATH);
        let manifest = InstallManifest {
            language: Some("zh-Hans".into()),
            entries: vec![PayloadEntry {
                path: "bin/app.exe".into(),
                size: 3,
                sha256: "abc".into(),
            }],
        };

        // The 0.4.1 shape: the metadata wrapper around the entries,
        // `language` skipped when unset.
        std::fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        assert_eq!(read_manifest(dir.path()).unwrap(), manifest);
        let bare = InstallManifest {
            language: None,
            entries: manifest.entries.clone(),
        };
        std::fs::write(&manifest_path, serde_json::to_vec_pretty(&bare).unwrap()).unwrap();
        assert!(
            !serde_json::to_string(&bare).unwrap().contains("language"),
            "an unset language is not serialized"
        );

        // The pre-0.4.1 shape (the bare entry list) still parses —
        // without metadata.
        std::fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest.entries).unwrap(),
        )
        .unwrap();
        let legacy = read_manifest(dir.path()).unwrap();
        assert_eq!(legacy.entries, manifest.entries);
        assert_eq!(legacy.language, None);
    }

    /// A relocation sweeps exactly the manifest-listed files (plus the
    /// manifest, marker and copied uninstaller) out of the abandoned
    /// directory: foreign files sharing it survive, and the directory
    /// itself goes only once empty — the game-folder install WoWSP
    /// ships its `relocate` rule for.
    #[test]
    fn relocate_cleanup_sweeps_manifest_files_and_spares_foreign_ones() {
        use crate::payload::MANIFEST_PATH;

        let shared = tempfile::tempdir().unwrap();
        let dir = shared.path();
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        std::fs::write(dir.join("bin").join("app.exe"), b"MZ").unwrap();
        std::fs::write(dir.join("README.txt"), b"payload file").unwrap();
        // The foreign neighbor the sweep must never touch.
        std::fs::write(dir.join("WorldOfWarships.exe"), b"MZ").unwrap();
        std::fs::write(dir.join(".portable"), b"").unwrap();
        std::fs::write(dir.join(UNINSTALLER_NAME), b"MZ").unwrap();
        std::fs::write(
            dir.join(MANIFEST_PATH),
            serde_json::to_vec(&InstallManifest {
                language: None,
                entries: vec![
                    PayloadEntry {
                        path: "bin/app.exe".into(),
                        size: 2,
                        sha256: "a".into(),
                    },
                    PayloadEntry {
                        path: "README.txt".into(),
                        size: 12,
                        sha256: "b".into(),
                    },
                ],
            })
            .unwrap(),
        )
        .unwrap();

        let removed = relocate_cleanup(dir, ".portable").unwrap();
        // The two payload files, the manifest, the marker, the
        // uninstaller — and nothing else had shun's name on it.
        assert_eq!(removed, 5);
        assert!(!dir.join("bin").join("app.exe").exists());
        assert!(!dir.join("README.txt").exists());
        assert!(!dir.join("bin").exists());
        assert!(!dir.join(MANIFEST_PATH).exists());
        assert!(!dir.join(".portable").exists());
        assert!(!dir.join(UNINSTALLER_NAME).exists());
        assert!(dir.join("WorldOfWarships.exe").exists());
        // The directory survives while the foreign file lives in it.
        assert!(dir.is_dir());

        // Once the neighbor goes, a repeat sweep takes the now-empty
        // directory too.
        std::fs::remove_file(dir.join("WorldOfWarships.exe")).unwrap();
        relocate_cleanup(dir, ".portable").unwrap();
        assert!(!dir.exists());
    }

    /// Stages a two-file tree (one at the root, one nested) with the
    /// given `marker` content, so the swap tests can tell old and new
    /// trees apart.
    fn stage_tree(root: &Path, marker: &str) {
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::write(root.join("README.txt"), marker).unwrap();
        std::fs::write(root.join("bin").join("app.exe"), marker.as_bytes()).unwrap();
    }

    /// Holds a file inside `dir` open — the deterministic, in-process
    /// stand-in for a lock survivor. Any open handle to a file inside a
    /// directory pins the directory against renames exactly like the
    /// field shape this fix exists for (a taskkilled host's WebView2
    /// children keeping the install dir as their working directory):
    /// the rename fails with a sharing violation no matter that the
    /// directory's own metadata is untouched. Dropping the handle is
    /// the survivor leaving.
    #[cfg(windows)]
    fn hold_directory(dir: &Path) -> std::fs::File {
        std::fs::File::create(dir.join("survivor-hold.lock")).unwrap()
    }

    #[test]
    fn unlocked_swap_replaces_the_tree_atomically() {
        let tmp = tempfile::tempdir().unwrap();
        let install_dir = tmp.path().join("app");
        let staging_dir = tmp.path().join("app.shun-partial");
        let backup = tmp.path().join("app.shun-old");
        stage_tree(&install_dir, "old");
        stage_tree(&staging_dir, "new");

        let swapped = swap_or_merge_into_place(
            &staging_dir,
            &install_dir,
            &backup,
            std::time::Duration::from_millis(300),
            &mut |_| {},
        )
        .unwrap();

        assert!(swapped, "nothing holds the directory — the swap wins");
        assert_eq!(
            std::fs::read_to_string(install_dir.join("README.txt")).unwrap(),
            "new"
        );
        assert!(!staging_dir.exists(), "the staged tree moved in");
        assert!(!backup.exists(), "the backup was swept");
    }

    /// The two held-directory lanes, run back to back: a survivor that
    /// stays put degrades to the in-place merge, and one that goes away
    /// lets the retry window complete the atomic swap after all.
    #[cfg(windows)]
    #[test]
    fn held_directory_degrades_then_recovers() {
        // ── A survivor that never leaves: the degraded merge wins. ──────
        let tmp = tempfile::tempdir().unwrap();
        let install_dir = tmp.path().join("app");
        let staging_dir = tmp.path().join("app.shun-partial");
        let backup = tmp.path().join("app.shun-old");
        stage_tree(&install_dir, "old");
        stage_tree(&staging_dir, "new");
        let _survivor = hold_directory(&install_dir);

        let swapped = swap_or_merge_into_place(
            &staging_dir,
            &install_dir,
            &backup,
            std::time::Duration::from_millis(300),
            &mut |_| {},
        )
        .unwrap();

        assert!(!swapped, "the survivor holds the rename — the merge wins");
        assert_eq!(
            std::fs::read_to_string(install_dir.join("README.txt")).unwrap(),
            "new"
        );
        assert_eq!(
            std::fs::read(install_dir.join("bin").join("app.exe")).unwrap(),
            b"new".to_vec(),
            "nested files merge too"
        );
        assert!(!staging_dir.exists(), "the staged tree was consumed");
        assert!(
            install_dir.join("README.txt").exists(),
            "the directory itself was never renamed away"
        );
        assert!(
            install_dir.join("survivor-hold.lock").exists(),
            "files the payload does not carry are left alone"
        );

        // ── The survivor leaves mid-retry: the atomic swap recovers. ────
        let install_dir = tmp.path().join("app2");
        let staging_dir = tmp.path().join("app2.shun-partial");
        let backup = tmp.path().join("app2.shun-old");
        stage_tree(&install_dir, "old");
        stage_tree(&staging_dir, "new");
        let survivor = hold_directory(&install_dir);
        // Run the swap on a helper thread so the test can end the
        // survivor while the retry loop is mid-window — the shape a
        // WebView2 subtree settling after its killed host produces.
        let swap = std::thread::spawn(move || {
            swap_or_merge_into_place(
                &staging_dir,
                &install_dir,
                &backup,
                std::time::Duration::from_secs(10),
                &mut |_| {},
            )
            .unwrap()
        });
        std::thread::sleep(std::time::Duration::from_millis(600));
        drop(survivor);
        assert!(
            swap.join().unwrap(),
            "the lock releases inside the retry window"
        );
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("app2").join("README.txt")).unwrap(),
            "new"
        );
        assert!(!tmp.path().join("app2.shun-old").exists());

        drop(_survivor);
    }
}
