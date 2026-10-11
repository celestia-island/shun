//! The startup splash's backing work — the init a GUI run performs
//! while the splash phase of the wizard's window is on screen.
//!
//! A large installer (the complete flavor embeds hundreds of MB)
//! spends its first seconds-to-minute decoding the payload and
//! bootstrapping the WebView2 runtime BEFORE any window can open —
//! the process looked hung. The wizard's egui app now opens in a
//! SPLASH PHASE (same window, same event loop: a second winit loop in
//! one process dies on the handover — verified in the field) and
//! renders a localized message, the percent, and an abort seat while
//! the worker spawned here decodes the archive (streaming, so the
//! percent is real), runs the carried Evergreen bootstrap, and stages
//! the fixed-version runtime when the webview face is reachable. The
//! outcome is delivered over the channel; the app phase-transitions
//! on receipt — or, when the face ladder re-resolves to the webview
//! face, hands the outcome back to `main` for the tauri loop.

use std::sync::mpsc::{self, Receiver, Sender};

use shun::config::ShunConfig;
use shun::payload::ArchivePayload;

/// What the init worker accomplished, handed to the splash phase's
/// consumer.
pub(crate) struct InitOutcome {
    /// The decoded archive — the single decode this process pays for.
    pub payload: ArchivePayload,
    /// The staged fixed-version runtime's directory, when the worker
    /// staged one (the tauri face then only points the loader at it —
    /// the env var itself is set on the main thread after the loop).
    pub fixed_runtime: Option<std::path::PathBuf>,
}

/// The worker feed's receiving end (named so state can hold one).
pub(crate) type InitMsgReceiver = Receiver<InitMsg>;

/// Worker → splash messages.
pub(crate) enum InitMsg {
    Percent(u8),
    Done(InitOutcome),
}

/// Spawns the init worker and returns its feed.
///
/// Percentages: 0-5 opening, 5-75 the streaming decode, 75-90 the
/// Evergreen carried-install / fixed-staging paths, 100 done.
pub(crate) fn spawn_init(
    archive: &'static [u8],
    config: ShunConfig,
    webview_candidate: bool,
    tauri_possible: bool,
    uninstall: bool,
) -> Receiver<InitMsg> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(init_worker(
        sender,
        archive,
        config,
        webview_candidate,
        tauri_possible,
        uninstall,
    ));
    receiver
}

/// The init work a GUI run performs under the splash phase: decode the
/// embedded archive (the dominant cost — hundreds of MB of zstd), then
/// the runtime bootstraps the face ladder depends on.
fn init_worker(
    sender: Sender<InitMsg>,
    archive: &'static [u8],
    config: ShunConfig,
    webview_candidate: bool,
    tauri_possible: bool,
    uninstall: bool,
) -> impl FnOnce() {
    move || {
        let percent = |value: u8| {
            let _ = sender.send(InitMsg::Percent(value));
        };
        percent(3);
        let payload = match ArchivePayload::from_bytes_with_progress(archive, &mut |ratio| {
            percent(5 + (ratio * 70.0) as u8);
        }) {
            Ok(payload) => payload,
            Err(err) => {
                // A corrupt payload is unrecoverable and the splash
                // phase cannot explain itself: the SYSTEM message box
                // is the only surface that always renders.
                let detail = format!("{}\n\n{err}", crate::fallback::fatal_init_string());
                crate::native_fatal_box(&config.product.name, &detail);
                std::process::exit(1);
            }
        };
        percent(76);
        // The Evergreen carried-install bootstrap (same gates as the
        // old pre-ladder call): on a runtime-less machine carrying the
        // offline installer this silently upgrades the machine —
        // minutes under the splash, exactly the wait that needs one.
        if webview_candidate {
            crate::bootstrap_evergreen_webview2_with_progress(&config, &payload, &mut |p| {
                percent(76 + p / 8);
            });
        }
        // The fixed-version staging runs only when the webview face is
        // actually reachable — the manifest offers it AND a runtime is
        // usable now (the Evergreen attempt above may have just fixed
        // the machine). A runtime-less machine takes the egui face
        // regardless; staging hundreds of MB for it would be waste.
        let fixed_runtime = if tauri_possible && !uninstall && crate::webview2_loader_ready() {
            crate::stage_fixed_runtime(&config, &payload, &mut |p| percent(76 + p / 8))
        } else {
            None
        };
        percent(100);
        let _ = sender.send(InitMsg::Done(InitOutcome {
            payload,
            fixed_runtime,
        }));
    }
}

/// A window's center on the primary monitor, in egui POINTS: Windows
/// reports physical pixels, so the system DPI folds back out. A
/// builder-time position is the only centering hook — egui has no
/// runtime Center command.
#[cfg(windows)]
pub(crate) fn center_on_primary(w: f32, h: f32) -> egui::Pos2 {
    use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    // SAFETY: plain metrics reads.
    let scale = unsafe { GetDpiForSystem() } as f32 / 96.0;
    let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) } as f32 / scale;
    let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) } as f32 / scale;
    egui::pos2((screen_w - w) / 2.0, (screen_h - h) / 2.0)
}

#[cfg(not(windows))]
pub(crate) fn center_on_primary(_w: f32, _h: f32) -> egui::Pos2 {
    egui::Pos2::default()
}
