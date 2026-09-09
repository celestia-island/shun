//! Offline fallback UI (egui) — the same install flow as a proper wizard.
//!
//! Tauri on Windows renders through WebView2 and there is no second
//! engine to switch to; on a machine without the runtime the shell would
//! die at window creation. This module is the escape hatch: an egui
//! wizard driven by the **same** embedded configuration and payload as
//! the hikari UI (one manifest, two renderers), used when WebView2 is
//! missing or when the operator forces it with `--fallback`.
//!
//! The look derives from the same source as the hikari shell:
//! `shell/web/src/theme.scss` (the "Abyssal Glass" token layer). The
//! fallback is the crippled sibling — no CSS effects, no web fonts, no
//! lucide icons — but the palette, radii, typography scale, wizard
//! layout and the UI copy (i18n strings of the web shell) are shared, so
//! both renderers are recognizably the same product. Theme knobs apply
//! to both sides: `shell.theme.accent` recolors primary controls,
//! `shell.theme.mode` picks the light/dark token set, `shell.timeline`
//! orients the step rail.
//!
//! The banner states why the fallback is running — a missing-runtime
//! install must say so, not silently degrade.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

use egui::{
    Align, Button, Color32, Context, CornerRadius, FontData, FontDefinitions, FontFamily, Frame,
    Layout, Margin, RichText, Sense, Stroke, TextEdit, TextureHandle, Vec2, pos2,
};
use shun::config::{ShunConfig, TargetConfig};
use shun::flow::{Flow, FlowEvent, FlowPhase};
use shun::payload::ArchivePayload;
use shun::targets::install::{InstallContext, InstallFlow, WindowsRegistration, uninstall};

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

// ── Theme: the shared token layer (see shell/web/src/theme.scss) ────────
//
// Values mirror the scss tokens 1:1: the dark set the installer ships,
// the light set kept for schema parity, and the accent channels that
// `shell.theme.accent` overrides. Semi-transparent whites use egui's
// from_white_alpha (230/153/115 ≈ 90%/60%/45%).

#[derive(Copy, Clone)]
pub(crate) struct Theme {
    pub(crate) background: Color32,
    pub(crate) surface: Color32,
    pub(crate) border: Color32,
    pub(crate) text: Color32,
    pub(crate) text_secondary: Color32,
    pub(crate) text_tertiary: Color32,
    pub(crate) primary: Color32,
    pub(crate) on_primary: Color32,
    pub(crate) success: Color32,
    pub(crate) error: Color32,
    pub(crate) warning: Color32,
}

impl Theme {
    /// The dark token set (the installer default), with the optional
    /// accent override from `shell.theme.accent`.
    fn dark(accent: Option<[u8; 3]>) -> Self {
        Self {
            background: Color32::from_rgb(12, 18, 30),
            surface: Color32::from_rgb(22, 30, 46),
            border: Color32::from_rgb(50, 60, 75),
            text: Color32::from_white_alpha(230),
            text_secondary: Color32::from_white_alpha(153),
            text_tertiary: Color32::from_white_alpha(115),
            primary: accent.map_or(Color32::from_rgb(0, 120, 200), |[r, g, b]| {
                Color32::from_rgb(r, g, b)
            }),
            on_primary: Color32::from_black_alpha(230),
            success: Color32::from_rgb(60, 180, 120),
            error: Color32::from_rgb(220, 80, 80),
            warning: Color32::from_rgb(230, 170, 50),
        }
    }

    /// The light token set (`shell.theme.mode = "light"`).
    fn light(accent: Option<[u8; 3]>) -> Self {
        Self {
            background: Color32::from_rgb(245, 248, 252),
            surface: Color32::from_rgb(255, 255, 255),
            border: Color32::from_rgb(200, 210, 220),
            text: Color32::from_rgb(30, 40, 55),
            text_secondary: Color32::from_rgb(90, 100, 115),
            text_tertiary: Color32::from_rgb(90, 100, 115),
            primary: accent.map_or(Color32::from_rgb(0, 120, 200), |[r, g, b]| {
                Color32::from_rgb(r, g, b)
            }),
            on_primary: Color32::from_rgb(255, 255, 255),
            success: Color32::from_rgb(60, 180, 120),
            error: Color32::from_rgb(220, 80, 80),
            warning: Color32::from_rgb(190, 140, 30),
        }
    }

    /// Accent-tinted fill (a selection card at ~14% over the surface).
    fn primary_tint(&self) -> Color32 {
        mix(self.surface, self.primary, 0.14)
    }

    /// Terminal pane background (a shade between page and surface).
    pub(crate) fn terminal_bg(&self) -> Color32 {
        mix(self.background, self.surface, 0.6)
    }

    /// Terminal plain-echo foreground.
    pub(crate) fn terminal_fg(&self) -> Color32 {
        self.text_secondary
    }

    /// Warning banner fill (warning at ~12% over the background).
    fn warning_tint(&self) -> Color32 {
        mix(self.background, self.warning, 0.12)
    }

    /// Error alert fill.
    fn error_tint(&self) -> Color32 {
        mix(self.background, self.error, 0.12)
    }
}

/// Alpha-blends `over` onto `base`.
fn mix(base: Color32, over: Color32, factor: f32) -> Color32 {
    let channel = |b: u8, o: u8| {
        let blended = f32::from(b) * (1.0 - factor) + f32::from(o) * factor;
        blended.round().clamp(0.0, 255.0) as u8
    };
    Color32::from_rgb(
        channel(base.r(), over.r()),
        channel(base.g(), over.g()),
        channel(base.b(), over.b()),
    )
}

/// Resolves `shell.theme.mode` (default dark; `system` asks Windows).
/// Folder badge prefixing the target row — the shittim-chest file-picker
/// look: a primary-tinted rounded square carrying an outlined folder
/// glyph (no font dependency, pure painter strokes).
fn folder_badge(ui: &mut egui::Ui, theme: &Theme, size: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(7), theme.primary_tint());
    let glyph = rect.shrink(6.5);
    let body_top = glyph.top() + glyph.height() * 0.30;
    let stroke = Stroke::new(1.6f32, theme.primary);
    // Tab: rises from the body's top edge, runs right, folds back down.
    painter.add(egui::Shape::line(
        vec![
            pos2(glyph.left(), body_top),
            pos2(glyph.left(), glyph.top()),
            pos2(glyph.left() + glyph.width() * 0.34, glyph.top()),
            pos2(glyph.left() + glyph.width() * 0.46, body_top),
        ],
        stroke,
    ));
    painter.rect_stroke(
        egui::Rect::from_min_max(
            pos2(glyph.left(), body_top),
            pos2(glyph.right(), glyph.bottom()),
        ),
        CornerRadius::same(2),
        stroke,
        egui::StrokeKind::Middle,
    );
    response
}

fn resolve_theme(config: &ShunConfig) -> Theme {
    let shell = config.shell.clone().unwrap_or_default();
    let accent = shell.theme.as_ref().and_then(|theme| theme.accent);
    // Same semantics as the webview shell (App.tsx applyTheme): an
    // unset mode defaults to dark — the installer ships dark — and only
    // an explicit `system` follows the OS preference.
    match shell.theme.as_ref().and_then(|theme| theme.mode) {
        Some(shun::config::ThemeMode::Light) => Theme::light(accent),
        Some(shun::config::ThemeMode::System) => {
            if system_prefers_light() {
                Theme::light(accent)
            } else {
                Theme::dark(accent)
            }
        }
        Some(shun::config::ThemeMode::Dark) | None => Theme::dark(accent),
    }
}

/// Windows "apps use light theme" probe; non-Windows assumes dark.
#[cfg(windows)]
fn system_prefers_light() -> bool {
    use winreg::RegKey;
    const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
    RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .open_subkey(KEY)
        .and_then(|key| key.get_value::<u32, _>("AppsUseLightTheme"))
        .is_ok_and(|light| light == 1)
}

#[cfg(not(windows))]
fn system_prefers_light() -> bool {
    false
}

// ── UI copy: the i18n strings of the web shell (shell/web/src/i18n.ts) ──

struct Texts {
    title_suffix: &'static str,
    version_prefix: &'static str,
    banner_missing: &'static str,
    banner_manual: &'static str,
    step_mode: &'static str,
    step_scope: &'static str,
    step_license: &'static str,
    step_install: &'static str,
    step_done: &'static str,
    next: &'static str,
    back: &'static str,
    license_agree: &'static str,
    scope_user: &'static str,
    scope_user_hint: &'static str,
    scope_machine: &'static str,
    scope_machine_hint: &'static str,
    mode_local: &'static str,
    mode_local_hint: &'static str,
    mode_portable: &'static str,
    mode_portable_hint: &'static str,
    dir_label: &'static str,
    browse: &'static str,
    browse_title: &'static str,
    dir_empty: &'static str,
    desktop_shortcut: &'static str,
    hint_local: &'static str,
    hint_portable: &'static str,
    install: &'static str,
    uninstall: &'static str,
    installing: &'static str,
    uninstalling: &'static str,
    done_title: &'static str,
    done_uninstall: &'static str,
    failed: &'static str,
    open_dir: &'static str,
    finish: &'static str,
    retry: &'static str,
    log: &'static str,
    log_expand: &'static str,
    log_collapse: &'static str,
    log_write: &'static str,
    log_reuse: &'static str,
    script_begin: &'static str,
    installing_percent: &'static str,
    warn_desktop_blocked: &'static str,
    warn_aumid_blocked: &'static str,
}

const TEXTS_ZH: Texts = Texts {
    title_suffix: "的交付方式",
    version_prefix: "版本",
    banner_missing: "未检测到 WebView2 运行时（缺失必要环境）—— 已自动切换至离线降级安装界面。安装功能不受影响，界面不带特效。",
    banner_manual: "已通过命令行参数 --fallback 手动启用离线降级安装界面（离线版本，不带特效）。",
    step_mode: "交付方式",
    step_scope: "安装范围",
    step_license: "许可协议",
    step_install: "安装",
    step_done: "完成",
    next: "下一步",
    back: "上一步",
    license_agree: "我已阅读并同意上述许可协议",
    scope_user: "仅为我安装",
    scope_user_hint: "每用户安装：写入当前用户的注册表与开始菜单，全程无需管理员权限。",
    scope_machine: "为本机所有用户安装",
    scope_machine_hint: "机器级安装：写入 HKLM 与全局开始菜单，确认后 Windows 将请求管理员权限（UAC）。",
    mode_local: "安装到本机",
    mode_local_hint: "直接 Windows 注册：ARP 卸载条目、开始菜单快捷方式与卸载器。",
    mode_portable: "便携模式",
    mode_portable_hint: "绿色免注册：只写 .shun-portable 标记，数据全部就地存放。",
    dir_label: "安装位置",
    browse: "浏览…",
    browse_title: "选择安装位置",
    dir_empty: "安装目录不能为空",
    desktop_shortcut: "创建桌面快捷方式",
    hint_local: "登记到系统「应用」列表，可从设置或本界面卸载。",
    hint_portable: "写入 .shun-portable 标记；卸载即删除整个目录。",
    install: "开始安装",
    uninstall: "卸载",
    installing: "正在安装…",
    uninstalling: "正在卸载…",
    done_title: "安装完成",
    done_uninstall: "卸载完成",
    failed: "失败",
    open_dir: "打开安装目录",
    finish: "完成",
    retry: "重试",
    log: "事件日志",
    log_expand: "展开日志 ▾",
    log_collapse: "收起日志 ▴",
    log_write: "写入",
    log_reuse: "复用",
    script_begin: "正在执行脚本",
    installing_percent: "正在安装…",
    warn_desktop_blocked: "桌面快捷方式被系统策略拦截（安全软件拒绝了 .lnk 写入）；开始菜单快捷方式与卸载注册不受影响",
    warn_aumid_blocked: "任务栏标识（AUMID）写入被系统策略拦截；手动固定的归组可能受影响",
};

const TEXTS_EN: Texts = Texts {
    title_suffix: "delivery",
    version_prefix: "Version",
    banner_missing: "WebView2 runtime not detected (missing required environment) — switched to the offline fallback installer. Installation is fully functional; the UI carries no effects.",
    banner_manual: "Offline fallback installer enabled manually via --fallback (the no-effects offline version).",
    step_mode: "Delivery mode",
    step_scope: "Install scope",
    step_license: "License",
    step_install: "Install",
    step_done: "Done",
    next: "Next",
    back: "Back",
    license_agree: "I have read and agree to the license above",
    scope_user: "Install for me only",
    scope_user_hint: "Per-user install: current user's registry and Start Menu; no administrator needed.",
    scope_machine: "Install for all users of this PC",
    scope_machine_hint: "Machine-wide install: HKLM and the all-users Start Menu; Windows will ask for administrator (UAC) on confirm.",
    mode_local: "Install to this PC",
    mode_local_hint: "direct Windows registration: ARP entry, start-menu shortcut, uninstaller.",
    mode_portable: "Portable",
    mode_portable_hint: "Green install: only a .shun-portable marker, data stays local.",
    dir_label: "Install location",
    browse: "Browse…",
    browse_title: "Choose install location",
    dir_empty: "Install directory cannot be empty",
    desktop_shortcut: "Create a desktop shortcut",
    hint_local: "Registered in system Apps; uninstall from Settings or here.",
    hint_portable: "Writes a .shun-portable marker; uninstalling removes the folder.",
    install: "Install",
    uninstall: "Uninstall",
    installing: "Installing…",
    uninstalling: "Uninstalling…",
    done_title: "Install complete",
    done_uninstall: "Uninstall complete",
    failed: "failed",
    open_dir: "Open install folder",
    finish: "Finish",
    retry: "Retry",
    log: "Events",
    log_expand: "Expand log ▾",
    log_collapse: "Collapse log ▴",
    log_write: "write",
    log_reuse: "reuse",
    script_begin: "running script",
    installing_percent: "Installing…",
    warn_desktop_blocked: "Desktop shortcut blocked by system policy (security software denied the .lnk write); the Start-menu shortcut and uninstall registration are unaffected",
    warn_aumid_blocked: "Taskbar identity (AUMID) stamp blocked by system policy; manual pin grouping may be affected",
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

/// The embedded product logo, decoded to an egui texture (like the
/// hikari title bar's icon prop). `None` when the manifest declares no
/// logo or the bytes do not decode.
fn load_logo(ctx: &Context, kind: &str, bytes: &[u8]) -> Option<TextureHandle> {
    if kind == "none" || bytes.is_empty() {
        return None;
    }
    let format = match kind.to_ascii_lowercase().as_str() {
        "png" => image::ImageFormat::Png,
        "jpg" | "jpeg" => image::ImageFormat::Jpeg,
        "webp" => image::ImageFormat::WebP,
        _ => return None,
    };
    let logo = image::load_from_memory_with_format(bytes, format).ok()?;
    // Title-bar scale — a 2048px webp would waste 16MB of VRAM.
    let logo = logo.resize_exact(48, 48, image::imageops::FilterType::Triangle);
    let rgba = logo.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw().as_slice());
    Some(ctx.load_texture("shun-logo", color_image, egui::TextureOptions::LINEAR))
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

/// Whether the install target's desktop-shortcut policy is `ask` (the
/// wizard checkbox); `always`/`never` never consult the user.
fn desktop_policy_asks(config: &ShunConfig) -> bool {
    install_of(config)
        .map(|install| install.desktop_shortcut == shun::config::DesktopShortcutPolicy::Ask)
        .unwrap_or(false)
}

/// Whether the install target's scope policy is `ask` (the wizard
/// choice); `user`/`machine` never consult the user.
fn scope_policy_asks(config: &ShunConfig) -> bool {
    install_of(config)
        .map(|install| install.scope == shun::config::ScopePolicy::Ask)
        .unwrap_or(false)
}

fn install_of(config: &ShunConfig) -> Option<&shun::config::InstallConfig> {
    config.targets.iter().find_map(|t| match t {
        TargetConfig::Install(install) => Some(install),
        _ => None,
    })
}

/// One wizard instance over the embedded config + payload.
struct FallbackApp {
    config: ShunConfig,
    payload: ArchivePayload,
    reason: FallbackReason,
    texts: &'static Texts,
    theme: Theme,
    timeline_left: bool,
    logo: Option<TextureHandle>,
    stage: Stage,
    mode: &'static str,
    dir: String,
    /// The wizard's answer to the `ask` desktop-shortcut policy
    /// (default checked, the NSIS convention).
    desktop_shortcut: bool,
    /// The wizard's answer to the `ask` install-scope policy
    /// (default per-user).
    machine: bool,
    /// The resolved wizard pipeline (ordered steps, bodies inlined).
    steps: Vec<shun::config::ResolvedStep>,
    /// Cursor into `steps` while on `Stage::Configure`.
    step: usize,
    /// The license checkbox (`license` steps gate progression on it).
    license_accepted: bool,
    /// Current progress step + percent (`None` while indeterminate).
    progress: Option<(String, Option<u8>)>,
    /// Overall run completion 0-100 (phase-weighted), `None` before the
    /// flow reports anything.
    overall: Option<u8>,
    /// The collapsible install-output terminal.
    terminal: crate::terminal::Terminal,
    /// Configured terminal verbosity (`shell.log-level`).
    log_level: shun::config::LogVerbosity,
    /// Delivery phases already finished (the checklist ticks them off).
    phases_done: Vec<FlowPhase>,
    phase_active: Option<FlowPhase>,
    /// What the running worker is doing; `true` = uninstalling.
    uninstalling: Option<bool>,
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
        logo: Option<TextureHandle>,
        steps: Vec<shun::config::ResolvedStep>,
    ) -> Self {
        let theme = resolve_theme(&config);
        let shell = config.shell.clone().unwrap_or_default();
        let mode = offered_modes(&config).first().copied().unwrap_or("local");
        Self {
            dir: default_dir(&config, mode).to_string_lossy().into_owned(),
            config,
            payload,
            reason,
            texts: if zh { &TEXTS_ZH } else { &TEXTS_EN },
            theme,
            timeline_left: shell.timeline == Some(shun::config::TimelineOrientation::Left),
            logo,
            stage: Stage::Configure,
            mode,
            desktop_shortcut: true,
            machine: false,
            steps,
            step: 0,
            license_accepted: false,
            progress: None,
            overall: None,
            terminal: crate::terminal::Terminal::new(true),
            log_level: shell.log_level.unwrap_or(shun::config::LogVerbosity::All),
            phases_done: Vec::new(),
            phase_active: None,
            uninstalling: None,
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
                TargetConfig::Install(install) => Some(install),
                _ => None,
            })
            .ok_or_else(|| "此配置未声明安装目标 / no install target declared".to_string())?;
        let dir = self.dir.trim().trim_end_matches('\\');
        if dir.is_empty() {
            return Err(self.texts.dir_empty.to_string());
        }
        let mut ctx = InstallContext::new(
            self.config.product.name.clone(),
            self.config.product.version.clone(),
            PathBuf::from(dir),
            self.mode == "portable",
        );
        ctx.publisher = self.config.product.publisher.clone();
        ctx.main_exe = install.main_exe.clone();
        ctx.apply_config(
            install,
            shun::targets::install::WizardAnswers {
                desktop_shortcut: self.desktop_shortcut,
                machine: self.machine,
            },
        );
        Ok(ctx)
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
        // Machine scope needs an elevated token; re-launch under UAC
        // carrying the resolved answers (headless) and exit this
        // instance — the elevated copy carries on.
        if let Err(err) = crate::ensure_elevated_for(
            &install_ctx,
            self.mode,
            self.dir.trim(),
            shun::targets::install::WizardAnswers {
                desktop_shortcut: self.desktop_shortcut,
                machine: self.machine,
            },
            uninstalling,
        ) {
            self.outcome = Some(Outcome::Failed(err));
            self.stage = Stage::Finished;
            return;
        }
        self.stage = Stage::Running;
        self.progress = None;
        self.phases_done.clear();
        self.phase_active = None;
        self.terminal.clear();
        self.overall = None;
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
                        Outcome::InstallOk => self.push_log(self.texts.done_title.to_string()),
                        Outcome::UninstallOk => {
                            self.push_log(self.texts.done_uninstall.to_string())
                        }
                        Outcome::Failed(err) => self.terminal.push(
                            crate::terminal::LineKind::Error,
                            format!("× {} {err}", self.texts.failed),
                        ),
                    }
                    self.outcome = Some(outcome);
                    self.stage = Stage::Finished;
                }
            }
        }
    }

    fn apply_event(&mut self, event: FlowEvent) {
        match event {
            FlowEvent::Started => {
                self.overall = Some(0);
                self.terminal
                    .push(crate::terminal::LineKind::Step, "» started".into())
            }
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
                // Overall completion is phase-weighted: download maps to
                // the first tenth, extraction to the following 85%; the
                // trailing registration is instant and `Completed` caps
                // the bar.
                if let Some(percent) = percent {
                    let weighted = match phase {
                        FlowPhase::Download => (percent as u16) / 10,
                        FlowPhase::Extract | FlowPhase::Verify => 10 + (percent as u16) * 85 / 100,
                        _ => self.overall.map(u16::from).unwrap_or(0),
                    };
                    let overall = weighted.max(self.overall.map(u16::from).unwrap_or(0));
                    self.overall = Some(u8::try_from(overall).unwrap_or(100));
                }
                self.progress = Some((step, percent));
            }
            FlowEvent::Log { record } => self.push_flow_log(record),
            FlowEvent::Completed => {
                self.overall = Some(100);
                self.push_log("√ completed".into())
            }
            FlowEvent::Failed { message } => self
                .terminal
                .push(crate::terminal::LineKind::Error, format!("× {message}")),
            // FlowEvent is #[non_exhaustive] — future events stay readable.
            _ => self.push_log("· event".into()),
        }
    }

    /// Renders one structured flow log record into the terminal, i18n'd,
    /// honoring the configured verbosity (`shell.log-level`).
    fn push_flow_log(&mut self, record: shun::flow::FlowLog) {
        use shun::config::LogVerbosity;
        use shun::flow::FlowLog;

        let scripts = record.is_script();
        match self.log_level {
            LogVerbosity::Off => return,
            LogVerbosity::Files if scripts => return,
            LogVerbosity::Scripts if !scripts => return,
            _ => {}
        }
        // Warnings bypass the family filter: only `off` hides them.
        if let FlowLog::Warning { code, detail } = &record {
            let text = match code.as_str() {
                "desktop-shortcut-blocked" => self.texts.warn_desktop_blocked,
                "aumid-stamp-blocked" => self.texts.warn_aumid_blocked,
                _ => detail.as_str(),
            };
            self.terminal
                .push(crate::terminal::LineKind::Error, format!("⚠ {text}"));
            return;
        }
        match record {
            FlowLog::FileWrite { path } => {
                let prefix = self.texts.log_write;
                self.push_log(format!("{prefix} {}", path.display()));
            }
            FlowLog::FileReuse { path } => {
                let prefix = self.texts.log_reuse;
                self.push_log(format!("{prefix} {}", path.display()));
            }
            FlowLog::ScriptBegin { name } => {
                let prefix = self.texts.script_begin;
                self.push_log(format!("{prefix} {name}"));
            }
            FlowLog::ScriptLine { line, .. } => {
                self.terminal.push(crate::terminal::LineKind::Echo, line);
            }
            FlowLog::CommandDone { command } => {
                self.push_log(format!("✓ {command}"));
            }
            // FlowLog is #[non_exhaustive] — future records still render.
            _ => {}
        }
    }

    fn push_log(&mut self, line: String) {
        self.terminal.push(crate::terminal::LineKind::Ok, line);
    }

    fn hint(&self) -> &'static str {
        match self.mode {
            "portable" => self.texts.hint_portable,
            _ => self.texts.hint_local,
        }
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

/// The egui window title (the screenshot path locates the window by it).
pub fn window_title(config: &ShunConfig) -> String {
    format!("{} — shun offline installer", config.product.name)
}

/// Opens a directory in the platform file manager.
fn open_directory(path: &str) {
    #[cfg(windows)]
    let opener = "explorer.exe";
    #[cfg(not(windows))]
    let opener = "xdg-open";
    let _ = std::process::Command::new(opener).arg(path).spawn();
}

/// Runs the fallback wizard. Does not return until the window closes.
pub fn run(
    config: ShunConfig,
    payload: ArchivePayload,
    reason: FallbackReason,
    logo_kind: &str,
    logo_bytes: &[u8],
    steps: Vec<shun::config::ResolvedStep>,
) {
    let title = window_title(&config);
    // Frameless like the hikari shell: the title bar below draws the
    // custom chrome (logo + caption + drag + close). This also keeps the
    // `--screenshot` capture aligned with the client area.
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_inner_size([800.0, 600.0])
            .with_min_inner_size([640.0, 560.0]),
        ..Default::default()
    };
    let result = eframe::run_native(
        &title,
        options,
        Box::new(move |cc| {
            let theme = resolve_theme(&config);
            let mut visuals = egui::Visuals::dark();
            visuals.panel_fill = theme.background;
            visuals.window_fill = theme.background;
            visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0f32, theme.border);
            cc.egui_ctx.set_visuals(visuals);
            // Language: `shell.language` pins it; `auto` (or unset)
            // follows what the machine can render (CJK font present).
            let font_found = install_cjk_font(&cc.egui_ctx);
            let zh = match config.shell.as_ref().and_then(|s| s.language.as_deref()) {
                Some("en") => false,
                Some(language) if language.starts_with("zh") => font_found,
                _ => font_found,
            };
            let logo = load_logo(&cc.egui_ctx, logo_kind, logo_bytes);
            let (_sender, receiver) = channel::<WorkerMsg>();
            Ok(Box::new(FallbackApp::new(
                config, payload, reason, zh, receiver, logo, steps,
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

// ── Rendering — mirrors the hikari wizard layout ────────────────────────

impl FallbackApp {
    /// The hikari caption bar: logo + title left, close right, the whole
    /// band drags the window (double-click toggles nothing — the shell
    /// window is not maximizable).
    fn title_bar(&mut self, ui: &mut egui::Ui) {
        let theme = &self.theme;
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            let bar_height = 24.0;
            if let Some(logo) = &self.logo {
                ui.add(egui::Image::from_texture(logo).fit_to_exact_size(Vec2::splat(20.0)));
            }
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!("{} Installer", self.config.product.name))
                    .color(theme.text_secondary)
                    .size(13.0),
            );
            ui.set_min_height(bar_height);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add(Button::new(RichText::new("×").size(16.0)).frame(false))
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                ui.add_space(8.0);
            });
        });
        let drag = ui.interact(ui.max_rect(), ui.id().with("titlebar-drag"), Sense::drag());
        if drag.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
    }

    /// The HTimeline analog: one marker per wizard step, then install,
    /// then done. Horizontal under the caption (default) or vertical on
    /// the left when `shell.timeline = "left"`.
    fn timeline(&self, ui: &mut egui::Ui, vertical: bool) {
        let theme = &self.theme;
        // A failed run sends the user back to configure.
        let current = match (&self.stage, self.outcome.as_ref()) {
            (Stage::Finished, Some(Outcome::Failed(_))) => Stage::Configure,
            (stage, _) => *stage,
        };
        // The rail lists every pre-install step (localized per kind,
        // configured title for content steps), then install and done.
        let preinstall: Vec<(usize, String)> = self
            .steps
            .iter()
            .enumerate()
            .map(|(index, step)| (index, self.step_label(step).into_owned()))
            .collect();
        let active_marker = match current {
            Stage::Configure => Some(self.step),
            _ => None,
        };
        let order_current = match current {
            Stage::Configure => self.step,
            Stage::Running => self.steps.len(),
            Stage::Finished => self.steps.len() + 1,
        };
        let mut items: Vec<(bool, bool, String)> = preinstall
            .iter()
            .map(|(index, label)| {
                (
                    active_marker == Some(*index),
                    *index < order_current,
                    label.clone(),
                )
            })
            .collect();
        items.push((
            current == Stage::Running,
            order_current > self.steps.len(),
            self.texts.step_install.to_owned(),
        ));
        items.push((
            current == Stage::Finished && !matches!(self.outcome, Some(Outcome::Failed(_))),
            false,
            self.texts.step_done.to_owned(),
        ));
        let rail: Box<dyn FnOnce(&mut egui::Ui)> = Box::new(|ui| {
            let items = items.clone();
            let paint = |ui: &mut egui::Ui| {
                for (index, (active, done, label)) in items.iter().enumerate() {
                    if !vertical && index > 0 {
                        let line_color = if *done { theme.success } else { theme.border };
                        ui.label(RichText::new("——").color(line_color).small());
                        ui.add_space(6.0);
                    } else if vertical && index > 0 {
                        ui.label(RichText::new("│").color(theme.border).small());
                    }
                    let (marker, marker_color, text_color) = if *active {
                        ("●", theme.primary, theme.text)
                    } else if *done {
                        ("●", theme.success, theme.text_secondary)
                    } else {
                        ("○", theme.text_tertiary, theme.text_tertiary)
                    };
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(marker).color(marker_color).small());
                        ui.label(RichText::new(label.as_str()).color(text_color).small());
                    });
                    ui.add_space(if vertical { 10.0 } else { 0.0 });
                }
            };
            if vertical {
                ui.vertical(paint);
            } else {
                ui.horizontal(paint);
            }
        });
        rail(ui);
    }

    /// Localized rail label for a step (content steps carry their
    /// configured title).
    fn step_label<'a>(&self, step: &'a shun::config::ResolvedStep) -> std::borrow::Cow<'a, str> {
        use shun::config::StepKind;
        match step.kind {
            StepKind::Mode => self.texts.step_mode.into(),
            StepKind::Scope => self.texts.step_scope.into(),
            StepKind::License => self.texts.step_license.into(),
            StepKind::Content => step.title.as_str().into(),
            StepKind::Install => self.texts.step_install.into(),
        }
    }

    /// The fallback-reason banner. This is the contract: a
    /// missing-environment install must say so.
    fn banner(&self, ui: &mut egui::Ui) {
        let theme = &self.theme;
        let text = match self.reason {
            FallbackReason::MissingWebview2 => self.texts.banner_missing,
            FallbackReason::ManualOverride => self.texts.banner_manual,
        };
        Frame::default()
            .fill(theme.warning_tint())
            .stroke(Stroke::new(1.0f32, theme.warning))
            .inner_margin(Margin::same(10))
            .corner_radius(CornerRadius::same(8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(text).color(theme.warning).size(12.5));
            });
    }

    /// The configure pane: dispatches on the current wizard step's kind.
    fn configure_view(&mut self, ui: &mut egui::Ui) {
        use shun::config::StepKind;
        match self
            .steps
            .get(self.step)
            .map(|step| step.kind)
            .unwrap_or(StepKind::Mode)
        {
            StepKind::Mode => self.mode_view(ui),
            StepKind::Scope => self.scope_view(ui),
            StepKind::License => self.license_view(ui),
            StepKind::Content => self.content_view(ui),
            // An install step never renders here (the run takes over),
            // but a mis-resolved pipeline still shows something sane.
            StepKind::Install => self.mode_view(ui),
        }
    }

    /// The install-scope pane (also embedded in the mode pane when its
    /// policy is `ask`): user vs machine radio.
    fn scope_view(&mut self, ui: &mut egui::Ui) {
        self.scope_choice(ui);
    }

    /// The license pane: the agreement text with an accept checkbox.
    fn license_view(&mut self, ui: &mut egui::Ui) {
        let theme = &self.theme;
        let body = self
            .steps
            .get(self.step)
            .and_then(|step| step.body.as_deref())
            .unwrap_or_default();
        Frame::default()
            .fill(theme.surface)
            .stroke(Stroke::new(1.0f32, theme.border))
            .inner_margin(Margin::same(12))
            .corner_radius(CornerRadius::same(10))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("license-body")
                    .auto_shrink([false, false])
                    .max_height(ui.available_height() - 44.0)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(body).size(12.5).color(theme.text_secondary));
                    });
            });
        ui.add_space(8.0);
        ui.checkbox(&mut self.license_accepted, self.texts.license_agree);
    }

    /// A markdown content step: title + body in a scroll area.
    fn content_view(&mut self, ui: &mut egui::Ui) {
        let theme = &self.theme;
        let step = self.steps.get(self.step);
        let title = step.map(|s| s.title.as_str()).unwrap_or_default();
        let body = step.and_then(|s| s.body.as_deref()).unwrap_or_default();
        if !title.is_empty() {
            ui.label(RichText::new(title).strong().size(17.0).color(theme.text));
            ui.add_space(8.0);
        }
        Frame::default()
            .fill(theme.surface)
            .stroke(Stroke::new(1.0f32, theme.border))
            .inner_margin(Margin::same(12))
            .corner_radius(CornerRadius::same(10))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("content-body")
                    .auto_shrink([false, false])
                    .max_height(ui.available_height() - 16.0)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(body).size(12.5).color(theme.text_secondary));
                    });
            });
    }

    /// The user/machine radio pair (shared by the standalone scope pane
    /// and the mode pane's embedded variant).
    fn scope_choice(&mut self, ui: &mut egui::Ui) {
        let theme = &self.theme;
        let texts = self.texts;
        ui.add_space(4.0);
        for (is_machine, title, hint) in [
            (false, texts.scope_user, texts.scope_user_hint),
            (true, texts.scope_machine, texts.scope_machine_hint),
        ] {
            let selected = self.machine == is_machine;
            let card = Frame::default()
                .fill(if selected {
                    theme.primary_tint()
                } else {
                    theme.surface
                })
                .stroke(if selected {
                    Stroke::new(1.5f32, theme.primary)
                } else {
                    Stroke::new(1.0f32, theme.border)
                })
                .inner_margin(Margin::same(12))
                .corner_radius(CornerRadius::same(10));
            let inner = card.show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(if selected { "●" } else { "○" })
                                .color(if selected {
                                    theme.primary
                                } else {
                                    theme.text_tertiary
                                })
                                .small(),
                        );
                        ui.label(RichText::new(title).strong().size(14.0).color(if selected {
                            theme.text
                        } else {
                            theme.text_secondary
                        }));
                    });
                    ui.add_space(4.0);
                    ui.label(RichText::new(hint).size(12.0).color(theme.text_tertiary));
                });
            });
            if ui.rect_contains_pointer(inner.response.rect) && inner.response.hovered() {
                self.machine = is_machine;
            }
            ui.add_space(8.0);
        }
    }

    /// Mode selection grid + target row (the "mode" pane).
    fn mode_view(&mut self, ui: &mut egui::Ui) {
        let theme = &self.theme;
        let texts = self.texts;

        // Hero — the h1 of the hikari wizard.
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(self.config.product.name.as_str())
                    .strong()
                    .size(24.0)
                    .color(theme.text),
            );
            ui.label(
                RichText::new(texts.title_suffix)
                    .strong()
                    .size(24.0)
                    .color(theme.text),
            );
        });
        ui.label(
            RichText::new(format!(
                "{} {}{}",
                texts.version_prefix,
                self.config.product.version,
                self.config
                    .product
                    .publisher
                    .as_deref()
                    .map(|p| format!(" · {p}"))
                    .unwrap_or_default()
            ))
            .color(theme.text_secondary)
            .size(14.0),
        );
        ui.add_space(14.0);

        // Selection grid: two cards side by side (hikari columns=2),
        // each allocated an exact equal share of the row.
        let modes = offered_modes(&self.config);
        let gap = 8.0;
        let count = modes.len().max(1) as f32;
        ui.horizontal(|ui| {
            for (index, &mode) in modes.iter().enumerate() {
                if index > 0 {
                    ui.add_space(gap);
                }
                let card_width = (ui.available_width() - gap * (count - index as f32 - 1.0))
                    / (count - index as f32);
                let selected = self.mode == mode;
                let (title, hint) = match mode {
                    "portable" => (texts.mode_portable, texts.mode_portable_hint),
                    _ => (texts.mode_local, texts.mode_local_hint),
                };
                let card = Frame::default()
                    .fill(if selected {
                        theme.primary_tint()
                    } else {
                        theme.surface
                    })
                    .stroke(if selected {
                        Stroke::new(1.5f32, theme.primary)
                    } else {
                        Stroke::new(1.0f32, theme.border)
                    })
                    .inner_margin(Margin::same(12))
                    .corner_radius(CornerRadius::same(10));
                let inner = card.show(ui, |ui| {
                    ui.set_width(card_width);
                    ui.vertical(|ui| {
                        ui.add_space(2.0);
                        ui.label(RichText::new(title).strong().size(15.0).color(if selected {
                            theme.text
                        } else {
                            theme.text_secondary
                        }));
                        ui.add_space(4.0);
                        ui.label(RichText::new(hint).size(12.0).color(theme.text_tertiary));
                    });
                });
                // Clicking anywhere on the card selects the mode.
                if ui.rect_contains_pointer(inner.response.rect)
                    && inner.response.hovered()
                    && !selected
                {
                    self.mode = mode;
                    self.dir = default_dir(&self.config, mode)
                        .to_string_lossy()
                        .into_owned();
                }
                ui.add_space(8.0);
            }
        });

        ui.add_space(14.0);

        // Target row: folder badge + input + browse button pinned right.
        // The native rfd dialog is the desktop shell's picker backend
        // (an OS window outside the app — cf. the directory-field note
        // in docs/en/design/delivery-model.md).
        ui.label(
            RichText::new(texts.dir_label)
                .size(13.0)
                .color(theme.text_secondary),
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            folder_badge(ui, theme, 28.0);
            let browse_width = 84.0;
            let gap = ui.spacing().item_spacing.x;
            let input = TextEdit::singleline(&mut self.dir)
                .desired_width(ui.available_width() - browse_width - gap)
                .text_color(theme.text);
            ui.add(input);
            if ui
                .add_sized(
                    Vec2::new(browse_width, 28.0),
                    Button::new(
                        RichText::new(texts.browse)
                            .size(13.0)
                            .color(theme.text_secondary),
                    )
                    .fill(theme.surface)
                    .stroke(Stroke::new(1.0f32, theme.border))
                    .corner_radius(CornerRadius::same(6)),
                )
                .clicked()
            {
                if let Some(picked) = rfd::FileDialog::new()
                    .set_title(texts.browse_title)
                    .pick_folder()
                {
                    self.dir = picked.to_string_lossy().into_owned();
                }
            }
        });
        ui.add_space(6.0);
        ui.label(
            RichText::new(self.hint())
                .size(12.0)
                .color(theme.text_tertiary),
        );

        // Desktop-shortcut toggle: only the `ask` policy consults the
        // wizard (always/never are decided by the config), and only for
        // registered installs — portable mode writes no shortcuts.
        if self.mode != "portable" && desktop_policy_asks(&self.config) {
            ui.add_space(8.0);
            ui.checkbox(&mut self.desktop_shortcut, texts.desktop_shortcut);
        }

        // The `ask` install-scope policy embeds its choice here when no
        // standalone scope step is declared.
        let has_scope_step = self
            .steps
            .iter()
            .any(|step| step.kind == shun::config::StepKind::Scope);
        if self.mode != "portable" && !has_scope_step && scope_policy_asks(&self.config) {
            ui.add_space(10.0);
            ui.label(
                RichText::new(texts.step_scope)
                    .strong()
                    .size(13.0)
                    .color(theme.text_secondary),
            );
            self.scope_choice(ui);
        }
    }

    /// Progress view (the "install" pane): per-phase rows like the
    /// webview UI (which renders download + extract), then the log.
    fn running_view(&mut self, ui: &mut egui::Ui) {
        let theme = &self.theme;
        let uninstalling = self.uninstalling == Some(true);

        // Headline + real overall progress. Uninstalling has no payload
        // phases to weigh, so it stays indeterminate (spinner); an
        // install renders the weighted bar with its live percent.
        ui.add_space(8.0);
        ui.label(
            RichText::new(if uninstalling {
                self.texts.uninstalling
            } else {
                self.texts.installing_percent
            })
            .strong()
            .size(16.0)
            .color(theme.text),
        );
        ui.add_space(12.0);
        if uninstalling {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(18.0));
            });
        } else {
            let percent = self.overall.unwrap_or(0);
            let bar = egui::ProgressBar::new(f32::from(percent) / 100.0)
                .show_percentage()
                .fill(theme.primary)
                .corner_radius(CornerRadius::same(8));
            ui.add(bar.desired_width(ui.available_width()).desired_height(16.0));
        }
        // The live step under the bar — the one thing actually happening.
        if let Some((step, _)) = &self.progress {
            ui.add_space(8.0);
            ui.label(
                RichText::new(step.as_str())
                    .size(12.0)
                    .color(theme.text_tertiary),
            );
        }
        ui.add_space(12.0);

        self.log_view(ui);
    }

    /// Result view (the "done" pane or the failure alert).
    fn finished_view(&mut self, ui: &mut egui::Ui) {
        let theme = &self.theme;
        ui.add_space(8.0);
        match self.outcome.as_ref().expect("Finished implies an outcome") {
            Outcome::InstallOk | Outcome::UninstallOk => {
                let (title, path) = match self.outcome.as_ref().expect("checked above") {
                    Outcome::UninstallOk => (self.texts.done_uninstall, None),
                    _ => (
                        self.texts.done_title,
                        Some(self.dir.trim().trim_end_matches('\\').to_string()),
                    ),
                };
                ui.vertical(|ui| {
                    ui.add_space(12.0);
                    ui.label(RichText::new("√").size(34.0).color(theme.success));
                    ui.label(RichText::new(title).strong().size(18.0).color(theme.text));
                    if let Some(path) = path {
                        ui.add_space(6.0);
                        ui.label(RichText::new(path).size(13.0).color(theme.text_secondary));
                    }
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(self.hint())
                            .size(12.0)
                            .color(theme.text_tertiary),
                    );
                });
            }
            Outcome::Failed(err) => {
                // The HAlert error analog.
                Frame::default()
                    .fill(theme.error_tint())
                    .stroke(Stroke::new(1.0f32, theme.error))
                    .inner_margin(Margin::same(12))
                    .corner_radius(CornerRadius::same(8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new(format!("× {}", self.texts.failed))
                                    .strong()
                                    .size(14.0)
                                    .color(theme.error),
                            );
                            ui.add_space(4.0);
                            ui.label(RichText::new(err.clone()).size(12.5).color(theme.error));
                        });
                    });
            }
        }
        ui.add_space(8.0);
        self.log_view(ui);
    }

    fn log_view(&mut self, ui: &mut egui::Ui) {
        use shun::config::LogVerbosity;
        if self.log_level == LogVerbosity::Off {
            return;
        }
        let theme = &self.theme;
        self.terminal.render(
            ui,
            theme,
            self.texts.log,
            self.texts.log_expand,
            self.texts.log_collapse,
        );
    }

    /// The installer footer: live flow step on the left, nav buttons on
    /// the right (primary action far right, like the webview footer).
    fn footer(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        // While a flow runs there are no actions to take — the pane
        // carries the live step and progress, and the footer (its
        // buttons in particular) stays out of the way.
        if self.stage == Stage::Running {
            return;
        }
        let theme = self.theme;
        ui.separator();
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            // Left: the live flow step (wizard-live analog).
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(if self.stage == Stage::Running {
                        self.progress
                            .as_ref()
                            .map(|(step, _)| step.clone())
                            .unwrap_or_default()
                    } else {
                        String::new()
                    })
                    .size(12.0)
                    .color(theme.text_tertiary),
                );
            });

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let configuring = self.stage == Stage::Configure;
                // The last pre-install step carries the action buttons;
                // intermediate steps walk the pipeline (license steps
                // gate progression on their checkbox).
                let on_last_step = self.step + 1 >= self.steps.len().max(1);
                let step_blocked = self.step_blocked();
                let (label, enabled) = match self.stage {
                    Stage::Configure if !on_last_step => (self.texts.next, !step_blocked),
                    Stage::Configure => (self.texts.install, !step_blocked),
                    Stage::Running => (
                        match self.uninstalling {
                            Some(true) => self.texts.uninstalling,
                            _ => self.texts.installing,
                        },
                        false,
                    ),
                    Stage::Finished => match self.outcome.as_ref() {
                        Some(Outcome::Failed(_)) => (self.texts.retry, true),
                        _ => (self.texts.finish, true),
                    },
                };
                let action = match (self.stage, self.outcome.as_ref()) {
                    (Stage::Configure, _) => Some((false, false)),
                    (Stage::Running, _) => None,
                    (Stage::Finished, Some(Outcome::Failed(_))) => Some((false, true)),
                    (Stage::Finished, _) => Some((false, false)),
                };
                if let Some((uninstalling, _)) = action {
                    if ui
                        .add_enabled(
                            enabled,
                            Button::new(
                                RichText::new(label)
                                    .strong()
                                    .size(13.5)
                                    .color(theme.on_primary),
                            )
                            .fill(theme.primary)
                            .corner_radius(CornerRadius::same(8))
                            .min_size(Vec2::new(112.0, 30.0)),
                        )
                        .clicked()
                    {
                        match self.stage {
                            Stage::Finished => match self.outcome.as_ref() {
                                Some(Outcome::Failed(_)) => {
                                    self.stage = Stage::Configure;
                                    self.outcome = None;
                                }
                                _ => std::process::exit(0),
                            },
                            Stage::Configure if !on_last_step => self.step += 1,
                            _ => self.spawn_worker(ctx, uninstalling),
                        }
                    }
                } else {
                    ui.add_enabled(
                        false,
                        Button::new(
                            RichText::new(label)
                                .strong()
                                .size(13.5)
                                .color(theme.on_primary),
                        )
                        .fill(theme.primary)
                        .corner_radius(CornerRadius::same(8))
                        .min_size(Vec2::new(112.0, 30.0)),
                    );
                }

                // Ghost buttons to the left of the primary: back while
                // walking steps; uninstall on the last configure step and
                // after a successful install; open-folder on success.
                let ghost = |ui: &mut egui::Ui, label: &str| {
                    ui.add(
                        Button::new(RichText::new(label).size(13.0).color(theme.text_secondary))
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::new(1.0f32, theme.border))
                            .corner_radius(CornerRadius::same(8))
                            .min_size(Vec2::new(88.0, 30.0)),
                    )
                    .clicked()
                };
                if configuring {
                    if self.step > 0 && ghost(ui, self.texts.back) {
                        self.step -= 1;
                    }
                    if on_last_step && ghost(ui, self.texts.uninstall) {
                        self.spawn_worker(ctx, true);
                    }
                } else if self.stage == Stage::Finished
                    && matches!(self.outcome, Some(Outcome::InstallOk))
                {
                    if ghost(ui, self.texts.uninstall) {
                        self.spawn_worker(ctx, true);
                    }
                    if ghost(ui, self.texts.open_dir) {
                        open_directory(self.dir.trim().trim_end_matches('\\'));
                    }
                }
            });
        });
        ui.add_space(6.0);
    }

    /// Whether the current step blocks progression (a license step
    /// without its checkbox ticked).
    fn step_blocked(&self) -> bool {
        match self.steps.get(self.step).map(|s| s.kind) {
            Some(shun::config::StepKind::License) => !self.license_accepted,
            _ => false,
        }
    }
}

impl eframe::App for FallbackApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.drain_worker();
        let theme = self.theme;

        // ── Caption bar (frameless chrome).
        egui::TopBottomPanel::top("titlebar")
            .frame(Frame::default().fill(theme.surface).inner_margin(Margin {
                left: 0,
                right: 10,
                top: 8,
                bottom: 8,
            }))
            .show(ctx, |ui| {
                self.title_bar(ui);
            });

        // ── Footer: live step + nav.
        egui::TopBottomPanel::bottom("footer")
            .frame(
                Frame::default()
                    .fill(theme.background)
                    .inner_margin(Margin::symmetric(20, 6)),
            )
            .show(ctx, |ui| {
                self.footer(ui, ctx);
            });

        // ── Optional left rail (`shell.timeline = "left"`).
        let timeline_left = self.timeline_left;
        if timeline_left {
            egui::SidePanel::left("timeline")
                .frame(
                    Frame::default()
                        .fill(theme.surface)
                        .inner_margin(Margin::same(16)),
                )
                .show(ctx, |ui| {
                    self.timeline(ui, true);
                });
        }

        // ── Content pane.
        egui::CentralPanel::default()
            .frame(
                Frame::default()
                    .fill(theme.background)
                    .inner_margin(Margin::symmetric(20, 12)),
            )
            .show(ctx, |ui| {
                if !timeline_left {
                    self.timeline(ui, false);
                    ui.add_space(10.0);
                }
                self.banner(ui);
                ui.add_space(12.0);
                // Every pane centers its content block — horizontally
                // always, vertically too (content panes that read as
                // documents keep their start-aligned text inside the
                // block; the running pane centers its bar). A fixed max
                // width keeps wizard content off the window edges.
                let align = self
                    .steps
                    .get(self.step)
                    .map(|step| step.align)
                    .unwrap_or(shun::config::StepAlign::Center);
                let pane = egui::Layout {
                    main_wrap: false,
                    main_dir: egui::Direction::TopDown,
                    cross_align: egui::Align::Center,
                    main_align: match self.stage {
                        Stage::Running => egui::Align::Min,
                        _ => egui::Align::Center,
                    },
                    main_justify: false,
                    cross_justify: false,
                };
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width().min(560.0), ui.available_height()),
                    pane,
                    |ui| {
                        // The inner column fixes the text alignment;
                        // the outer layout centers the block.
                        let text_align = if self.stage == Stage::Configure {
                            match align {
                                shun::config::StepAlign::Center => egui::Align::Center,
                                shun::config::StepAlign::Start => egui::Align::LEFT,
                            }
                        } else {
                            egui::Align::Center
                        };
                        ui.with_layout(egui::Layout::top_down(text_align), |ui| {
                            ui.set_width(ui.available_width().min(560.0));
                            match self.stage {
                                Stage::Configure => self.configure_view(ui),
                                Stage::Running => self.running_view(ui),
                                Stage::Finished => self.finished_view(ui),
                            }
                        });
                    },
                );
            });
    }
}
