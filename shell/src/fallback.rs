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

use std::path::{Path, PathBuf};
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
#[derive(Clone)]
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
    // Neutral plate: the accent-blue tile clashed with the dark pane —
    // the badge now sits in the surface tone with a hairline border and
    // a muted glyph.
    painter.rect_filled(rect, CornerRadius::same(7), theme.surface);
    painter.rect_stroke(
        rect,
        CornerRadius::same(7),
        Stroke::new(1.0f32, theme.border),
        egui::StrokeKind::Inside,
    );
    let glyph = rect.shrink(6.5);
    let body_top = glyph.top() + glyph.height() * 0.30;
    let stroke = Stroke::new(1.6f32, theme.text_secondary);
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

/// The language the fallback UI renders in — the egui side carries the
/// two TEXTS tables below, so its first-step language selector offers
/// these two locales (the web shell offers all eight). The chosen value
/// is what reaches the install context (and from there `SHUN_LANGUAGE`
/// for scripts and the install manifest).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum FallbackLanguage {
    En,
    Zh,
}

impl FallbackLanguage {
    /// The locale code this table maps to (the shell's i18n spelling).
    fn code(self) -> &'static str {
        match self {
            FallbackLanguage::En => "en",
            FallbackLanguage::Zh => "zh-Hans",
        }
    }

    /// The key into the per-locale license documents
    /// (`shun-license-docs.json`, shared with the web shell).
    fn doc_key(self) -> &'static str {
        self.code()
    }

    /// The option label in the language itself (the autonym).
    fn autonym(self) -> &'static str {
        match self {
            FallbackLanguage::En => "English",
            FallbackLanguage::Zh => "简体中文",
        }
    }

    /// The UI text table for this language.
    fn texts(self) -> &'static Texts {
        match self {
            FallbackLanguage::En => &TEXTS_EN,
            FallbackLanguage::Zh => &TEXTS_ZH,
        }
    }

    /// Maps a saved/known locale code (the shell's i18n spelling) onto
    /// the closest table this renderer carries.
    fn from_code(code: &str) -> Self {
        if code.starts_with("zh") {
            FallbackLanguage::Zh
        } else {
            FallbackLanguage::En
        }
    }
}

struct Texts {
    banner_missing: &'static str,
    banner_manual: &'static str,
    step_language: &'static str,
    step_location: &'static str,
    step_license: &'static str,
    step_install: &'static str,
    step_done: &'static str,
    lang_heading: &'static str,
    lang_sub: &'static str,
    location_heading: &'static str,
    titlebar_installer: &'static str,
    titlebar_uninstall: &'static str,
    next: &'static str,
    back: &'static str,
    license_agree: &'static str,
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
    banner_missing: "未检测到 WebView2 运行时（缺失必要环境）—— 已自动切换至离线降级安装界面。安装功能不受影响，界面不带特效。",
    banner_manual: "已通过命令行参数 --no-webview 启用离线降级安装界面（离线版本，不带特效）。",
    step_language: "安装语言",
    step_location: "安装位置",
    step_license: "许可协议",
    step_install: "安装",
    step_done: "完成",
    lang_heading: "选择安装向导的语言",
    lang_sub: "向导的其余步骤都将以所选语言显示。",
    location_heading: "选择安装位置",
    titlebar_installer: "安装器",
    titlebar_uninstall: "卸载",
    next: "下一步",
    back: "上一步",
    license_agree: "我已阅读并同意上述全部协议文档",
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
    banner_missing: "WebView2 runtime not detected (missing required environment) — switched to the offline fallback installer. Installation is fully functional; the UI carries no effects.",
    banner_manual: "Offline fallback installer enabled manually via --no-webview (the no-effects offline version).",
    step_language: "Language",
    step_location: "Location",
    step_license: "License",
    step_install: "Install",
    step_done: "Done",
    lang_heading: "Choose the installer language",
    lang_sub: "Later steps render in it.",
    location_heading: "Choose the install location",
    titlebar_installer: "Installer",
    titlebar_uninstall: "Uninstall",
    next: "Next",
    back: "Back",
    license_agree: "I have read and agree to all documents above",
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

/// Pads one folder level under a bare filesystem root target (a picked
/// drive like `D:\`) so the payload never lands directly on the root —
/// the field and the done page show the real target (`install_context`
/// re-applies the same guard).
fn nested_dir(config: &ShunConfig, raw: &str) -> String {
    let folder = install_of(config).and_then(|i| i.root_dir_folder.as_deref());
    shun::targets::install::nest_root_dir(Path::new(raw.trim()), &config.product.name, folder)
        .to_string_lossy()
        .into_owned()
}

/// Whether the install target's desktop-shortcut policy is `ask` (the
/// wizard checkbox); `always`/`never` never consult the user.
fn desktop_policy_asks(config: &ShunConfig) -> bool {
    install_of(config)
        .map(|install| install.desktop_shortcut == shun::config::DesktopShortcutPolicy::Ask)
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
    /// The wizard language (the first-step selector); `texts` mirrors
    /// it — see [`FallbackApp::apply_language`].
    language: FallbackLanguage,
    /// Whether a CJK font registered (the Zh table is only offered when
    /// the machine can render it — the historical `zh = font_found`).
    cjk_font: bool,
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
    /// License documents resolved per locale (`shun-license-docs.json`,
    /// shared with the web shell); the license step re-picks from this
    /// map when the language changes, falling back to `steps`.
    license_docs: std::collections::BTreeMap<String, Vec<shun::config::ResolvedLicenseDoc>>,
    /// Cursor into `steps` while on `Stage::Configure`.
    step: usize,
    /// The license checkbox (`license` steps gate progression on it).
    license_accepted: bool,
    /// Which license document the pane shows (multi-document licenses
    /// page through [`shun::config::ResolvedStep::licenses`]); reset
    /// whenever the wizard moves to another step.
    license_doc_index: usize,
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
    #[allow(clippy::too_many_arguments)]
    fn new(
        config: ShunConfig,
        payload: ArchivePayload,
        reason: FallbackReason,
        language: FallbackLanguage,
        cjk_font: bool,
        receiver: Receiver<WorkerMsg>,
        logo: Option<TextureHandle>,

        license_docs: std::collections::BTreeMap<String, Vec<shun::config::ResolvedLicenseDoc>>,
    ) -> Self {
        let theme = resolve_theme(&config);
        let shell = config.shell.clone().unwrap_or_default();
        let mode = offered_modes(&config).first().copied().unwrap_or("local");
        Self {
            dir: default_dir(&config, mode).to_string_lossy().into_owned(),
            config,
            payload,
            reason,
            language,
            cjk_font,
            texts: language.texts(),
            theme,
            // The side rail is the standard look (the web face renders
            // left too); an explicit `timeline = "top"` restores the
            // horizontal strip.
            timeline_left: shell.timeline != Some(shun::config::TimelineOrientation::Top),
            logo,
            stage: Stage::Configure,
            mode,
            desktop_shortcut: true,
            machine: false,
            license_docs,
            step: 0,
            license_accepted: false,
            license_doc_index: 0,
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

    /// Switches the wizard language: the TEXTS table, the license
    /// documents and the document pager all follow, and the choice is
    /// remembered for the next run — unless the selected mode is
    /// portable, in which case nothing leaves this process.
    fn apply_language(&mut self, language: FallbackLanguage) {
        if self.language == language {
            return;
        }
        self.language = language;
        self.texts = language.texts();
        // The documents switched under the pager — restart at doc 1.
        self.license_doc_index = 0;
        // Remember the choice unless this run is portable: a portable
        // copy writes no system state, so it stays in memory only.
        let _ = crate::remember_language(
            &crate::local_appdata(),
            &self.config.product.name,
            language.code().to_string(),
            self.mode == "portable",
        );
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
        // The wizard language rides into the flow: exported to payload
        // scripts as `SHUN_LANGUAGE`, recorded in the install manifest.
        ctx.language = Some(self.language.code().to_string());
        ctx.apply_config(
            install,
            shun::targets::install::WizardAnswers {
                desktop_shortcut: self.desktop_shortcut,
                start_menu_shortcut: true,
                machine: self.machine,
                // No done-page launch toggle in the demo shell yet: the
                // answer is the default-checked one (nothing calls
                // `shun::targets::install::launch` here).
                launch_after_install: true,
            },
        );
        Ok(ctx)
    }

    /// Spawns the worker thread driving the flow. The egui context is
    /// cloned in so the worker can request repaints as events arrive.
    fn spawn_worker(&mut self, ctx: &Context, uninstalling: bool) {
        let raw = self.dir.clone();
        self.dir = nested_dir(&self.config, &raw);
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
            self.desktop_shortcut,
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
    // The same locale resolution the webview face applies to its frame
    // (saved wizard language → system locale → zh-Hans default); the
    // English literal below is the non-Windows floor.
    #[cfg(windows)]
    return crate::os_window_title(config, false);
    #[cfg(not(windows))]
    return format!("{} Installer", config.product.name);
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
/// The viewport builder carrying the product logo as the window/taskbar
/// icon — without it the egui face ships the default eframe "e".
fn icon_viewport_builder(logo_kind: &str, logo_bytes: &[u8]) -> egui::ViewportBuilder {
    if logo_kind == "none" || logo_bytes.is_empty() {
        return egui::ViewportBuilder::default();
    }
    let icon = image::load_from_memory(logo_bytes).ok().map(|img| {
        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();
        egui::IconData {
            width,
            height,
            rgba: rgba.into_raw(),
        }
    });
    match icon {
        Some(icon) => egui::ViewportBuilder::default().with_icon(icon),
        None => egui::ViewportBuilder::default(),
    }
}

pub fn run(
    config: ShunConfig,
    payload: ArchivePayload,
    reason: FallbackReason,
    logo_kind: &str,
    logo_bytes: &[u8],
    license_docs: std::collections::BTreeMap<String, Vec<shun::config::ResolvedLicenseDoc>>,
) {
    let title = window_title(&config);
    // Frameless like the hikari shell: the title bar below draws the
    // custom chrome (logo + caption + drag + close). This also keeps the
    // `--screenshot` capture aligned with the client area.
    let options = eframe::NativeOptions {
        viewport: icon_viewport_builder(logo_kind, logo_bytes)
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
            // The installer paints its own dark token set — following the
            // system theme would re-apply light visuals after the
            // set_visuals below, leaving native widgets (the path
            // TextEdit) white with light text on the dark pane.
            cc.egui_ctx.set_theme(egui::Theme::Dark);
            cc.egui_ctx
                .options_mut(|o| o.theme_preference = egui::ThemePreference::Dark);
            let mut visuals = egui::Visuals::dark();
            visuals.panel_fill = theme.background;
            visuals.window_fill = theme.background;
            visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0f32, theme.border);
            cc.egui_ctx.set_visuals(visuals);
            // A CJK font is the egui renderer's proxy for "the machine
            // can render the Zh table" (the historical `zh = font_found`).
            let font_found = install_cjk_font(&cc.egui_ctx);
            // Language: the remembered preference wins (the same
            // `installer-prefs.json` the web shell writes), then
            // `shell.language`, then what the machine can render.
            let language = crate::load_prefs(&crate::local_appdata(), &config.product.name)
                .language
                .as_deref()
                .map(FallbackLanguage::from_code)
                .or_else(|| {
                    config
                        .shell
                        .as_ref()
                        .and_then(|s| s.language.as_deref())
                        .filter(|pin| *pin != "auto")
                        .map(FallbackLanguage::from_code)
                })
                .unwrap_or(if font_found {
                    FallbackLanguage::Zh
                } else {
                    FallbackLanguage::En
                });
            let logo = load_logo(&cc.egui_ctx, logo_kind, logo_bytes);
            let (_sender, receiver) = channel::<WorkerMsg>();
            Ok(Box::new(FallbackApp::new(
                config,
                payload,
                reason,
                language,
                font_found,
                receiver,
                logo,
                license_docs,
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
        let theme = self.theme;
        let texts = self.texts;
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            let bar_height = 24.0;
            if let Some(logo) = &self.logo {
                ui.add(egui::Image::from_texture(logo).fit_to_exact_size(Vec2::splat(20.0)));
            }
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!(
                    "{} {}",
                    self.config.product.name,
                    if self.uninstalling == Some(true) {
                        texts.titlebar_uninstall
                    } else {
                        texts.titlebar_installer
                    }
                ))
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

    /// The rail: the FIXED five wizard steps — language, location,
    /// license, install, done — the same labels the webview face shows.
    /// Horizontal under the caption (explicit `timeline = "top"`) or
    /// vertical on the left (the default).
    fn timeline(&self, ui: &mut egui::Ui, vertical: bool) {
        let theme = &self.theme;
        // A failed run sends the user back to configure.
        let current = match (&self.stage, self.outcome.as_ref()) {
            (Stage::Finished, Some(Outcome::Failed(_))) => Stage::Configure,
            (stage, _) => *stage,
        };
        let labels = [
            self.texts.step_language.to_owned(),
            self.texts.step_location.to_owned(),
            self.texts.step_license.to_owned(),
            self.texts.step_install.to_owned(),
            self.texts.step_done.to_owned(),
        ];
        let active_marker = match current {
            Stage::Configure => Some(self.step.min(2)),
            _ => None,
        };
        let order_current = match current {
            Stage::Configure => self.step.min(2),
            Stage::Running => 3,
            Stage::Finished => 4,
        };
        let items: Vec<(bool, bool, String)> = labels
            .iter()
            .enumerate()
            .map(|(index, label)| {
                (
                    active_marker == Some(index),
                    index < order_current,
                    label.clone(),
                )
            })
            .collect();
        let rail: Box<dyn FnOnce(&mut egui::Ui)> = Box::new(|ui| {
            let items = items.clone();
            // One rail item: marker + label. The loop lives with the
            // caller so the vertical branch controls row geometry.
            let paint = |ui: &mut egui::Ui, index: usize| {
                let (active, done, label) = &items[index];
                if !vertical && index > 0 {
                    let line_color = if *done { theme.success } else { theme.border };
                    ui.label(RichText::new("——").color(line_color).small());
                    ui.add_space(6.0);
                }
                let (marker, marker_color, text_color) = if *active {
                    ("●", theme.primary, theme.text)
                } else if *done {
                    ("●", theme.success, theme.text_secondary)
                } else {
                    ("○", theme.text_tertiary, theme.text_tertiary)
                };
                ui.horizontal(|ui| {
                    ui.label(RichText::new(marker).color(marker_color).size(15.0));
                    ui.label(RichText::new(label.as_str()).color(text_color).size(14.5));
                });
                if !vertical {
                    ui.add_space(0.0);
                }
            };
            if vertical {
                // Deterministic centering, both axes: fixed row metrics
                // give the vertical offset (egui's main-align Center
                // cannot place a sized-to-content block in one pass), and
                // the block — all rows spanning one width, so the marker
                // axis stays aligned — centers horizontally as a unit.
                // The connector is a PAINTED hairline on that axis: a
                // text glyph would wander with the label widths.
                ui.spacing_mut().item_spacing.y = 0.0;
                let row_height = 30.0f32;
                let connector_slot = 38.0f32;
                let n = items.len() as f32;
                let total = n * row_height + (n - 1.0).max(0.0) * connector_slot;
                ui.add_space(((ui.available_height() - total) / 2.0).max(0.0));
                // Horizontal: the block width is the widest label plus
                // the marker column; a computed left offset centers it —
                // egui's cross-align nests unreliably through the pane's
                // fixed-width block.
                let font = egui::FontId::proportional(14.5);
                let label_w = items
                    .iter()
                    .map(|(_, _, label)| {
                        ui.painter()
                            .layout_no_wrap(label.clone(), font.clone(), theme.text)
                            .size()
                            .x
                    })
                    .fold(0.0f32, f32::max);
                let marker_w = 16.0f32;
                let block_w = marker_w + 8.0 + label_w;
                let left_offset = ((ui.available_width() - block_w) / 2.0).max(0.0);
                for (index, _) in items.iter().enumerate() {
                    if index > 0 {
                        // Connector slot: the hairline rides the marker
                        // axis, spanning the slot.
                        let slot_left = ui.cursor().left() + left_offset + marker_w / 2.0;
                        let slot_top = ui.cursor().top() + 4.0;
                        ui.add_space(connector_slot);
                        ui.painter().line_segment(
                            [
                                pos2(slot_left, slot_top),
                                pos2(slot_left, slot_top + connector_slot - 8.0),
                            ],
                            Stroke::new(1.5f32, theme.border),
                        );
                    }
                    let row = ui.available_width();
                    ui.allocate_ui_with_layout(
                        egui::vec2(row, row_height),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.add_space(left_offset);
                            paint(ui, index);
                        },
                    );
                }
            } else {
                for (index, _) in items.iter().enumerate() {
                    paint(ui, index);
                }
            }
        });
        rail(ui);
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

    /// The configure pane: dispatches on the fixed wizard steps —
    /// language → location → license — the same progression the webview
    /// face renders.
    fn configure_view(&mut self, ui: &mut egui::Ui) {
        match self.step {
            0 => self.language_view(ui),
            1 => self.location_view(ui),
            _ => self.license_view(ui),
        }
    }

    /// The language pane — the wizard's opening step, mirroring the
    /// webview face: heading, sub line, and the picker (the Zh entry is
    /// only offered when the machine registered a CJK font).
    fn language_view(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        let texts = self.texts;
        ui.add_space(4.0);
        ui.label(
            RichText::new(texts.lang_heading)
                .strong()
                .size(22.0)
                .color(theme.text),
        );
        ui.add_space(6.0);
        ui.label(
            RichText::new(texts.lang_sub)
                .size(13.0)
                .color(theme.text_secondary),
        );
        ui.add_space(16.0);

        let current = self.language;
        let zh_offered = self.cjk_font;
        let mut picked: Option<FallbackLanguage> = None;
        // The picker stands alone and centered, matching the webview
        // face's language step.
        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
            egui::ComboBox::from_id_salt("wizard-language")
                .selected_text(current.autonym())
                .width(280.0)
                .show_ui(ui, |ui| {
                    let candidates = [
                        (FallbackLanguage::En, true),
                        (FallbackLanguage::Zh, zh_offered),
                    ];
                    for (language, offered) in candidates {
                        let response = ui.add_enabled(
                            offered,
                            egui::Button::selectable(language == current, language.autonym()),
                        );
                        if response.clicked() {
                            picked = Some(language);
                        }
                    }
                });
        });
        if let Some(language) = picked {
            self.apply_language(language);
        }
    }

    /// The location pane — product hero + the install-directory row
    /// (badge, field, browse) and its hint. The mode cards are gone:
    /// these products install per-user locally, exactly what the
    /// webview face shows.
    fn location_view(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        let texts = self.texts;

        ui.add_space(4.0);
        ui.label(
            RichText::new(texts.location_heading)
                .strong()
                .size(22.0)
                .color(theme.text),
        );
        ui.add_space(16.0);

        ui.label(
            RichText::new(texts.dir_label)
                .size(13.0)
                .color(theme.text_secondary),
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            folder_badge(ui, &theme, 28.0);
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
                    self.dir = nested_dir(&self.config, &picked.to_string_lossy());
                }
            }
        });
        ui.add_space(6.0);
        ui.label(
            RichText::new(self.hint())
                .size(12.0)
                .color(theme.text_tertiary),
        );
    }

    /// The license pane: the agreement documents (paged when several
    /// resolve) with an accept checkbox gating all of them.
    fn license_view(&mut self, ui: &mut egui::Ui) {
        let theme = &self.theme;
        let texts = self.texts;
        // Snapshot the documents so the ui closures below can mutate
        // wizard state freely (the step borrow would otherwise span the
        // checkbox and pager). The documents follow the wizard language:
        // the per-locale map (shun-license-docs.json) first, then the
        // locale-less pipeline resolution as the fallback.
        let docs: Vec<(Option<String>, String)> = {
            let localized = self
                .license_docs
                .get(self.language.doc_key())
                .filter(|docs| !docs.is_empty());
            match localized {
                Some(docs) => docs
                    .iter()
                    .map(|doc| (doc.title.clone(), doc.body.clone()))
                    .collect(),
                None => {
                    // Back-compat: the locale-less pipeline's license step.
                    let pipeline: Vec<shun::config::ResolvedStep> = serde_json::from_str(
                        include_str!(concat!(env!("OUT_DIR"), "/shun-steps.json")),
                    )
                    .unwrap_or_default();
                    let step = pipeline
                        .iter()
                        .find(|s| s.kind == shun::config::StepKind::License);
                    let licenses = step.map(|s| s.licenses.as_slice()).unwrap_or(&[]);
                    if licenses.is_empty() {
                        // Legacy single-string body: one untitled document.
                        step.and_then(|s| s.body.clone())
                            .map(|body| vec![(None, body)])
                            .unwrap_or_default()
                    } else {
                        licenses
                            .iter()
                            .map(|doc| (doc.title.clone(), doc.body.clone()))
                            .collect()
                    }
                }
            }
        };
        let total = docs.len();
        let index = if total == 0 {
            0
        } else {
            self.license_doc_index.min(total - 1)
        };
        let (title, body) = docs
            .get(index)
            .map(|(title, body)| (title.as_deref(), body.as_str()))
            .unwrap_or((None, ""));

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
                        if let Some(title) = title {
                            ui.label(RichText::new(title).strong().size(15.0).color(theme.text));
                            ui.add_space(6.0);
                        }
                        ui.label(RichText::new(body).size(12.5).color(theme.text_secondary));
                    });
            });
        // Pager for multi-document licenses: [<] left, the position
        // indicator centered between, [>] right (disabled at the ends).
        if total > 1 {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let pager_button = |ui: &mut egui::Ui, glyph: &str, enabled: bool| {
                    ui.add_enabled(
                        enabled,
                        Button::new(RichText::new(glyph).size(13.0).color(theme.text_secondary))
                            .fill(theme.surface)
                            .stroke(Stroke::new(1.0f32, theme.border))
                            .corner_radius(CornerRadius::same(6))
                            .min_size(Vec2::new(44.0, 24.0)),
                    )
                };
                if pager_button(ui, "[<]", index > 0).clicked() {
                    self.license_doc_index = index - 1;
                }
                let indicator = format!(
                    "{}/{} {}",
                    index + 1,
                    total,
                    title.unwrap_or(texts.step_license)
                );
                let galley = ui.painter().layout_no_wrap(
                    indicator.clone(),
                    egui::FontId::proportional(12.5),
                    theme.text_secondary,
                );
                // Reserve the free space around the indicator (minus the
                // right button's share) so it sits centered between the
                // two pager buttons.
                let pad = ((ui.available_width() - galley.size().x - 44.0) / 2.0).max(0.0);
                ui.add_space(pad);
                ui.label(
                    RichText::new(indicator)
                        .size(12.5)
                        .color(theme.text_secondary),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if pager_button(ui, "[>]", index + 1 < total).clicked() {
                        self.license_doc_index = index + 1;
                    }
                });
            });
        }
        ui.add_space(8.0);
        ui.checkbox(&mut self.license_accepted, self.texts.license_agree);
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
        let theme = self.theme;
        let texts = self.texts;
        let desktop_asks = desktop_policy_asks(&self.config);
        ui.add_space(8.0);
        // Clone the outcome out of self: the pane below mutates the
        // desktop-shortcut answer while painting.
        let outcome = self.outcome.clone();
        match outcome.as_ref().expect("Finished implies an outcome") {
            Outcome::InstallOk | Outcome::UninstallOk => {
                let (title, path) = match outcome.as_ref().expect("checked above") {
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
                    // The done-page shortcut answer — the flow created
                    // none (pinned "never"); the finish button applies it.
                    if desktop_asks {
                        ui.add_space(10.0);
                        ui.checkbox(&mut self.desktop_shortcut, texts.desktop_shortcut);
                    }
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
                // The license step (2) carries the install button;
                // language and location walk the pipeline.
                let on_last_step = self.step >= 2;
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
                                    // Re-entering the wizard restarts
                                    // the license pager at doc 1.
                                    self.license_doc_index = 0;
                                }
                                _ => {
                                    // The done page owns the shortcut
                                    // answer (the manifest pins both
                                    // launcher policies to "never", so
                                    // the flow created none): apply it,
                                    // then close.
                                    let install =
                                        self.config.targets.iter().find_map(|t| match t {
                                            shun::config::TargetConfig::Install(install) => {
                                                Some(install.clone())
                                            }
                                            _ => None,
                                        });
                                    if let Some(install) = install {
                                        let main_exe = install
                                            .main_exe
                                            .clone()
                                            .unwrap_or_else(|| {
                                                std::path::PathBuf::from(format!(
                                                    "{}.exe",
                                                    self.config.product.name
                                                ))
                                            })
                                            .to_string_lossy()
                                            .into_owned();
                                        let aumid = install.aumid.clone().unwrap_or_else(|| {
                                            shun::targets::install::default_aumid(
                                                self.config.product.publisher.as_deref(),
                                                &self.config.product.name,
                                            )
                                        });
                                        let _ = shun::targets::shortcuts::apply_shortcut_choices(
                                            &aumid,
                                            &main_exe,
                                            Some(self.desktop_shortcut),
                                            Some(true),
                                            self.dir.trim(),
                                        );
                                    }
                                    std::process::exit(0);
                                }
                            },
                            Stage::Configure if !on_last_step => {
                                // Moving to another step restarts the
                                // license pager at its first document.
                                self.license_doc_index = 0;
                                self.step += 1;
                            }
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
                        self.license_doc_index = 0;
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
        // Step 2 is the license: progression gates on the accept box.
        self.step >= 2 && !self.license_accepted
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
                .exact_width(200.0)
                .frame(
                    Frame::default()
                        .fill(theme.surface)
                        // The brightness split: a hairline against the
                        // pane plus the surface/background tone step.
                        .stroke(Stroke::new(1.0f32, theme.border))
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
                let align = shun::config::StepAlign::Center;
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
