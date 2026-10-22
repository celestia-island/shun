//! The silent lane's passive progress window (`[shell]
//! silent-progress = true`).
//!
//! `--silent` promises no INTERACTION, and for fleet tooling that has
//! always meant no UI at all. But the lane's other caller is an
//! unattended updater: the old app spawns the new installer with
//! `--silent --dir=…` and exits, and on a slow machine (a Defender
//! first-run scan alone can outlast a minute) the update then reads as
//! "nothing happened" — the player launches the old build from a
//! shortcut and pins the directory the installer is swapping, the exact
//! overwrite-failure loop the CWD pinning and lock retries fight.
//!
//! A product that opts in gets the honest middle: the install stays
//! automatic — no pages, no clicks — while a small always-on-top window
//! mirrors what stdout already says (the running step, the percent when
//! the step has one) and folds on completion. The old build needs to
//! know nothing: ANY spawn of a `silent-progress` installer gets the
//! window, because the installer renders it itself. Stdout is
//! untouched, so redirected fleet runs log exactly as before; a machine
//! without a desktop session (the window cannot open there) silently
//! stays console-only.
//!
//! The window rides the SAME design surface the wizard faces ship: the
//! WEBVIEW lane renders it first — a product auto-updating through this
//! installer by definition has WebView2 (the app being replaced needed
//! it) — running the same embedded hikari bundle in its passive act,
//! with the shell's custom caption bar, theme token stack and
//! localized copy. The egui face is the FLOOR: when the webview lane
//! cannot come up (runtime missing, face disabled, forced fallback
//! flags), the window re-renders through the egui face's own chrome —
//! its frameless custom title bar, its `shell.theme`/OS light-dark
//! token resolution, its progress bar — so even the degraded path
//! never falls back to a native-titled window. Only when no desktop
//! session exists at all does the run stay console-only.

use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align, Context, CornerRadius, Layout, RichText, Sense, Stroke, Vec2, ViewportCommand,
    WindowLevel,
};
use tauri::Emitter;

use shun::config::ShunConfig;
use shun::payload::ArchivePayload;

use crate::fallback::{self, SilentStrings};

/// What the tapped headless lane reports to the window. Step-level
/// only: the log pane's per-file stream is console territory.
pub(crate) enum PassiveNote {
    /// A step label (and its percent, when the step has one).
    Step { step: String, percent: Option<u8> },
    /// A headline the headless lane prints between steps (runtime
    /// acquisition, dir-less upgrade resolution, …).
    Note(String),
}

/// Worker → window messages, shared by both lanes: the egui window
/// drains them from an mpsc receiver, the webview forwarder re-emits
/// them as `passive-progress` events.
pub(crate) enum LaneEvent {
    Note(PassiveNote),
    /// The headless lane returned: success, plus its error text.
    Done(bool, Option<String>),
}

/// The `passive-progress` event payload (serde `kind`-tagged; see
/// PassiveView.tsx's twin interface).
#[derive(Clone, serde::Serialize)]
#[serde(tag = "kind")]
pub(crate) enum PassiveEvent {
    #[serde(rename = "step")]
    Step { step: String, percent: Option<u8> },
    #[serde(rename = "note")]
    Note { text: String },
    #[serde(rename = "done")]
    Done { ok: bool, failure: Option<String> },
}

impl From<&LaneEvent> for PassiveEvent {
    fn from(event: &LaneEvent) -> Self {
        match event {
            LaneEvent::Note(PassiveNote::Step { step, percent }) => PassiveEvent::Step {
                step: step.clone(),
                percent: *percent,
            },
            LaneEvent::Note(PassiveNote::Note(text)) => PassiveEvent::Note { text: text.clone() },
            LaneEvent::Done(ok, failure) => PassiveEvent::Done {
                ok: *ok,
                failure: failure.clone(),
            },
        }
    }
}

/// The install worker of a webview-lane run, parked so the loop's exit
/// (window closed) still joins the install before the process reports.
static WEBVIEW_WORKER: std::sync::Mutex<Option<std::thread::JoinHandle<Result<(), String>>>> =
    std::sync::Mutex::new(None);

/// The lane's latest coalesced event. The webview registers its
/// `passive-progress` listener only after the page has booted — a fast
/// run may already have emitted past it — so the page replays this
/// snapshot on mount (tauri events are not buffered).
static LAST_EVENT: std::sync::Mutex<Option<PassiveEvent>> = std::sync::Mutex::new(None);

/// The mount-time replay of the lane's state (see [`LAST_EVENT`]); the
/// passive page invokes it right after arming its listener and applies
/// it only when no live event has arrived first.
pub(crate) fn last_event() -> Option<PassiveEvent> {
    LAST_EVENT.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Whether the webview lane's install worker is attached — an Err out
/// of the tauri loop WITH a worker attached means the window ran and
/// the loop died abnormally: the install owns this run now, and the
/// caller must join it instead of spawning a second one over the same
/// target directory.
pub(crate) fn worker_attached() -> bool {
    WEBVIEW_WORKER
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .is_some()
}

/// Spawn the headless worker for the WEBVIEW lane and start forwarding
/// its feed to the window as `passive-progress` events. The forwarder
/// dies at the worker's Done message; events into an already-closed
/// window are fire-and-forget and simply drop.
pub(crate) fn attach_worker(
    app: tauri::AppHandle,
    cli: &crate::Cli,
    config: &ShunConfig,
    payload: &ArchivePayload,
) {
    let (sender, receiver) = mpsc::channel::<LaneEvent>();
    let worker = spawn_worker(sender, cli.clone(), config.clone(), payload.clone());
    WEBVIEW_WORKER
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .replace(worker);
    std::thread::spawn(move || {
        while let Ok(event) = receiver.recv() {
            let done = matches!(event, LaneEvent::Done(..));
            let payload = PassiveEvent::from(&event);
            LAST_EVENT
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .replace(payload.clone());
            let _ = app.emit("passive-progress", payload);
            if done {
                break;
            }
        }
    });
}

/// Join the webview lane's worker after its window went down. `Ok` when
/// no worker was ever attached (the window failed before setup reached
/// it — the caller then re-routes to the egui lane).
pub(crate) fn join_worker() -> Result<(), String> {
    let worker = WEBVIEW_WORKER
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    match worker {
        Some(worker) => worker
            .join()
            .map_err(|_| "the silent worker panicked".to_string())?,
        None => Ok(()),
    }
}

/// Whether the passive window should ride the WEBVIEW lane: the loader
/// must answer for a usable runtime, the product's face list must still
/// allow the webview face, and none of the forced-egui flags may be set
/// (the explicit `--no-webview` family keeps its promise even here).
/// Pure so the ladder is unit-testable without owning a machine's
/// runtime state.
pub(crate) fn webview_lane_allowed(
    cli: &crate::Cli,
    faces: &[shun::config::UiFace],
    loader_ready: bool,
) -> bool {
    !cli.no_webview
        && !cli.fallback
        && !cli.egui
        && loader_ready
        && faces.contains(&shun::config::UiFace::Webview)
}

/// Where the run stands.
enum Phase {
    Running,
    Done,
    Failed,
}

/// The window's complete copy + live state.
struct PassiveApp {
    strings: SilentStrings,
    heading: String,
    receiver: Receiver<LaneEvent>,
    phase: Phase,
    /// This run drives the uninstall flow (picks the done/failed copy —
    /// the heading had `%PRODUCT%` spliced, so comparing it against the
    /// raw template can never match).
    uninstall: bool,
    /// The running step's label and percent (None = indeterminate).
    step: String,
    percent: Option<u8>,
    /// The worker's failure text on the Failed phase.
    failure: Option<String>,
    /// When the Done phase should close the window (a short linger so
    /// the completion line is readable).
    close_at: Option<Instant>,
    /// The egui face's resolved theme: the manifest's `shell.theme`
    /// layers over the OS light/dark side (`fallback::resolve_theme` —
    /// the same resolution the wizard's egui face ships).
    theme: fallback::Theme,
    /// The caption bar's lucide glyphs + the product logo.
    caption_icons: fallback::CaptionIcons,
    logo: Option<egui::TextureHandle>,
    /// The caption's left cluster: product name + role word + version.
    product: String,
    titlebar_word: String,
    version: String,
    /// One-shot guard for the DWM corner rounding (the frameless
    /// window's rounded corners and the system shadow that rides them).
    dwm_rounded: bool,
}

const DONE_LINGER: Duration = Duration::from_millis(1500);

/// The passive window's inner size (points): the caption band over the
/// content column.
const PASSIVE_W: f32 = 420.0;
const PASSIVE_H: f32 = 240.0;

impl PassiveApp {
    fn fold(&mut self, msg: LaneEvent) {
        match msg {
            LaneEvent::Note(PassiveNote::Step { step, percent }) => {
                self.step = step;
                self.percent = percent;
            }
            LaneEvent::Note(PassiveNote::Note(note)) => {
                self.step = note;
                self.percent = None;
            }
            LaneEvent::Done(true, _) => {
                self.phase = Phase::Done;
                self.close_at = Some(Instant::now() + DONE_LINGER);
            }
            LaneEvent::Done(false, failure) => {
                self.phase = Phase::Failed;
                self.failure = failure;
            }
        }
    }

    /// The done/failed lines pick the uninstall copy on an uninstall run.
    fn is_uninstall_run(&self) -> bool {
        self.uninstall
    }

    /// The hikari caption bar, the egui face's own chrome drawn small:
    /// 12pt lead-in, 16pt logo, the product + role word at 11pt/600,
    /// the version subtitle at 10pt/70%, and one CLOSE plate flush to
    /// the right edge (the passive window minimizes nowhere — it is a
    /// mirror). The band outside the plate drags the window.
    fn caption_bar(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        let (strip, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), fallback::TITLEBAR_H),
            Sense::hover(),
        );
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
                    RichText::new(format!("{} {}", self.product, self.titlebar_word))
                        .color(theme.text)
                        .size(11.0)
                        .strong(),
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new(&self.version)
                        .color(theme.text_secondary.gamma_multiply(0.7))
                        .size(10.0),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    // Flush plates: zero the item spacing (HkTitleBar's
                    // geometry — a gap here would read as a stray seam).
                    ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                    let (rect, response) = ui.allocate_exact_size(
                        Vec2::new(fallback::CAPTION_W, fallback::TITLEBAR_H),
                        Sense::click(),
                    );
                    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
                    let painter = ui.painter_at(rect);
                    let hovered = response.hovered();
                    let active = response.is_pointer_button_down_on();
                    let hover_t =
                        ui.ctx()
                            .animate_bool_with_time(response.id.with("hover"), hovered, 0.12);
                    if hover_t > 0.0 || active {
                        // The close plate's red wash, pressed stepping to
                        // the lighter #f1707a, rounding the window's
                        // top-right corner — the SCSS recipe.
                        let fill = if active {
                            egui::Color32::from_rgb(0xf1, 0x70, 0x7a)
                        } else {
                            fallback::mix(
                                theme.background,
                                egui::Color32::from_rgb(0xe8, 0x11, 0x23),
                                hover_t,
                            )
                        };
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
                    let icon_color = if hovered || active {
                        egui::Color32::WHITE
                    } else {
                        theme.text_secondary
                    };
                    painter.image(
                        self.caption_icons.x.id(),
                        egui::Rect::from_center_size(rect.center(), egui::vec2(14.0, 14.0)),
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        icon_color,
                    );
                    if response.clicked() {
                        ui.ctx().send_viewport_cmd(ViewportCommand::Close);
                    }
                });
            },
        );
        // The drag zone stops short of the caption plate (egui
        // hit-tests later widgets first — a full-band drag rect on top
        // would eat the close click; the wizard's bar does the same).
        let mut drag_rect = strip;
        let cluster_left = drag_rect.right() - fallback::CAPTION_W;
        drag_rect.set_right(cluster_left.max(drag_rect.left()));
        let drag = ui.interact(drag_rect, ui.id().with("titlebar-drag"), Sense::drag());
        if drag.drag_started() {
            ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
        }
    }

    /// The wizard's running-pane bar, drawn small: a 6pt rounded track
    /// with a primary fill; the indeterminate phase rides the sine
    /// sweep the web bar's loading state animates. Only the Running
    /// phase draws it (a settled run's verdict is the line, not a bar).
    fn progress_bar(&mut self, ui: &mut egui::Ui, width: f32) {
        let theme = self.theme;
        let fraction = match self.percent {
            Some(percent) => f32::from(percent) / 100.0,
            None => {
                let t = ui.input(|i| i.time) as f32;
                let sweep = (t * 0.9).sin() * 0.5 + 0.5;
                ui.ctx().request_repaint();
                sweep.clamp(0.02, 0.98)
            }
        };
        let (track, _) = ui.allocate_exact_size(Vec2::new(width, 6.0), Sense::hover());
        let painter = ui.painter_at(track);
        painter.rect_filled(
            track,
            CornerRadius::same(3),
            fallback::mix(theme.background, theme.text, 0.08),
        );
        let fill_w = (track.width() * fraction.clamp(0.0, 1.0)).max(6.0);
        painter.rect_filled(
            egui::Rect::from_min_size(track.min, Vec2::new(fill_w, 6.0)),
            CornerRadius::same(3),
            theme.primary,
        );
    }
}

impl eframe::App for PassiveApp {
    fn update(&mut self, ctx: &Context, frame: &mut eframe::Frame) {
        // Frameless windows lose BOTH the rounded corners and the shadow
        // until DWMWCP_ROUND lands — the same one-shot the wizard's egui
        // face applies to its own window (our OWN handle, not a title
        // lookup). The same one-shot installs the winit-0.30 cross-DPI
        // drag guard (see dpi_fix.rs).
        if !self.dwm_rounded {
            self.dwm_rounded = true;
            #[cfg(windows)]
            fallback::apply_dwm_rounding(frame);
            crate::dpi_fix::install(frame);
        }
        while let Ok(msg) = self.receiver.try_recv() {
            self.fold(msg);
        }

        let theme = self.theme;
        let (line, line_color) = match &self.phase {
            Phase::Running => (self.step.clone(), theme.text_secondary),
            Phase::Done => (
                if self.is_uninstall_run() {
                    self.strings.done_uninstall.clone()
                } else {
                    self.strings.done.clone()
                },
                theme.success,
            ),
            Phase::Failed => (
                if self.is_uninstall_run() {
                    self.strings.failed_uninstall.clone()
                } else {
                    self.strings.failed.clone()
                },
                theme.error,
            ),
        };

        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(theme.background))
            .show(ctx, |ui| {
                self.caption_bar(ui);
                ui.with_layout(Layout::top_down(Align::Center), |ui| {
                    ui.add_space(14.0);
                    ui.label(
                        RichText::new(&self.heading)
                            .strong()
                            .size(16.0)
                            .color(theme.text),
                    );
                    ui.add_space(14.0);
                    if matches!(self.phase, Phase::Running) {
                        self.progress_bar(ui, 300.0);
                        ui.add_space(8.0);
                        match self.percent {
                            Some(percent) => ui.label(
                                RichText::new(format!("{line} · {percent}%"))
                                    .size(12.5)
                                    .color(line_color),
                            ),
                            None => ui.label(RichText::new(line).size(12.5).color(line_color)),
                        };
                    } else {
                        // A settled run drops its bar and percent — the
                        // line is the verdict now, and a full primary bar
                        // under "install failed" would contradict it.
                        ui.add_space(6.0);
                        ui.label(RichText::new(line).size(12.5).color(line_color));
                    }
                    if let Phase::Failed = self.phase {
                        ui.add_space(6.0);
                        if let Some(failure) = &self.failure {
                            ui.label(RichText::new(failure).size(11.5).color(theme.text_tertiary));
                        }
                        ui.add_space(12.0);
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new(&self.strings.close)
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
                            ctx.send_viewport_cmd(ViewportCommand::Close);
                        }
                    }
                });
            });

        if matches!(self.phase, Phase::Done) && self.close_at.is_some_and(|at| Instant::now() >= at)
        {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
        // Keep the indeterminate sweep alive between worker events.
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

/// Run the headless lane with a passive window over it — the WEBVIEW
/// window when the machine (and the manifest, and the flags) allow it,
/// the egui window as the floor. Either way the lane itself works on a
/// worker thread (stdout printing intact, the tap feeding the window)
/// while the event loop runs on this thread. Returns the lane's own
/// result either way — when neither window can open (service session,
/// headless CI box) the worker has already started, so the run simply
/// finishes console-only.
pub(crate) fn run_silent_with_progress(
    cli: &crate::Cli,
    config: &ShunConfig,
    payload: &ArchivePayload,
) -> Result<(), String> {
    let faces = crate::faces_of(config);
    if webview_lane_allowed(cli, &faces, crate::webview2_loader_ready()) {
        match crate::run_passive_tauri(cli.clone(), config.clone(), payload.clone()) {
            // The window came and went (an early user close included) —
            // the worker's own verdict is the run's verdict.
            Ok(result) => return result,
            Err(err) => {
                // A worker already attached means the window RAN and the
                // loop died abnormally — the install owns this run; join
                // it to completion instead of spawning a second one over
                // the same target directory.
                if worker_attached() {
                    crate::diag!(
                        "shun: webview passive window died mid-run ({err}); joining the install it started"
                    );
                    return join_worker();
                }
                crate::diag!(
                    "shun: webview passive window unavailable ({err}); the egui passive window takes over"
                );
            }
        }
    }
    run_silent_egui(cli, config, payload)
}

/// The egui passive window: the fallback face's own chrome — frameless
/// with the custom caption bar, the manifest/OS-resolved theme tokens,
/// the wizard's progress bar — never a native-titled window.
fn run_silent_egui(
    cli: &crate::Cli,
    config: &ShunConfig,
    payload: &ArchivePayload,
) -> Result<(), String> {
    let (sender, receiver) = mpsc::channel::<LaneEvent>();
    let worker = spawn_worker(sender, cli.clone(), config.clone(), payload.clone());

    let strings = fallback::silent_strings(&config.product.name);
    let heading = {
        let template = if cli.uninstall {
            &strings.uninstall_heading
        } else {
            &strings.install_heading
        };
        template.replace("%PRODUCT%", &config.product.name)
    };
    let titlebar_word = if cli.uninstall {
        strings.titlebar_uninstall.clone()
    } else {
        strings.titlebar_installer.clone()
    };
    // The taskbar/alt-tab title: the same localized frame title the
    // wizard's egui face resolves (saved language → system locale →
    // zh-Hans floor).
    let title = crate::os_window_title(config, cli.uninstall);
    // The debug capture lane, same as the wizard's egui face: locate the
    // window by its exact title once it exists.
    #[cfg(windows)]
    if let Some(path) = &cli.screenshot {
        crate::screenshot::schedule_by_title(
            title.clone(),
            path.clone(),
            cli.screenshot_delay.unwrap_or(4000),
        );
    }

    let options = eframe::NativeOptions {
        viewport: fallback::icon_viewport_builder(crate::LOGO_KIND, crate::LOGO_BYTES)
            .with_decorations(false)
            .with_inner_size([PASSIVE_W, PASSIVE_H])
            .with_resizable(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_position(crate::splash::center_on_primary(PASSIVE_W, PASSIVE_H)),
        ..Default::default()
    };
    let run = eframe::run_native(
        &title,
        options,
        Box::new(|cc| {
            let _fonts = fallback::install_system_fonts(&cc.egui_ctx);
            // The wizard face's canvas contract (see build_app): the
            // token set paints over the dark visuals — the panel fill
            // and widget strokes below are what a light manifest or a
            // light OS side flips.
            let theme = fallback::resolve_theme(config);
            cc.egui_ctx.set_theme(egui::Theme::Dark);
            cc.egui_ctx
                .options_mut(|o| o.theme_preference = egui::ThemePreference::Dark);
            let mut visuals = egui::Visuals::dark();
            visuals.panel_fill = theme.background;
            visuals.window_fill = theme.background;
            visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0f32, theme.border);
            cc.egui_ctx.set_visuals(visuals);
            cc.egui_ctx
                .style_mut(|style| style.interaction.selectable_labels = false);
            Ok(Box::new(PassiveApp {
                strings,
                heading,
                receiver,
                phase: Phase::Running,
                uninstall: cli.uninstall,
                step: String::new(),
                percent: None,
                failure: None,
                close_at: None,
                theme,
                caption_icons: fallback::CaptionIcons::load(&cc.egui_ctx),
                logo: fallback::load_logo(&cc.egui_ctx, crate::LOGO_KIND, crate::LOGO_BYTES),
                product: config.product.name.clone(),
                titlebar_word,
                version: format!("v{}", config.product.version),
                dwm_rounded: false,
            }))
        }),
    );
    if run.is_err() {
        // No desktop session for this run: the lane keeps its
        // console-only contract, the worker is already delivering.
        crate::diag!("shun: passive window unavailable — the silent run stays console-only");
    }
    worker
        .join()
        .map_err(|_| "the silent worker panicked".to_string())?
}

/// The worker: the tapped headless lane. The tap only SENDS — the
/// window may already be gone (user closed it mid-run), which must
/// never affect the install.
fn spawn_worker(
    sender: Sender<LaneEvent>,
    cli: crate::Cli,
    config: ShunConfig,
    payload: ArchivePayload,
) -> std::thread::JoinHandle<Result<(), String>> {
    std::thread::spawn(move || {
        let result = crate::run_headless_with_tap(&cli, &config, &payload, &mut |note| {
            let _ = sender.send(LaneEvent::Note(note));
        });
        let _ = sender.send(LaneEvent::Done(
            result.is_ok(),
            result.as_ref().err().cloned(),
        ));
        result
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser as _;

    fn cli(args: &[&str]) -> crate::Cli {
        let mut argv = vec!["shun-installer".to_string()];
        argv.extend(args.iter().map(|s| s.to_string()));
        crate::Cli::try_parse_from(argv).expect("fixture parses")
    }

    #[test]
    fn the_webview_lane_is_the_default_when_the_runtime_answers() {
        let faces = vec![shun::config::UiFace::Webview, shun::config::UiFace::Egui];
        assert!(webview_lane_allowed(&cli(&["--silent"]), &faces, true));
    }

    #[test]
    fn a_missing_runtime_falls_to_the_egui_lane() {
        let faces = vec![shun::config::UiFace::Webview, shun::config::UiFace::Egui];
        assert!(!webview_lane_allowed(&cli(&["--silent"]), &faces, false));
    }

    #[test]
    fn forced_egui_flags_keep_their_promise_on_the_passive_window() {
        let faces = vec![shun::config::UiFace::Webview, shun::config::UiFace::Egui];
        for flag in ["--no-webview", "--fallback", "--egui"] {
            assert!(
                !webview_lane_allowed(&cli(&["--silent", flag]), &faces, true),
                "{flag} must pin the egui passive window"
            );
        }
    }

    #[test]
    fn a_disabled_webview_face_pins_the_egui_lane() {
        let faces = vec![shun::config::UiFace::Egui];
        assert!(!webview_lane_allowed(&cli(&["--silent"]), &faces, true));
    }
}
