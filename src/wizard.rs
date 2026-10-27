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
use crate::payload::{
    ArchivePayload, MANIFEST_PATH, PayloadEntry, PayloadSource, entry_path_is_unsafe,
};
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
    /// "installed" | "default-dir" | "appdata" | "program-files" | "drive".
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
    /// A pairing claim cashed by the pane, delivered as the manifest's
    /// `env-file` inside the install dir at delivery time. `None` until
    /// an operator accepts a displayed code.
    pub pairing_outcome: Option<PairingOutcome>,
}

/// The claimed credential the pane hands to the delivery lane.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PairingOutcome {
    /// The gateway the code was minted through.
    pub gateway: String,
    /// The device id the credential belongs to.
    pub node_id: String,
    /// The device secret — written once, never logged.
    pub device_secret: String,
    /// The accepting account.
    pub owner: String,
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
            pairing_outcome: None,
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
        config_main_exe(&self.config)
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
        // Manifest-aware: a configured install.default-dir leads the
        // chain (see location_defaults_for) so every face — webview,
        // egui, TUI — defaults to the pinned location alike.
        let candidates = location_defaults_for(&self.config);
        let dir = candidates
            .iter()
            .find(|candidate| candidate.writable)
            .map(|candidate| candidate.path.clone())
            .or_else(|| candidates.first().map(|candidate| candidate.path.clone()));
        if let Some(dir) = dir {
            self.state.dir = dir;
            self.state.dir_writable = Some(true);
        }
        self.state.candidates = candidates;
    }

    /// The directory a relocation pass should move an existing install
    /// out of: the first location candidate that already holds an
    /// install of this product AND matches a `relocate`-action
    /// `dir-rules` entry. Faces consult this once at startup — the
    /// manual-rerun path, where the wizard's leading candidate IS the
    /// flagged install (the registered location leads the chain) — and
    /// seed the wizard straight into the location step with the
    /// relocation notice. `None` when no candidate needs relocating;
    /// the updater-driven path passes `--relocate-from` explicitly
    /// instead.
    pub fn relocation_candidate(&self) -> Option<String> {
        location_defaults_for(&self.config)
            .into_iter()
            .find(|candidate| relocation_required(&self.config, &candidate.path, None).is_some())
            .map(|candidate| candidate.path)
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
    /// touching state. In the default configuration (root refusal on)
    /// this is the identity: the typed path stays as typed so the
    /// refusal is visible where it was typed.
    pub fn nested_path(&self, raw: &str) -> String {
        pad_root_dir(&self.config, raw)
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
    /// the config; the flow itself creates no shortcuts — the done page
    /// owns that answer).
    fn install_context(
        &self,
        dir: &str,
        portable: bool,
        machine: bool,
    ) -> Result<InstallContext, String> {
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
                machine,
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
    /// Machine-wide scope (HKLM / all-users surfaces). Faces without a
    /// scope toggle — and every elevated relaunch that already passed the
    /// gate — file `false`; the headless lane is the only `true` source
    /// after the UAC gate.
    pub machine: bool,
    /// The previous install directory a relocation leaves behind (the
    /// `--relocate-from` handover): after the flow lands in `dir`, only
    /// this product's manifest-listed files are swept out of that
    /// directory — foreign files stay untouched, and the directory
    /// itself goes only once empty. `None` for plain installs and
    /// in-place upgrades.
    pub relocate_from: Option<String>,
}

/// Why an install directory cannot take the payload. The shared guard
/// ([`check_install_dir`]) reports the first fault; the headless lane
/// renders it through [`DirCheckFault::message`] (English, like every
/// lib message) while the faces map it into their locale tables — a
/// script veto carries the script's own localized message verbatim.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum DirCheckFault {
    /// The directory is empty.
    Empty,
    /// The directory (or its nearest existing ancestor) is not
    /// writable.
    NotWritable,
    /// The directory is a bare filesystem root and the manifest did not
    /// opt out of the root guard (`forbid-drive-root`, default on).
    DriveRoot,
    /// The directory holds foreign files and the manifest did not opt
    /// out of the non-empty guard (`forbid-non-empty`, default on) —
    /// fresh installs refuse to tangle with a neighbor the update swap
    /// and uninstall sweep would endanger.
    NonEmpty,
    /// A `check-dir` script hook vetoed the location; the message is
    /// the script's own (it localizes via `SHUN_LANGUAGE`).
    Script {
        /// The veto reason, shown to the user verbatim.
        message: String,
    },
    /// A declarative `dir-rules` entry matched the location (fresh
    /// picks, or an existing install flagged for relocation); the
    /// message is the rule's own (localized through its
    /// `message-locales`).
    Rule {
        /// The reason, shown to the user verbatim.
        message: String,
    },
}

impl DirCheckFault {
    /// The headless-lane rendering.
    pub fn message(&self) -> String {
        match self {
            DirCheckFault::Empty => "the install directory must not be empty".into(),
            DirCheckFault::NotWritable => {
                "the target directory is not writable — pick another location".into()
            }
            DirCheckFault::DriveRoot => {
                "the install directory must not sit directly on a drive root — pick a \
                 folder under it"
                    .into()
            }
            DirCheckFault::NonEmpty => {
                "the target folder is not empty — pick an empty folder (or clean it) so \
                 nothing beside the install gets swept away later"
                    .into()
            }
            DirCheckFault::Script { message } => message.clone(),
            DirCheckFault::Rule { message } => message.clone(),
        }
    }
}

/// Runs the install-directory guards every face and the headless lane
/// share: the empty gate, the drive-root refusal (default on), the
/// non-empty refusal (default on), the declarative `dir-rules` and the
/// manifest's `check-dir` script hooks. The fresh-install guards — root,
/// non-empty, `block` rules, hooks — stand down for an upgrade: a
/// directory that already holds an install of this product (the on-disk
/// manifest or the main executable) had its location decision made when
/// it landed, and vetoing it now would strand silent updates (an
/// updatable install in a vetoed location must stay updatable). The one
/// exception is a `relocate` rule matching an existing install: the
/// location is dangerous to SHARE (the update swap moves the whole
/// directory aside), so the guard faults it and the silent lane hands
/// over to the wizard's relocation pass instead of updating in place.
/// The writability gate stays unconditional — an upgrade into an
/// unwritable location must fail loudly, never relocate.
///
/// Returns the first fault, or `None` when the location is clear. The
/// `portable` flag and `language` reach the check-dir hooks as the
/// `SHUN_PORTABLE` / `SHUN_LANGUAGE` delivery facts (see
/// docs/en/design/scripting.md).
pub fn check_install_dir(
    config: &ShunConfig,
    payload: &ArchivePayload,
    raw_dir: &str,
    language: Option<&str>,
    portable: bool,
) -> Option<DirCheckFault> {
    let dir = raw_dir.trim().trim_end_matches('\\').to_string();
    if dir.is_empty() {
        return Some(DirCheckFault::Empty);
    }
    let install = install_target_of(config);
    let forbid_root = install.is_none_or(|i| i.forbid_drive_root);
    // The legacy pad lane delivers into the padded directory — guard
    // what will actually land (the default lane refuses roots, so the
    // raw path is the delivered path).
    let effective = if forbid_root {
        dir.clone()
    } else {
        pad_root_dir(config, &dir)
    };
    let path = Path::new(&effective);
    let upgrading =
        read_installed_manifest(path).is_some() || path.join(config_main_exe(config)).is_file();
    if !upgrading && forbid_root && crate::targets::install::is_fs_root(path) {
        return Some(DirCheckFault::DriveRoot);
    }
    if !crate::fs_probe::is_dir_writable(path) {
        return Some(DirCheckFault::NotWritable);
    }
    if upgrading {
        // Only a `relocate` rule or foreign content can fault an
        // upgrade — a `block` rule must never strand the silent
        // updates of an install that landed in its location before
        // the rule existed. Foreign content is the rule-less danger:
        // nothing in the manifest names the neighbor, but the swap
        // would carry it into the backup slot (see Upgrade swap
        // safety), so the interactive location step answers with the
        // same refusal the silent lane hands over to the wizard on.
        // Portable mode is exempt — data beside the executable is
        // that mode's design.
        if let Some(message) = relocate_rule_message(config, path, language) {
            return Some(DirCheckFault::Rule { message });
        }
        if !portable {
            if let Some(message) = foreign_share_reason(config, &effective, false) {
                return Some(DirCheckFault::Rule { message });
            }
        }
        return None;
    }
    // A fresh pick answers to every rule action — a `relocate` rule
    // vetoes taking the location anew exactly like a `block` one.
    if let Some(rule) = install
        .iter()
        .flat_map(|i| i.dir_rules.iter())
        .find(|r| r.matches(path))
    {
        return Some(DirCheckFault::Rule {
            message: rule
                .message_for(language)
                .unwrap_or_else(|| GENERIC_RULE_MESSAGE.into()),
        });
    }
    // The most specific veto wins: a script hook's own reason beats
    // the generic non-empty refusal below it.
    if let Some(message) = run_check_dir_hooks(config, payload, &effective, language, portable) {
        return Some(DirCheckFault::Script { message });
    }
    if install.is_none_or(|i| i.forbid_non_empty) && dir_has_content_beyond(path, install) {
        return Some(DirCheckFault::NonEmpty);
    }
    None
}

/// The fallback reason when a `dir-rules` entry declares no message of
/// its own (English, like every lib fallback).
const GENERIC_RULE_MESSAGE: &str =
    "a manifest install-location rule rejects this folder — pick another location";

/// Whether an existing install of this product sits in a directory a
/// `relocate`-action `dir-rules` entry flags. The silent lane asks this
/// before updating in place (a flagged location would endanger the
/// foreign files sharing it — the update swap moves the WHOLE directory
/// aside) and hands over to the wizard's relocation pass instead.
/// Returns the rule's reason for the handover line. `raw_dir` follows
/// the same padding rules as [`check_install_dir`].
pub fn relocation_required(
    config: &ShunConfig,
    raw_dir: &str,
    language: Option<&str>,
) -> Option<String> {
    let dir = raw_dir.trim().trim_end_matches('\\').to_string();
    if dir.is_empty() {
        return None;
    }
    let install = install_target_of(config);
    let forbid_root = install.is_none_or(|i| i.forbid_drive_root);
    let effective = if forbid_root {
        dir
    } else {
        pad_root_dir(config, &dir)
    };
    let path = Path::new(&effective);
    let upgrading =
        read_installed_manifest(path).is_some() || path.join(config_main_exe(config)).is_file();
    if !upgrading {
        return None;
    }
    relocate_rule_message(config, path, language)
}

/// Whether an existing install of this product shares its directory
/// with content shun never delivered — the rule-less sibling of
/// [`relocation_required`]: no `dir-rules` entry names the neighbor
/// (a player's own folder dropped beside the app, say), but the
/// update swap would still carry it into the backup slot and a
/// wholesale backup delete would destroy it. The silent lane asks
/// this beside the rule probe before updating in place and hands
/// over to the wizard's relocation pass alike; the location step's
/// upgrade lane faults on it for interactive runs. Portable mode is
/// exempt (data beside the executable is that mode's design), and a
/// fresh directory never fires it (the non-empty refusal already
/// guards that shape). Returns a reason naming up to three foreign
/// samples. `raw_dir` follows the same padding rules as
/// [`check_install_dir`].
pub fn foreign_share_reason(config: &ShunConfig, raw_dir: &str, portable: bool) -> Option<String> {
    if portable {
        return None;
    }
    let dir = raw_dir.trim().trim_end_matches('\\').to_string();
    if dir.is_empty() {
        return None;
    }
    let install = install_target_of(config);
    let forbid_root = install.is_none_or(|i| i.forbid_drive_root);
    let effective = if forbid_root {
        dir
    } else {
        pad_root_dir(config, &dir)
    };
    let path = Path::new(&effective);
    let upgrading =
        read_installed_manifest(path).is_some() || path.join(config_main_exe(config)).is_file();
    if !upgrading {
        return None;
    }
    let marker = install
        .and_then(|i| i.portable_marker.clone())
        .unwrap_or_else(|| crate::targets::install::PORTABLE_MARKER.to_string());
    let allowlist = install
        .map(|i| i.non_empty_allowlist.clone())
        .unwrap_or_default();
    match crate::targets::install::foreign_files_beyond_manifest(path, &marker, &allowlist) {
        Ok(foreign) if foreign.is_empty() => None,
        Ok(mut foreign) => {
            // Walk order is filesystem-dependent — sort (case-
            // insensitive, like the accounting's keys) for a stable
            // handover line (and stable tests).
            foreign.sort_by_key(|path| path.to_lowercase());
            let shown = foreign.len().min(3);
            let sample = foreign[..shown].join(", ");
            let ellipsis = if foreign.len() > shown { ", …" } else { "" };
            Some(format!(
                "the directory holds {} file(s) shun never delivered ({sample}{ellipsis}) \
                 — relocate the install into a folder of its own",
                foreign.len(),
            ))
        }
        // An unreadable tree is unknown, never safe — treat it as
        // foreign, exactly like the swap guard does.
        Err(_) => Some(
            "the directory holds content shun cannot inspect — relocate \
             the install into a folder of its own"
                .into(),
        ),
    }
}

/// The first `relocate`-action rule matching `path`, as its localized
/// reason — the relocation pass shared by the upgrade lane of
/// [`check_install_dir`] and [`relocation_required`].
fn relocate_rule_message(
    config: &ShunConfig,
    path: &Path,
    language: Option<&str>,
) -> Option<String> {
    install_target_of(config)?
        .dir_rules
        .iter()
        .filter(|rule| rule.action == crate::config::DirRuleAction::Relocate)
        .find(|rule| rule.matches(path))
        .map(|rule| {
            rule.message_for(language)
                .unwrap_or_else(|| GENERIC_RULE_MESSAGE.into())
        })
}

/// Whether `path` holds anything beyond the manifest's
/// `non-empty-allowlist` junk names (compared case-insensitively). A
/// missing directory counts as empty — the flow creates it.
fn dir_has_content_beyond(path: &Path, install: Option<&crate::config::InstallConfig>) -> bool {
    let read_dir = match std::fs::read_dir(path) {
        Ok(read_dir) => read_dir,
        // A missing directory counts as empty — the flow creates it.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return false,
        // An unreadable directory is unknown, not empty: the refusal
        // must fire, or an access-denied target slips the non-empty
        // guard and receives a payload beside content nobody can see.
        Err(_) => return true,
    };
    let allowlist = install
        .map(|i| i.non_empty_allowlist.as_slice())
        .unwrap_or_default();
    read_dir.flatten().any(|entry| {
        let name = entry.file_name();
        let name = name.to_string_lossy().to_lowercase();
        !allowlist.iter().any(|junk| junk.to_lowercase() == name)
    })
}

/// Runs the manifest's `check-dir` hooks (the payload-embedded
/// scripts, read pre-extraction exactly like `prepare`) and resolves
/// their verdict: a hook vetoes the location by setting
/// [`crate::targets::install::DIR_BLOCKED_ENV`] to a human-readable
/// reason, which surfaces verbatim; a hook that errors fails the check
/// with its error text. The delivery facts ride along with
/// `SHUN_INSTALL_DIR` in forward-slash form (a backslash is an escape
/// character in duckscript arguments).
fn run_check_dir_hooks(
    config: &ShunConfig,
    payload: &ArchivePayload,
    dir: &str,
    language: Option<&str>,
    portable: bool,
) -> Option<String> {
    let mut ctx = InstallContext::new(
        config.product.name.clone(),
        config.product.version.clone(),
        PathBuf::from(dir.replace('\\', "/")),
        portable,
    );
    ctx.language = language
        .map(str::to_string)
        .filter(|l| !l.trim().is_empty());
    // A stale veto from an earlier probe must not leak into this one.
    // SAFETY: shun's own SHUN_* namespace, set/read single-threaded per
    // lane (the same contract the hook runner's script_env export uses).
    unsafe { std::env::remove_var(crate::targets::install::DIR_BLOCKED_ENV) };
    let mut ignored = |_: FlowEvent| {};
    let verdict = crate::targets::install::run_phase_hooks(
        "check-dir",
        config,
        &ctx,
        &mut ignored,
        &|path| payload.read_file(path).map(|b| b.to_vec()),
    )
    .err()
    .map(|e| e.to_string())
    .or_else(|| {
        std::env::var(crate::targets::install::DIR_BLOCKED_ENV)
            .ok()
            .filter(|message| !message.trim().is_empty())
    });
    unsafe { std::env::remove_var(crate::targets::install::DIR_BLOCKED_ENV) };
    verdict
}

/// The manifest's install-target configuration, when it declares one.
fn install_target_of(config: &ShunConfig) -> Option<&InstallConfig> {
    config.targets.iter().find_map(|t| match t {
        TargetConfig::Install(install) => Some(install),
        _ => None,
    })
}

/// The main executable name: the manifest's `main-exe`, else the
/// product name with an `.exe` suffix — the shared resolution behind
/// [`WizardCore::main_exe`] for callers without a core.
fn config_main_exe(config: &ShunConfig) -> String {
    install_target_of(config)
        .and_then(|install| install.main_exe.clone())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| format!("{}.exe", config.product.name))
}

/// Drives one delivery through the shared path every face uses:
/// directory guards (writability, the root refusal, the check-dir
/// hooks) → overwrite hygiene (stop the running app, capture the
/// previous manifest) → the install flow → the stale-file sweep.
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
    let portable = request.mode != "local";
    // The check-dir hooks localize through the wizard language when the
    // request carries one, else the language the previous install
    // recorded (the repair path).
    let language = match request.language.as_deref() {
        Some(l) if !l.trim().is_empty() => Some(l.to_string()),
        _ => current_exe_dir().as_deref().and_then(manifest_language_of),
    };
    if let Some(fault) =
        check_install_dir(&core.config, payload, &dir, language.as_deref(), portable)
    {
        return Err(fault.message());
    }
    let mut ctx = core
        .install_context(&dir, portable, request.machine)
        .map_err(|e| e.to_string())?;
    ctx.language = language;

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
    // Prepare-phase script hooks (docs/en/design/scripting.md): the
    // payload may carry runner scripts that hold the flow's opening —
    // the demo's screenshot-window delay rides this.
    crate::targets::install::run_phase_hooks(
        "prepare",
        &core.config,
        &ctx,
        &mut |event| on_event(&event),
        &|path| payload.read_file(path).map(|b| b.to_vec()),
    )
    .map_err(|e| e.to_string())?;
    let flow = InstallFlow {
        payload,
        registration: &WindowsRegistration,
        ctx: ctx.clone(),
    };
    let mut forwarding = |event: FlowEvent| on_event(&event);
    flow.run(&mut forwarding).map_err(|e| e.to_string())?;
    let current: std::collections::HashSet<std::path::PathBuf> = payload
        .manifest()
        .iter()
        .map(|entry| entry.path.clone())
        .collect();
    let stale = remove_stale_payload_files(&install_dir, &previous, &current);
    if stale > 0 {
        on_event(&FlowEvent::Progress {
            phase: FlowPhase::Extract,
            step: format!("Removed {stale} stale file(s)"),
            percent: None,
        });
    }
    // post-install hooks: after the flow's Completed event but before
    // this returns — the wizard stays on the running page while they
    // run, so their log lines land in the live pane.
    crate::targets::install::run_phase_hooks(
        "post-install",
        &core.config,
        &ctx,
        &mut |event| on_event(&event),
        // The flow has extracted by now — read from the install dir
        // (the payload bytes are identical, but the on-disk copy is
        // what a repair would have run too).
        &|path| std::fs::read(install_dir.join(path)).ok(),
    )
    .map_err(|e| e.to_string())?;

    // A relocation's final step: sweep this product's own files out of
    // the abandoned directory. Only the old manifest's entries (plus
    // the manifest, marker and copied uninstaller) go — foreign files
    // sharing that directory stay untouched, and the directory itself
    // is removed only once empty. The new install is already complete,
    // so a leftover that refuses to delete (a locked old executable,
    // say) is reported, never fatal.
    if let Some(old) = request
        .relocate_from
        .as_deref()
        .map(str::trim)
        .filter(|old| !old.is_empty())
    {
        let old_path = PathBuf::from(old);
        if !same_location(&old_path, &install_dir) {
            if old_path.join(&main_exe).is_file() {
                on_event(&FlowEvent::Progress {
                    phase: FlowPhase::Prepare,
                    step: format!("Stopping the previous {main_exe}"),
                    percent: None,
                });
                stop_running_app(&old_path, &main_exe);
            }
            let marker = ctx.portable_marker.clone();
            let step = match crate::targets::install::relocate_cleanup(&old_path, &marker) {
                Ok(removed) => {
                    format!("Removed {removed} file(s) from the previous location")
                }
                Err(e) => format!("Could not fully clean the previous location: {e}"),
            };
            on_event(&FlowEvent::Progress {
                phase: FlowPhase::Register,
                step,
                percent: None,
            });
        }
    }
    Ok(())
}

/// Whether two paths address the same location: a case-insensitive
/// separator-normalized textual compare first (Windows paths, the
/// common `--relocate-from` handover form), then a canonicalized
/// compare when both sides exist. A relocation whose target IS its
/// origin ran as a plain in-place upgrade — there is nothing to clean.
fn same_location(a: &Path, b: &Path) -> bool {
    let normalize = |path: &Path| -> String {
        let mut text = path.to_string_lossy().replace('/', "\\");
        while text.ends_with('\\') {
            text.pop();
        }
        if cfg!(windows) {
            text.to_lowercase()
        } else {
            text
        }
    };
    if normalize(a) == normalize(b) {
        return true;
    }
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Drives the uninstall for the install dir this process sits in (the
/// uninstall page and the headless ARP path both land here).
pub fn run_uninstall(core: &WizardCore) -> Result<(), String> {
    run_uninstall_with_events(core, &mut |_| {})
}

/// The uninstall with the event stream surfaced (the headless lane
/// prints it; GUI faces drive their panes through the worker instead).
pub fn run_uninstall_with_events(
    core: &WizardCore,
    on_event: &mut dyn FnMut(&FlowEvent),
) -> Result<(), String> {
    let ctx = core.uninstall_context()?;
    // pre-uninstall hooks: after the uninstall context resolves (the
    // install dir this process sits in), before any removal work. The
    // scripts read from that dir on disk — the uninstaller carries no
    // payload, and the installed copy is what ran at install time.
    crate::targets::install::run_phase_hooks(
        "pre-uninstall",
        &core.config,
        &ctx,
        &mut |event| on_event(&event),
        &|path| std::fs::read(ctx.install_dir.join(path)).ok(),
    )
    .map_err(|e| e.to_string())?;
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
    // The pairing claim rides the delivery into the install dir as the
    // manifest's env-file — write-then-rename, 0600, no-op unpaired.
    if let Some(outcome) = &core.pairing_outcome {
        let env_file = core
            .config
            .steps
            .as_ref()
            .and_then(|steps| {
                steps
                    .iter()
                    .find(|s| s.kind == crate::config::StepKind::Pairing)
                    .and_then(|s| s.pairing.as_ref())
            })
            .and_then(|p| p.env_file.clone());
        if let Some(env_file) = env_file {
            let path = std::path::Path::new(dir.trim()).join(&env_file);
            let mut body = String::new();
            use std::fmt::Write as _;
            let _ = writeln!(body, "# Written by the installer's first-run pairing step.");
            let _ = writeln!(
                body,
                "# The device credential below was issued once by the pairing"
            );
            let _ = writeln!(body, "# service; treat it like a password.");
            // Every VALUE is control/whitespace-stripped: hostile input
            // can never break the line or smuggle another one.
            let safe = |v: &str| -> String {
                v.chars()
                    .filter(|c| !c.is_control() && !c.is_whitespace())
                    .collect::<String>()
                    .trim_end_matches('/')
                    .to_string()
            };
            let _ = writeln!(body, "SERVER_URL={}", safe(&outcome.gateway));
            let _ = writeln!(body, "DEVICE_SECRET={}", safe(&outcome.device_secret));
            let _ = writeln!(body, "BOOTSTRAP_NODE_ID={}", safe(&outcome.node_id));
            let _ = writeln!(body, "BOOTSTRAP_GATEWAY={}", safe(&outcome.gateway));
            // Audit shun-R1-4: create the file with 0600 from the start —
            // the old write-then-chmod had a brief 0644 window on Unix,
            // and Windows relied solely on inherited ACLs.
            let tmp = path.with_extension("env.tmp");
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt as _;
                let mut f = std::fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .mode(0o600)
                    .open(&tmp)
                    .map_err(|e| format!("create pairing credential: {e}"))?;
                use std::io::Write as _;
                f.write_all(body.as_bytes())
                    .map_err(|e| format!("write pairing credential: {e}"))?;
            }
            #[cfg(not(unix))]
            {
                std::fs::write(&tmp, body).map_err(|e| format!("write pairing credential: {e}"))?;
            }
            std::fs::rename(&tmp, &path).map_err(|e| format!("install pairing credential: {e}"))?;
        }
    }
    crate::targets::shortcuts::apply_shortcut_choices(
        &shortcut_aumid_for(&core.config),
        &core.config.product.name,
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

/// The install context for the machine-scope elevation gate: the gate
/// resolves the target's scope and relays the wizard language to the
/// elevated copy, so it needs a full context — but the shortcut answers
/// ride the relaunch flags (the elevated copy runs the headless lane,
/// which re-resolves them from its own argv), and the flow itself
/// never creates shortcuts. One construction, shared by every face.
pub fn elevation_context(
    config: &ShunConfig,
    dir: &str,
    portable: bool,
    machine: bool,
    language: Option<&str>,
) -> Result<InstallContext, String> {
    let install = config
        .targets
        .iter()
        .find_map(|t| match t {
            TargetConfig::Install(install) => Some(install.clone()),
            _ => None,
        })
        .ok_or("this configuration declares no install target")?;
    let mut ctx = InstallContext::new(
        config.product.name.clone(),
        config.product.version.clone(),
        PathBuf::from(dir),
        portable,
    );
    ctx.publisher = config.product.publisher.clone();
    ctx.main_exe = install.main_exe.clone();
    ctx.language = language
        .map(str::to_string)
        .filter(|l| !l.trim().is_empty());
    ctx.apply_config(
        &install,
        WizardAnswers {
            desktop_shortcut: false,
            start_menu_shortcut: false,
            launch_after_install: false,
            machine,
        },
    );
    Ok(ctx)
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

/// The writability-probed location candidates under the product-name
/// convention (`%LOCALAPPDATA%\<product>` first). Manifest-aware
/// callers — the shell's command layer, the egui face,
/// [`WizardCore::refresh_dir_defaults`] — go through
/// [`location_defaults_for`], which additionally leads with the
/// registered install location and a configured `install.default-dir`
/// when either exists.
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

/// [`location_defaults`], manifest-aware, upgrade-aware: the product's
/// registered install location (ARP `InstallLocation`, when that
/// directory still exists — see
/// [`crate::targets::install::registered_install_dir`]) leads the
/// chain, so an upgrade's preselected location is where the product
/// already lives instead of the manifest pin. See
/// [`location_defaults_with`] for the chain contract.
pub fn location_defaults_for(config: &ShunConfig) -> Vec<DirCandidate> {
    let previous = crate::targets::install::registered_install_dir(&config.product.name);
    location_defaults_with(config, previous.as_deref())
}

/// [`location_defaults_for`]'s chain, hermetic over the previous-install
/// input: `previous` (already existence-checked) leads, then a
/// configured `install.default-dir` (env-expanded, see
/// [`expand_env_vars`]), then the conventional chain — each MINUS a
/// path-equal, case-insensitive twin (duplicate rows would double up in
/// every picker).
pub fn location_defaults_with(config: &ShunConfig, previous: Option<&Path>) -> Vec<DirCandidate> {
    let mut seen: Vec<String> = Vec::new();
    let mut candidates = Vec::new();
    if let Some(dir) = previous {
        seen.push(dir.to_string_lossy().to_lowercase());
        candidates.push(DirCandidate {
            kind: "installed",
            writable: crate::fs_probe::is_dir_writable(dir),
            path: dir.to_string_lossy().into_owned(),
        });
    }
    if let Some(dir) = configured_default_dir(config) {
        // The pin carries its own kind (both faces render the
        // kind label; "appdata" would lie for off-appdata pins).
        let path = dir.to_string_lossy().to_lowercase();
        if !seen.contains(&path) {
            seen.push(path);
            candidates.push(DirCandidate {
                kind: "default-dir",
                writable: crate::fs_probe::is_dir_writable(&dir),
                path: dir.display().to_string(),
            });
        }
    }
    candidates.extend(
        location_defaults(&config.product.name)
            .into_iter()
            .filter(|candidate| !seen.contains(&candidate.path.to_lowercase())),
    );
    candidates
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

/// [`default_location`], manifest- and upgrade-aware (see
/// [`location_defaults_for`]): the first writable candidate of the
/// chain, led by the registered install when one exists.
pub fn default_location_for(config: &ShunConfig) -> String {
    let previous = crate::targets::install::registered_install_dir(&config.product.name);
    default_location_with(config, previous.as_deref())
}

/// [`default_location_for`], hermetic over the previous-install input
/// (see [`location_defaults_with`]).
pub fn default_location_with(config: &ShunConfig, previous: Option<&Path>) -> String {
    let candidates = location_defaults_with(config, previous);
    candidates
        .iter()
        .find(|candidate| candidate.writable)
        .map(|candidate| candidate.path.clone())
        .or_else(|| candidates.first().map(|candidate| candidate.path.clone()))
        .unwrap_or_else(|| default_location(&config.product.name))
}

/// The install target's `default-dir` knob, env-expanded — `None` when
/// the manifest leaves the location convention to the wizard.
fn configured_default_dir(config: &ShunConfig) -> Option<PathBuf> {
    let raw = config.targets.iter().find_map(|t| match t {
        TargetConfig::Install(install) => install.default_dir.as_deref(),
        _ => None,
    })?;
    Some(PathBuf::from(expand_env_vars(raw)))
}

/// Expands `%VAR%` and `${VAR}` references from the process environment
/// (unknown variables empty out — a half-expanded path would silently
/// install somewhere wrong, so the segment just vanishes; a bare `$`
/// and an unclosed marker stay literal). Manifests are shared across
/// users, so any user-specific location must ride an env var rather
/// than a literal.
fn expand_env_vars(raw: &str) -> String {
    fn push_var(out: &mut String, name: &str) {
        if let Some(value) = std::env::var_os(name).filter(|v| !v.is_empty()) {
            out.push_str(&value.to_string_lossy());
        }
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(idx) = rest.find(['%', '$']) {
        out.push_str(&rest[..idx]);
        let after = &rest[idx..];
        if let Some(body) = after.strip_prefix("${") {
            let Some(end) = body.find('}') else {
                out.push_str(after);
                return out;
            };
            push_var(&mut out, &body[..end]);
            rest = &body[end + 1..];
        } else if let Some(body) = after.strip_prefix('%') {
            let Some(end) = body.find('%') else {
                out.push_str(after);
                return out;
            };
            push_var(&mut out, &body[..end]);
            rest = &body[end + 1..];
        } else {
            // A bare `$` (no `${`): not a reference we expand.
            out.push('$');
            rest = &after[1..];
        }
    }
    out.push_str(rest);
    out
}

/// The root-drive guard for a bare path, config-aware (free-function
/// form for the command layer). The default lane (`forbid-drive-root`,
/// on) refuses roots outright in [`check_install_dir`] — padding here
/// would rewrite the path into something the user never typed, so it
/// returns the input unchanged; only the opted-out legacy lane pads
/// one folder level under the root.
pub fn pad_root_dir(config: &ShunConfig, raw: &str) -> String {
    let install = config.targets.iter().find_map(|t| match t {
        TargetConfig::Install(install) => Some(install),
        _ => None,
    });
    if install.is_none_or(|i| i.forbid_drive_root) {
        return raw.trim().to_string();
    }
    nest_root_dir(
        Path::new(raw.trim()),
        &config.product.name,
        install.and_then(|i| i.root_dir_folder.clone()).as_deref(),
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

/// The on-disk payload manifest a previous install left behind.
/// `read_manifest` accepts both the current `InstallManifest` wrapper
/// and the pre-0.4.1 bare entry list — a direct `Vec<PayloadEntry>`
/// parse here would read every modern manifest as empty and silently
/// disable the stale-file sweep (exactly what happened between 0.4.1
/// and this fix).
fn read_installed_manifest(install_dir: &Path) -> Option<Vec<PayloadEntry>> {
    read_manifest(install_dir)
        .ok()
        .map(|manifest| manifest.entries)
}

/// Deletes the files the PREVIOUS payload delivered that the new one no
/// longer carries; best-effort, run only after a successful flow.
/// Entries the current payload still delivers are skipped by path: the
/// sweep runs after the delivery landed, so removing them would delete
/// freshly installed files (both the swap and the degraded merge lane
/// pass through here).
fn remove_stale_payload_files(
    install_dir: &Path,
    previous: &[PayloadEntry],
    current: &std::collections::HashSet<PathBuf>,
) -> usize {
    if previous.is_empty() {
        return 0;
    }
    let mut removed = 0usize;
    for entry in previous {
        if current.contains(&entry.path) {
            continue;
        }
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

    #[test]
    fn stale_sweep_reads_wrapped_manifests_and_spares_current_files() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        // The post-0.4.1 wrapper shape: a direct Vec<PayloadEntry> parse
        // reads this as empty and silently disables the sweep (the bug
        // this pins).
        let manifest = crate::targets::install::InstallManifest {
            language: None,
            entries: vec![
                PayloadEntry {
                    path: PathBuf::from("kept.txt"),
                    size: 1,
                    sha256: "a".into(),
                },
                PayloadEntry {
                    path: PathBuf::from("dropped.txt"),
                    size: 1,
                    sha256: "b".into(),
                },
                PayloadEntry {
                    // The platform's uninstaller name (uninstall.exe on
                    // Windows, bare `uninstall` elsewhere) — the sweep
                    // must recognize it as an installer artifact.
                    path: PathBuf::from(crate::targets::install::UNINSTALLER_NAME),
                    size: 1,
                    sha256: "c".into(),
                },
            ],
        };
        std::fs::write(
            dir.join(MANIFEST_PATH),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        std::fs::write(dir.join("kept.txt"), "k").unwrap();
        std::fs::write(dir.join("dropped.txt"), "d").unwrap();
        std::fs::write(dir.join(crate::targets::install::UNINSTALLER_NAME), "u").unwrap();

        let previous = read_installed_manifest(dir).unwrap();
        assert_eq!(
            previous.len(),
            3,
            "the wrapper manifest parses — the pre-fix Vec parse read it as empty"
        );
        let current: std::collections::HashSet<PathBuf> =
            [PathBuf::from("kept.txt")].into_iter().collect();
        let removed = remove_stale_payload_files(dir, &previous, &current);

        assert_eq!(removed, 1, "only the dropped entry goes");
        assert!(dir.join("kept.txt").exists(), "current-payload files stay");
        assert!(
            !dir.join("dropped.txt").exists(),
            "dropped entries are swept"
        );
        assert!(
            dir.join(crate::targets::install::UNINSTALLER_NAME).exists(),
            "installer artifacts are never swept"
        );
    }

    /// A minimal config with an optional install.default-dir — the
    /// knob under test; everything else stays at the defaults.
    fn location_config(default_dir: Option<&str>) -> crate::config::ShunConfig {
        crate::config::ShunConfig {
            product: crate::config::ProductIdentity {
                name: "Wowsp".into(),
                version: "1.0.0".into(),
                publisher: None,
                logo: None,
            },
            payload: None,
            webview2: None,
            script: None,
            targets: vec![crate::config::TargetConfig::Install(
                crate::config::InstallConfig {
                    default_dir: default_dir.map(str::to_string),
                    ..Default::default()
                },
            )],
            shell: None,
            source: None,
            variants: None,
            update: None,
            attachments: Vec::new(),
            license_sysl: None,
            msix: None,
            license: None,
            license_locales: Default::default(),
            licenses: Vec::new(),
            custom_steps: Vec::new(),
            steps: None,
            signing: None,
        }
    }

    /// `%VAR%` and `${VAR}` expand; unknown vars vanish; a bare `$` and
    /// unclosed markers stay literal.
    #[test]
    fn env_vars_expand_in_both_spellings() {
        // SAFETY: a SHUN_-namespaced test variable; nothing else reads
        // it concurrently.
        unsafe { std::env::set_var("SHUN_TEST_LOC", r"D:\Base") };
        assert_eq!(
            expand_env_vars(r"%SHUN_TEST_LOC%\Programs\Wowsp"),
            r"D:\Base\Programs\Wowsp"
        );
        assert_eq!(
            expand_env_vars("${SHUN_TEST_LOC}/Programs/Wowsp"),
            r"D:\Base/Programs/Wowsp"
        );
        assert_eq!(expand_env_vars(r"%SHUN_TEST_MISSING%\App"), r"\App");
        assert_eq!(expand_env_vars(r"C:\costs $5"), r"C:\costs $5");
        assert_eq!(expand_env_vars("%SHUN_TEST_LOC"), "%SHUN_TEST_LOC");
        assert_eq!(expand_env_vars("${SHUN_TEST_LOC"), "${SHUN_TEST_LOC");
        // Two refs in one string, mixed spellings.
        assert_eq!(
            expand_env_vars(r"%SHUN_TEST_LOC%\${SHUN_TEST_LOC}"),
            r"D:\Base\D:\Base"
        );
        // Empty names vanish; an unclosed % keeps the WHOLE remainder
        // literal (a later well-formed ref does not rescue it).
        assert_eq!(expand_env_vars("a%%b"), "ab");
        assert_eq!(expand_env_vars("${}x"), "x");
        assert_eq!(
            expand_env_vars(r"100% off ${SHUN_TEST_LOC}"),
            r"100% off ${SHUN_TEST_LOC}"
        );
        // SAFETY: test-scope cleanup, see the set above.
        unsafe { std::env::remove_var("SHUN_TEST_LOC") };
    }

    /// A configured install.default-dir leads the candidate chain and
    /// becomes the default; an unconfigured manifest keeps the product
    /// convention byte-for-byte. Exercises the hermetic `_with` twins —
    /// the registry-reading `location_defaults_for` is covered by the
    /// ARP round-trip integration test (a machine's real installs would
    /// otherwise leak into these assertions; ARP keys are
    /// case-insensitive, so even the product NAME can collide).
    #[test]
    fn default_dir_leads_the_location_candidates() {
        // SAFETY: a SHUN_-namespaced test variable (its own name — the
        // sibling test mutates a different one, and cargo runs these
        // in parallel).
        unsafe { std::env::set_var("SHUN_TEST_PIN", r"D:\Base") };
        let pinned = location_config(Some(r"%SHUN_TEST_PIN%\Programs\Wowsp"));
        let candidates = location_defaults_with(&pinned, None);
        // The pin carries its own kind; the conventional chain follows
        // minus a path-equal twin.
        assert_eq!(candidates[0].kind, "default-dir");
        // The pin expanded (%SHUN_TEST_LOC% → the test value) and leads
        // the chain — the expanded form is what the faces display.
        assert!(
            candidates[0].path.contains("Base") && candidates[0].path.ends_with("Wowsp"),
            "first candidate is the expanded pinned dir: {}",
            candidates[0].path
        );
        // The default is the first WRITABLE candidate of the pinned
        // chain (a pin may point somewhere non-writable — a D:\ path on
        // Linux — in which case the conventional chain legitimately
        // wins; the structural contract is what both faces render).
        let default = default_location_with(&pinned, None);
        let expected = candidates
            .iter()
            .find(|candidate| candidate.writable)
            .map(|candidate| candidate.path.clone())
            .or_else(|| candidates.first().map(|candidate| candidate.path.clone()));
        assert_eq!(Some(default), expected);
        assert_eq!(
            location_defaults_with(&location_config(None), None),
            location_defaults("Wowsp"),
            "no knob: identical to the product-based chain"
        );
        // SAFETY: test-scope cleanup, see the set above.
        unsafe { std::env::remove_var("SHUN_TEST_PIN") };
    }

    /// The registered install location leads the chain ahead of the
    /// pinned default-dir, and a path-equal (case-insensitive) pin does
    /// not double up behind it — an upgrade's preselected location is
    /// where the product already lives.
    #[test]
    fn the_registered_install_leads_the_location_candidates() {
        unsafe { std::env::set_var("SHUN_TEST_PREV", r"D:\Games") };
        let pinned = location_config(Some(r"%SHUN_TEST_PREV%\Programs\Wowsp"));
        // A previous install AWAY from the pin (the relocation case the
        // headless lane guards: the upgrade must stay here, not jump to
        // the pinned default).
        let previous = Path::new(r"E:\Elsewhere\Wowsp");
        let candidates = location_defaults_with(&pinned, Some(previous));
        assert_eq!(candidates[0].kind, "installed");
        assert_eq!(candidates[0].path, previous.to_string_lossy());
        // The pin follows — a different path, same chain.
        assert_eq!(candidates[1].kind, "default-dir");
        // The pin twin-dedup is case-insensitive: a previous install at
        // the pinned location leaves exactly one row (the "installed"
        // one — the upgrade keeps its own kind label).
        let deduped = location_defaults_with(&pinned, Some(Path::new(r"D:\Games\PROGRAMS\WOWSP")));
        assert_eq!(
            deduped
                .iter()
                .filter(|candidate| candidate.path.to_lowercase() == r"d:\games\programs\wowsp")
                .count(),
            1,
            "no duplicated row for the registered install"
        );
        assert_eq!(deduped[0].kind, "installed");
        // Without a pin the previous install still leads.
        let unpinned = location_defaults_with(&location_config(None), Some(previous));
        assert_eq!(unpinned[0].kind, "installed");
        // SAFETY: test-scope cleanup, see the set above.
        unsafe { std::env::remove_var("SHUN_TEST_PREV") };
    }

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

    // ── Install-directory guards (check_install_dir) ──────────────────

    /// Serializes the guard tests that run check-dir hooks: the
    /// duckscript runner exports the SHUN_* delivery facts and the veto
    /// rides SHUN_DIR_BLOCKED — process-global state, held under one
    /// lock for the whole run (the same contract the scripting tests
    /// use).
    static GUARD_ENV_LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();

    fn guard_env_lock() -> std::sync::MutexGuard<'static, ()> {
        GUARD_ENV_LOCK
            .get_or_init(std::sync::Mutex::default)
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// A config whose payload carries one `check-dir` duckscript hook
    /// (the script body under test), plus the packed payload itself.
    fn guard_config(
        script_body: &str,
        forbid_root: bool,
    ) -> (crate::config::ShunConfig, ArchivePayload) {
        let mut config = location_config(None);
        if let Some(crate::config::TargetConfig::Install(install)) = config.targets.first_mut() {
            install.forbid_drive_root = forbid_root;
        }
        config.script = Some(crate::config::ScriptHooksConfig {
            runner: "duckscript".into(),
            hooks: vec![crate::config::ScriptHookConfig {
                phase: "check-dir".into(),
                script: "check.dk".into(),
            }],
        });
        let stage = tempfile::tempdir().unwrap();
        std::fs::write(stage.path().join("check.dk"), script_body).unwrap();
        let packed = crate::payload::pack_directory(stage.path()).unwrap();
        let payload = ArchivePayload::from_bytes(&packed).unwrap();
        (config, payload)
    }

    /// The empty gate answers first; a target whose nearest existing
    /// ancestor is a FILE cannot take the probe's temp file.
    #[test]
    fn dir_guards_refuse_empty_and_unwritable_dirs() {
        let _guard = guard_env_lock();
        let (config, payload) = guard_config("noop", true);
        assert_eq!(
            check_install_dir(&config, &payload, "   ", None, false),
            Some(DirCheckFault::Empty)
        );
        let tmp = tempfile::tempdir().unwrap();
        let blocker = tmp.path().join("blocker");
        std::fs::write(&blocker, "a file, not a directory").unwrap();
        assert_eq!(
            check_install_dir(
                &config,
                &payload,
                &blocker.join("sub").to_string_lossy(),
                None,
                false
            ),
            Some(DirCheckFault::NotWritable)
        );
    }

    /// A bare filesystem root is refused under the default
    /// `forbid-drive-root` — the payload would scatter across the root
    /// and an uninstall would try to sweep the drive.
    #[test]
    fn dir_guards_refuse_a_bare_root_by_default() {
        let _guard = guard_env_lock();
        let (config, payload) = guard_config("noop", true);
        let root = if cfg!(windows) { r"C:\" } else { "/" };
        assert_eq!(
            check_install_dir(&config, &payload, root, None, false),
            Some(DirCheckFault::DriveRoot)
        );
    }

    /// The WoWSP-shaped hook: a check-dir script vetoes a directory
    /// that contains the host application it must not cohabit (the
    /// uninstall sweep would take the host with it), and clears a
    /// directory without it.
    #[test]
    fn check_dir_hooks_veto_through_the_blocked_env() {
        let _guard = guard_env_lock();
        // `${…}` interpolates duckscript VARIABLES, not the process
        // environment — the delivery facts arrive via get_env (the
        // contract the scripting tests pin).
        let script = r#"
dir = get_env SHUN_INSTALL_DIR
wows = is_file ${dir}/WorldOfWarships.exe
if ${wows}
  set_env SHUN_DIR_BLOCKED "this is the game folder"
end
"#;
        let (config, payload) = guard_config(script, true);

        // Clean directory: no veto, the location is clear.
        let clean = tempfile::tempdir().unwrap();
        assert_eq!(
            check_install_dir(
                &config,
                &payload,
                &clean.path().to_string_lossy(),
                None,
                false
            ),
            None
        );

        // The game folder: the script's message surfaces verbatim.
        let game = tempfile::tempdir().unwrap();
        std::fs::write(game.path().join("WorldOfWarships.exe"), b"MZ").unwrap();
        assert_eq!(
            check_install_dir(
                &config,
                &payload,
                &game.path().to_string_lossy(),
                None,
                false
            ),
            Some(DirCheckFault::Script {
                message: "this is the game folder".into()
            })
        );

        // The veto never leaks past the check that read it.
        // SAFETY: shun-owned SHUN_* namespace, still under the lock.
        assert!(std::env::var(crate::targets::install::DIR_BLOCKED_ENV).is_err());
    }

    /// A directory that already holds an install of this product is an
    /// upgrade: both fresh-install guards stand down — a location
    /// vetoed today must never strand the silent updates of an install
    /// that landed there yesterday.
    #[test]
    fn dir_guards_skip_fresh_checks_for_an_existing_install() {
        let _guard = guard_env_lock();
        // The hook vetoes EVERYTHING it sees.
        let (config, payload) = guard_config("set_env SHUN_DIR_BLOCKED nope", true);

        // An on-disk manifest marks a previous install.
        let manifest = crate::targets::install::InstallManifest {
            language: None,
            entries: vec![],
        };
        let installed = tempfile::tempdir().unwrap();
        std::fs::write(
            installed.path().join(MANIFEST_PATH),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        assert_eq!(
            check_install_dir(
                &config,
                &payload,
                &installed.path().to_string_lossy(),
                None,
                false
            ),
            None
        );

        // The main executable alone marks one too (the updater's
        // `--dir` always points at the parent of the running exe) —
        // but without an on-disk manifest the foreign accounting
        // cannot vouch for the executable itself, so the upgrade
        // lane faults it like any other unaccounted content (the
        // swap guard refuses the same shape post-extraction).
        let exe_dir = tempfile::tempdir().unwrap();
        std::fs::write(exe_dir.path().join("Wowsp.exe"), b"MZ").unwrap();
        assert_eq!(
            check_install_dir(
                &config,
                &payload,
                &exe_dir.path().to_string_lossy(),
                None,
                false
            ),
            Some(DirCheckFault::Rule {
                message: "the directory holds 1 file(s) shun never delivered (Wowsp.exe) \
                         — relocate the install into a folder of its own"
                    .into()
            })
        );
    }

    /// The opt-out pad lane keeps its rewrite (and keeps running the
    /// script hooks for every non-root directory); the default lane
    /// leaves the typed path alone so the refusal is visible.
    #[test]
    fn pad_root_dir_follows_the_forbid_knob() {
        // A bare root textually identical on every CI leg (the repo's
        // other root tests use the same cfg split — `D:\` is a plain
        // relative path on the POSIX side).
        let (root, padded) = if cfg!(windows) {
            (r"C:\", r"C:\Wowsp")
        } else {
            ("/", "/Wowsp")
        };
        let mut config = location_config(None);
        assert_eq!(pad_root_dir(&config, root), root);
        assert_eq!(pad_root_dir(&config, r"D:\Games"), r"D:\Games");
        if let Some(crate::config::TargetConfig::Install(install)) = config.targets.first_mut() {
            install.forbid_drive_root = false;
        }
        assert_eq!(pad_root_dir(&config, root), padded);
        assert_eq!(pad_root_dir(&config, r"D:\Games"), r"D:\Games");
    }

    /// A fresh install refuses a non-empty target (the delivery must
    /// never tangle with foreign files), the OS junk allowlist softens
    /// it, and an existing install always passes — the never-strand
    /// rule the fresh guards live under.
    #[test]
    fn non_empty_guard_refuses_foreign_content_but_not_junk_or_upgrades() {
        let _guard = guard_env_lock();
        let (config, payload) = guard_config("noop", true);

        // A foreign file refuses the fresh install…
        let foreign = tempfile::tempdir().unwrap();
        std::fs::write(foreign.path().join("someone-elses.dat"), b"x").unwrap();
        assert_eq!(
            check_install_dir(
                &config,
                &payload,
                &foreign.path().to_string_lossy(),
                None,
                false
            ),
            Some(DirCheckFault::NonEmpty)
        );

        // …the default allowlist's OS junk does not…
        let junk = tempfile::tempdir().unwrap();
        std::fs::write(junk.path().join("desktop.ini"), b"[.ShellClassInfo]").unwrap();
        std::fs::write(junk.path().join("Thumbs.db"), b"x").unwrap();
        assert_eq!(
            check_install_dir(
                &config,
                &payload,
                &junk.path().to_string_lossy(),
                None,
                false
            ),
            None
        );

        // …and neither does an existing install re-answer the
        // NON-EMPTY refusal (its location decision was made when it
        // landed) — but a directory sharing space with foreign
        // content now faults through the upgrade lane's foreign
        // accounting instead: the swap would carry the neighbor into
        // the backup slot, so the location step asks for a folder of
        // the install's own (with no on-disk manifest, the main
        // executable itself counts as unaccounted).
        let installed = tempfile::tempdir().unwrap();
        std::fs::write(installed.path().join("Wowsp.exe"), b"MZ").unwrap();
        std::fs::write(installed.path().join("someone-elses.dat"), b"x").unwrap();
        assert_eq!(
            check_install_dir(
                &config,
                &payload,
                &installed.path().to_string_lossy(),
                None,
                false
            ),
            Some(DirCheckFault::Rule {
                message: "the directory holds 2 file(s) shun never delivered \
                         (someone-elses.dat, Wowsp.exe) — relocate the install \
                         into a folder of its own"
                    .into()
            })
        );

        // The opt-out restores the pre-0.6.30 anything-goes lane.
        let (mut relaxed, payload) = guard_config("noop", true);
        if let Some(crate::config::TargetConfig::Install(install)) = relaxed.targets.first_mut() {
            install.forbid_non_empty = false;
        }
        assert_eq!(
            check_install_dir(
                &relaxed,
                &payload,
                &foreign.path().to_string_lossy(),
                None,
                false
            ),
            None
        );
    }

    /// The declarative rules: a `block` rule vetoes fresh picks only,
    /// a `relocate` rule also flags an existing install (the silent
    /// lane then hands over to the wizard), and the rule's own
    /// localized message rides the fault.
    #[test]
    fn dir_rules_block_fresh_picks_and_relocate_flags_existing_installs() {
        let _guard = guard_env_lock();
        let (mut config, payload) = guard_config("noop", true);
        if let Some(crate::config::TargetConfig::Install(install)) = config.targets.first_mut() {
            install.dir_rules = vec![
                crate::config::DirRuleConfig {
                    files: vec!["WorldOfWarships.exe".into()],
                    action: crate::config::DirRuleAction::Relocate,
                    message: Some("the game folder".into()),
                    message_locales: [("zh-Hans".to_string(), "这是游戏目录".to_string())]
                        .into_iter()
                        .collect(),
                },
                crate::config::DirRuleConfig {
                    files: vec!["rival-tool.dat".into()],
                    action: crate::config::DirRuleAction::Block,
                    message: Some("a rival lives here".into()),
                    message_locales: Default::default(),
                },
            ];
        }

        // A fresh pick into the game folder is vetoed by the FIRST
        // matching rule, with the wizard-locale message.
        let game = tempfile::tempdir().unwrap();
        std::fs::write(game.path().join("WorldOfWarships.exe"), b"MZ").unwrap();
        assert_eq!(
            check_install_dir(
                &config,
                &payload,
                &game.path().to_string_lossy(),
                Some("zh-Hans"),
                false
            ),
            Some(DirCheckFault::Rule {
                message: "这是游戏目录".into()
            })
        );

        // A `block` rule vetoes its fresh pick with its own message…
        let rival = tempfile::tempdir().unwrap();
        std::fs::write(rival.path().join("rival-tool.dat"), b"x").unwrap();
        assert_eq!(
            check_install_dir(
                &config,
                &payload,
                &rival.path().to_string_lossy(),
                None,
                false
            ),
            Some(DirCheckFault::Rule {
                message: "a rival lives here".into()
            })
        );

        // …never an existing install does the BLOCK RULE fault: a
        // location disliked today must not strand the silent updates
        // of an install that landed there yesterday. The foreign
        // accounting still answers, though — the rival's file is
        // content shun never delivered, exactly what the swap must
        // not carry into the backup slot (the main executable counts
        // as unaccounted too: no on-disk manifest lists it).
        let rival_installed = tempfile::tempdir().unwrap();
        std::fs::write(rival_installed.path().join("Wowsp.exe"), b"MZ").unwrap();
        std::fs::write(rival_installed.path().join("rival-tool.dat"), b"x").unwrap();
        assert_eq!(
            check_install_dir(
                &config,
                &payload,
                &rival_installed.path().to_string_lossy(),
                None,
                false
            ),
            Some(DirCheckFault::Rule {
                message: "the directory holds 2 file(s) shun never delivered \
                         (rival-tool.dat, Wowsp.exe) — relocate the install \
                         into a folder of its own"
                    .into()
            })
        );
        // Portable mode is exempt from the foreign accounting — data
        // beside the executable is that mode's design.
        assert_eq!(
            check_install_dir(
                &config,
                &payload,
                &rival_installed.path().to_string_lossy(),
                None,
                true
            ),
            None
        );

        // An existing install in the GAME folder is flagged for
        // relocation — the one fresh-guard exception — through both
        // the guard and the silent lane's relocation_required probe.
        let legacy = tempfile::tempdir().unwrap();
        std::fs::write(legacy.path().join("Wowsp.exe"), b"MZ").unwrap();
        std::fs::write(legacy.path().join("WorldOfWarships.exe"), b"MZ").unwrap();
        assert_eq!(
            check_install_dir(
                &config,
                &payload,
                &legacy.path().to_string_lossy(),
                None,
                false
            ),
            Some(DirCheckFault::Rule {
                message: "the game folder".into()
            })
        );
        assert_eq!(
            relocation_required(&config, &legacy.path().to_string_lossy(), Some("zh-Hans")),
            Some("这是游戏目录".to_string())
        );

        // The shared-games-root shape the 2026-10 WoWSP incident
        // lived on: the install sits in the PARENT of a game folder
        // — no stub at the target's own root, one level down
        // instead. The rule's one-level-down probing must flag it
        // for relocation before the silent swap carries the whole
        // tree aside and the backup sweep destroys the game with it.
        let shared_root = tempfile::tempdir().unwrap();
        std::fs::write(shared_root.path().join("Wowsp.exe"), b"MZ").unwrap();
        let game_child = shared_root.path().join("World_of_Warships_CN360");
        std::fs::create_dir_all(&game_child).unwrap();
        std::fs::write(game_child.join("WorldOfWarships.exe"), b"MZ").unwrap();
        assert_eq!(
            relocation_required(&config, &shared_root.path().to_string_lossy(), None),
            Some("the game folder".to_string())
        );

        // A location with neither our install nor a rule stays plain —
        // and relocation_required never fires where no install lives.
        assert_eq!(
            relocation_required(&config, &game.path().to_string_lossy(), None),
            None
        );
    }

    /// The rule-less sibling of the relocation probe: an install
    /// sharing its directory with content shun never delivered hands
    /// the silent lane over to the wizard (or faults the interactive
    /// location step) even though no `dir-rules` entry names the
    /// neighbor — while a clean upgrade, portable mode and fresh
    /// directories never fire it.
    #[test]
    fn foreign_share_reason_covers_the_rule_less_shapes() {
        let config = location_config(None);
        let manifest = |entries: &[&str]| {
            serde_json::to_vec(&crate::targets::install::InstallManifest {
                language: None,
                entries: entries
                    .iter()
                    .map(|path| crate::payload::PayloadEntry {
                        path: (*path).into(),
                        size: 2,
                        sha256: String::new(),
                    })
                    .collect(),
            })
            .unwrap()
        };

        // An accounted install sharing space with one player file:
        // the probe fires and names it.
        let shared = tempfile::tempdir().unwrap();
        std::fs::write(
            shared.path().join(crate::payload::MANIFEST_PATH),
            manifest(&["Wowsp.exe"]),
        )
        .unwrap();
        std::fs::write(shared.path().join("Wowsp.exe"), b"MZ").unwrap();
        std::fs::write(shared.path().join("战绩备份.txt"), b"stats").unwrap();
        assert_eq!(
            foreign_share_reason(&config, &shared.path().to_string_lossy(), false),
            Some(
                "the directory holds 1 file(s) shun never delivered (战绩备份.txt) \
                 — relocate the install into a folder of its own"
                    .into()
            )
        );

        // A clean upgrade — every file accounted — never fires.
        let clean = tempfile::tempdir().unwrap();
        std::fs::write(
            clean.path().join(crate::payload::MANIFEST_PATH),
            manifest(&["Wowsp.exe", "data/pack.bin"]),
        )
        .unwrap();
        std::fs::create_dir_all(clean.path().join("data")).unwrap();
        std::fs::write(clean.path().join("Wowsp.exe"), b"MZ").unwrap();
        std::fs::write(clean.path().join("data").join("pack.bin"), b"x").unwrap();
        // Allowlisted OS droppings are forgiven like everywhere else.
        std::fs::write(clean.path().join("Thumbs.db"), b"junk").unwrap();
        assert_eq!(
            foreign_share_reason(&config, &clean.path().to_string_lossy(), false),
            None
        );

        // Portable mode never fires (data beside the executable is
        // that mode's design)…
        assert_eq!(
            foreign_share_reason(&config, &shared.path().to_string_lossy(), true),
            None
        );
        // …and neither does a directory with no install of ours —
        // the fresh lane's non-empty refusal owns that shape.
        let fresh = tempfile::tempdir().unwrap();
        std::fs::write(fresh.path().join("someone.dat"), b"x").unwrap();
        assert_eq!(
            foreign_share_reason(&config, &fresh.path().to_string_lossy(), false),
            None
        );

        // Four foreign files fold into a three-sample ellipsis, sorted
        // for a stable line.
        let crowded = tempfile::tempdir().unwrap();
        std::fs::write(
            crowded.path().join(crate::payload::MANIFEST_PATH),
            manifest(&["Wowsp.exe"]),
        )
        .unwrap();
        std::fs::write(crowded.path().join("Wowsp.exe"), b"MZ").unwrap();
        for name in ["d.dat", "b.dat", "c.dat", "a.dat"] {
            std::fs::write(crowded.path().join(name), b"x").unwrap();
        }
        assert_eq!(
            foreign_share_reason(&config, &crowded.path().to_string_lossy(), false),
            Some(
                "the directory holds 4 file(s) shun never delivered \
                 (a.dat, b.dat, c.dat, …) — relocate the install into \
                 a folder of its own"
                    .into()
            )
        );
    }
}
