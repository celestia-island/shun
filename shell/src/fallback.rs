//! Offline fallback UI (egui) — the same install flow as a proper wizard.
//!
//! Tauri on Windows renders through WebView2 and there is no second
//! engine to switch to; on a machine without the runtime the shell would
//! die at window creation. This module is the escape hatch: an egui
//! wizard driven by the **same** embedded configuration and payload as
//! the hikari UI (one manifest, two renderers), used when WebView2 is
//! missing or when the operator forces it with `--fallback`.
//!
//! The banner states why the fallback is running — a missing-runtime
//! install must say so, not silently degrade.
//!
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

use egui::{
    Align, Button, Color32, Context, CornerRadius, FontData, FontDefinitions, FontFamily, Frame,
    Layout, Margin, ProgressBar, RichText, ScrollArea, TextEdit, Vec2, Visuals,
};
use shun::config::{ShunConfig, TargetConfig};
use shun::flow::{Flow, FlowEvent, FlowPhase};
use shun::payload::ArchivePayload;
use shun::targets::install::{InstallContext, InstallFlow, WindowsRegistration, uninstall};

/// Brand accent (matches the hikari theme default).
const ACCENT: Color32 = Color32::from_rgb(34, 211, 238);
const ACCENT_FILL: Color32 = Color32::from_rgb(23, 54, 66);
const PANEL: Color32 = Color32::from_rgb(17, 26, 44);
const PANEL_EDGE: Color32 = Color32::from_rgb(30, 41, 59);
const OK_GREEN: Color32 = Color32::from_rgb(74, 222, 128);
const ERR_RED: Color32 = Color32::from_rgb(248, 113, 113);
const WARN_AMBER: Color32 = Color32::from_rgb(253, 224, 71);
const WARN_FILL: Color32 = Color32::from_rgb(66, 47, 4);
const WARN_EDGE: Color32 = Color32::from_rgb(133, 100, 16);

/// Why the fallback UI is running — drives the banner text.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum FallbackReason {
    /// WebView2 was not detected on this machine.
    MissingWebview2,
    /// Forced through the command line (`--fallback` / `--egui`).
    ManualOverride,
}

/// Messages from the worker thread back to the UI thread.
enum WorkerMsg {
    Event(FlowEvent),
    Done(Result<(), String>),
}

/// Wizard stage — the step rail and the content area follow it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Stage {
    /// Mode + directory selection.
    Configure,
    /// A flow is running (install or uninstall).
    Running,
    /// Terminal state; carries what the worker left behind.
    Finished,
}

/// What the finished worker left behind, for the result view.
enum Outcome {
    InstallOk,
    UninstallOk,
    Failed(String),
}

/// Which delivery modes the config declares (rendered in declaration
/// order, mirroring the hikari shell view).
fn offered_modes(config: &ShunConfig) -> Vec<&'static str> {
    let mut modes = Vec::new();
    for target in &config.targets {
        if let TargetConfig::Install(install) = target {
            if install.local {
                modes.push("local");
            }
            if install.portable {
                modes.push("portable");
            }
        }
    }
    modes
}

fn default_dir(config: &ShunConfig, mode: &str) -> PathBuf {
    let product = &config.product.name;
    if mode == "portable" {
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|d| d.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."))
            .join(format!("{product}-portable"))
    } else {
        std::env::var_os("LOCALAPPDATA")
            .map(|local| PathBuf::from(local).join(product))
            .unwrap_or_else(|| PathBuf::from(".").join(product))
    }
}

/// UI strings. Chinese when a CJK font is available, English otherwise —
/// egui's default fonts are Latin-only and tofu boxes help nobody.
struct Texts {
    subtitle: &'static str,
    banner_missing: &'static str,
    banner_manual: &'static str,
    step_choose: &'static str,
    step_install: &'static str,
    step_done: &'static str,
    mode: &'static str,
    mode_local: &'static str,
    mode_local_hint: &'static str,
    mode_portable: &'static str,
    mode_portable_hint: &'static str,
    dir: &'static str,
    browse: &'static str,
    dir_empty: &'static str,
    install: &'static str,
    uninstall: &'static str,
    installing: &'static str,
    uninstalling: &'static str,
    done_install: &'static str,
    done_uninstall: &'static str,
    failed: &'static str,
    entry: &'static str,
    open_dir: &'static str,
    retry: &'static str,
    finish: &'static str,
    log: &'static str,
}

const TEXTS_ZH: Texts = Texts {
    subtitle: "shun 离线安装程序",
    banner_missing: "未检测到 WebView2 运行时（缺失必要环境）—— 已自动切换至离线降级安装界面。安装功能不受影响，界面不带特效。",
    banner_manual: "已通过命令行参数 --fallback 手动启用离线降级安装界面（离线版本，不带特效）。",
    step_choose: "选择模式",
    step_install: "安装",
    step_done: "完成",
    mode: "安装模式",
    mode_local: "本机安装",
    mode_local_hint: "ARP 注册项 + 卸载器 + 开始菜单快捷方式",
    mode_portable: "便携安装",
    mode_portable_hint: "不写注册表，落一个 .shun-portable 标记",
    dir: "安装位置",
    browse: "浏览…",
    dir_empty: "安装目录不能为空",
    install: "安装",
    uninstall: "卸载",
    installing: "正在安装…",
    uninstalling: "正在卸载…",
    done_install: "安装完成",
    done_uninstall: "卸载完成",
    failed: "失败",
    entry: "入口",
    open_dir: "打开安装目录",
    retry: "重试",
    finish: "完成",
    log: "事件日志",
};

const TEXTS_EN: Texts = Texts {
    subtitle: "shun offline installer",
    banner_missing: "WebView2 runtime not detected (missing required environment) — switched to the offline fallback installer. Installation is fully functional; the UI carries no effects.",
    banner_manual: "Offline fallback installer enabled manually via --fallback (the no-effects offline version).",
    step_choose: "Choose",
    step_install: "Install",
    step_done: "Done",
    mode: "Mode",
    mode_local: "Local install",
    mode_local_hint: "ARP entry + uninstaller + Start-menu shortcut",
    mode_portable: "Portable install",
    mode_portable_hint: "No registry, drops a .shun-portable marker",
    dir: "Install location",
    browse: "Browse…",
    dir_empty: "Install directory cannot be empty",
    install: "Install",
    uninstall: "Uninstall",
    installing: "Installing…",
    uninstalling: "Uninstalling…",
    done_install: "Install complete",
    done_uninstall: "Uninstall complete",
    failed: "failed",
    entry: "Entry",
    open_dir: "Open install folder",
    retry: "Retry",
    finish: "Finish",
    log: "Events",
};

/// Registers a system CJK font as a glyph fallback so the Chinese UI
/// renders. Returns `false` when none is found (the UI falls back to
/// English strings). Only single-file `.ttf` fonts are probed — egui
/// cannot index `.ttc` collections.
fn install_cjk_font(ctx: &Context) -> bool {
    const CJK_FONTS: [&str; 4] = [
        r"C:\Windows\Fonts\simhei.ttf",
        r"C:\Windows\Fonts\deng.ttf",
        r"C:\Windows\Fonts\simfang.ttf",
        r"C:\Windows\Fonts\simkai.ttf",
    ];
    for path in CJK_FONTS {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let mut fonts = FontDefinitions::default();
        fonts
            .font_data
            .insert("cjk".into(), FontData::from_owned(bytes).into());
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            fonts.families.entry(family).or_default().push("cjk".into());
        }
        ctx.set_fonts(fonts);
        return true;
    }
    false
}

/// One wizard instance over the embedded config + payload.
struct FallbackApp {
    config: ShunConfig,
    /// Accent color from `shell.theme.accent` (brand default otherwise).
    accent: Color32,
    payload: ArchivePayload,
    reason: FallbackReason,
    texts: &'static Texts,
    stage: Stage,
    mode: &'static str,
    dir: String,
    /// Current progress step + percent (`None` while indeterminate).
    progress: Option<(String, Option<u8>)>,
    /// Delivery phases already finished (the checklist ticks them off).
    phases_done: Vec<FlowPhase>,
    phase_active: Option<FlowPhase>,
    /// What the running worker is doing; `true` = uninstalling.
    uninstalling: Option<bool>,
    log: Vec<String>,
    outcome: Option<Outcome>,
    entry: Option<PathBuf>,
    receiver: Receiver<WorkerMsg>,
}

impl FallbackApp {
    fn new(
        config: ShunConfig,
        payload: ArchivePayload,
        reason: FallbackReason,
        zh: bool,
        receiver: Receiver<WorkerMsg>,
    ) -> Self {
        let mode = offered_modes(&config).first().copied().unwrap_or("local");
        // The wizard follows the same theme knobs as the hikari UI: the
        // accent override in `shell.theme.accent` recolors buttons,
        // highlights and the step rail.
        let accent = config
            .shell
            .as_ref()
            .and_then(|shell| shell.theme.as_ref())
            .and_then(|theme| theme.accent)
            .map(|[r, g, b]| Color32::from_rgb(r, g, b))
            .unwrap_or(ACCENT);
        Self {
            dir: default_dir(&config, mode).to_string_lossy().into_owned(),
            accent,
            config,
            payload,
            reason,
            texts: if zh { &TEXTS_ZH } else { &TEXTS_EN },
            stage: Stage::Configure,
            mode,
            progress: None,
            phases_done: Vec::new(),
            phase_active: None,
            uninstalling: None,
            log: Vec::new(),
            outcome: None,
            entry: None,
            receiver,
        }
    }

    fn install_context(&self) -> Result<InstallContext, String> {
        let install = self
            .config
            .targets
            .iter()
            .find_map(|t| match t {
                TargetConfig::Install(install) => Some(install.clone()),
                _ => None,
            })
            .ok_or_else(|| "此配置未声明安装目标 / no install target declared".to_string())?;
        let dir = self.dir.trim().trim_end_matches('\\');
        if dir.is_empty() {
            return Err(self.texts.dir_empty.to_string());
        }
        Ok(InstallContext {
            product: self.config.product.name.clone(),
            version: self.config.product.version.clone(),
            publisher: self.config.product.publisher.clone(),
            install_dir: PathBuf::from(dir),
            main_exe: install.main_exe.clone(),
            portable: self.mode == "portable",
            estimated_size_kb: 0,
        })
    }

    /// Spawns the worker thread driving the flow. The egui context is
    /// cloned in so the worker can request repaints as events arrive.
    fn spawn_worker(&mut self, ctx: &Context, uninstalling: bool) {
        let install_ctx = match self.install_context() {
            Ok(ctx) => ctx,
            Err(err) => {
                self.outcome = Some(Outcome::Failed(err));
                self.stage = Stage::Finished;
                return;
            }
        };
        self.stage = Stage::Running;
        self.progress = None;
        self.phases_done.clear();
        self.phase_active = None;
        self.log.clear();
        self.uninstalling = Some(uninstalling);
        self.entry = install_ctx
            .main_exe
            .as_ref()
            .map(|main| install_ctx.install_dir.join(main));

        let (sender, receiver) = channel();
        self.receiver = receiver;
        let repaint = ctx.clone();
        let payload = self.payload.clone();
        std::thread::spawn(move || {
            let result = if uninstalling {
                uninstall(&install_ctx, &WindowsRegistration).map_err(|e| e.to_string())
            } else {
                let flow = InstallFlow {
                    payload: &payload,
                    registration: &WindowsRegistration,
                    ctx: install_ctx,
                };
                let mut forward = |event: FlowEvent| {
                    let _ = sender.send(WorkerMsg::Event(event));
                    repaint.request_repaint();
                };
                flow.run(&mut forward).map_err(|e| e.to_string())
            };
            let _ = sender.send(WorkerMsg::Done(result));
            repaint.request_repaint();
        });
    }

    fn drain_worker(&mut self) {
        while let Ok(msg) = self.receiver.try_recv() {
            match msg {
                WorkerMsg::Event(event) => self.apply_event(event),
                WorkerMsg::Done(result) => {
                    self.progress = None;
                    if matches!(result, Ok(())) {
                        self.phases_done = ALL_PHASES.to_vec();
                        self.phase_active = None;
                    }
                    let uninstalling = self.uninstalling.take().unwrap_or(false);
                    let outcome = match result {
                        Ok(()) => match uninstalling {
                            true => Outcome::UninstallOk,
                            false => Outcome::InstallOk,
                        },
                        Err(err) => Outcome::Failed(err),
                    };
                    match &outcome {
                        Outcome::InstallOk => self.push_log(self.texts.done_install.to_string()),
                        Outcome::UninstallOk => {
                            self.push_log(self.texts.done_uninstall.to_string())
                        }
                        Outcome::Failed(err) => {
                            self.push_log(format!("× {} {err}", self.texts.failed))
                        }
                    }
                    self.outcome = Some(outcome);
                    self.stage = Stage::Finished;
                }
            }
        }
    }

    fn apply_event(&mut self, event: FlowEvent) {
        match event {
            FlowEvent::Started => self.push_log("» started".into()),
            FlowEvent::Progress {
                phase,
                step,
                percent,
            } => {
                // Only log step transitions, not every percentage tick.
                let changed = match &self.progress {
                    Some((last_step, _)) => last_step != &step,
                    None => true,
                };
                if changed {
                    self.push_log(step.clone());
                }
                if self.phase_active != Some(phase) {
                    if let Some(previous) = self.phase_active.replace(phase) {
                        if !self.phases_done.contains(&previous) {
                            self.phases_done.push(previous);
                        }
                    }
                }
                self.progress = Some((step, percent));
            }
            FlowEvent::Completed => self.push_log("√ completed".into()),
            FlowEvent::Failed { message } => self.push_log(format!("× {message}")),
            // FlowEvent is #[non_exhaustive] — future events stay readable.
            _ => self.push_log("· event".into()),
        }
    }

    fn push_log(&mut self, line: String) {
        self.log.push(line);
        let len = self.log.len();
        if len > 400 {
            self.log.drain(0..len - 400);
        }
    }

    /// The step rail: Choose → Install → Done, current one highlighted,
    /// earlier ones ticked green.
    fn step_rail(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let steps: [(Stage, &str); 3] = [
                (Stage::Configure, self.texts.step_choose),
                (Stage::Running, self.texts.step_install),
                (Stage::Finished, self.texts.step_done),
            ];
            // A failed run sends the user back to configure.
            let current = match (&self.stage, self.outcome.as_ref()) {
                (Stage::Finished, Some(Outcome::Failed(_))) => Stage::Configure,
                (stage, _) => *stage,
            };
            let order = |stage: Stage| match stage {
                Stage::Configure => 0,
                Stage::Running => 1,
                Stage::Finished => 2,
            };
            for (index, (stage, label)) in steps.iter().enumerate() {
                if index > 0 {
                    ui.label(RichText::new("—").weak().small());
                    ui.add_space(6.0);
                }
                let done = order(*stage) < order(current);
                let (marker, text) = if current == *stage {
                    (
                        RichText::new("●").color(ACCENT).small(),
                        RichText::new(*label).color(ACCENT).strong(),
                    )
                } else if done {
                    (
                        RichText::new("●").color(OK_GREEN).small(),
                        RichText::new(*label).weak(),
                    )
                } else {
                    (
                        RichText::new("○").weak().small(),
                        RichText::new(*label).weak(),
                    )
                };
                ui.label(marker);
                ui.label(text);
                ui.add_space(6.0);
            }
        });
    }

    /// The fallback-reason banner. This is the contract: a
    /// missing-environment install must say so.
    fn banner(&self, ui: &mut egui::Ui) {
        let text = match self.reason {
            FallbackReason::MissingWebview2 => self.texts.banner_missing,
            FallbackReason::ManualOverride => self.texts.banner_manual,
        };
        Frame::default()
            .fill(WARN_FILL)
            .stroke(egui::Stroke::new(1.0f32, WARN_EDGE))
            .inner_margin(Margin::same(10))
            .corner_radius(CornerRadius::same(6))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(text).color(WARN_AMBER).small());
            });
    }

    /// Mode cards + directory row (stage: Configure).
    fn configure_view(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new(self.texts.mode).strong().size(15.0));
        ui.add_space(6.0);

        for mode in offered_modes(&self.config) {
            let selected = self.mode == mode;
            let (title, hint) = match mode {
                "portable" => (self.texts.mode_portable, self.texts.mode_portable_hint),
                _ => (self.texts.mode_local, self.texts.mode_local_hint),
            };
            Frame::default()
                .fill(if selected { ACCENT_FILL } else { PANEL })
                .stroke(if selected {
                    egui::Stroke::new(1.5f32, self.accent)
                } else {
                    egui::Stroke::new(1.0f32, PANEL_EDGE)
                })
                .inner_margin(Margin::same(12))
                .corner_radius(CornerRadius::same(8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        if ui.add(egui::RadioButton::new(selected, "")).clicked() {
                            self.mode = mode;
                            self.dir = default_dir(&self.config, mode)
                                .to_string_lossy()
                                .into_owned();
                        }
                        ui.vertical(|ui| {
                            ui.label(RichText::new(title).strong().size(14.0));
                            ui.label(RichText::new(hint).weak().small());
                        });
                    });
                });
            ui.add_space(6.0);
        }

        ui.add_space(8.0);
        ui.label(RichText::new(self.texts.dir).strong().size(15.0));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_sized(
                Vec2::new(ui.available_width() - 92.0, 22.0),
                TextEdit::singleline(&mut self.dir),
            );
            if ui
                .add_sized(
                    Vec2::new(80.0, 22.0),
                    Button::new(RichText::new(self.texts.browse).small()),
                )
                .clicked()
            {
                if let Some(picked) = rfd::FileDialog::new().pick_folder() {
                    self.dir = picked.to_string_lossy().into_owned();
                }
            }
        });
    }

    /// Progress view (stage: Running): phase checklist + bar + log.
    fn running_view(&mut self, ui: &mut egui::Ui) {
        let uninstalling = self.uninstalling == Some(true);
        ui.add_space(6.0);
        ui.label(
            RichText::new(if uninstalling {
                self.texts.uninstalling
            } else {
                self.texts.installing
            })
            .strong()
            .size(16.0),
        );
        ui.add_space(12.0);

        if !uninstalling {
            // Phase checklist: finished phases tick green, the active one
            // is accent, the rest are pending.
            ui.horizontal(|ui| {
                for phase in ALL_PHASES {
                    let label = match phase {
                        FlowPhase::Download => "下载",
                        FlowPhase::Extract => "解压",
                        FlowPhase::Verify => "校验",
                        FlowPhase::Register => "注册",
                        FlowPhase::Prepare => "准备",
                    };
                    let (marker, color) = if self.phases_done.contains(&phase) {
                        (format!("√ {label}"), OK_GREEN)
                    } else if self.phase_active == Some(phase) {
                        (format!("● {label}"), self.accent)
                    } else {
                        (format!("○ {label}"), Color32::GRAY)
                    };
                    ui.label(RichText::new(marker).color(color).small());
                    ui.add_space(8.0);
                }
            });
            ui.add_space(10.0);
        }

        match &self.progress {
            Some((step, Some(percent))) => {
                ui.add(
                    ProgressBar::new(f32::from(*percent) / 100.0)
                        .desired_height(18.0)
                        .corner_radius(CornerRadius::same(9))
                        .text(step.clone()),
                );
            }
            Some((step, None)) => {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new().size(18.0));
                    ui.label(RichText::new(step.as_str()).small().weak());
                });
            }
            None => {
                ui.add(ProgressBar::new(0.0).desired_height(18.0).show_percentage());
            }
        }

        self.log_view(ui);
    }

    /// Result view (stage: Finished).
    fn finished_view(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        match self.outcome.as_ref().expect("Finished implies an outcome") {
            Outcome::InstallOk => {
                ui.vertical_centered(|ui| {
                    ui.add_space(12.0);
                    ui.label(RichText::new("√").size(34.0).color(OK_GREEN));
                    ui.label(
                        RichText::new(self.texts.done_install)
                            .strong()
                            .size(17.0)
                            .color(OK_GREEN),
                    );
                    ui.add_space(8.0);
                    if let Some(entry) = &self.entry {
                        ui.label(
                            RichText::new(format!("{}: {}", self.texts.entry, entry.display()))
                                .weak()
                                .small(),
                        );
                    }
                });
            }
            Outcome::UninstallOk => {
                ui.vertical_centered(|ui| {
                    ui.add_space(12.0);
                    ui.label(RichText::new("√").size(34.0).color(OK_GREEN));
                    ui.label(
                        RichText::new(self.texts.done_uninstall)
                            .strong()
                            .size(17.0)
                            .color(OK_GREEN),
                    );
                });
            }
            Outcome::Failed(err) => {
                ui.vertical_centered(|ui| {
                    ui.add_space(12.0);
                    ui.label(RichText::new("×").size(34.0).color(ERR_RED));
                    ui.label(
                        RichText::new(self.texts.failed)
                            .strong()
                            .size(17.0)
                            .color(ERR_RED),
                    );
                    ui.add_space(6.0);
                });
                Frame::default()
                    .fill(Color32::from_rgb(53, 17, 20))
                    .stroke(egui::Stroke::new(1.0f32, Color32::from_rgb(127, 29, 29)))
                    .inner_margin(Margin::same(10))
                    .corner_radius(CornerRadius::same(6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(err.clone()).small().color(ERR_RED));
                    });
            }
        }
        ui.add_space(8.0);
        self.log_view(ui);
    }

    fn log_view(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        ui.set_min_height(ui.available_height());
        ui.label(RichText::new(self.texts.log).weak().small());
        ui.add_space(2.0);
        Frame::default()
            .fill(Color32::from_rgb(11, 18, 32))
            .stroke(egui::Stroke::new(1.0f32, PANEL_EDGE))
            .inner_margin(Margin::same(8))
            .corner_radius(CornerRadius::same(6))
            .show(ui, |ui| {
                ui.set_min_height(ui.available_height());
                ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.style_mut().override_text_style = Some(egui::TextStyle::Small);
                        for line in &self.log {
                            ui.label(RichText::new(line).weak());
                        }
                    });
            });
    }
}

/// The wizard's own order of delivery phases for the checklist.
const ALL_PHASES: [FlowPhase; 5] = [
    FlowPhase::Prepare,
    FlowPhase::Download,
    FlowPhase::Extract,
    FlowPhase::Verify,
    FlowPhase::Register,
];

/// Opens a directory in the platform file manager.
fn open_directory(path: &str) {
    #[cfg(windows)]
    let opener = "explorer.exe";
    #[cfg(not(windows))]
    let opener = "xdg-open";
    let _ = std::process::Command::new(opener).arg(path).spawn();
}

/// The egui window title (the screenshot path locates the window by it).
pub fn window_title(config: &ShunConfig) -> String {
    format!("{} — shun offline installer", config.product.name)
}

/// Runs the fallback wizard. Does not return until the window closes.
pub fn run(config: ShunConfig, payload: ArchivePayload, reason: FallbackReason) {
    let title = window_title(&config);
    // Frameless like the hikari shell: the header below draws the custom
    // chrome (drag area + close button). This also keeps eframe's
    // EFRAME_SCREENSHOT_TO capture aligned with the client area.
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_inner_size([640.0, 640.0])
            .with_min_inner_size([560.0, 560.0]),
        ..Default::default()
    };
    let result = eframe::run_native(
        &title,
        options,
        Box::new(move |cc| {
            let mut visuals = Visuals::dark();
            visuals.panel_fill = PANEL;
            visuals.window_fill = PANEL;
            cc.egui_ctx.set_visuals(visuals);
            // Language: `shell.language` pins it; `auto` (or unset)
            // follows what the machine can render (CJK font present).
            let font_found = install_cjk_font(&cc.egui_ctx);
            let zh = match config.shell.as_ref().and_then(|s| s.language.as_deref()) {
                Some("en") => false,
                Some(language) if language.starts_with("zh") => font_found,
                _ => font_found,
            };
            let (_sender, receiver) = channel::<WorkerMsg>();
            Ok(Box::new(FallbackApp::new(
                config, payload, reason, zh, receiver,
            )))
        }),
    );
    if let Err(err) = result {
        // The GUI failed to start (no graphics context, ...). There is no
        // UI left to report through; the exit code says it.
        eprintln!("shun: fallback UI failed: {err}");
        std::process::exit(1);
    }
}

impl eframe::App for FallbackApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.drain_worker();

        // ── Header band (custom chrome): product identity + step rail on
        //    the left, close button on the right, the whole band is the
        //    window drag area.
        egui::TopBottomPanel::top("header")
            .frame(Frame::default().fill(PANEL).inner_margin(Margin {
                left: 16,
                right: 8,
                top: 14,
                bottom: 10,
            }))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(self.config.product.name.as_str())
                                .strong()
                                .size(21.0),
                        );
                        ui.label(
                            RichText::new(format!(
                                "{} · v{} · {}",
                                self.texts.subtitle,
                                self.config.product.version,
                                self.config.product.publisher.as_deref().unwrap_or("")
                            ))
                            .weak()
                            .small(),
                        );
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add(Button::new(RichText::new("×").size(17.0)).frame(false))
                            .clicked()
                        {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                });
                ui.add_space(8.0);
                self.step_rail(ui);
                // Window dragging: the header band doubles as the title
                // bar (the window is frameless).
                let drag = ui.interact(
                    ui.max_rect(),
                    ui.id().with("header-drag"),
                    egui::Sense::drag(),
                );
                if drag.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
            });

        // ── Footer band: actions, installer style — destructive at the
        //    far left, the primary action at the far right.
        egui::TopBottomPanel::bottom("footer").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let configuring = self.stage == Stage::Configure;
                if ui
                    .add_enabled(
                        configuring,
                        Button::new(RichText::new(self.texts.uninstall).small().color(ERR_RED)),
                    )
                    .clicked()
                {
                    self.spawn_worker(ctx, true);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    match self.stage {
                        Stage::Configure => {
                            if ui
                                .add_sized(
                                    Vec2::new(120.0, 28.0),
                                    Button::new(
                                        RichText::new(self.texts.install)
                                            .strong()
                                            .color(Color32::BLACK),
                                    )
                                    .fill(self.accent)
                                    .corner_radius(CornerRadius::same(6)),
                                )
                                .clicked()
                            {
                                self.spawn_worker(ctx, false);
                            }
                        }
                        Stage::Running => {
                            let label = match self.uninstalling {
                                Some(true) => self.texts.uninstalling,
                                _ => self.texts.installing,
                            };
                            ui.add_enabled(
                                false,
                                Button::new(RichText::new(label).strong().color(Color32::BLACK))
                                    .fill(self.accent)
                                    .corner_radius(CornerRadius::same(6)),
                            );
                        }
                        Stage::Finished => match self.outcome.as_ref() {
                            Some(Outcome::Failed(_)) => {
                                if ui
                                    .add_sized(
                                        Vec2::new(120.0, 28.0),
                                        Button::new(RichText::new(self.texts.retry).strong())
                                            .corner_radius(CornerRadius::same(6)),
                                    )
                                    .clicked()
                                {
                                    self.stage = Stage::Configure;
                                    self.outcome = None;
                                }
                            }
                            _ => {
                                if self
                                    .outcome
                                    .as_ref()
                                    .is_some_and(|o| matches!(o, Outcome::InstallOk))
                                    && ui
                                        .add(
                                            Button::new(RichText::new(self.texts.open_dir).small())
                                                .corner_radius(CornerRadius::same(6)),
                                        )
                                        .clicked()
                                {
                                    open_directory(self.dir.trim().trim_end_matches('\\'));
                                }
                                if ui
                                    .add_sized(
                                        Vec2::new(120.0, 28.0),
                                        Button::new(
                                            RichText::new(self.texts.finish)
                                                .strong()
                                                .color(Color32::BLACK),
                                        )
                                        .fill(self.accent)
                                        .corner_radius(CornerRadius::same(6)),
                                    )
                                    .clicked()
                                {
                                    std::process::exit(0);
                                }
                            }
                        },
                    }
                });
            });
            ui.add_space(8.0);
        });

        // ── Content.
        egui::CentralPanel::default()
            .frame(
                Frame::default()
                    .fill(Color32::from_rgb(11, 18, 32))
                    .inner_margin(Margin::same(16)),
            )
            .show(ctx, |ui| {
                self.banner(ui);
                ui.add_space(12.0);
                match self.stage {
                    Stage::Configure => self.configure_view(ui),
                    Stage::Running => self.running_view(ui),
                    Stage::Finished => self.finished_view(ui),
                }
            });
    }
}
