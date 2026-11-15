//! WebView context-menu hardening for the installer shell.
//!
//! User direction 2026-10-15: the PRODUCTION-built shell must not surface
//! the webview's native right-click menu — the wizard owns the whole
//! surface and the browser-flavored menu (Back / Reload / Save as …) has
//! no business in a delivery tool. Debug builds keep it for development.
//!
//! Family twin: a richer module of the same name ships in shittim-chest's
//! desktop shell (credential-manager suppression, accelerator keys,
//! devtools) and the flasher. This shell intentionally carries the
//! minimal, cast-free slice: `AreDefaultContextMenusEnabled` and
//! `AreDevToolsEnabled` both live on the v1 `ICoreWebView2Settings`
//! interface, so no `windows-core` dependency is needed — a diverging
//! windows-core copy is exactly what breaks `settings.cast::<…>()`
//! against the webview2-com build `with_webview` hands over.
//!
//! The DOM layer (the wizard's own `contextmenu` handling) stays the
//! primary owner; this is the native belt for frames where JS has not
//! attached yet and for WebView2's non-DOM menu items.
//!
//! # CSP and the global Tauri object (2026-11-16)
//!
//! The shell's `tauri.conf.json` pins BOTH halves of the webview's script
//! posture, and they belong together:
//!
//! * `app.withGlobalTauri: false` — no `window.__TAURI__` object is
//!   injected. The frontend imports `@tauri-apps/api` and
//!   `@tauri-apps/plugin-dialog` explicitly (`shell/web/src/tauri.ts`), so
//!   an asset injected into the page has no pre-built handle to drive the
//!   command surface with; the ipc core (`window.__TAURI_INTERNALS__`)
//!   stays because that IS the transport.
//! * `app.security.csp` — `default-src 'self'; script-src 'self';
//!   style-src 'self' 'unsafe-inline'; img-src 'self' data: https:;
//!   media-src 'self' data: https:; connect-src 'self' ipc:
//!   http://ipc.localhost; object-src 'none'; base-uri 'self';
//!   form-action 'none'`.
//!
//!   Each allowance answers a real consumer of the shipped bundle, not a
//!   guess: `'self'` is the whole frontend; `data:` is the manifest's
//!   embedded logo/theme/wallpaper; `https:` is the ONLINE wallpaper lane
//!   the manifest may declare (hikari paints it through `<img>`/`<video>`
//!   `src`, so blocking it would silently stand the surface down to the
//!   solid floor); `'unsafe-inline'` on styles covers runtime-injected
//!   style attributes (Vue's patches, hikari's tooltip/toast surfaces) —
//!   style injection cannot execute script, and `script-src` stays
//!   hash/nonce-clean (tauri appends its own nonce/hash sources to
//!   `script-src`/`style-src` when it stamps an asset). `connect-src`
//!   names the ipc transport only: `ipc:` on Linux/macOS,
//!   `http://ipc.localhost` on Windows. Nothing in the bundle fetches over
//!   the network: wallpaper sources load as an element `src`, and
//!   attachment downloads run in Rust.
//!
//! Verified against the built `web/dist` served under this exact policy in
//! headless Chromium (zero CSP violations across language → location →
//! license, the wizard's own screens), and the bundle audited for `eval`,
//! `new Function`, `blob:` object URLs and inline `<script>`/`<style>`
//! (all absent). NOT verified here: the Windows WebView2 runtime itself —
//! the ipc round-trip, the install flow and the guide/toast surfaces need
//! a real machine.

/// Attach the hardening to the shell's webview window. Best-effort: a
/// webview2 COM failure degrades to the browser default, so errors are
/// logged and swallowed.
pub fn attach(window: &tauri::WebviewWindow) {
    // Release builds only: a debug build is a development surface and
    // keeps both the menu and devtools.
    if cfg!(debug_assertions) {
        let _ = window;
        return;
    }

    #[cfg(target_os = "windows")]
    {
        let label = window.label().to_string();
        let _ = window.with_webview(move |webview| unsafe {
            // PlatformWebview::controller() returns the controller directly
            // (not a Result) on Windows — tauri clones the COM ref out.
            let controller = webview.controller();
            let Ok(core) = controller.CoreWebView2() else {
                eprintln!("shun: webview hardening: no CoreWebView2 for {label}");
                return;
            };
            let Ok(settings) = core.Settings() else {
                eprintln!("shun: webview hardening: no Settings for {label}");
                return;
            };
            if let Err(e) = settings.SetAreDefaultContextMenusEnabled(false) {
                eprintln!("shun: webview hardening: context menu off failed: {e}");
            }
            if let Err(e) = settings.SetAreDevToolsEnabled(false) {
                eprintln!("shun: webview hardening: devtools off failed: {e}");
            }
        });
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = window;
    }
}

#[cfg(test)]
mod tests {
    //! The pinned script posture, held by tests so neither half can drift
    //! back on its own: the config (`withGlobalTauri` off + a CSP that
    //! covers what the shipped bundle actually loads), the frontend (explicit
    //! `@tauri-apps` imports, no global object), and the COMMITTED bundle
    //! (rebuilt after the source change — a stale `dist/` would otherwise
    //! ship a page that asks for `window.__TAURI__` nobody injects).

    use std::path::{Path, PathBuf};

    /// `tauri.conf.json` as tauri-build reads it.
    const TAURI_CONF: &str = include_str!("../tauri.conf.json");

    /// The sibling demo app's webview face (`demo-app/ui`), which tauri-build
    /// reads the same way. Kept beside the shell's so a regression in either
    /// surface fails one `cargo test`.
    const DEMO_CONF: &str = include_str!("../../demo-app/tauri.conf.json");
    const DEMO_HTML: &str = include_str!("../../demo-app/ui/index.html");
    /// The demo face SOURCE — the only place a global-object read could be
    /// introduced; the committed bundle below must be its build.
    const DEMO_JS_SRC: &str = include_str!("../../demo-app/web/src/app.js");
    /// The committed esbuild output (pnpm --dir web build), shipped through
    /// tauri.conf.json's frontendDist: the source plus the inlined packaged API.
    const DEMO_BUNDLE: &str = include_str!("../../demo-app/ui/app.js");
    /// The demo web project — the build that turns the source into the bundle.
    const DEMO_PACKAGE_JSON: &str = include_str!("../../demo-app/web/package.json");
    /// The shell's Tauri surface (the only module allowed to name it).
    const TAURI_TS: &str = include_str!("../web/src/tauri.ts");
    /// The frontend's dependency list — the imports must be installable.
    const WEB_PACKAGE_JSON: &str = include_str!("../web/package.json");

    fn conf() -> serde_json::Value {
        serde_json::from_str(TAURI_CONF).expect("tauri.conf.json parses")
    }

    fn csp() -> String {
        conf()["app"]["security"]["csp"]
            .as_str()
            .expect("app.security.csp is set (a null CSP would leave the page unguarded)")
            .to_string()
    }

    /// Every file under `shell/web/dist/assets`, the committed bundle the
    /// shell embeds at compile time.
    fn bundle_scripts(root: &Path, into: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(root) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                bundle_scripts(&path, into);
            } else if path.extension().is_some_and(|ext| ext == "js") {
                into.push(path);
            }
        }
    }

    #[test]
    fn the_global_tauri_object_stays_off() {
        assert_eq!(
            conf()["app"]["withGlobalTauri"],
            serde_json::Value::Bool(false),
            "app.withGlobalTauri must stay false: the global object is a ready-made \
             command surface for any asset injected into the page"
        );
        // The frontend must not READ it either — the explicit imports are
        // the only path to the command surface. (The packaged API still
        // PROBES the global for compat paths, e.g. plugin-dialog's opener
        // fallback; the tell-tale is a read of its `core`.)
        for read in ["__TAURI__.core", "__TAURI__?.core"] {
            assert!(
                !TAURI_TS.contains(read),
                "shell/web/src/tauri.ts still reads the global object ({read})"
            );
        }
        assert!(
            TAURI_TS.contains("@tauri-apps/api/core") && TAURI_TS.contains("@tauri-apps/api/event"),
            "the shell's Tauri surface must ride the explicit @tauri-apps imports"
        );
        assert!(
            TAURI_TS.contains("@tauri-apps/plugin-dialog"),
            "the folder picker must use the packaged dialog plugin"
        );
        // …and those packages have to be declared, or the build cannot
        // resolve the imports.
        for package in ["\"@tauri-apps/api\"", "\"@tauri-apps/plugin-dialog\""] {
            assert!(
                WEB_PACKAGE_JSON.contains(package),
                "shell/web/package.json is missing {package}"
            );
        }
    }

    #[test]
    fn the_csp_covers_what_the_bundle_loads_and_nothing_wider() {
        let csp = csp();
        // The bundle is the only script origin.
        assert!(csp.contains("default-src 'self'"), "csp: {csp}");
        assert!(csp.contains("script-src 'self'"), "csp: {csp}");
        // Vue/hikari patch inline style attributes at runtime; style
        // injection cannot execute script, so this is the one allowance.
        assert!(
            csp.contains("style-src 'self' 'unsafe-inline'"),
            "csp: {csp}"
        );
        // Manifest-embedded images ride `data:`, online wallpapers `https:`.
        assert!(csp.contains("img-src 'self' data: https:"), "csp: {csp}");
        // The video wallpaper lane.
        assert!(csp.contains("media-src 'self' data: https:"), "csp: {csp}");
        // The ipc transport: `ipc:` on Linux/macOS, `http://ipc.localhost`
        // on Windows — dropping either strands a face on that platform.
        assert!(csp.contains("ipc:"), "csp: {csp}");
        assert!(csp.contains("http://ipc.localhost"), "csp: {csp}");
        // No plugins, no <base> rewrites, no forms in the wizard.
        assert!(csp.contains("object-src 'none'"), "csp: {csp}");
        assert!(csp.contains("base-uri 'self'"), "csp: {csp}");
        assert!(csp.contains("form-action 'none'"), "csp: {csp}");
        // A directive list this long must stay a single header line.
        assert!(!csp.contains('\n'), "csp must stay one line: {csp}");
    }

    #[test]
    fn the_committed_bundle_has_no_global_tauri_object() {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let dist = manifest.join("web").join("dist");
        let index = dist.join("index.html");
        let html = std::fs::read_to_string(&index).expect(
            "the committed shell/web/dist must exist (AGENTS: web dist ships with the source)",
        );
        // `script-src 'self'` has no hash/nonce for the PAGE's own tags, so
        // every script/style there must be an external same-origin file.
        for tag in ["<script", "<style"] {
            assert!(
                !html.contains(&format!("{tag}>")),
                "dist/index.html carries an inline {tag}> the CSP would block"
            );
        }
        let mut scripts = Vec::new();
        bundle_scripts(&dist.join("assets"), &mut scripts);
        assert!(
            !scripts.is_empty(),
            "dist/assets carries no bundle to audit"
        );
        for script in scripts {
            let source = std::fs::read_to_string(&script).expect("bundle reads");
            // The packaged API probes the global object for compat paths
            // (`plugin-dialog`'s opener fallback), which is harmless — the
            // regression this guards is OUR code reading `__TAURI__.core`,
            // i.e. a bundle rebuilt from the pre-explicit-imports sources.
            for read in ["__TAURI__.core", "__TAURI__?.core"] {
                assert!(
                    !source.contains(read),
                    "{} still reads the global Tauri object ({read}) — rebuild web/dist",
                    script.display()
                );
            }
        }
    }

    /// The demo app's sibling webview face carries the same posture the
    /// installer adopted: a pinned CSP, no inline script/style blocks, and no
    /// global Tauri object. Its script is BUILD OUTPUT now (esbuild bundles
    /// demo-app/web/src into demo-app/ui/app.js; the HTML and stylesheet stay
    /// authored files), so both halves are checked: the source must ride the
    /// packaged imports, and the committed bundle must be the build of that
    /// source — a stale app.js still reading the global object would ship a
    /// page with no command surface nothing injects.
    #[test]
    fn the_demo_app_carries_the_pinned_posture_too() {
        let conf: serde_json::Value =
            serde_json::from_str(DEMO_CONF).expect("demo tauri.conf.json parses");
        let csp = conf
            .get("app")
            .and_then(|a| a.get("security"))
            .and_then(|s| s.get("csp"))
            .and_then(serde_json::Value::as_str)
            .expect("demo-app must carry a CSP string, not null");
        for directive in [
            "default-src 'self'",
            "script-src 'self'",
            "style-src 'self'",
            "img-src 'self' data:",
            "connect-src 'self' ipc: http://ipc.localhost",
            "object-src 'none'",
            "base-uri 'self'",
            "form-action 'none'",
        ] {
            assert!(csp.contains(directive), "demo-app CSP lacks {directive}");
        }
        assert!(
            !csp.contains("unsafe-inline"),
            "demo-app CSP must not re-open inline script/style: {csp}"
        );
        // The global object is off, exactly like the installer's.
        assert_eq!(
            conf["app"]["withGlobalTauri"],
            serde_json::Value::Bool(false),
            "demo-app must run with app.withGlobalTauri false"
        );
        // With no inline allowance, the HTML must not carry inline blocks.
        for (label, needle) in [("<script>", "<script>"), ("<style>", "<style>")] {
            assert!(
                !DEMO_HTML.contains(needle),
                "demo-app/index.html has an inline {label} block; extract it to a file"
            );
        }
        assert!(
            DEMO_HTML.contains("type=\"module\" src=\"app.js\""),
            "demo-app/index.html must load the external bundle as a module script"
        );
        assert!(
            DEMO_HTML.contains("href=\"app.css\""),
            "demo-app/index.html must load its stylesheet as an external file"
        );
        // The source that bundle is built from rides the packaged API only.
        assert!(
            DEMO_JS_SRC.contains("@tauri-apps/api/core"),
            "demo-app/web/src/app.js must import invoke from @tauri-apps/api/core"
        );
        for read in ["__TAURI__.core", "__TAURI__?.core"] {
            assert!(
                !DEMO_JS_SRC.contains(read),
                "demo-app/web/src/app.js still reads the global Tauri object ({read})"
            );
        }
        // …and the committed bundle is the one built from that source: the
        // packaged ipc transport is inlined, the global object is not.
        assert!(
            DEMO_BUNDLE.contains("__TAURI_INTERNALS__"),
            "demo-app/ui/app.js lacks the packaged ipc transport — run pnpm --dir web build"
        );
        assert!(
            !DEMO_BUNDLE.contains("__TAURI__"),
            "demo-app/ui/app.js still carries the global Tauri object — rebuild it"
        );
        // A distinctive line of the source must survive into the bundle, or
        // the committed artifact was built from something else entirely.
        assert!(
            DEMO_BUNDLE.contains("chip.classList.remove(\"placeholder\")"),
            "demo-app/ui/app.js does not match demo-app/web/src/app.js — rebuild it"
        );
        // The build must stay reproducible from the tree it ships in.
        assert!(
            DEMO_PACKAGE_JSON.contains("\"@tauri-apps/api\""),
            "demo-app/web/package.json must declare @tauri-apps/api"
        );
        assert!(
            DEMO_PACKAGE_JSON.contains("\"build\": \"esbuild"),
            "demo-app/web/package.json must carry the esbuild build script"
        );
        let command = conf["build"]["beforeBuildCommand"]
            .as_str()
            .unwrap_or_default();
        assert!(
            command.contains("pnpm --dir web build"),
            "demo tauri.conf.json must rebuild its face before a tauri build: {command:?}"
        );
    }
}
