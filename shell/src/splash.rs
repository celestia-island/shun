//! The startup splash — a tiny egui placeholder window that exists
//! from the FIRST moment of a GUI run until the real face's
//! initialization completes.
//!
//! A large installer (the complete flavor embeds hundreds of MB)
//! spends its first seconds-to-minute decoding the payload and
//! bootstrapping the WebView2 runtime BEFORE any window can open —
//! the process looked hung. The splash window opens immediately,
//! reports the initialization as a percentage (message + percent
//! text + bar, localized through the system locale), and offers an
//! abort button; the decode and bootstrap run on a worker thread
//! feeding it progress. When the work settles, the splash closes and
//! the resolved face's window takes over — the wizard/tauri faces
//! receive the already-decoded payload.
//!
//! The splash is deliberately egui: it renders with zero runtime
//! dependencies, which is exactly the state the machine may be in.

use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui::{self, Align, CornerRadius, Layout, RichText, Sense, Stroke, Vec2};

use crate::fallback::{self, Theme};
use shun::config::ShunConfig;
use shun::payload::ArchivePayload;

/// What the init worker accomplished, handed back to `main` when the
/// splash closes.
pub(crate) struct InitOutcome {
    /// The decoded archive — the single decode this process pays for
    /// (the faces used to each re-decode the embedded blob).
    pub payload: ArchivePayload,
    /// The staged fixed-version runtime's directory, when the worker
    /// staged one (the tauri face then only points the loader at it —
    /// the env var itself is set on the main thread after the splash).
    pub fixed_runtime: Option<std::path::PathBuf>,
}

/// Worker → splash messages.
enum InitMsg {
    Percent(u8),
    Done(InitOutcome),
}

/// Runs the splash window over the init worker. `Err(())` = the user
/// aborted (button or window close) — the caller exits silently;
/// nothing has been installed at this stage.
pub(crate) fn run_splash(
    config: &ShunConfig,
    archive: &'static [u8],
    webview_candidate: bool,
    tauri_possible: bool,
    uninstall: bool,
    debug_screenshot: Option<(std::path::PathBuf, u64)>,
) -> Result<InitOutcome, ()> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(init_worker(
        sender,
        archive,
        config.clone(),
        webview_candidate,
        tauri_possible,
        uninstall,
    ));
    // SHUN_DEBUG_SPLASH=1 + --screenshot: the face-level capture never
    // runs (the frozen splash never hands over), so the splash
    // schedules its own by-title capture — the debug lane for
    // capturing this very window.
    #[cfg(windows)]
    if std::env::var("SHUN_DEBUG_SPLASH").is_ok_and(|v| v == "1") {
        if let Some((path, delay)) = debug_screenshot {
            let title = crate::os_window_title(config, false);
            crate::screenshot::schedule_by_title(title, path, delay);
        }
    }
    #[cfg(not(windows))]
    let _ = debug_screenshot;

    let outcome_slot: std::sync::Arc<std::sync::Mutex<Option<InitOutcome>>> =
        std::sync::Arc::new(std::sync::Mutex::new(None));
    let title = crate::os_window_title(config, false);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([400.0, 220.0])
            .with_min_inner_size([400.0, 220.0])
            .with_resizable(false)
            .with_title(title),
        ..Default::default()
    };
    let slot = outcome_slot.clone();
    let result = eframe::run_native(
        "shun-splash",
        options,
        Box::new(move |cc| {
            // The full calibrated font stack (face reads + probe
            // layouts — a couple hundred ms at most) so the splash's
            // own mixed-script text is baseline-aligned too.
            fallback::install_system_fonts(&cc.egui_ctx);
            Ok(Box::new(SplashApp::new(receiver, slot)))
        }),
    );
    if result.is_err() {
        crate::diag!("shun: the splash failed to start ({result:?})");
        return Err(());
    }
    // The app stored the outcome on Done; None past here means the
    // user closed the window before the init settled.
    outcome_slot
        .lock()
        .ok()
        .and_then(|mut slot| slot.take())
        .ok_or(())
}

/// The init work a GUI run performs under the splash: decode the
/// embedded archive (the dominant cost — hundreds of MB of zstd),
/// then the runtime bootstraps the face ladder depends on.
/// Percentages: 0-5 opening, 5-75 decode, 75-90 the Evergreen
/// carried-install path, 75-90 fixed-runtime staging, 100 done.
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
                // A corrupt payload is unrecoverable; the splash
                // cannot explain itself without the wizard — fail the
                // process like the old inline decode did.
                crate::diag!("shun: embedded payload decode failed ({err})");
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

/// The splash face: system-locale message, percent text + bar, abort.
struct SplashApp {
    receiver: Receiver<InitMsg>,
    outcome_slot: std::sync::Arc<std::sync::Mutex<Option<InitOutcome>>>,
    percent: u8,
    message: String,
    abort: String,
    theme: Theme,
}

impl SplashApp {
    fn new(
        receiver: Receiver<InitMsg>,
        outcome_slot: std::sync::Arc<std::sync::Mutex<Option<InitOutcome>>>,
    ) -> Self {
        let (message, abort) = crate::fallback::splash_strings();
        Self {
            receiver,
            outcome_slot,
            percent: 0,
            message,
            abort,
            theme: Theme::dark(None),
        }
    }
}

impl eframe::App for SplashApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Drain the worker feed; keep polling while the init runs.
        let mut done = None;
        while let Ok(msg) = self.receiver.try_recv() {
            match msg {
                InitMsg::Percent(value) => self.percent = self.percent.max(value),
                InitMsg::Done(outcome) => done = Some(outcome),
            }
        }
        // Debug freeze (SHUN_DEBUG_SPLASH=1): park at a sample percent
        // so --screenshot can capture the splash itself.
        let frozen = std::env::var("SHUN_DEBUG_SPLASH").is_ok_and(|v| v == "1");
        if frozen {
            self.percent = 37;
            done = None;
        }

        let theme = &self.theme;
        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(theme.background))
            .show(ctx, |ui| {
                ui.with_layout(Layout::top_down(Align::Center), |ui| {
                    ui.add_space(52.0);
                    ui.label(
                        RichText::new(self.message.as_str())
                            .strong()
                            .size(16.0)
                            .color(theme.text),
                    );
                    ui.add_space(20.0);
                    // The percent text + bar pair (the wizard's bar
                    // recipe: 6pt rounded track, primary fill).
                    ui.label(
                        RichText::new(format!("{}%", self.percent))
                            .size(12.0)
                            .color(theme.text_tertiary),
                    );
                    ui.add_space(8.0);
                    let (bar, _) = ui.allocate_exact_size(Vec2::new(300.0, 6.0), Sense::hover());
                    let painter = ui.painter_at(bar);
                    painter.rect_filled(bar, CornerRadius::same(3), theme.surface);
                    let fill = (bar.width() * f32::from(self.percent) / 100.0).max(4.0);
                    painter.rect_filled(
                        egui::Rect::from_min_size(bar.min, Vec2::new(fill, 6.0)),
                        CornerRadius::same(3),
                        theme.primary,
                    );
                });
                // The abort seat: bottom of the window, the wizard's
                // secondary button recipe.
                ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
                    ui.add_space(20.0);
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(self.abort.as_str())
                                    .size(13.0)
                                    .color(theme.text_secondary),
                            )
                            .fill(theme.surface)
                            .stroke(Stroke::new(1.0f32, theme.border))
                            .corner_radius(CornerRadius::same(6))
                            .min_size(Vec2::new(120.0, 28.0)),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        // Nothing is installed at this stage — a
                        // clean exit is a safe abort.
                        std::process::exit(0);
                    }
                });
            });

        if ctx.input(|i| i.viewport().close_requested()) {
            std::process::exit(0);
        }
        if let Some(outcome) = done {
            *self.outcome_slot.lock().expect("the splash owns the slot") = Some(outcome);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(80));
    }
}
