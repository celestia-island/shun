//! The shun build CLI.
//!
//! - `shun pack`      — payload directory → `*.shun` archive
//! - `shun config`    — resolve and validate a delivery manifest
//! - `shun icons`     — derive per-OS icon sets from a single logo
//! - `shun sign`      — Authenticode / codesign artifacts
//! - `shun build`     — manifest + payload → single-file installer
//! - `shun msix`      — pack the payload into a signed-ready .msix
//! - `shun flash list`— enumerate flash-candidate devices

use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

use clap::{Parser, Subcommand};
use shun::config::ShunConfig;
use shun::payload::pack_directory;
use shun::targets::flash::{FlashTarget, LogicalDrives};

#[derive(Parser)]
#[command(
    name = "shun",
    version,
    about = "Flow-driven payload delivery — build tool for installers and flashers"
)]
struct Cli {
    #[command(subcommand)]
    command: CliCommand,
}

#[derive(Subcommand)]
enum CliCommand {
    /// Pack a payload directory into a shun archive (*.shun).
    Pack {
        /// Directory to pack (becomes the payload root).
        payload_dir: PathBuf,
        /// Output archive path (default: <dir-name>.shun next to it).
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Resolve a delivery manifest and print the effective config as JSON.
    Config {
        /// Path to a Cargo.toml (uses [package.metadata.shun]) or a
        /// standalone .toml/.json document.
        #[arg(long)]
        path: PathBuf,
    },
    /// Derive per-OS icon sets from a single logo image.
    Icons {
        /// Logo source (webp/png; square renders best).
        #[arg(long)]
        logo: PathBuf,
        /// Output directory (windows/ linux/ macos/ subfolders are created).
        #[arg(long, default_value = "icons")]
        out: PathBuf,
    },
    /// Sign binaries per the signing section of a delivery manifest.
    Sign {
        /// Files to sign.
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Delivery manifest (Cargo.toml) or standalone config.
        #[arg(long)]
        manifest: PathBuf,
    },
    /// Build delivery artifacts: resolve the manifest, pack the payload,
    /// drive the shell build, then sign the installer.
    Build {
        /// Delivery manifest: a Cargo.toml with [package.metadata.shun] or
        /// a standalone .toml/.json config.
        #[arg(long)]
        manifest: PathBuf,
        /// Shell crate source directory (contains its own Cargo.toml with
        /// [package.metadata.shun] plus the UI). Defaults to ./shell.
        #[arg(long)]
        shell_src: Option<PathBuf>,
        /// Output directory for artifacts.
        #[arg(long, default_value = "dist")]
        out: PathBuf,
        /// Skip code signing even when configured.
        #[arg(long)]
        no_sign: bool,
    },
    /// Build an MSIX package from the payload (Windows SDK MakeAppx).
    Msix {
        /// Delivery manifest declaring [msix] (identity, publisher, ...).
        #[arg(long)]
        manifest: PathBuf,
        /// Output .msix path (default: <product>-<version>-x64.msix).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Logo for the manifest assets (png/webp; optional).
        #[arg(long)]
        logo: Option<PathBuf>,
        /// Skip code signing even when configured.
        #[arg(long)]
        no_sign: bool,
    },
    /// List flash-candidate devices.
    FlashList {},
}

fn main() {
    let cli = Cli::parse();
    if let Err(err) = run(cli.command) {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run(command: CliCommand) -> Result<(), String> {
    match command {
        CliCommand::Pack { payload_dir, out } => {
            let archive = pack_directory(&payload_dir).map_err(|e| e.to_string())?;
            let out = out.unwrap_or_else(|| {
                let name = payload_dir
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "payload".to_string());
                payload_dir.join(format!("{name}.shun"))
            });
            std::fs::write(&out, &archive).map_err(|e| e.to_string())?;
            println!(
                "packed {} ({} bytes) -> {}",
                payload_dir.display(),
                archive.len(),
                out.display()
            );
            Ok(())
        }
        CliCommand::Config { path } => {
            let config = resolve_config(&path)?;
            let json = serde_json::to_vec_pretty(&config).map_err(|e| e.to_string())?;
            println!("{}", String::from_utf8_lossy(&json));
            Ok(())
        }
        CliCommand::Icons { logo, out } => {
            icons::generate(&logo, &out).map_err(|e| e.to_string())?;
            println!("icon sets written under {}", out.display());
            Ok(())
        }
        CliCommand::Sign { files, manifest } => {
            let config = resolve_config(&manifest)?;
            let signing = config.signing.clone().unwrap_or_default();
            sign::sign_files(&files, &signing).map_err(|e| e.to_string())?;
            println!("signed {} file(s)", files.len());
            Ok(())
        }
        CliCommand::Build {
            manifest,
            shell_src,
            out,
            no_sign,
        } => {
            let config = resolve_config(&manifest)?;
            let product = config.product.clone();

            // 1. Payload package (*.shun) — works for both embedded and
            //    online shells (the online installer still needs the
            //    package hosted somewhere).
            let payload_dir = manifest.parent().unwrap_or(Path::new(".")).join(
                config
                    .payload
                    .clone()
                    .unwrap_or_else(|| PathBuf::from("payload")),
            );
            let archive = pack_directory(&payload_dir).map_err(|e| e.to_string())?;

            // 2. Shell build: hand the manifest to the shell via env and
            //    let its build.rs resolve config + payload embedding.
            let shell_src = shell_src.unwrap_or_else(|| PathBuf::from("shell"));
            let shell_manifest = shell_src.join("Cargo.toml");
            let shell_package: toml::Value =
                toml::from_str(&std::fs::read_to_string(&shell_manifest).map_err(|e| {
                    format!(
                        "cannot read shell manifest {}: {e}",
                        shell_manifest.display()
                    )
                })?)
                .map_err(|e| format!("shell manifest parse: {e}"))?;
            let shell_bin = shell_package
                .get("package")
                .and_then(|p| p.get("name"))
                .and_then(toml::Value::as_str)
                .unwrap_or("shun-demo-shell")
                .to_string();

            let status = StdCommand::new("cargo")
                .args([
                    "build",
                    "--release",
                    "--manifest-path",
                    &shell_manifest.display().to_string(),
                ])
                .env(
                    "SHUN_MANIFEST",
                    std::fs::canonicalize(&manifest).map_err(|e| e.to_string())?,
                )
                .status()
                .map_err(|e| format!("cargo build failed: {e}"))?;
            if !status.success() {
                return Err(format!("shell build failed (exit {status:?})"));
            }

            // 3. Collect artifacts: single-file installer + payload package.
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            let target_exe = shell_src
                .join("..")
                .join("target")
                .join("release")
                .join(format!("{shell_bin}.exe"));
            let installer = out.join(format!(
                "{}-{}-setup.exe",
                product.name.to_lowercase().replace(' ', "-"),
                product.version
            ));
            std::fs::copy(&target_exe, &installer).map_err(|e| {
                format!(
                    "copy installer failed (expected {}): {e}",
                    target_exe.display()
                )
            })?;
            let package = out.join(format!(
                "{}.shun",
                product.name.to_lowercase().replace(' ', "-")
            ));
            std::fs::write(&package, &archive).map_err(|e| e.to_string())?;

            // 4. Sign (unless disabled or unconfigured).
            if !no_sign {
                let signing = config.signing.clone().unwrap_or_default();
                if let Some(windows) = &signing.windows {
                    if windows.enabled {
                        sign::sign_files(std::slice::from_ref(&installer), &signing)
                            .map_err(|e| e.to_string())?;
                        println!("signed {}", installer.display());
                    }
                }
            }

            println!("build complete:");
            println!("  installer package: {}", package.display());
            println!("  installer binary:  {}", installer.display());
            Ok(())
        }
        CliCommand::Msix {
            manifest,
            out,
            logo,
            no_sign,
        } => {
            let config = resolve_config(&manifest)?;
            let msix = config.msix.clone().ok_or_else(|| {
                "the manifest declares no [msix] table — add identity-name, publisher and                  display-name to the delivery manifest"
                    .to_string()
            })?;
            let payload_dir = manifest.parent().unwrap_or(Path::new(".")).join(
                config
                    .payload
                    .clone()
                    .unwrap_or_else(|| PathBuf::from("payload")),
            );

            let logo_png = logo.as_deref().map(logo_png_bytes).transpose()?;
            let base = out.unwrap_or_else(|| PathBuf::from("dist"));
            std::fs::create_dir_all(&base).map_err(|e| e.to_string())?;
            let out_msix = base.join(format!(
                "{}-{}-x64.msix",
                msix.identity_name.to_lowercase().replace(' ', "-"),
                config.product.version
            ));

            let inputs = shun::msix::MsixInputs {
                identity_name: &msix.identity_name,
                publisher: &msix.publisher,
                display_name: &msix.display_name,
                description: msix.description.as_deref().unwrap_or(""),
                version: &config.product.version,
                executable: msix
                    .executable
                    .as_deref()
                    .unwrap_or(Path::new("bin/shun-demo.cmd")),
                logo_png: logo_png.as_deref(),
            };
            shun::msix::build_msix(&payload_dir, &out_msix, &inputs).map_err(|e| e.to_string())?;

            if !no_sign {
                let signing = config.signing.clone().unwrap_or_default();
                if let Some(windows) = &signing.windows {
                    if windows.enabled {
                        sign::sign_files(std::slice::from_ref(&out_msix), &signing)
                            .map_err(|e| e.to_string())?;
                        println!("signed {}", out_msix.display());
                    }
                }
            }
            println!("msix ready: {}", out_msix.display());
            Ok(())
        }
        CliCommand::FlashList {} => {
            let devices = LogicalDrives.list_devices().map_err(|e| e.to_string())?;
            if devices.is_empty() {
                println!("no removable drives found");
                return Ok(());
            }
            println!("{:<14} {:>14}  LABEL", "ID", "SIZE (B)");
            for device in devices {
                println!("{:<14} {:>14}  {}", device.id, device.size, device.label);
            }
            Ok(())
        }
    }
}

/// Converts any logo image into PNG bytes for the MSIX assets.
fn logo_png_bytes(logo: &Path) -> Result<Vec<u8>, String> {
    let img = image::open(logo)
        .map_err(|e| format!("cannot open logo: {e}"))?
        .into_rgba8();
    let mut png = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|e| format!("logo png encode: {e}"))?;
    Ok(png)
}

fn resolve_config(path: &Path) -> Result<ShunConfig, String> {
    if path.file_name().and_then(|n| n.to_str()) == Some("Cargo.toml") {
        ShunConfig::from_cargo_manifest(path)
    } else {
        ShunConfig::from_path(path)
    }
    .map_err(|e| e.to_string())
}

mod icons {
    use std::path::Path;

    use image::imageops::FilterType;

    const SIZES: [[u32; 2]; 8] = [
        [16, 16],
        [24, 24],
        [32, 32],
        [48, 48],
        [64, 64],
        [128, 128],
        [256, 256],
        [512, 512],
    ];

    /// Derives per-OS icon sets from one square logo:
    /// - windows/icon.ico (multi-size, from the largest render)
    /// - linux/*.png (full size ladder)
    /// - macos/icon.iconset/*.png (Apple iconset layout for iconutil)
    pub fn generate(logo: &Path, out: &Path) -> Result<(), String> {
        let img = image::open(logo).map_err(|e| format!("cannot open logo: {e}"))?;
        let img = img.into_rgba8();
        let largest = SIZES.iter().map(|s| s[0]).max().unwrap_or(512);
        let img = if img.width() != largest || img.height() != largest {
            image::imageops::resize(&img, largest, largest, FilterType::Lanczos3)
        } else {
            img
        };

        let windows_dir = out.join("windows");
        std::fs::create_dir_all(&windows_dir).map_err(|e| e.to_string())?;
        let ico = image::DynamicImage::ImageRgba8(img.clone());
        ico.save_with_format(windows_dir.join("icon.ico"), image::ImageFormat::Ico)
            .map_err(|e| format!("ico encode: {e}"))?;

        let linux_dir = out.join("linux");
        std::fs::create_dir_all(&linux_dir).map_err(|e| e.to_string())?;
        for [w, h] in SIZES {
            let scaled = image::imageops::resize(&img, w, h, FilterType::Lanczos3);
            scaled
                .save(linux_dir.join(format!("icon_{w}x{h}.png")))
                .map_err(|e| format!("png encode: {e}"))?;
        }

        let macos_dir = out.join("macos").join("shun.iconset");
        std::fs::create_dir_all(&macos_dir).map_err(|e| e.to_string())?;
        for [w, h] in SIZES {
            let scaled = image::imageops::resize(&img, w, h, FilterType::Lanczos3);
            scaled
                .save(macos_dir.join(format!("icon_{w}x{h}.png")))
                .map_err(|e| format!("png encode: {e}"))?;
            let (w2, h2) = (w * 2, h * 2);
            let scaled2 = image::imageops::resize(&img, w2, h2, FilterType::Lanczos3);
            scaled2
                .save(macos_dir.join(format!("icon_{w}x{h}@2x.png")))
                .map_err(|e| format!("png encode: {e}"))?;
        }

        // macOS: `iconutil -c icns shun.iconset` on any mac produces the
        // .icns; Linux CI can use `icnsutil`. Documented in the CLI guide.
        Ok(())
    }
}

mod sign {
    use std::path::{Path, PathBuf};

    use super::StdCommand;
    use shun::config::SigningConfig;

    /// Signs each file per the platform profile. Windows: signtool from
    /// PATH or the newest Windows Kits install; timestamping via RFC 3161.
    pub fn sign_files(files: &[PathBuf], signing: &SigningConfig) -> Result<(), String> {
        #[cfg(windows)]
        {
            let Some(windows) = &signing.windows else {
                return Err("no windows signing profile in the delivery manifest".into());
            };
            if !windows.enabled {
                return Err("windows signing is disabled in the manifest".into());
            }
            let signtool = find_signtool().ok_or_else(|| {
                "signtool.exe not found (install the Windows SDK or add it to PATH)".to_string()
            })?;
            for file in files {
                let mut cmd = StdCommand::new(&signtool);
                cmd.arg("sign").arg("/fd").arg("SHA256");
                cmd.arg("/tr").arg(&windows.timestamp_url);
                cmd.arg("/td").arg("SHA256");
                if let Some(thumbprint) = &windows.thumbprint {
                    cmd.arg("/sha1").arg(thumbprint);
                } else {
                    cmd.arg("/a");
                }
                cmd.arg(file);
                let status = cmd
                    .status()
                    .map_err(|e| format!("cannot run signtool: {e}"))?;
                if !status.success() {
                    return Err(format!("signtool failed for {}", file.display()));
                }
                println!("signed {}", file.display());
            }
            Ok(())
        }
        #[cfg(not(windows))]
        {
            let _ = (files, signing);
            Err("signing is only implemented for windows in this build".into())
        }
    }

    #[cfg(windows)]
    fn find_signtool() -> Option<PathBuf> {
        // 1. PATH.
        if let Ok(path_var) = std::env::var("PATH") {
            for dir in std::env::split_paths(&path_var) {
                let candidate = dir.join("signtool.exe");
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        // 2. Newest Windows Kits installation.
        let kits_root = std::env::var("ProgramFiles(x86)")
            .map(|root| Path::new(&root).join("Windows Kits").join("10").join("bin"))
            .ok()?;
        let mut versions: Vec<PathBuf> = std::fs::read_dir(&kits_root)
            .ok()?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        versions.sort();
        for version in versions.iter().rev() {
            let candidate = version.join("x64").join("signtool.exe");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        None
    }
}
