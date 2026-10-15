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
