//! Declarative configuration — one document drives both the build CLI
//! (artifact matrix) and the runtime shell (flows).
//!
//! Three sources, one schema:
//!
//! - **the application's own `Cargo.toml`**, via a
//!   `[package.metadata.shun]` table (see
//!   [`ShunConfig::from_cargo_manifest`]) — the cargo-deb / cargo-wix
//!   pattern. Product identity defaults to `[package]` (`name`, `version`),
//!   and everything under `metadata.shun` customizes the delivery flow;
//! - a standalone **TOML** document with the same table shape at the top
//!   level ([`ShunConfig::from_path`]);
//! - a standalone **JSON** document (the serialized [`ShunConfig`]).
//!
//! The schema is settling against three real consumers: the WoWSP installer
//! shell (install + portable modes, dual WebView2 variants), shittim-chest
//! local, and the evernight image flasher. Anything not demanded by one of
//! those stays out.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Top-level shun configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShunConfig {
    /// Product identity rendered by the runtime shell.
    pub product: ProductIdentity,

    /// Directory packed into the delivery artifacts (build-time input),
    /// relative to the manifest that declared it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<PathBuf>,

    /// Windows-only WebView2 delivery strategy. Absent on other platforms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webview2: Option<Webview2Strategy>,

    /// Delivery targets enabled for this product.
    pub targets: Vec<TargetConfig>,

    /// Runtime shell UI knobs (timeline placement, theme, language).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<ShellUiConfig>,

    /// Where the payload comes from at install time. Defaults to the
    /// embedded archive; `online` turns the artifact into a web installer
    /// that streams download → extract → verify in one pass.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceConfig>,

    /// Optional companion resources (asset packs) declared beside the
    /// payload. Full builds carry them inside the payload under their dest
    /// prefix; lite builds embed only the declarations, and the shell
    /// offers the download (see [`crate::attachments`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<AttachmentConfig>,

    /// Bundle the Synthetic Source License (plus official translations)
    /// from its own repository instead of vendoring license files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license_sysl: Option<LicenseSyslConfig>,

    /// MSIX packaging (Windows): identity, publisher and display strings
    /// for generating the AppxManifest and a signed-free deployment story
    /// (Store distribution signs the package for you).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub msix: Option<MsixConfig>,

    /// License document (markdown), relative to the config source. Shown on
    /// the license step; per-locale overrides via [`Self::license_locales`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<PathBuf>,

    /// Per-locale license overrides keyed by locale (`zh-Hans`, `ja`, ...).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub license_locales: BTreeMap<String, PathBuf>,

    /// Extra content steps injected into the wizard, rendered as markdown.
    ///
    /// Legacy injection model (steps keyed `after` built-ins); superseded
    /// by [`Self::steps`], which declares the whole ordered pipeline.
    /// Declaring both is a configuration error.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_steps: Vec<CustomStepConfig>,

    /// The ordered wizard pipeline, freely composed from the step kinds
    /// (mode/scope/license/content/install). Absent = the default
    /// pipeline: mode → license (when a license is declared) → install,
    /// with `custom-steps` injected per their `after` keys.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steps: Option<Vec<StepConfig>>,

    /// Code-signing configuration applied to built artifacts. Absent =
    /// unsigned artifacts (fine for local testing).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signing: Option<SigningConfig>,
}

impl ShunConfig {
    /// Loads the configuration from a `Cargo.toml`.
    ///
    /// Product identity defaults to the package's own `name` and `version`
    /// (`version.workspace = true` inherits from the workspace root); a
    /// `[package.metadata.shun]` table overrides the publisher, logo,
    /// payload directory, entry point, install modes, and WebView2
    /// strategy.
    pub fn from_cargo_manifest(cargo_toml: &Path) -> Result<Self, crate::error::ShunError> {
        let raw = std::fs::read_to_string(cargo_toml)?;
        let draft: CargoTomlDraft = toml::from_str(&raw)
            .map_err(|e| crate::error::ShunError::Config(format!("manifest parse: {e}")))?;

        let package = draft.package;
        let (name, version, metadata) = match package.version {
            Some(toml::Value::String(version)) => (package.name, version, package.metadata),
            Some(table) if table.get("workspace").and_then(toml::Value::as_bool) == Some(true) => {
                // `version.workspace = true` — inherit from the enclosing
                // workspace root manifest: walk the ancestor directories
                // until a Cargo.toml declares `[workspace.package] version`
                // (members may be nested several levels below the root, and
                // plain package manifests in between carry no
                // `[workspace]` table so they are skipped).
                let inherited = cargo_toml
                    .ancestors()
                    .skip(1) // the member manifest's own directory
                    .map(|dir| dir.join("Cargo.toml"))
                    .filter_map(|path| std::fs::read_to_string(path).ok())
                    .filter_map(|raw| toml::from_str::<toml::Value>(&raw).ok())
                    .find_map(|value| {
                        value
                            .get("workspace")?
                            .get("package")?
                            .get("version")?
                            .as_str()
                            .map(String::from)
                    });
                let version = inherited.ok_or_else(|| {
                    crate::error::ShunError::Config(
                        "version.workspace = true, but the workspace root declares no version"
                            .into(),
                    )
                })?;
                (package.name, version, package.metadata)
            }
            _ => {
                return Err(crate::error::ShunError::Config(
                    "package.version missing or not a string".into(),
                ));
            }
        };

        let base = cargo_toml.parent().unwrap_or(Path::new(""));
        let shun_meta = metadata.and_then(|m| m.shun).unwrap_or_default();
        Ok(shun_meta.into_config(name, version, base))
    }

    /// Dispatches by file shape: a `Cargo.toml` is read via
    /// [`from_cargo_manifest`], anything else via [`from_path`].
    pub fn from_any(path: &Path) -> Result<Self, crate::error::ShunError> {
        if path.file_name().and_then(|n| n.to_str()) == Some("Cargo.toml") {
            Self::from_cargo_manifest(path)
        } else {
            Self::from_path(path)
        }
    }

    /// Loads a standalone shun configuration document. TOML documents use
    /// the `[package.metadata.shun]` table shape at the top level; JSON
    /// documents are the serialized [`ShunConfig`].
    pub fn from_path(path: &Path) -> Result<Self, crate::error::ShunError> {
        let raw = std::fs::read_to_string(path)?;
        match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
            "json" => serde_json::from_str(&raw)
                .map_err(|e| crate::error::ShunError::Config(format!("config parse: {e}"))),
            "toml" => {
                let draft: ShunMetadataDraft = toml::from_str(&raw)
                    .map_err(|e| crate::error::ShunError::Config(format!("config parse: {e}")))?;
                let name = draft
                    .product
                    .clone()
                    .unwrap_or_else(|| "shun-product".to_string());
                Ok(draft.into_config(
                    name,
                    "0.0.0".to_string(),
                    path.parent().unwrap_or(Path::new("")),
                ))
            }
            other => Err(crate::error::ShunError::Config(format!(
                "unsupported config extension: {other}",
            ))),
        }
    }

    /// Resolves the wizard pipeline: the declared `steps` when present
    /// (validated: exactly one `install` step, and `custom-steps`
    /// unset), otherwise the default mode → license → install pipeline
    /// with the legacy `custom-steps` injected after their `after`
    /// keys. Markdown bodies (license and content steps) are read
    /// relative to `base` and inlined, locale-aware for the license, so
    /// runtime shells carry no file dependencies.
    pub fn resolve_steps(
        &self,
        base: &Path,
        locale: Option<&str>,
    ) -> Result<Vec<ResolvedStep>, crate::error::ShunError> {
        let config_error = |message: &str| crate::error::ShunError::Config(message.to_string());
        let read_markdown = |path: &str, what: &str| -> Result<String, crate::error::ShunError> {
            std::fs::read_to_string(base.join(path)).map_err(|e| {
                crate::error::ShunError::Config(format!("{what} document `{path}`: {e}"))
            })
        };

        let pipeline: Vec<StepConfig> = match &self.steps {
            Some(steps) => {
                if !self.custom_steps.is_empty() {
                    return Err(config_error(
                        "declare one of `steps` or `custom-steps`, not both",
                    ));
                }
                let installs = steps.iter().filter(|s| s.kind == StepKind::Install).count();
                match installs {
                    1 => {
                        // Content steps must declare both a title and a
                        // document before anything resolves.
                        for step in steps {
                            if step.kind == StepKind::Content
                                && (step.title.is_none() || step.markdown.is_none())
                            {
                                return Err(config_error(
                                    "content steps need both `title` and `markdown`",
                                ));
                            }
                        }
                        steps.clone()
                    }
                    0 => {
                        return Err(config_error(
                            "the `steps` pipeline must contain one `install` step",
                        ));
                    }
                    n => {
                        return Err(config_error(&format!(
                            "the `steps` pipeline contains {n} `install` steps; \
                             exactly one is allowed"
                        )));
                    }
                }
            }
            None => {
                // Default pipeline with legacy injections.
                let mut steps = vec![bare_step(StepKind::Mode)];
                steps.extend(
                    self.custom_steps
                        .iter()
                        .filter(|c| c.after == "mode")
                        .map(custom_to_step),
                );
                if self.license.is_some() || !self.license_locales.is_empty() {
                    steps.push(bare_step(StepKind::License));
                    steps.extend(
                        self.custom_steps
                            .iter()
                            .filter(|c| c.after == "license")
                            .map(custom_to_step),
                    );
                }
                steps.push(bare_step(StepKind::Install));
                steps.extend(
                    self.custom_steps
                        .iter()
                        .filter(|c| c.after == "install")
                        .map(custom_to_step),
                );
                steps
            }
        };

        let license_body = || -> Result<Option<String>, crate::error::ShunError> {
            let path = locale
                .and_then(|l| self.license_locales.get(l))
                .or(self.license.as_ref());
            match path {
                Some(path) => read_markdown(&path.display().to_string(), "license").map(Some),
                None => Ok(None),
            }
        };

        pipeline
            .into_iter()
            .map(|step| {
                let markdown = match (step.kind, step.markdown.as_deref()) {
                    (StepKind::License, _) => license_body()?,
                    (StepKind::Content, Some(markdown)) => {
                        Some(read_markdown(markdown, "content step")?)
                    }
                    _ => None,
                };
                let columns = match step.kind {
                    StepKind::Mode => match step.columns {
                        Some(n @ 2..=4) => Some(n),
                        Some(other) => {
                            return Err(config_error(&format!(
                                "mode step columns must be 2..=4, got {other}"
                            )));
                        }
                        None => None,
                    },
                    _ => None,
                };
                Ok(ResolvedStep {
                    align: step.align.unwrap_or_else(|| step.kind.default_align()),
                    kind: step.kind,
                    title: step.title.unwrap_or_default(),
                    body: markdown,
                    columns,
                })
            })
            .collect()
    }
}

fn bare_step(kind: StepKind) -> StepConfig {
    StepConfig {
        kind,
        align: None,
        title: None,
        markdown: None,
        columns: None,
    }
}

fn custom_to_step(custom: &CustomStepConfig) -> StepConfig {
    StepConfig {
        kind: StepKind::Content,
        align: None,
        title: Some(custom.title.clone()),
        markdown: Some(custom.markdown.clone()),
        columns: None,
    }
}

/// Branding and identity rendered by the runtime shell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProductIdentity {
    /// Product name (titles, ARP display name, flash labels).
    pub name: String,

    /// Product version (ARP display version).
    pub version: String,

    /// Publisher shown in ARP (e.g. `celestia-island`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,

    /// Shell logo asset, path relative to the config document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo: Option<String>,
}

/// WebView2 runtime delivery strategy on Windows.
///
/// The fixed-version runtime is the fully self-contained story: the
/// extracted folder is carried once and shared by the installer shell and
/// the installed app across install and portable modes — no admin, no
/// system writes, no version drift. The Evergreen installer instead
/// registers a system-wide runtime (~127 MB embedded) shared with other
/// apps, but requires elevation at install time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Webview2Strategy {
    /// Carry nothing: the host must already provide WebView2.
    Skip,

    /// Embed the Evergreen standalone installer and register the runtime
    /// system-wide during install.
    EvergreenInstaller,

    /// Carry a fixed-version runtime privately.
    FixedVersion {
        /// Build-time path to the extracted fixed-version runtime folder.
        path: String,
    },
}

/// Delivery target. `install` performs direct Windows registration; `flash`
/// writes images to block devices.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum TargetConfig {
    /// Filesystem delivery with (optional) registration.
    Install(InstallConfig),
    /// Block-device image writing.
    Flash(FlashConfig),
}

/// Install target: standard (registered) install and portable mode, both
/// enabled by default.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct InstallConfig {
    /// Standard install mode: ARP entry, uninstaller, shortcuts.
    #[serde(default = "default_true")]
    pub local: bool,

    /// Portable mode: no registry, no shortcuts; all data stays beside the
    /// executable.
    #[serde(default = "default_true")]
    pub portable: bool,

    /// Portable-mode marker file name written into the install directory
    /// (default: `.shun-portable`). Products whose runtime detects an
    /// existing marker name (e.g. wowsp's `.portable`) point this at that
    /// name so the delivered copy follows the product's own convention.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub portable_marker: Option<String>,

    /// Payload-relative path of the app entry point the shortcut targets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_exe: Option<PathBuf>,

    /// Desktop-shortcut policy for local mode. `ask` (the default) is the
    /// NSIS checkbox convention: the wizard shows a default-checked toggle
    /// (headless runs treat it as checked).
    #[serde(default)]
    pub desktop_shortcut: DesktopShortcutPolicy,

    /// Install scope: per-user (the default, no elevation anywhere) or
    /// machine-wide (Windows: HKLM, all-users shortcuts; the shell
    /// self-elevates), or a wizard question.
    #[serde(default)]
    pub scope: ScopePolicy,

    /// Context-menu verbs registered beside the app's launchers
    /// (Explorer verbs under `HKCU\Software\Classes\Applications` on
    /// Windows, Desktop Actions on Linux; macOS has no analog yet).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub verbs: Vec<VerbConfig>,

    /// URL-scheme deep links the app owns (`myapp://…`): protocol
    /// registration under `HKCU\Software\Classes` on Windows, the
    /// launcher's `MimeType=` on Linux, `CFBundleURLTypes` in a
    /// synthesized macOS `Info.plist`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deep_links: Vec<String>,

    /// Explicit `System.AppUserModel.ID` for the shortcuts (Windows) —
    /// the grouping identity the app should also pass to
    /// `SetCurrentProcessExplicitAppUserModelID`. Defaults to a generated
    /// `{publisher}.{product}` value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aumid: Option<String>,

    /// Payload-relative icon file the Linux launcher references
    /// (`Icon=` accepts absolute paths; macOS bundles use `Contents/Resources`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<PathBuf>,
}

impl Default for InstallConfig {
    fn default() -> Self {
        Self {
            local: true,
            portable: true,
            portable_marker: None,
            main_exe: None,
            desktop_shortcut: DesktopShortcutPolicy::Ask,
            scope: ScopePolicy::User,
            verbs: Vec::new(),
            deep_links: Vec::new(),
            aumid: None,
            icon: None,
        }
    }
}

/// Who decides whether the desktop shortcut is created.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum DesktopShortcutPolicy {
    /// Wizard checkbox, default checked (the NSIS convention).
    #[default]
    Ask,
    /// Always create it.
    Always,
    /// Never create it.
    Never,
}

/// One context-menu verb offered on the app's launchers. The `target`
/// tag picks what the verb invokes; `key` and `display` are shared by
/// every target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "target", rename_all = "kebab-case")]
pub enum VerbConfig {
    /// Open the install directory in the file manager.
    DataFolder {
        /// Stable verb key — the registry segment / desktop-action id.
        key: String,
        /// Display string shown in the menu.
        display: String,
    },

    /// Run the copied uninstaller.
    Uninstall {
        /// Stable verb key — the registry segment / desktop-action id.
        key: String,
        /// Display string shown in the menu.
        display: String,
    },

    /// Launch the app entry point with extra `arguments`.
    App {
        /// Stable verb key — the registry segment / desktop-action id.
        key: String,
        /// Display string shown in the menu.
        display: String,
        /// Extra command-line arguments appended to the entry point.
        #[serde(default)]
        arguments: String,
    },
}

/// Flash target: write an image to a block device with post-write
/// verification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct FlashConfig {
    /// Refuse non-removable devices unless explicitly overridden.
    #[serde(default = "default_true")]
    pub require_removable: bool,
}

fn default_true() -> bool {
    true
}

/// Runtime shell UI knobs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct ShellUiConfig {
    /// Step indicator placement: `top` (horizontal rail) or `left`
    /// (vertical rail beside the panes).
    #[serde(default)]
    pub timeline: Option<TimelineOrientation>,

    /// Theme selection: follow the system, or pin light/dark.
    #[serde(default)]
    pub theme: Option<ThemeConfig>,

    /// UI language: `auto` (follow the system) or a fixed locale
    /// (`en`, `zh-Hans`, `zh-Hant`, `ja`, `ko`, `fr`, `ru`, `es`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,

    /// Terminal log verbosity of the install pane: everything (the
    /// default), one family only (`files` or `scripts`), or `off` for
    /// no output pane at all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_level: Option<LogVerbosity>,

    /// Install-log line ordering: `newest` (the default — latest line on
    /// top, Docker-Desktop style) or `oldest` (append at the tail).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_order: Option<LogOrder>,
}

/// The install pane's log line ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum LogOrder {
    /// Latest line on top (the default).
    #[default]
    Newest,
    /// Append at the tail, classic terminal order.
    Oldest,
}

/// What the install pane's terminal shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum LogVerbosity {
    /// File operations and script activity (default).
    #[default]
    All,
    /// Only file operations (copy/reuse lines); scripts stay silent.
    Files,
    /// Only script activity (begin markers, script output, instruction
    /// lines); file operations stay silent.
    Scripts,
    /// No terminal pane at all.
    Off,
}

/// Step indicator placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TimelineOrientation {
    /// Horizontal rail across the top (default).
    #[default]
    Top,
    /// Vertical rail beside the panes.
    Left,
}

/// Theme selection for the runtime shell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct ThemeConfig {
    /// `system` follows the OS preference; `light`/`dark` pin the mode.
    #[serde(default)]
    pub mode: Option<ThemeMode>,
    /// Accent tint override as RGB channels (drives --color-primary).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent: Option<[u8; 3]>,
}

/// Theme mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeMode {
    /// Follow the OS preference.
    #[default]
    System,
    Light,
    Dark,
}

/// Where the payload comes from at install time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SourceConfig {
    /// The payload archive is embedded in the installer binary.
    #[default]
    Embedded,
    /// The installer downloads the payload from `url` and streams
    /// download → extract → verify in a single pass (online installer).
    Online {
        /// Release URL of the packed payload (`*.shun`).
        url: String,
    },
}

/// Where an optional attachment is fetched from at install time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct AttachmentOnlineConfig {
    /// Release URL of the packed attachment archive (`*.shun`, entries
    /// carrying the attachment's dest prefix).
    pub url: String,
}

/// The Synthetic Source License fetched from its official repository —
/// one declaration bundles the license text for every requested locale,
/// so wizards can show the agreement in the user's language without
/// vendoring the translations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct LicenseSyslConfig {
    /// GitHub repo carrying the license (default `celestia-island/sysl`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,

    /// Branch or tag the documents live on (default `main`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,

    /// Locales to bundle, mapped onto the sysl repo's i18n directories
    /// (`zh-Hans` → `zhs`, `zh-Hant` → `zht`, `ja`/`ko`/`fr`/`ru`/`es`/
    /// `de`/`pt`/`ar` direct). The root English text is always bundled.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub locales: Vec<String>,
}

/// An optional companion resource (an asset pack) declared beside the
/// payload. Full builds carry the attachment inside the payload under its
/// dest prefix; lite builds embed only this declaration, and the shell
/// offers [`crate::attachments::download`] as the way to fetch it instead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct AttachmentConfig {
    /// Stable key the shell's download command addresses.
    pub key: String,

    /// Display string for the shell's download affordance.
    pub title: String,

    /// Payload- and archive-relative directory the attachment occupies
    /// (`models`, `assets/voices`, ...). Used to detect whether the
    /// embedded payload already carries the attachment and as the contract
    /// for the online archive's entry layout.
    pub dest: PathBuf,

    /// Display size of the unpacked attachment (UI hint; the online
    /// archive's own manifest carries the authoritative verification data).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,

    /// Where to fetch the attachment when it is not bundled.
    pub online: AttachmentOnlineConfig,
}

/// A custom content step injected into the wizard, rendered as markdown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CustomStepConfig {
    /// Stable step key used for ordering.
    pub key: String,

    /// Insert the step after this built-in step key
    /// (`mode` | `license` | `install`).
    pub after: String,

    /// Step label on the timeline.
    pub title: String,

    /// Markdown document, relative to the config source.
    pub markdown: String,
}

/// One wizard step in the declarative pipeline. Steps render in
/// declaration order; the pipeline must contain exactly one `install`
/// step (the delivery run itself). Panes center their content by
/// default (`align = "start"` opts a step into left-aligned text —
/// agreements and documents; the vertical axis stays centered).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct StepConfig {
    /// Which pane renders this step.
    pub kind: StepKind,

    /// Content alignment override; the default comes from the kind
    /// (license/content align start, everything else centers).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<StepAlign>,

    /// Timeline label (content steps).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Markdown document relative to the config source (content steps).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub markdown: Option<String>,

    /// Columns of the mode-selection grid (`mode` steps). Unspecified =
    /// one column per declared mode, so two modes split the row evenly
    /// instead of leaving empty slots.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns: Option<u8>,
}

/// Content alignment of a wizard pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StepAlign {
    /// Centered on both axes (the default).
    Center,
    /// Left-aligned text in a centered block; vertically centered —
    /// agreements and documents.
    Start,
}

impl StepKind {
    /// The default content alignment for this kind of pane.
    pub fn default_align(&self) -> StepAlign {
        match self {
            StepKind::License | StepKind::Content => StepAlign::Start,
            _ => StepAlign::Center,
        }
    }
}

/// A wizard step with its content resolved for embedding: markdown
/// documents (license and content steps) are read relative to the
/// config source and inlined, so runtime shells carry no file
/// dependencies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedStep {
    /// Which pane renders this step.
    pub kind: StepKind,
    /// Content alignment (explicit override or the kind's default).
    pub align: StepAlign,
    /// Timeline label.
    pub title: String,
    /// Inlined markdown body (`None` for non-content steps).
    pub body: Option<String>,
    /// Mode-grid columns (`mode` steps; `None` = one column per mode).
    pub columns: Option<u8>,
}

/// The rendered pane behind a resolved step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StepKind {
    /// Mode + directory selection.
    Mode,
    /// User/machine install scope.
    Scope,
    /// License agreement.
    License,
    /// Markdown content.
    Content,
    /// The delivery run.
    Install,
}

/// Who decides the install scope (per-user vs machine-wide).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ScopePolicy {
    /// Per-user install (HKCU / `~/.local` / `~/Applications`): the
    /// default, no elevation anywhere.
    #[default]
    User,
    /// Machine-wide install (Windows: HKLM, all-users shortcuts,
    /// Program Files; requires elevation — the shell self-elevates).
    Machine,
    /// A wizard step asks (embedded in the mode pane or standalone);
    /// headless runs default to per-user.
    Ask,
}

/// Code-signing configuration for built artifacts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct SigningConfig {
    /// Windows Authenticode signing (signtool).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub windows: Option<WindowsSigningConfig>,
    /// macOS codesign profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub macos: Option<MacSigningConfig>,
}

/// Windows Authenticode signing via signtool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct WindowsSigningConfig {
    /// Sign the produced binaries.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Certificate thumbprint — selects a certificate from the user or
    /// machine store (signtool /sha1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbprint: Option<String>,
    /// RFC 3161 timestamp server (signtool /tr + /td SHA256).
    #[serde(default = "default_timestamp_url")]
    pub timestamp_url: String,
}

/// macOS codesign profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct MacSigningConfig {
    /// Sign the produced bundles (codesign --sign).
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Signing identity passed to codesign --sign.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
}

fn default_timestamp_url() -> String {
    "http://timestamp.digicert.com".to_string()
}

/// MSIX packaging inputs (Windows). The manifest is generated from these
/// fields; the identity publisher MUST match the signing certificate
/// subject, or the package will not install.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct MsixConfig {
    /// Identity Name — no spaces (Package Identity).
    pub identity_name: String,
    /// Identity Publisher — must equal the signing cert subject
    /// (e.g. `CN=celestia-island`).
    pub publisher: String,
    /// Display name shown in the Start menu / Settings.
    pub display_name: String,
    /// Package description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Payload-relative path of the app executable (full-trust entry).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable: Option<PathBuf>,
    /// Plate color (`#RRGGBB`) flattened under a transparent logo.
    ///
    /// Windows plates packaged-desktop logos over the default system blue
    /// (#0078D7) on every surface that ignores `BackgroundColor=
    /// "transparent"` — the App Installer dialog among them. An explicit
    /// color keeps the brand in control: the logo asset is composited
    /// onto it and the manifest declares it as `BackgroundColor`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo_background: Option<String>,
}

// ── Cargo.toml draft types ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct CargoTomlDraft {
    package: PackageDraft,
}

#[derive(Debug, Deserialize)]
struct PackageDraft {
    name: String,
    /// Either a plain string or `{ workspace = true }` (inherited from the
    /// enclosing `[workspace.package]`).
    #[serde(default)]
    version: Option<toml::Value>,
    #[serde(default)]
    metadata: Option<MetadataDraft>,
}

#[derive(Debug, Deserialize)]
struct MetadataDraft {
    #[serde(default)]
    shun: Option<ShunMetadataDraft>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct ShunMetadataDraft {
    /// Product name override; defaults to the package name.
    #[serde(default)]
    product: Option<String>,
    #[serde(default)]
    publisher: Option<String>,
    #[serde(default)]
    logo: Option<String>,
    /// Directory packed into the artifacts, relative to the manifest.
    #[serde(default)]
    payload: Option<String>,
    /// Payload-relative entry point the shortcut targets.
    #[serde(default)]
    main_exe: Option<String>,
    #[serde(default)]
    webview2: Option<Webview2Strategy>,
    /// `[package.metadata.shun.install]` — local/portable switches.
    #[serde(default)]
    install: Option<InstallConfig>,
    /// `[package.metadata.shun.flash]`.
    #[serde(default)]
    flash: Option<FlashConfig>,
    /// `[package.metadata.shun.shell]` — runtime UI knobs.
    #[serde(default)]
    shell: Option<ShellUiConfig>,
    /// `[package.metadata.shun.source]` — embedded (default) or online.
    #[serde(default)]
    source: Option<SourceConfig>,
    /// `[[package.metadata.shun.attachments]]` — optional companion
    /// resources (asset packs) with an online source for lite builds.
    #[serde(default)]
    attachments: Option<Vec<AttachmentConfig>>,
    /// `[package.metadata.shun.license-sysl]` — bundle the SySL license
    /// and its official translations from the upstream repository.
    #[serde(default)]
    license_sysl: Option<LicenseSyslConfig>,
    /// License document (markdown), relative to the manifest.
    #[serde(default)]
    license: Option<String>,
    /// Per-locale license overrides keyed by locale.
    #[serde(default, rename = "license-locales")]
    license_locales: Option<BTreeMap<String, String>>,
    /// Custom content steps injected into the wizard.
    #[serde(default)]
    custom_steps: Option<Vec<CustomStepConfig>>,
    /// The ordered wizard pipeline (`[[package.metadata.shun.steps]]`).
    #[serde(default)]
    steps: Option<Vec<StepConfig>>,
    /// `[package.metadata.shun.signing]` — code-signing profile.
    #[serde(default)]
    signing: Option<SigningConfig>,
    /// `[package.metadata.shun.msix]` — MSIX packaging inputs.
    #[serde(default)]
    msix: Option<MsixConfig>,
}

impl ShunMetadataDraft {
    fn into_config(self, product_name: String, version: String, _base: &Path) -> ShunConfig {
        let mut targets = Vec::new();
        match self.install {
            Some(mut install) => {
                // The top-level `main-exe` is the default entry point;
                // an explicit one inside the `[install]` table wins.
                if install.main_exe.is_none() {
                    install.main_exe = self.main_exe.clone().map(PathBuf::from);
                }
                targets.push(TargetConfig::Install(install));
            }
            None => targets.push(TargetConfig::Install(InstallConfig {
                main_exe: self.main_exe.clone().map(PathBuf::from),
                ..InstallConfig::default()
            })),
        }
        if let Some(flash) = self.flash {
            targets.push(TargetConfig::Flash(flash));
        }

        ShunConfig {
            product: ProductIdentity {
                name: self.product.unwrap_or(product_name),
                version,
                publisher: self.publisher,
                logo: self.logo,
            },
            payload: self.payload.map(PathBuf::from),
            webview2: self.webview2,
            targets,
            shell: self.shell,
            source: self.source,
            attachments: self.attachments.unwrap_or_default(),
            license_sysl: self.license_sysl,
            license: self.license.map(PathBuf::from),
            license_locales: self
                .license_locales
                .unwrap_or_default()
                .into_iter()
                .map(|(k, v)| (k, PathBuf::from(v)))
                .collect(),
            custom_steps: self.custom_steps.unwrap_or_default(),
            steps: self.steps,
            signing: self.signing,
            msix: self.msix,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ShunConfig {
        ShunConfig {
            product: ProductIdentity {
                name: "ShunDemo".into(),
                version: "0.1.0".into(),
                publisher: Some("celestia-island".into()),
                logo: Some("logo.webp".into()),
            },
            payload: Some("examples/demo_payload".into()),
            webview2: Some(Webview2Strategy::FixedVersion {
                path: "WebView2Runtime".into(),
            }),
            targets: vec![
                TargetConfig::Install(InstallConfig::default()),
                TargetConfig::Flash(FlashConfig::default()),
            ],
            shell: None,
            source: None,
            attachments: Vec::new(),
            license_sysl: None,
            license: None,
            license_locales: BTreeMap::new(),
            custom_steps: Vec::new(),
            steps: None,
            signing: None,
            msix: None,
        }
    }

    #[test]
    fn webview2_strategy_uses_kebab_case_tags() {
        let json = serde_json::to_value(sample()).unwrap();
        assert_eq!(json["webview2"]["type"], "fixed-version");
    }

    #[test]
    fn targets_use_kebab_case_kinds() {
        let json = serde_json::to_value(sample()).unwrap();
        assert_eq!(json["targets"][0]["kind"], "install");
        assert_eq!(json["targets"][1]["kind"], "flash");
    }

    #[test]
    fn install_modes_default_to_enabled() {
        let json = serde_json::to_value(sample()).unwrap();
        assert_eq!(json["targets"][0]["local"], true);
        assert_eq!(json["targets"][0]["portable"], true);
    }

    #[test]
    fn loads_from_cargo_manifest_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = dir.path().join("Cargo.toml");
        std::fs::write(
            &manifest,
            r#"
[package]
name = "shun-demo-shell"
version = "0.3.1"
edition = "2024"

[package.metadata.shun]
product = "ShunDemo"
publisher = "celestia-island"
logo = "docs/logo.webp"
payload = "examples/demo_payload"
main-exe = "bin/shun-demo.cmd"

[package.metadata.shun.webview2]
type = "fixed-version"
path = "WebView2Runtime"

[package.metadata.shun.flash]
require-removable = true
"#,
        )
        .unwrap();

        let config = ShunConfig::from_cargo_manifest(&manifest).unwrap();
        assert_eq!(config.product.name, "ShunDemo");
        assert_eq!(config.product.version, "0.3.1");
        assert_eq!(config.product.publisher.as_deref(), Some("celestia-island"));
        assert_eq!(
            config.payload.as_deref(),
            Some(Path::new("examples/demo_payload"))
        );

        let [TargetConfig::Install(install), TargetConfig::Flash(flash)] = &config.targets[..]
        else {
            panic!("expected install + flash targets");
        };
        assert!(install.local && install.portable);
        assert_eq!(
            install.main_exe.as_deref(),
            Some(Path::new("bin/shun-demo.cmd"))
        );
        assert!(flash.require_removable);
    }

    #[test]
    fn attachments_parse_from_the_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = dir.path().join("Cargo.toml");
        std::fs::write(
            &manifest,
            r#"
[package]
name = "attach-demo"
version = "0.1.0"

[package.metadata.shun]
product = "AttachDemo"
payload = "payload"

[[package.metadata.shun.attachments]]
key = "models"
title = "2D/3D model pack"
dest = "models"
size = 123
[package.metadata.shun.attachments.online]
url = "https://example.test/models.shun"
"#,
        )
        .unwrap();

        let config = ShunConfig::from_cargo_manifest(&manifest).unwrap();
        assert_eq!(config.attachments.len(), 1);
        let attachment = &config.attachments[0];
        assert_eq!(attachment.key, "models");
        assert_eq!(attachment.title, "2D/3D model pack");
        assert_eq!(attachment.dest, Path::new("models"));
        assert_eq!(attachment.size, Some(123));
        assert_eq!(attachment.online.url, "https://example.test/models.shun");
    }

    #[test]
    fn inherits_workspace_version_from_a_nested_member() {
        // Members may sit several levels below the workspace root, with
        // plain package manifests in between (the wowsp layout:
        // <root>/packages/installer-shell).
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("repo");
        let member = root.join("packages").join("installer-shell");
        std::fs::create_dir_all(&member).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            r#"
[workspace]
resolver = "2"
members = ["packages/installer-shell"]

[workspace.package]
version = "1.2.3"
"#,
        )
        .unwrap();
        let manifest = member.join("Cargo.toml");
        std::fs::write(
            &manifest,
            r#"
[package]
name = "nested-shell"
version.workspace = true
edition = "2024"

[package.metadata.shun]
product = "Nested"
payload = "payload"
"#,
        )
        .unwrap();

        let config = ShunConfig::from_cargo_manifest(&manifest).unwrap();
        assert_eq!(config.product.version, "1.2.3");
    }

    #[test]
    fn install_registration_knobs_parse_from_the_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = dir.path().join("Cargo.toml");
        std::fs::write(
            &manifest,
            r#"
[package]
name = "shun-demo"
version = "0.3.1"

[package.metadata.shun]
main-exe = "bin/shun-demo.exe"

[package.metadata.shun.install]
desktop-shortcut = "always"
aumid = "celestia-island.ShunDemo"
icon = "assets/icon.png"
deep-links = ["shundemo"]

[[package.metadata.shun.install.verbs]]
key = "open-data"
display = "Open data folder"
target = "data-folder"

[[package.metadata.shun.install.verbs]]
key = "safe-mode"
display = "Safe mode"
target = "app"
arguments = "--safe"
"#,
        )
        .unwrap();

        let config = ShunConfig::from_cargo_manifest(&manifest).unwrap();
        let TargetConfig::Install(install) = &config.targets[0] else {
            panic!("expected an install target");
        };
        assert_eq!(install.desktop_shortcut, DesktopShortcutPolicy::Always);
        assert_eq!(install.aumid.as_deref(), Some("celestia-island.ShunDemo"));
        assert_eq!(install.icon.as_deref(), Some(Path::new("assets/icon.png")));
        assert_eq!(install.deep_links, vec!["shundemo".to_string()]);
        assert_eq!(install.verbs.len(), 2);
        assert_eq!(
            install.verbs[0],
            VerbConfig::DataFolder {
                key: "open-data".into(),
                display: "Open data folder".into(),
            },
            "data-folder verbs take no arguments"
        );
        assert_eq!(
            install.verbs[1],
            VerbConfig::App {
                key: "safe-mode".into(),
                display: "Safe mode".into(),
                arguments: "--safe".into(),
            }
        );
    }

    #[test]
    fn manifest_without_shun_metadata_defaults_to_install() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = dir.path().join("Cargo.toml");
        std::fs::write(
            &manifest,
            "[package]\nname = \"plain-app\"\nversion = \"1.2.3\"\n",
        )
        .unwrap();

        let config = ShunConfig::from_cargo_manifest(&manifest).unwrap();
        assert_eq!(config.product.name, "plain-app");
        assert_eq!(config.product.version, "1.2.3");
        assert_eq!(config.targets.len(), 1);
        assert!(
            matches!(&config.targets[0], TargetConfig::Install(install) if install.local && install.portable)
        );
    }

    #[test]
    fn workspace_version_is_inherited() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"app\"]\n\n[workspace.package]\nversion = \"2.5.0\"\n",
        )
        .unwrap();
        let app_dir = dir.path().join("app");
        std::fs::create_dir(&app_dir).unwrap();
        let manifest = app_dir.join("Cargo.toml");
        std::fs::write(
            &manifest,
            "[package]\nname = \"app\"\nversion.workspace = true\n",
        )
        .unwrap();

        let config = ShunConfig::from_cargo_manifest(&manifest).unwrap();
        assert_eq!(config.product.name, "app");
        assert_eq!(config.product.version, "2.5.0");
    }

    #[test]
    fn default_pipeline_is_mode_license_install() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = dir.path().join("Cargo.toml");
        std::fs::write(
            &manifest,
            "[package]
name = \"app\"
version = \"1.0.0\"
",
        )
        .unwrap();
        let config = ShunConfig::from_cargo_manifest(&manifest).unwrap();

        // No license declared: mode → install.
        let steps = config.resolve_steps(dir.path(), None).unwrap();
        assert_eq!(
            steps.iter().map(|s| s.kind).collect::<Vec<_>>(),
            vec![StepKind::Mode, StepKind::Install]
        );

        // A declared license adds its step (with the body inlined).
        std::fs::write(
            dir.path().join("LICENSE.md"),
            "# terms
",
        )
        .unwrap();
        let mut config = config;
        config.license = Some("LICENSE.md".into());
        let steps = config.resolve_steps(dir.path(), None).unwrap();
        assert_eq!(
            steps.iter().map(|s| s.kind).collect::<Vec<_>>(),
            vec![StepKind::Mode, StepKind::License, StepKind::Install]
        );
        assert_eq!(steps[1].body.as_deref(), Some("# terms\n"));
    }

    #[test]
    fn declared_pipeline_orders_and_inlines_freely() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("why.md"),
            "# why
",
        )
        .unwrap();
        let config_path = dir.path().join("shun.toml");
        std::fs::write(
            &config_path,
            r#"
product = "App"

[[steps]]
kind = "content"
title = "Why"
markdown = "why.md"

[[steps]]
kind = "scope"

[[steps]]
kind = "license"

[[steps]]
kind = "mode"

[[steps]]
kind = "install"
"#,
        )
        .unwrap();
        let config = ShunConfig::from_path(&config_path).unwrap();
        let steps = config.resolve_steps(dir.path(), None).unwrap();
        assert_eq!(
            steps.iter().map(|s| s.kind).collect::<Vec<_>>(),
            vec![
                StepKind::Content,
                StepKind::Scope,
                StepKind::License,
                StepKind::Mode,
                StepKind::Install,
            ]
        );
        assert_eq!(steps[0].title, "Why");
        assert_eq!(steps[0].body.as_deref(), Some("# why\n"));
        // The license step exists even with no license configured — the
        // pane renders whatever (empty) body it resolved.
        assert_eq!(steps[2].body, None);
    }

    #[test]
    fn pipeline_validation_rejects_bad_declarations() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = sample();

        let bare = |kind| StepConfig {
            kind,
            align: None,
            title: None,
            markdown: None,
            columns: None,
        };

        // No install step.
        config.steps = Some(vec![bare(StepKind::Mode)]);
        assert!(config.resolve_steps(dir.path(), None).is_err());

        // Two install steps.
        config.steps = Some(vec![
            bare(StepKind::Mode),
            bare(StepKind::Install),
            bare(StepKind::Install),
        ]);
        assert!(config.resolve_steps(dir.path(), None).is_err());

        // Content steps must declare title + markdown.
        config.steps = Some(vec![
            bare(StepKind::Mode),
            StepConfig {
                kind: StepKind::Content,
                align: None,
                title: None,
                markdown: None,
                columns: None,
            },
            bare(StepKind::Install),
        ]);
        assert!(config.resolve_steps(dir.path(), None).is_err());

        // The legacy custom-steps injection stays working alongside no
        // declared pipeline, but not with one.
        config.steps = Some(vec![bare(StepKind::Mode), bare(StepKind::Install)]);
        config.custom_steps = vec![CustomStepConfig {
            key: "extra".into(),
            after: "mode".into(),
            title: "Extra".into(),
            markdown: "extra.md".into(),
        }];
        let error = config.resolve_steps(dir.path(), None).unwrap_err();
        assert!(error.to_string().contains("not both"));

        // Legacy injections alone keep their after-key ordering.
        config.steps = None;
        config.license = None;
        config.license_locales.clear();
        config.custom_steps = vec![
            CustomStepConfig {
                key: "a".into(),
                after: "mode".into(),
                title: "A".into(),
                markdown: "a.md".into(),
            },
            CustomStepConfig {
                key: "b".into(),
                after: "install".into(),
                title: "B".into(),
                markdown: "b.md".into(),
            },
        ];
        std::fs::write(dir.path().join("a.md"), "a").unwrap();
        std::fs::write(dir.path().join("b.md"), "b").unwrap();
        let steps = config.resolve_steps(dir.path(), None).unwrap();
        assert_eq!(
            steps.iter().map(|s| s.kind).collect::<Vec<_>>(),
            vec![
                StepKind::Mode,
                StepKind::Content,
                StepKind::Install,
                StepKind::Content
            ]
        );
    }

    #[test]
    fn step_alignment_overrides_and_log_level_parse() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("why.md"),
            "# why
",
        )
        .unwrap();
        let config_path = dir.path().join("shun.toml");
        std::fs::write(
            &config_path,
            concat!(
                "product = \"App\"

",
                "[shell]
",
                "log-level = \"scripts\"

",
                "[[steps]]
",
                "kind = \"license\"
",
                "align = \"center\"

",
                "[[steps]]
",
                "kind = \"mode\"

",
                "[[steps]]
",
                "kind = \"content\"
",
                "title = \"Notes\"
",
                "markdown = \"why.md\"

",
                "[[steps]]
",
                "kind = \"install\"
",
            ),
        )
        .unwrap();
        let config = ShunConfig::from_path(&config_path).unwrap();
        assert_eq!(
            config.shell.as_ref().unwrap().log_level,
            Some(LogVerbosity::Scripts)
        );

        let steps = config.resolve_steps(dir.path(), None).unwrap();
        // Explicit override wins over the kind default…
        assert_eq!(steps[0].align, StepAlign::Center);
        // …otherwise kinds pick: documents start, choices center.
        assert_eq!(steps[1].align, StepAlign::Center);
        assert_eq!(steps[2].align, StepAlign::Start);
    }

    #[test]
    fn install_scope_policy_parses() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = dir.path().join("Cargo.toml");
        std::fs::write(
            &manifest,
            concat!(
                "[package]\n",
                "name = \"app\"\n",
                "version = \"1.0.0\"\n",
                "\n",
                "[package.metadata.shun.install]\n",
                "scope = \"machine\"\n",
            ),
        )
        .unwrap();
        let config = ShunConfig::from_cargo_manifest(&manifest).unwrap();
        let TargetConfig::Install(install) = &config.targets[0] else {
            panic!("install target expected")
        };
        assert_eq!(install.scope, ScopePolicy::Machine);
    }

    #[test]
    fn loads_shell_ui_and_source_tables() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("shun.toml");
        std::fs::write(
            &config_path,
            r#"
product = "ShunDemo"
payload = "payload"
license-locales = { zh-Hans = "LICENSE.zh.md" }

[shell]
timeline = "left"
language = "zh-Hans"

[shell.theme]
mode = "dark"
accent = [34, 211, 238]

[source]
type = "online"
url = "https://example.test/ShunDemo.shun"
"#,
        )
        .unwrap();

        let config = ShunConfig::from_path(&config_path).unwrap();
        let shell = config.shell.expect("shell table parsed");
        assert_eq!(shell.timeline, Some(TimelineOrientation::Left));
        assert_eq!(shell.language.as_deref(), Some("zh-Hans"));
        let theme = shell.theme.expect("theme parsed");
        assert_eq!(theme.mode, Some(ThemeMode::Dark));
        assert_eq!(theme.accent, Some([34, 211, 238]));
        assert!(matches!(
            config.source,
            Some(SourceConfig::Online { ref url }) if url == "https://example.test/ShunDemo.shun"
        ));
        assert!(config.license_locales.contains_key("zh-Hans"));
    }
}
