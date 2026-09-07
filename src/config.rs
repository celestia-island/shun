//! Declarative configuration — one document drives both the build CLI
//! (artifact matrix) and the runtime shell (flows).
//!
//! Two sources, one schema:
//!
//! - a standalone document (JSON/TOML), or
//! - **the application's own `Cargo.toml`**, via a
//!   `[package.metadata.shun]` table (see
//!   [`ShunConfig::from_cargo_manifest`]) — the cargo-deb / cargo-wix
//!   pattern. Product identity defaults to `[package]` (`name`, `version`),
//!   and everything under `metadata.shun` customizes the delivery flow:
//!   publisher, logo, payload directory, entry point, install modes, and
//!   the WebView2 strategy.
//!
//! The schema is settling against three real consumers: the WoWSP installer
//! shell (install + portable modes, dual WebView2 variants), shittim-chest
//! local, and the evernight image flasher. Anything not demanded by one of
//! those stays out.

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
}

impl ShunConfig {
    /// Loads the configuration from a `Cargo.toml`.
    ///
    /// Product identity defaults to the package's own `name` and `version`;
    /// a `[package.metadata.shun]` table overrides the publisher, logo,
    /// payload directory, entry point, install modes, and WebView2
    /// strategy.
    pub fn from_cargo_manifest(cargo_toml: &Path) -> Result<Self, crate::error::ShunError> {
        let raw = std::fs::read_to_string(cargo_toml)?;
        let draft: CargoTomlDraft = toml::from_str(&raw)
            .map_err(|e| crate::error::ShunError::Config(format!("manifest parse: {e}")))?;
        draft.into_config(cargo_toml)
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
    #[serde(default, rename = "main-exe")]
    main_exe: Option<String>,
    #[serde(default)]
    webview2: Option<Webview2Strategy>,
    /// `[package.metadata.shun.install]` — local/portable switches.
    #[serde(default)]
    install: Option<InstallConfig>,
    /// `[package.metadata.shun.flash]`.
    #[serde(default)]
    flash: Option<FlashConfig>,
}

impl CargoTomlDraft {
    fn into_config(self, manifest_path: &Path) -> Result<ShunConfig, crate::error::ShunError> {
        let shun = self
            .package
            .metadata
            .and_then(|m| m.shun)
            .unwrap_or_default();

        let version = match self.package.version {
            Some(toml::Value::String(version)) => version,
            Some(table) if table.get("workspace").and_then(toml::Value::as_bool) == Some(true) => {
                // `version.workspace = true` — inherit from the enclosing
                // workspace root manifest (one level up).
                let workspace_manifest = manifest_path
                    .parent()
                    .and_then(Path::parent)
                    .map(|dir| dir.join("Cargo.toml"));
                let inherited = workspace_manifest
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
                inherited.ok_or_else(|| {
                    crate::error::ShunError::Config(
                        "version.workspace = true, but the workspace root declares no version"
                            .into(),
                    )
                })?
            }
            _ => {
                return Err(crate::error::ShunError::Config(
                    "package.version missing or not a string".into(),
                ));
            }
        };

        let mut targets = Vec::new();
        match shun.install {
            Some(install) => targets.push(TargetConfig::Install(install)),
            None => targets.push(TargetConfig::Install(InstallConfig {
                main_exe: shun.main_exe.clone().map(PathBuf::from),
                ..InstallConfig::default()
            })),
        }
        if let Some(flash) = shun.flash {
            targets.push(TargetConfig::Flash(flash));
        }

        Ok(ShunConfig {
            product: ProductIdentity {
                name: shun.product.unwrap_or(self.package.name),
                version,
                publisher: shun.publisher,
                logo: shun.logo,
            },
            payload: shun.payload.map(PathBuf::from),
            webview2: shun.webview2,
            targets,
        })
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
}
