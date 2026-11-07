//! Build-time inputs for the shell, resolved from a delivery manifest —
//! or, in the GENERIC preset, from nothing at all.
//!
//! Two embedding paths now share one implementation ([`shun::stamp`]):
//!
//! - **Manifest path** (this build under `SHUN_MANIFEST`, or the sibling
//!   demo app by default): the shun configuration is written to
//!   `OUT_DIR/shun-config.json`, the wizard pipeline and per-locale
//!   license documents beside it, and the payload directory declared in
//!   the config is packed into `OUT_DIR/shun-demo-payload.shun` — all
//!   embedded via include_bytes! by the runtime. This is the LOCAL and
//!   DEMO lane; the transforms themselves moved into `shun::stamp` so
//!   the stamp CLI embeds byte-identical data and the two paths cannot
//!   drift.
//! - **Generic preset** (`SHUN_SHELL_PRESET=generic`): minimal
//!   placeholders and no product identity — the binary is a blank shell
//!   whose runtime prefers a stamp container ([`shun::stamp`] format,
//!   appended by `shun build --shell`). A generic exe is a BUILD INPUT,
//!   never a runnable installer: its placeholder config decodes to
//!   nothing on purpose.
//!
//! The application manifest (comctl32 v6 + per-monitor DPI) is embedded
//! in both paths: without it, Windows loads comctl32 v5 and the process
//! dies at startup with STATUS_ENTRYPOINT_NOT_FOUND (tao imports
//! TaskDialogIndirect).

use std::path::{Path, PathBuf};

/// Warns when the embedded web frontend (`web/dist`, TRACKED in this
/// repo) is older than its sources.
///
/// The dist is committed so tag checkouts and local `shun build` runs
/// carry a usable face without a node toolchain — but that also means a
/// stale build is invisible: several web-face fixes once reached `src`
/// without ever reaching an installer because nobody rebuilt the dist
/// (found in the field: the license step's spacing still showed the old
/// layout). A `cargo:warning` in every build surfaces the trap;
/// `pnpm build` in `shell/web` + committing the dist clears it.
fn warn_on_stale_web_dist(manifest_dir: &Path) {
    let web = manifest_dir.join("web");
    let dist_mtime = match std::fs::metadata(web.join("dist").join("index.html"))
        .and_then(|meta| meta.modified())
    {
        Ok(mtime) => mtime,
        // No dist at all: the include step fails loudly on its own.
        Err(_) => return,
    };
    fn newest_mtime(dir: &Path, newest: &mut Option<std::time::SystemTime>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name == "node_modules" || name == "dist" {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                newest_mtime(&entry.path(), newest);
            } else if let Ok(mtime) = meta.modified() {
                if newest.is_none_or(|current| mtime > current) {
                    *newest = Some(mtime);
                }
            }
        }
    }
    let mut newest_src = None;
    newest_mtime(&web.join("src"), &mut newest_src);
    for file in ["package.json", "vite.config.ts", "index.html", "../strings"] {
        let path = web.join(file);
        let entries: Vec<PathBuf> = if path.is_dir() {
            std::fs::read_dir(&path)
                .map(|read| read.flatten().map(|entry| entry.path()).collect())
                .unwrap_or_default()
        } else {
            vec![path]
        };
        for file in entries {
            if let Ok(mtime) = std::fs::metadata(&file).and_then(|meta| meta.modified()) {
                if newest_src.is_none_or(|current| mtime > current) {
                    newest_src = Some(mtime);
                }
            }
        }
    }
    // Fresh git checkouts stamp every file with the checkout time in
    // arbitrary write order, so a few seconds of "src newer than dist"
    // is noise, not staleness; a real forgotten rebuild is minutes or
    // more apart.
    const MTIME_SLACK: std::time::Duration = std::time::Duration::from_secs(60);
    if newest_src
        .and_then(|src_mtime| src_mtime.duration_since(dist_mtime).ok())
        .is_some_and(|age| age > MTIME_SLACK)
    {
        println!(
            "cargo:warning=shell/web/dist is OLDER than its sources — the embedded web face is stale. Run `pnpm build` in shell/web and commit the dist."
        );
    }
}

fn main() {
    let own_manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by cargo");
    warn_on_stale_web_dist(Path::new(&own_manifest_dir));

    println!("cargo:rerun-if-env-changed=SHUN_MANIFEST");
    println!("cargo:rerun-if-env-changed=SHUN_VARIANT");
    println!("cargo:rerun-if-env-changed=SHUN_SHELL_PRESET");
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR is set by cargo");
    let out_dir = Path::new(&out_dir);

    if std::env::var("SHUN_SHELL_PRESET").as_deref() == Ok("generic") {
        // The generic preset embeds NOTHING product-shaped: placeholder
        // files under the exact names the runtime include!s expect, and
        // no tauri identity overlay (the conf file's neutral defaults
        // stand). A stamped exe never decodes these placeholders — the
        // runtime prefers its stamp container, and a generic exe run
        // bare is a build input mishandled as a product, which the
        // placeholder config decode refuses loudly.
        std::fs::write(out_dir.join("shun-config.json"), "{}").expect("write placeholder config");
        std::fs::write(out_dir.join("shun-steps.json"), "[]").expect("write placeholder steps");
        std::fs::write(out_dir.join("shun-license-docs.json"), "{}")
            .expect("write placeholder license docs");
        std::fs::write(out_dir.join("shun-logo.bin"), []).expect("write placeholder logo");
        std::fs::write(out_dir.join("shun-logo-kind.txt"), "none")
            .expect("write placeholder logo kind");
        std::fs::write(out_dir.join("shun-demo-payload.shun"), [])
            .expect("write placeholder payload");
        println!("cargo:rustc-env=SHUN_PRODUCT_LOG_NAME=installer");
        tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(
            tauri_build::WindowsAttributes::new().app_manifest(include_str!("app.manifest")),
        ))
        .expect("tauri-build failed");
        return;
    }

    // Delivery manifest resolution: the `shun build` CLI points
    // SHUN_MANIFEST at the application's manifest; a plain cargo build of
    // the shell falls back to the sibling demo application's manifest —
    // one source of truth (product identity, payload, license all live
    // there). SHUN_VARIANT applies the same named variant the CLI
    // selected, so the embedded payload/faces match the packed artifact.
    let manifest_path = std::env::var("SHUN_MANIFEST")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            Path::new(&own_manifest_dir)
                .join("..")
                .join("demo-app")
                .join("Cargo.toml")
        });
    let variant = std::env::var("SHUN_VARIANT").ok();

    // 1. Resolve the shun configuration (metadata.shun or standalone
    //    doc), apply the selected variant, and turn theme images into
    //    data URLs — the shared transform the stamp CLI runs too.
    let (config, manifest_dir) = shun::stamp::resolved_config(&manifest_path, variant.as_deref())
        .expect("shun metadata in the delivery manifest resolves");
    let config_json = serde_json::to_vec_pretty(&config).expect("config serializes");
    std::fs::write(out_dir.join("shun-config.json"), config_json).expect("write embedded config");
    println!("cargo:rerun-if-changed={}", manifest_path.display());
    // The per-run setup log's directory name (diag.rs sinks there): the
    // product identity, sanitized for a path segment.
    let log_name: String = config
        .product
        .name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    println!("cargo:rustc-env=SHUN_PRODUCT_LOG_NAME={log_name}");

    // 1b. Resolve the wizard pipeline with markdown bodies inlined —
    //    runtime shells carry no file dependencies, so license/content
    //    documents are read (relative to the manifest) and embedded here.
    let mut steps = config
        .resolve_steps(&manifest_dir, None)
        .expect("wizard pipeline resolves");

    // The demo's release-notes content step pulls the LATEST GitHub
    // release body at build time — the categorized gitmoji summary the
    // release-notes workflow publishes (### ✨ Features / 🐛 Fixes / …).
    // Offline or on failure the vendored markdown file stays as-is.
    // Gated to the demo app: product installers author their own
    // release-notes content.
    if manifest_dir.ends_with("demo-app") {
        if let Ok(response) =
            ureq::get("https://api.github.com/repos/celestia-island/shun/releases/latest")
                .set("Accept", "application/vnd.github+json")
                .set("User-Agent", "shun-demo-build")
                .timeout(std::time::Duration::from_secs(15))
                .call()
        {
            let body: Option<String> = response.into_string().ok().and_then(|text| {
                serde_json::from_str::<serde_json::Value>(&text)
                    .ok()
                    .and_then(|json| {
                        json.get("body")
                            .and_then(|body| body.as_str().map(str::to_string))
                    })
            });
            if let Some(body) = body {
                for step in &mut steps {
                    // The demo's release-notes step declares
                    // markdown = "release-notes.md"; match the resolved
                    // body against that file's first line rather than the
                    // (already-inlined) path.
                    if step.kind == shun::config::StepKind::Content
                        && step
                            .body
                            .as_deref()
                            .is_some_and(|body| body.contains("ShunDemo 发行说明"))
                    {
                        step.body = Some(body);
                        break;
                    }
                }
            }
        }
    }
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

    // 1c. Resolve the license documents per shell locale — the shared
    //    transform again (the same eight-locale list lives in
    //    shun::stamp::SHELL_LOCALES, mirroring the shell's i18n tables).
    let license_docs_json = shun::stamp::license_docs_json(&config, &manifest_dir)
        .expect("license docs resolve per locale");
    std::fs::write(out_dir.join("shun-license-docs.json"), license_docs_json)
        .expect("write embedded license docs");

    // 2. Pack the payload directory declared in the configuration — the
    //    shared transform (the exact bytes the stamp CLI would embed).
    let payload_archive =
        shun::stamp::payload_archive(&config, &manifest_dir).expect("payload packs cleanly");
    std::fs::write(out_dir.join("shun-demo-payload.shun"), payload_archive)
        .expect("write embedded payload");
    let payload_relative = config
        .payload
        .clone()
        .expect("metadata.shun.payload declares the payload directory");
    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir.join(payload_relative).display()
    );

    // 3. Embed the product logo for the UIs (title bars) — the shared
    //    transform, same kind-file contract as before. Always written so
    //    include_bytes! has a stable target; the kind file says which
    //    decoder to use ("none" when the manifest declares no logo).
    let (logo_bytes, kind) =
        shun::stamp::logo_parts(&config, &manifest_dir).expect("the product logo resolves");
    std::fs::write(out_dir.join("shun-logo.bin"), &logo_bytes).expect("write embedded logo");
    std::fs::write(out_dir.join("shun-logo-kind.txt"), &kind).expect("write logo kind");

    // 3b. The webview face's exe/window icon follows the manifest too:
    //     the product logo re-encoded as a multi-size ICO and embedded
    //     as the Windows exe resource (Explorer, the pre-first-paint
    //     taskbar icon, and any shortcut pointing at the setup exe).
    //     Without it tauri-build stamps its generic icon while the egui
    //     face (`ViewportBuilder::with_icon`) follows the manifest —
    //     the two faces would disagree. (The stamp CLI re-runs this same
    //     encoder when stamping a generic shell — see
    //     shun::stamp::logo_ico_bytes.)
    let mut windows_attributes =
        tauri_build::WindowsAttributes::new().app_manifest(include_str!("app.manifest"));
    if kind != "none" {
        let ico_path = out_dir.join("shun-logo.ico");
        let ico_bytes =
            shun::stamp::logo_ico_bytes(&logo_bytes).expect("the product logo encodes as an ICO");
        std::fs::write(&ico_path, ico_bytes).expect("write logo ICO");
        windows_attributes = windows_attributes.window_icon_path(&ico_path);
    }

    // 4. Carry the product identity into tauri-build: the version
    //    resource it stamps (FileDescription/ProductName — what Explorer,
    //    the taskbar and the UAC prompt show) and the webview face's
    //    window title otherwise read this shell's generic "shun
    //    installer" defaults from tauri.conf.json, so every product's
    //    setup.exe introduced itself as the packager. TAURI_CONFIG is a
    //    JSON merge-patch tauri-build applies over that file; arrays are
    //    replaced wholesale (RFC 7396), so the windows entry is mirrored
    //    in full with only the title swapped. (For stamped generic
    //    shells the identity rides the container + the OS resource
    //    editor instead — see shun::stamp.)
    let slug = config.product.name.to_lowercase().replace(' ', "-");
    let identity_overlay = serde_json::json!({
        "productName": config.product.name,
        "version": config.product.version,
        "identifier": format!("world.celestia.{slug}.installer"),
        "app": {
            "windows": [{
                "label": "installer",
                "title": config.product.name,
                "width": 800,
                "height": 600,
                "resizable": false,
                "fullscreen": false,
                "center": true,
                "visible": true,
                "decorations": false,
                "transparent": false,
                "shadow": true,
                "minimizable": true,
                "maximizable": false,
                "closable": true
            }]
        }
    });
    // SAFETY: build scripts are single-threaded; the variable is set
    // after all parallel file stages and immediately consumed by
    // try_build in this same process.
    unsafe { std::env::set_var("TAURI_CONFIG", identity_overlay.to_string()) };

    // 5. Embed the application manifest (comctl32 v6 + per-monitor DPI).
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows_attributes))
        .expect("tauri-build failed");
}
