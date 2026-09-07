# ruff: noqa
# One-shot migration: config.rs v2 schema (shell ui / source / license / custom steps).
p = "src/config.rs"
s = open(p, encoding="utf-8").read()

old1 = '''/// Top-level shun configuration.
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
}'''
new1 = '''/// Top-level shun configuration.
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
}'''
assert old1 in s, "ShunConfig block not found"
s = s.replace(old1, new1, 1)

old2 = "use std::path::{Path, PathBuf};\n\nuse serde::{Deserialize, Serialize};"
new2 = "use std::collections::BTreeMap;\nuse std::path::{Path, PathBuf};\n\nuse serde::{Deserialize, Serialize};"
assert old2 in s, "imports not found"
s = s.replace(old2, new2, 1)

marker = "// ── Cargo.toml draft types ──"
newtypes = '''/// Runtime shell UI knobs.
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

// ── Cargo.toml draft types ──'''
assert marker in s, "draft marker not found"
s = s.replace(marker, newtypes, 1)

old3 = '''    /// `[package.metadata.shun.flash]`.
    #[serde(default)]
    flash: Option<FlashConfig>,
}'''
new3 = '''    /// `[package.metadata.shun.flash]`.
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
    #[serde(default)]
    license_locales: Option<BTreeMap<String, String>>,
    /// Custom content steps injected into the wizard.
    #[serde(default)]
    custom_steps: Option<Vec<CustomStepConfig>>,
}'''
assert old3 in s, "draft flash block not found"
s = s.replace(old3, new3, 1)

old4 = '''        Ok(ShunConfig {
            product: ProductIdentity {
                name: shun.product.unwrap_or(self.package.name),
                version,
                publisher: shun.publisher,
                logo: shun.logo,
            },
            payload: shun.payload.map(PathBuf::from),
            webview2: shun.webview2,
            targets,
        })'''
new4 = '''        Ok(ShunConfig {
            product: ProductIdentity {
                name: shun.product.unwrap_or(self.package.name),
                version,
                publisher: shun.publisher,
                logo: shun.logo,
            },
            payload: shun.payload.map(PathBuf::from),
            webview2: shun.webview2,
            targets,
            shell: shun.shell,
            source: shun.source,
            license: shun.license.map(PathBuf::from),
            license_locales: shun
                .license_locales
                .unwrap_or_default()
                .into_iter()
                .map(|(k, v)| (k, PathBuf::from(v)))
                .collect(),
            custom_steps: shun.custom_steps.unwrap_or_default(),
        })'''
assert old4 in s, "into_config return not found"
s = s.replace(old4, new4, 1)

old5 = '''        draft.into_config(cargo_toml)
    }'''
new5 = '''        draft.into_config(cargo_toml)
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
                Ok(draft.into_config(Path::new("")))
            }
            other => Err(crate::error::ShunError::Config(format!(
                "unsupported config extension: {other}"
            ))),
        }
    }'''
assert old5 in s, "from_cargo_manifest tail not found"
s = s.replace(old5, new5, 1)

open(p, "w", encoding="utf-8", newline="\n").write(s)
print("config v2 written")
