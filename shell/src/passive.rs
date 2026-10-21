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

use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align, Context, CornerRadius, Layout, RichText, Sense, Stroke, Vec2, ViewportCommand,
    WindowLevel,
};

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

/// Worker → window messages.
enum WindowMsg {
    Note(PassiveNote),
    /// The headless lane returned: success, plus its error text.
    Done(bool, Option<String>),
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
    receiver: Receiver<WindowMsg>,
    phase: Phase,
    /// The running step's label and percent (None = indeterminate).
    step: String,
    percent: Option<u8>,
    /// The worker's failure text on the Failed phase.
    failure: Option<String>,
    /// When the Done phase should close the window (a short linger so
    /// the completion line is readable).
    close_at: Option<Instant>,
}

const DONE_LINGER: Duration = Duration::from_millis(1500);

impl PassiveApp {
    fn fold(&mut self, msg: WindowMsg) {
        match msg {
            WindowMsg::Note(PassiveNote::Step { step, percent }) => {
                self.step = step;
                self.percent = percent;
            }
            WindowMsg::Note(PassiveNote::Note(note)) => {
                self.step = note;
                self.percent = None;
            }
            WindowMsg::Done(true, _) => {
                self.phase = Phase::Done;
                self.close_at = Some(Instant::now() + DONE_LINGER);
            }
            WindowMsg::Done(false, failure) => {
                self.phase = Phase::Failed;
                self.failure = failure;
            }
        }
    }
}

impl eframe::App for PassiveApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        while let Ok(msg) = self.receiver.try_recv() {
            self.fold(msg);
        }

        let theme = fallback::Theme::dark(None);
        let (line, line_color) = match &self.phase {
            Phase::Running => (self.step.clone(), theme.text_secondary),
            Phase::Done => (
                if self.heading_is_uninstall() {
                    self.strings.done_uninstall.clone()
                } else {
                    self.strings.done.clone()
                },
                theme.success,
            ),
            Phase::Failed => (
                if self.heading_is_uninstall() {
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
                ui.with_layout(Layout::top_down(Align::Center), |ui| {
                    ui.add_space(30.0);
                    ui.label(
                        RichText::new(&self.heading)
                            .strong()
                            .size(16.0)
                            .color(theme.text),
                    );
                    ui.add_space(14.0);
                    ui.label(RichText::new(line).size(12.5).color(line_color));
                    ui.add_space(10.0);
                    match self.percent {
                        // A step with a known percent: the splash's bar.
                        Some(percent) => {
                            let (bar, _) =
                                ui.allocate_exact_size(Vec2::new(300.0, 6.0), Sense::hover());
                            let painter = ui.painter_at(bar);
                            painter.rect_filled(bar, CornerRadius::same(3), theme.surface);
                            let fill = (bar.width() * f32::from(percent) / 100.0).max(4.0);
                            painter.rect_filled(
                                egui::Rect::from_min_size(bar.min, Vec2::new(fill, 6.0)),
                                CornerRadius::same(3),
                                theme.primary,
                            );
                        }
                        // Indeterminate: a spinner while work continues;
                        // nothing once the run settled.
                        None if matches!(self.phase, Phase::Running) => {
                            ui.add(egui::Spinner::new().size(18.0));
                        }
                        None => {
                            ui.add_space(18.0);
                        }
                    }
                    if let Phase::Failed = self.phase {
                        ui.add_space(6.0);
                        if let Some(failure) = &self.failure {
                            ui.label(RichText::new(failure).size(11.5).color(theme.text_tertiary));
                        }
                        ui.add_space(14.0);
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
        // Keep the spinner alive between worker events.
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

impl PassiveApp {
    /// The heading already chose the uninstall copy when the run is one.
    fn heading_is_uninstall(&self) -> bool {
        self.heading == self.strings.uninstall_heading
    }
}

/// Run the headless lane with the passive window over it: the lane
/// works on a worker thread (stdout printing intact, the tap feeding
/// the window), the window runs this thread's event loop. Returns the
/// lane's own result either way — when the window cannot open (service
/// session, headless CI box) the worker has already started, so the
/// run simply finishes console-only.
pub(crate) fn run_silent_with_progress(
    cli: &crate::Cli,
    config: &ShunConfig,
    payload: &ArchivePayload,
) -> Result<(), String> {
    let (sender, receiver) = mpsc::channel::<WindowMsg>();
    let worker = spawn_worker(sender, cli.clone(), config.clone(), payload.clone());

    let strings = fallback::silent_strings();
    let heading = {
        let template = if cli.uninstall {
            &strings.uninstall_heading
        } else {
            &strings.install_heading
        };
        template.replace("%PRODUCT%", &config.product.name)
    };
    // The bar names the product; the heading (with the product spliced
    // in) lives inside the window where the locale's word order works.
    let title = config.product.name.clone();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([420.0, 208.0])
            .with_resizable(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_position(crate::splash::center_on_primary(420.0, 208.0)),
        ..Default::default()
    };
    let run = eframe::run_native(
        &title,
        options,
        Box::new(|cc| {
            let _fonts = fallback::install_system_fonts(&cc.egui_ctx);
            Ok(Box::new(PassiveApp {
                strings,
                heading,
                receiver,
                phase: Phase::Running,
                step: String::new(),
                percent: None,
                failure: None,
                close_at: None,
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
    sender: Sender<WindowMsg>,
    cli: crate::Cli,
    config: ShunConfig,
    payload: ArchivePayload,
) -> std::thread::JoinHandle<Result<(), String>> {
    std::thread::spawn(move || {
        let result = crate::run_headless_with_tap(&cli, &config, &payload, &mut |note| {
            let _ = sender.send(WindowMsg::Note(note));
        });
        let _ = sender.send(WindowMsg::Done(
            result.is_ok(),
            result.as_ref().err().cloned(),
        ));
        result
    })
}
