//! Plain tauri-build entry for the demo application, with the same
//! comctl32 v6 + per-monitor-DPI application manifest the installer shell
//! embeds: without it Windows loads comctl32 v5 and the process dies at
//! startup with STATUS_ENTRYPOINT_NOT_FOUND (tao imports
//! TaskDialogIndirect).

/// Warns when the committed webview face (`ui/app.js`, the esbuild output
/// `pnpm --dir web build` produces) is older than its source — the same
/// trap shell/build.rs watches for: a stale build is invisible, and the
/// posture test cannot catch a bundle that merely drifted from an edited
/// source. A cargo:warning surfaces it on every compile.
fn warn_on_stale_demo_bundle(manifest_dir: &std::path::Path) {
    let web = manifest_dir.join("web");
    let bundle_mtime = match std::fs::metadata(web.join("..").join("ui").join("app.js"))
        .and_then(|meta| meta.modified())
    {
        Ok(mtime) => mtime,
        // No bundle at all: the include step fails loudly on its own.
        Err(_) => return,
    };
    fn newest_mtime(dir: &std::path::Path, newest: &mut Option<std::time::SystemTime>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                if entry.file_name() == "node_modules" {
                    continue;
                }
                newest_mtime(&entry.path(), newest);
            } else if let Ok(mtime) = entry.metadata().and_then(|meta| meta.modified()) {
                if newest.is_none_or(|current| mtime > current) {
                    *newest = Some(mtime);
                }
            }
        }
    }
    let mut newest_src = None;
    newest_mtime(&web.join("src"), &mut newest_src);
    for file in ["package.json", "pnpm-workspace.yaml"] {
        if let Ok(mtime) = std::fs::metadata(web.join(file)).and_then(|meta| meta.modified()) {
            if newest_src.is_none_or(|current| mtime > current) {
                newest_src = Some(mtime);
            }
        }
    }
    // Fresh git checkouts stamp arbitrary write order: a minute of
    // "src newer than bundle" is noise, a forgotten rebuild is not.
    const MTIME_SLACK: std::time::Duration = std::time::Duration::from_secs(60);
    if newest_src
        .and_then(|src| src.duration_since(bundle_mtime).ok())
        .is_some_and(|age| age > MTIME_SLACK)
    {
        println!(
            "cargo:warning=demo-app/ui/app.js is OLDER than demo-app/web — the committed bundle is stale. Run pnpm --dir demo-app/web build and commit ui/app.js."
        );
    }
}

fn main() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    warn_on_stale_demo_bundle(manifest_dir);
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(
        tauri_build::WindowsAttributes::new().app_manifest(include_str!("app.manifest")),
    ))
    .expect("tauri-build failed");
}
