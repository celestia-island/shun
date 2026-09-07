//! MSIX packaging (Windows).
//!
//! Generates an `AppxManifest.xml` from the delivery configuration, stages
//! the payload beside it, and packs everything into a `.msix` via
//! `MakeAppx.exe` (Windows SDK — detected on the machine, with install
//! guidance when absent).
//!
//! Signing story: an `.msix` must be signed to install. The free paths are
//! (a) distributing through the Microsoft Store, which signs the package
//! for you, or (b) a self-signed certificate whose public half is trusted
//! on the target machines. A real code-signing certificate removes the
//! trust dance but costs money — all three work through `shun sign`.

use std::fs;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::error::ShunError;

/// MSIX packaging inputs resolved from the delivery config.
#[derive(Debug, Clone)]
pub struct MsixInputs<'a> {
    /// Identity Name — no spaces.
    pub identity_name: &'a str,
    /// Identity Publisher — must match the signing cert subject.
    pub publisher: &'a str,
    /// Display name (Start menu / Settings).
    pub display_name: &'a str,
    /// Package description.
    pub description: &'a str,
    /// Package version.
    pub version: &'a str,
    /// Payload-relative entry executable declared in the manifest.
    pub executable: &'a Path,
    /// Logo bytes written to `assets/logo.png` (optional).
    pub logo_png: Option<&'a [u8]>,
}

/// Locates the newest `MakeAppx.exe` from the Windows SDK installations.
pub fn find_makeappx() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let kits_root = std::env::var("ProgramFiles(x86)")
            .ok()
            .map(|root| Path::new(&root).join("Windows Kits").join("10").join("bin"))?;
        let mut versions: Vec<PathBuf> = fs::read_dir(&kits_root)
            .ok()?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        versions.sort();
        for version in versions.iter().rev() {
            for arch in ["x64", "arm64", "x86"] {
                let candidate = version.join(arch).join("makeappx.exe");
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        None
    }
    #[cfg(not(windows))]
    {
        None
    }
}

fn copy_payload_dir(source: &Path, staging: &Path) -> Result<(), ShunError> {
    for entry in WalkDir::new(source).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|e| ShunError::Config(format!("payload walk drift: {e}")))?;
        // MSIX manifests use forward-slash executable paths; keep the
        // staged file layout identical.
        let normalized = relative.to_string_lossy().replace('\\', "/");
        let target = staging.join(&normalized);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(entry.path(), &target)?;
    }
    Ok(())
}

/// Writes `AppxManifest.xml` and the logo asset into the staging directory.
pub fn write_manifest(staging: &Path, inputs: &MsixInputs<'_>) -> Result<(), ShunError> {
    // MSIX Identity Version must be a four-part number (Major.Minor.Build.
    // Revision); pad the semver from Cargo.toml with trailing zeros.
    let mut segments: Vec<String> = inputs.version.split('.').map(str::to_string).collect();
    while segments.len() < 4 {
        segments.push("0".to_string());
    }
    let msix_version = segments.join(".");
    let description = if inputs.description.is_empty() {
        format!("{} — delivered by shun", inputs.display_name)
    } else {
        inputs.description.to_string()
    };
    let manifest = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"
         xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10"
         xmlns:rescap="http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities">
  <Identity Name="{name}" Publisher="{publisher}" Version="{msix_version}" ProcessorArchitecture="x64" />
  <Properties>
    <DisplayName>{display}</DisplayName>
    <PublisherDisplayName>{publisher_display}</PublisherDisplayName>
    <Logo>assets/logo.png</Logo>
    <Description>{description}</Description>
  </Properties>
  <Resources>
    <Resource Language="en-us" />
  </Resources>
  <Dependencies>
    <TargetDeviceFamily Name="Windows.Desktop" MinVersion="10.0.17763.0" MaxVersionTested="10.0.26100.0" />
  </Dependencies>
  <Capabilities>
    <rescap:Capability Name="runFullTrust" />
  </Capabilities>
  <Applications>
    <Application Id="App" Executable="{executable}" EntryPoint="Windows.FullTrustApplication">
      <uap:VisualElements DisplayName="{display}" Description="{description}" BackgroundColor="transparent" Square150x150Logo="assets/logo.png" Square44x44Logo="assets/logo.png" />
    </Application>
  </Applications>
</Package>
"#,
        name = inputs.identity_name,
        publisher = xml_escape(inputs.publisher),
        msix_version = msix_version,
        display = xml_escape(inputs.display_name),
        publisher_display = xml_escape(inputs.publisher),
        description = xml_escape(&description),
        executable = inputs.executable.to_string_lossy().replace('\\', "/"),
    );

    let assets = staging.join("assets");
    fs::create_dir_all(&assets)?;
    if let Some(logo) = inputs.logo_png {
        fs::write(assets.join("logo.png"), logo)?;
    }
    fs::write(staging.join("AppxManifest.xml"), manifest)?;
    Ok(())
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Packs a staging directory into an `.msix` via `MakeAppx.exe`.
///
/// `makeappx` is located on the machine ([`find_makeappx`]); when absent
/// the error message carries install guidance.
pub fn pack(staging: &Path, out_msix: &Path) -> Result<(), ShunError> {
    let Some(makeappx) = find_makeappx() else {
        return Err(ShunError::Config(
            "MakeAppx.exe not found — install the Windows SDK (or the MSIX Packaging Tool) \
             to produce MSIX artifacts"
                .into(),
        ));
    };
    let out_msix = out_msix.with_extension("msix");
    if out_msix.exists() {
        fs::remove_file(&out_msix)?;
    }
    let status = std::process::Command::new(makeappx)
        .args(["pack", "/o", "/d"])
        .arg(staging)
        .arg("/p")
        .arg(&out_msix)
        .status()?;
    if !status.success() {
        return Err(ShunError::Config(format!(
            "makeappx failed (exit {status:?})"
        )));
    }
    Ok(())
}

/// One-shot MSIX artifact build: stage payload + manifest, pack.
pub fn build_msix(
    payload_dir: &Path,
    out_msix: &Path,
    inputs: &MsixInputs<'_>,
) -> Result<PathBuf, ShunError> {
    let staging = out_msix
        .parent()
        .unwrap_or(Path::new("."))
        .join(format!("{}.msix-staging", inputs.identity_name));
    fs::create_dir_all(&staging)?;
    copy_payload_dir(payload_dir, &staging)?;
    write_manifest(&staging, inputs)?;
    pack(&staging, out_msix)?;
    Ok(out_msix.to_path_buf())
}
