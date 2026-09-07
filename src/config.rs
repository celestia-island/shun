//! Declarative configuration — one document drives both the build CLI
//! (artifact matrix) and the runtime shell (flows).
//!
//! The schema is settling against three real consumers: the WoWSP installer
//! shell (install + portable modes, dual WebView2 variants), shittim-chest
//! local, and the evernight image flasher. Anything not demanded by one of
//! those stays out.

use serde::{Deserialize, Serialize};

/// Top-level shun configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShunConfig {
    /// Product identity rendered by the runtime shell.
    pub product: ProductIdentity,

    /// Windows-only WebView2 delivery strategy. Absent on other platforms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webview2: Option<Webview2Strategy>,

    /// Delivery targets enabled for this product.
    pub targets: Vec<TargetConfig>,
}

/// Branding rendered by the runtime shell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProductIdentity {
    /// Product name (titles, ARP display name, flash labels).
    pub name: String,

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
}

impl Default for InstallConfig {
    fn default() -> Self {
        Self {
            local: true,
            portable: true,
        }
    }
}

/// Flash target: write an image to a block device with post-write
/// verification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FlashConfig {
    /// Refuse non-removable devices unless explicitly overridden.
    #[serde(default = "default_true")]
    pub require_removable: bool,
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ShunConfig {
        ShunConfig {
            product: ProductIdentity {
                name: "WoWSP".into(),
                logo: Some("logo.webp".into()),
            },
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
}
