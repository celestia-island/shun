//! Build-time inputs for the demo shell, resolved from a delivery
//! manifest:
//!
//! - the shun configuration (loaded from `[package.metadata.shun]` via
//!   `ShunConfig::from_any` — or from the path in `SHUN_MANIFEST` when the
//!   `shun build` CLI drives an external application) is written to
//!   `OUT_DIR/shun-config.json` and embedded by the runtime; and
//! - the payload directory declared there is packed into
//!   `OUT_DIR/shun-demo-payload.shun` and embedded via `include_bytes!` —
//!   the single-file installer pattern, exercised for real.
//!
//! The application manifest (comctl32 v6 + per-monitor DPI) is also
//! embedded here: without it, Windows loads comctl32 v5 and the process
//! dies at startup with STATUS_ENTRYPOINT_NOT_FOUND (tao imports
//! TaskDialogIndirect).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn main() {
    let own_manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by cargo");

    // Delivery manifest resolution: the `shun build` CLI points
    // SHUN_MANIFEST at the application's manifest; a plain cargo build of
    // the shell falls back to the sibling demo application's manifest —
    // one source of truth (product identity, payload, license all live
    // there). SHUN_VARIANT applies the same named variant the CLI
    // selected, so the embedded payload/faces match the packed artifact.
    println!("cargo:rerun-if-env-changed=SHUN_MANIFEST");
    println!("cargo:rerun-if-env-changed=SHUN_VARIANT");
    let manifest_path = std::env::var("SHUN_MANIFEST")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            Path::new(&own_manifest_dir)
                .join("..")
                .join("demo-app")
                .join("Cargo.toml")
        });
    let manifest_dir = manifest_path
        .parent()
        .map(|d| d.to_path_buf())
        .unwrap_or_else(|| Path::new(&own_manifest_dir).to_path_buf());
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR is set by cargo");
    let out_dir = Path::new(&out_dir);

    // 1. Resolve the shun configuration (metadata.shun or standalone
    //    doc), then apply the selected variant's overrides so the
    //    embedded payload matches what `shun build --variant` packs.
    let mut config = shun::config::ShunConfig::from_any(&manifest_path)
        .expect("shun metadata in the delivery manifest parses");
    if let Ok(variant) = std::env::var("SHUN_VARIANT") {
        config
            .apply_variant(&variant)
            .unwrap_or_else(|e| panic!("SHUN_VARIANT: {e}"));
    }
    // Wallpaper backgrounds resolve to data URLs here: the runtime
    // config must carry its pixels (a single-file installer serves no
    // asset files). Theme images read relative to the manifest.
    embed_theme_images(&mut config, &manifest_dir);
    let config_json = serde_json::to_vec_pretty(&config).expect("config serializes");
    std::fs::write(out_dir.join("shun-config.json"), config_json).expect("write embedded config");
    println!("cargo:rerun-if-changed={}", manifest_path.display());

    // 1b. Resolve the wizard pipeline with markdown bodies inlined —
    // runtime shells carry no file dependencies, so license/content
    // documents are read (relative to the manifest) and embedded here.
    let steps = config
        .resolve_steps(&manifest_dir, None)
        .expect("wizard pipeline resolves");
    for doc in config.license.iter().chain(config.license_locales.values()) {
        println!(
            "cargo:rerun-if-changed={}",
            manifest_dir.join(doc).display()
        );
    }
    for license in &config.licenses {
        println!(
            "cargo:rerun-if-changed={}",
            manifest_dir.join(&license.path).display()
        );
        for doc in license.locale_paths.values() {
            println!(
                "cargo:rerun-if-changed={}",
                manifest_dir.join(doc).display()
            );
        }
    }
    for step in &config.steps.clone().unwrap_or_default() {
        if step.kind == shun::config::StepKind::Content {
            if let Some(markdown) = &step.markdown {
                println!(
                    "cargo:rerun-if-changed={}",
                    manifest_dir.join(markdown).display()
                );
            }
        }
    }
    std::fs::write(
        out_dir.join("shun-steps.json"),
        serde_json::to_vec_pretty(&steps).expect("steps serialize"),
    )
    .expect("write embedded steps");

    // 1c. Resolve the license documents per shell locale — the wizard
    // switches the agreement along with the language picked on its
    // first step. The locale set mirrors the shell's i18n tables
    // (`shell/web/src/i18n.ts` `LOCALES`); documents without a variant
    // for a locale resolve to their base path (resolve_steps' existing
    // fallback). The egui fallback shares this one JSON and reads its
    // two supported tables out of it.
    const SHELL_LOCALES: [&str; 8] = ["en", "zh-Hans", "zh-Hant", "ja", "ko", "fr", "ru", "es"];
    let mut license_docs: BTreeMap<String, Vec<shun::config::ResolvedLicenseDoc>> = BTreeMap::new();
    for locale in SHELL_LOCALES {
        let locale_steps = config
            .resolve_steps(&manifest_dir, Some(locale))
            .expect("wizard pipeline resolves per locale");
        if let Some(license) = locale_steps
            .iter()
            .find(|step| step.kind == shun::config::StepKind::License)
        {
            license_docs.insert(locale.to_string(), license.licenses.clone());
        }
    }
    std::fs::write(
        out_dir.join("shun-license-docs.json"),
        serde_json::to_vec_pretty(&license_docs).expect("license docs serialize"),
    )
    .expect("write embedded license docs");

    // 2. Pack the payload directory declared in the configuration (paths in
    //    the manifest are relative to it).
    let payload_relative = config
        .payload
        .clone()
        .expect("metadata.shun.payload declares the payload directory");
    let payload_dir = manifest_dir.join(payload_relative);
    let archive = shun::payload::pack_directory(&payload_dir).expect("demo payload packs cleanly");
    std::fs::write(out_dir.join("shun-demo-payload.shun"), archive)
        .expect("write embedded payload");
    println!("cargo:rerun-if-changed={}", payload_dir.display());

    // 3. Embed the product logo for the UIs (title bars). Always written
    //    so include_bytes! has a stable target; the kind file says which
    //    decoder to use ("none" when the manifest declares no logo).
    let kind = config
        .product
        .logo
        .as_ref()
        .and_then(|logo| std::fs::read(manifest_dir.join(logo)).ok())
        .map(|bytes| {
            let extension = config
                .product
                .logo
                .as_deref()
                .map(Path::new)
                .and_then(|path| path.extension().map(|e| e.to_string_lossy().into_owned()))
                .unwrap_or_else(|| "none".into());
            std::fs::write(out_dir.join("shun-logo.bin"), bytes).expect("write embedded logo");
            extension
        })
        .unwrap_or_else(|| {
            std::fs::write(out_dir.join("shun-logo.bin"), []).expect("write empty logo");
            "none".into()
        });
    std::fs::write(out_dir.join("shun-logo-kind.txt"), kind).expect("write logo kind");

    // 4. Embed the application manifest (comctl32 v6 + per-monitor DPI).
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(
        tauri_build::WindowsAttributes::new().app_manifest(include_str!("app.manifest")),
    ))
    .expect("tauri-build failed");
}

/// Rewrites every wallpaper `BackgroundSpec::Image` in the theme into a
/// `DataUrl` carrying the image bytes (base64, mime by extension), so
/// the embedded runtime config renders wallpapers with no sidecar
/// files. Missing files are a build error — a declared wallpaper that
/// silently vanishes is a broken theme.
fn embed_theme_images(config: &mut shun::config::ShunConfig, manifest_dir: &std::path::Path) {
    use base64::Engine;
    use shun::config::{BackgroundSpec, ThemeConfig, WallpaperSourceSpec};

    let Some(theme) = config.shell.as_mut().and_then(|shell| shell.theme.as_mut()) else {
        return;
    };
    // The wallpaper chain embeds the same way: local videos and images
    // become data URLs (offline playback), URLs pass through.
    if let Some(wallpaper) = theme.wallpaper.as_mut() {
        for source in wallpaper.sources.iter_mut() {
            let (kind, path_ref) = match source {
                WallpaperSourceSpec::Video { video } => ("video", video),
                WallpaperSourceSpec::Image { image } => ("image", image),
                WallpaperSourceSpec::Pipeline { .. } => continue,
            };
            if path_ref.starts_with("https://") || path_ref.starts_with("http://") {
                continue;
            }
            let path = manifest_dir.join(path_ref.clone());
            let bytes = std::fs::read(&path)
                .unwrap_or_else(|e| panic!("wallpaper {kind} {}: {e}", path.display()));
            let mime = match path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str()
            {
                "mp4" | "m4v" => "video/mp4",
                "webm" => "video/webm",
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "webp" => "image/webp",
                "gif" => "image/gif",
                other => panic!("wallpaper {kind} {other}: unsupported format"),
            };
            let url = format!(
                "data:{mime};base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            );
            println!("cargo:rerun-if-changed={}", path.display());
            *path_ref = url;
        }
    }
    let ThemeConfig {
        background,
        rail_background,
        pane_background,
        ..
    } = theme;
    for spec in [background, rail_background, pane_background]
        .into_iter()
        .flatten()
    {
        {
            let inner = spec;
            if let BackgroundSpec::Image { image } = inner {
                let path = manifest_dir.join(image.clone());
                let bytes = std::fs::read(&path)
                    .unwrap_or_else(|e| panic!("theme wallpaper {}: {e}", path.display()));
                let mime = match path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase()
                    .as_str()
                {
                    "png" => "image/png",
                    "jpg" | "jpeg" => "image/jpeg",
                    "webp" => "image/webp",
                    "gif" => "image/gif",
                    "bmp" => "image/bmp",
                    other => panic!("theme wallpaper {other}: unsupported format"),
                };
                let url = format!(
                    "data:{mime};base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(bytes)
                );
                println!("cargo:rerun-if-changed={}", path.display());
                *inner = BackgroundSpec::DataUrl(url);
            }
        }
    }
}
