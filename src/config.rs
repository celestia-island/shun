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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_steps: Vec<CustomStepConfig>,

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
                // workspace root manifest (one level up).
                let inherited = cargo_toml
                    .parent()
                    .and_then(Path::parent)
                    .map(|dir| dir.join("Cargo.toml"))
                    .and_then(|path| std::fs::read_to_string(path).ok())
                    .and_then(|raw| toml::from_str::<toml::Value>(&raw).ok())
                    .and_then(|value| {
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
                "unsupported config extension: {other}"
            ))),
        }
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

/// Delivery target. `install` performs NSIS-like registration; `flash`
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
pub struct InstallConfig {
    /// Standard install mode: ARP entry, uninstaller, shortcuts.
    #[serde(default = "default_true")]
    pub local: bool,

    /// Portable mode: no registry, no shortcuts; all data stays beside the
    /// executable.
    #[serde(default = "default_true")]
    pub portable: bool,

    /// Payload-relative path of the app entry point the shortcut targets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_exe: Option<PathBuf>,
}

impl Default for InstallConfig {
    fn default() -> Self {
        Self {
            local: true,
            portable: true,
            main_exe: None,
        }
    }
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
    /// License document (markdown), relative to the manifest.
    #[serde(default)]
    license: Option<String>,
    /// Per-locale license overrides keyed by locale.
    #[serde(default, rename = "license-locales")]
    license_locales: Option<BTreeMap<String, String>>,
    /// Custom content steps injected into the wizard.
    #[serde(default)]
    custom_steps: Option<Vec<CustomStepConfig>>,
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
            license: self.license.map(PathBuf::from),
            license_locales: self
                .license_locales
                .unwrap_or_default()
                .into_iter()
                .map(|(k, v)| (k, PathBuf::from(v)))
                .collect(),
            custom_steps: self.custom_steps.unwrap_or_default(),
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
            license: None,
            license_locales: BTreeMap::new(),
            custom_steps: Vec::new(),
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
