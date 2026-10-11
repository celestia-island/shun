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

use eframe::egui::{
    self, Align, Color32, Context, CornerRadius, Layout, RichText, Sense, Stroke, TextureHandle,
    Vec2,
};

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

    // The splash carries the wizard's own chrome: frameless, the
    // custom title bar below, the theme's visuals — and it opens
    // CENTERED on the primary monitor (a small utility window flying
    // into the top-left corner reads as a glitch, not a placeholder).
    let (w, h) = (400.0f32, 252.0f32);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_inner_size([w, h])
            .with_min_inner_size([w, h])
            .with_resizable(false)
            .with_position(center_on_primary(w, h))
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
            apply_splash_visuals(&cc.egui_ctx);
            let logo =
                fallback::load_logo(&cc.egui_ctx, crate::LOGO_KIND.trim(), crate::LOGO_BYTES);
            let icons = fallback::CaptionIcons::load(&cc.egui_ctx);
            let product = config.product.clone();
            Ok(Box::new(SplashApp::new(
                receiver, slot, product, logo, icons,
            )))
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

/// The splash window's center on the primary monitor, in egui
/// POINTS: Windows reports physical pixels, so the system DPI folds
/// back out. A pre-ctx builder position is the only centering hook —
/// egui has no runtime Center command.
#[cfg(windows)]
fn center_on_primary(w: f32, h: f32) -> egui::Pos2 {
    use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    // SAFETY: plain metrics reads.
    let scale = unsafe { GetDpiForSystem() } as f32 / 96.0;
    let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) } as f32 / scale;
    let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) } as f32 / scale;
    egui::pos2((screen_w - w) / 2.0, (screen_h - h) / 2.0)
}

#[cfg(not(windows))]
fn center_on_primary(_w: f32, _h: f32) -> egui::Pos2 {
    egui::Pos2::default()
}

/// The wizard's visual foundation (dark token set, panel fills), so
/// the splash's undecorated frame matches the face it precedes.
fn apply_splash_visuals(ctx: &Context) {
    let theme = Theme::dark(None);
    ctx.set_theme(egui::Theme::Dark);
    ctx.options_mut(|o| o.theme_preference = egui::ThemePreference::Dark);
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = theme.background;
    visuals.window_fill = theme.background;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0f32, theme.border);
    ctx.set_visuals(visuals);
}

/// The splash face: system-locale message, percent text + bar, abort.
struct SplashApp {
    receiver: Receiver<InitMsg>,
    outcome_slot: std::sync::Arc<std::sync::Mutex<Option<InitOutcome>>>,
    percent: u8,
    message: String,
    abort: String,
    titlebar_installer: String,
    product: shun::config::ProductIdentity,
    logo: Option<TextureHandle>,
    icons: fallback::CaptionIcons,
    theme: Theme,
    /// DWM rounding applied once — the first frame carries the handle.
    dwm_rounded: bool,
}

/// HkTitleBar geometry, shared with the wizard's own bar.
const TITLEBAR_H: f32 = 32.0;
const CAPTION_W: f32 = 46.0;

impl SplashApp {
    fn new(
        receiver: Receiver<InitMsg>,
        outcome_slot: std::sync::Arc<std::sync::Mutex<Option<InitOutcome>>>,
        product: shun::config::ProductIdentity,
        logo: Option<TextureHandle>,
        icons: fallback::CaptionIcons,
    ) -> Self {
        let (message, abort) = crate::fallback::splash_strings();
        let titlebar_installer = crate::fallback::titlebar_installer_string();
        Self {
            receiver,
            outcome_slot,
            percent: 0,
            message,
            abort,
            titlebar_installer,
            product,
            logo,
            icons,
            theme: Theme::dark(None),
            dwm_rounded: false,
        }
    }

    /// The wizard's HkTitleBar, compact: logo + product/title/version
    /// left, the close plate right, the band drags. No minimize/theme
    /// seats — a splash is a modal moment, and the close plate IS the
    /// abort.
    fn title_bar(&self, ui: &mut egui::Ui) {
        let theme = &self.theme;
        let (strip, drag) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), TITLEBAR_H), Sense::click());
        let painter = ui.painter_at(strip);
        painter.rect_filled(strip, CornerRadius::ZERO, theme.background);
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(strip)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| {
                ui.add_space(12.0);
                if let Some(logo) = &self.logo {
                    ui.add(egui::Image::from_texture(logo).fit_to_exact_size(Vec2::splat(16.0)));
                }
                ui.add_space(8.0);
                ui.label(
                    RichText::new(format!("{} {}", self.product.name, self.titlebar_installer))
                        .color(theme.text)
                        .size(11.0)
                        .strong(),
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new(format!("v{}", self.product.version))
                        .color(theme.text_secondary.gamma_multiply(0.7))
                        .size(10.0),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::new(CAPTION_W, TITLEBAR_H), Sense::click());
                    let painter = ui.painter_at(rect);
                    let hover_t = ui.ctx().animate_bool_with_time(
                        response.id.with("hover"),
                        response.hovered(),
                        0.12,
                    );
                    if hover_t > 0.0 || response.is_pointer_button_down_on() {
                        let fill = if response.is_pointer_button_down_on() {
                            Color32::from_rgb(0xf1, 0x70, 0x7a)
                        } else {
                            crate::fallback::mix(
                                theme.background,
                                Color32::from_rgb(0xe8, 0x11, 0x23),
                                hover_t,
                            )
                        };
                        // Round the window's top-right corner on hover,
                        // exactly like the wizard's close plate.
                        painter.rect_filled(
                            rect,
                            CornerRadius {
                                nw: 0,
                                ne: 8,
                                sw: 0,
                                se: 0,
                            },
                            fill,
                        );
                    }
                    let glyph = if response.hovered() || response.is_pointer_button_down_on() {
                        Color32::WHITE
                    } else {
                        theme.text_secondary
                    };
                    painter.image(
                        self.icons.x.id(),
                        egui::Rect::from_center_size(rect.center(), Vec2::splat(12.0)),
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        glyph,
                    );
                    if response.clicked() {
                        std::process::exit(0);
                    }
                });
            },
        );
        if drag.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
    }
}

impl eframe::App for SplashApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Win11 rounded corners for the frameless window — the system
        // drop shadow rides on the same preference, the wizard's own
        // first-frame ritual.
        if !self.dwm_rounded {
            self.dwm_rounded = true;
            #[cfg(windows)]
            fallback::apply_dwm_rounding(frame);
            // Keep `frame` live on the other hosts: the rounding call
            // is Windows-only, but the trait signature is uniform.
            #[cfg(not(windows))]
            let _ = frame;
        }
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
                self.title_bar(ui);
                ui.with_layout(Layout::top_down(Align::Center), |ui| {
                    ui.add_space(38.0);
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
