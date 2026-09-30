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

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

use egui::{
    Align, Button, Color32, Context, CornerRadius, FontDefinitions, FontFamily, Frame,
    Layout, Margin, RichText, Sense, Stroke, TextEdit, TextureHandle, Vec2, pos2, vec2,
};
use shun::config::{ShunConfig, TargetConfig};
use shun::flow::{FlowEvent, FlowPhase};
use shun::payload::ArchivePayload;
use serde::Deserialize;
use shun::wizard::{InstallRequest, WizardCore};
use crate::SHUN_FLAVOR;

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

/// One configure pane of the wizard's flow, built from the resolved
/// pipeline: custom content steps slot in at their declaration position
/// relative to the license (mode/scope fold into the location pane —
/// the scope pane is a known gap).
#[derive(Clone)]
enum Page {
    Language,
    Location,
    Content { title: String, body: String },
    License,
}

impl Page {
    fn is_license(&self) -> bool {
        matches!(self, Page::License)
    }
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
    /// The manifest's terminal-pane background (a solid or a gradient's
    /// midpoint), when the theme declares one.
    pub(crate) terminal_bg_override: Option<Color32>,
    /// The rail layer's flat fill (`shell.theme.rail-background`) — a
    /// gradient mixes to its midpoint.
    pub(crate) rail_bg_override: Option<Color32>,
    /// The content pane's flat fill (`shell.theme.pane-background`).
    pub(crate) pane_bg_override: Option<Color32>,
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
            // The web face's theme.scss tokens are the configured
            // default — blue in BOTH modes (its dark block only flips
            // text/border/surfaces). hikari's own no-config fallback
            // (the default theme's daytime pink) only exists where no
            // tokens ship at all, which never happens here.
            primary: accent.map_or(Color32::from_rgb(0, 120, 200), |[r, g, b]| {
                Color32::from_rgb(r, g, b)
            }),
            on_primary: Color32::from_white_alpha(235),
            success: Color32::from_rgb(60, 180, 120),
            error: Color32::from_rgb(220, 80, 80),
            warning: Color32::from_rgb(230, 170, 50),
            terminal_bg_override: None,
            rail_bg_override: None,
            pane_bg_override: None,
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
            // Mirror the web face's light token: --color-primary 0 120 200.
            primary: accent.map_or(Color32::from_rgb(0, 120, 200), |[r, g, b]| {
                Color32::from_rgb(r, g, b)
            }),
            on_primary: Color32::from_rgb(255, 255, 255),
            success: Color32::from_rgb(60, 180, 120),
            error: Color32::from_rgb(220, 80, 80),
            warning: Color32::from_rgb(190, 140, 30),
            terminal_bg_override: None,
            rail_bg_override: None,
            pane_bg_override: None,
        }
    }

    /// Terminal pane background (a shade between page and surface).
    pub(crate) fn terminal_bg(&self) -> Color32 {
        if let Some(override_bg) = self.terminal_bg_override {
            return override_bg;
        }
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

/// The machine's app-theme preference: Windows 11 personalization's
/// `AppsUseLightTheme` (1 = light). Any failure to read it — older
/// Windows, a stripped registry — resolves LIGHT (the user direction's
/// floor), and so does every non-Windows platform.
#[cfg(windows)]
fn os_prefers_light() -> bool {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.open_subkey_with_flags(
        r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
        KEY_READ,
    )
    .and_then(|key| key.get_value::<u32, _>("AppsUseLightTheme"))
    .map(|value| value != 0)
    .unwrap_or(true)
}

fn resolve_theme(config: &ShunConfig) -> Theme {
    let shell = config.shell.clone().unwrap_or_default();
    let accent = shell.theme.as_ref().and_then(|theme| theme.accent);
    // A manifest background (solid or gradient) tints the base tokens:
    // egui has no gradient fill, so gradients mix to their midpoint —
    // honest approximation, zero extra infrastructure.
    // Same semantics as hikari's useTheme: an unset mode defaults to
    // dark — the installer ships dark — `system` resolves from the SUN
    // (the solar clock refines it as fixes land and time passes), and
    // explicit light/dark pin.
    let mut theme = match shell.theme.as_ref().and_then(|theme| theme.mode) {
        Some(shun::config::ThemeMode::Light) => Theme::light(accent),
        Some(shun::config::ThemeMode::Dark) => Theme::dark(accent),
        // `system` (and unset): follow the MACHINE's app theme —
        // Windows 11's personalization light/dark — not the sun.
        // Undetectable resolves light (user direction).
        Some(shun::config::ThemeMode::System) | None => {
            if os_prefers_light() {
                Theme::light(accent)
            } else {
                Theme::dark(accent)
            }
        }
    };
    // The manifest's background tints the tokens: solid applies as-is, a
    // gradient mixes to its midpoint (egui paints flat fills). Panes and
    // the rail then derive from it, so the whole face shifts with the
    // manifest's theme.
    apply_theme_layers(&mut theme, &shell);
    theme
}

/// Applies the manifest's background layers onto a base token set —
/// shared by the startup resolution AND every mode flip (a flip rebuilds
/// the base tokens and must re-tint, or the manifest background would
/// silently vanish the first time the sun or the user changes sides).
/// Wallpapers have no egui renderer — the face stays on the token
/// background (the banner already says "no effects").
fn apply_theme_layers(theme: &mut Theme, shell: &shun::config::ShellUiConfig) {
    match shell.theme.as_ref().and_then(|t| t.background.as_ref()) {
        Some(shun::config::BackgroundSpec::Color(color)) => {
            if let Some(rgba) = css_color_to32(color) {
                theme.background = rgba;
                theme.surface = mix(rgba, theme.text, 0.06);
                theme.terminal_bg_override = Some(mix(rgba, theme.text, 0.1));
            }
        }
        Some(spec @ shun::config::BackgroundSpec::Gradient { .. }) => {
            if let Some(mid) = gradient_midpoint(spec) {
                theme.background = mid;
                theme.surface = mix(mid, theme.text, 0.06);
                theme.terminal_bg_override = Some(mix(mid, theme.text, 0.1));
            }
        }
        _ => {}
    }
    // The rail/pane layers: a configured color (or a gradient's
    // midpoint) becomes the layer's flat fill — egui's honest
    // approximation of the web face's per-layer CSS.
    let flat = |spec: Option<&shun::config::BackgroundSpec>| -> Option<Color32> {
        match spec {
            Some(shun::config::BackgroundSpec::Color(color)) => css_color_to32(color),
            Some(spec @ shun::config::BackgroundSpec::Gradient { .. }) => gradient_midpoint(spec),
            _ => None,
        }
    };
    theme.rail_bg_override = flat(shell.theme.as_ref().and_then(|t| t.rail_background.as_ref()));
    theme.pane_bg_override = flat(shell.theme.as_ref().and_then(|t| t.pane_background.as_ref()));
}

/// Parses the CSS color shapes the theme accepts for egui fills:
/// `#rgb` / `#rrggbb` hex. `None` for anything else (the web face can
/// render more; egui approximates rather than failing the theme).
fn css_color_to32(color: &str) -> Option<Color32> {
    let hex = color.strip_prefix('#')?;
    let (r, g, b) = match hex.len() {
        3 => (
            u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?,
            u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?,
            u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?,
        ),
        6 => (
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
        ),
        _ => return None,
    };
    Some(Color32::from_rgb(r, g, b))
}

/// A gradient's midpoint color (from/to may be hex or anything parseable
/// by [`css_color_to32`]; unparsable sides keep the base token).
fn gradient_midpoint(spec: &shun::config::BackgroundSpec) -> Option<Color32> {
    let shun::config::BackgroundSpec::Gradient { from, to, .. } = spec else {
        return None;
    };
    let a = css_color_to32(from)?;
    let b = css_color_to32(to)?;
    Some(Color32::from_rgb(
        (a.r() + b.r()) / 2,
        (a.g() + b.g()) / 2,
        (a.b() + b.b()) / 2,
    ))
}

// ── UI copy: the i18n strings of the web shell (shell/web/src/i18n.ts) ──

/// The language the fallback UI renders in — the egui side carries the
/// two TEXTS tables below, so its first-step language selector offers
/// these two locales (the web shell offers all eight). The chosen value
/// is what reaches the install context (and from there `SHUN_LANGUAGE`
/// for scripts and the install manifest).




/// One locale's wizard copy — deserialized straight from
/// shell/strings/wizard-strings.json, the SINGLE authored source both
/// faces render from (the web i18n exports it via `pnpm dump-strings`).
// serde(default): a JSON key missing from a locale degrades to an
// empty string instead of panicking the face at startup.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default)]
struct Texts {
    banner_missing: String,
    banner_manual: String,
    step_language: String,
    step_location: String,
    step_license: String,
    step_install: String,
    step_done: String,
    lang_heading: String,
    lang_sub: String,
    location_heading: String,
    titlebar_installer: String,
    titlebar_uninstall: String,
    next: String,
    back: String,
    license_agree: String,
    license_title: String,
    license_sub: String,
    agree_install: String,
    agree_install_wait: String,
    dir_label: String,
    browse: String,
    browse_title: String,
    quick_title: String,
    dir_empty: String,
    desktop_shortcut: String,
    menu_shortcut: String,
    launch_after: String,
    hint_local: String,
    hint_portable: String,
    location_sub: String,
    target_hint_local: String,
    attach_title: String,
    attach_bundled: String,
    flash_hint: String,
    warn_unwritable: String,
    warn_no_writable: String,
    flavor_full: String,
    flavor_full_webview2: String,
    kind_removable: String,
    kind_fixed: String,
    kind_network: String,
    kind_cdrom: String,
    kind_ramdisk: String,
    kind_unknown: String,
    install: String,
    uninstall: String,
    installing: String,
    uninstalling: String,
    done_title: String,
    done_uninstall: String,
    failed: String,
    open_dir: String,
    finish: String,
    retry: String,
    // The standalone uninstaller page (web `uninstall` block): confirm →
    // running → done/failed with the repair variant.
    un_heading: String,
    un_sub: String,
    un_cancel: String,
    un_repair: String,
    un_close: String,
    un_repairing: String,
    done_repair: String,
    failed_uninstall: String,
    failed_repair: String,
    log: String,
    log_expand: String,
    log_collapse: String,
    log_write: String,
    log_reuse: String,
    script_begin: String,
    installing_percent: String,
    warn_desktop_blocked: String,
    warn_aumid_blocked: String,
}



/// The single authored string source, shared with the web face:
/// `shell/web` exports it (`pnpm dump-strings`) and both faces render
/// from it. Parsed once, on first touch.
fn wizard_strings() -> &'static WizardStrings {
    static WIZARD_STRINGS: std::sync::OnceLock<WizardStrings> = std::sync::OnceLock::new();
    WIZARD_STRINGS.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../strings/wizard-strings.json"
        ))
        .expect("wizard-strings.json parses")
    })
}

#[derive(Debug, Deserialize)]
struct WizardStrings {
    locales: Vec<String>,
    labels: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    default: String,
    strings: std::collections::BTreeMap<String, Texts>,
}

impl WizardStrings {
    /// The picker's entries: every authored locale, named in its own
    /// script (the web face's LOCALE_OPTIONS).
    fn options(&self) -> Vec<(String, String)> {
        self.locales
            .iter()
            .map(|code| {
                (
                    code.clone(),
                    self.labels.get(code).cloned().unwrap_or_else(|| code.clone()),
                )
            })
            .collect()
    }

    fn texts(&self, locale: &str) -> Texts {
        let fallback = if self.default.is_empty() {
            "zh-Hans".to_string()
        } else {
            self.default.clone()
        };
        self.strings
            .get(locale)
            .cloned()
            .unwrap_or_else(|| self.strings.get(&fallback).cloned().unwrap_or_default())
    }

    fn is_known(&self, locale: &str) -> bool {
        self.locales.iter().any(|l| l == locale)
    }

    /// The web face's resolveSystemLocale: zh-TW/HK/Hant → zh-Hant,
    /// other zh* → zh-Hans, prefix matches for the rest, else en.
    fn resolve_system_locale(&self, tag: &str) -> String {
        let lower = tag.to_lowercase();
        if lower.starts_with("zh") {
            if lower.starts_with("zh-tw")
                || lower.starts_with("zh-hk")
                || lower.starts_with("zh-hant")
            {
                return "zh-Hant".into();
            }
            return "zh-Hans".into();
        }
        for prefix in ["ru", "ja", "ko", "fr", "es", "de", "pt"] {
            if lower.starts_with(prefix) {
                return prefix.into();
            }
        }
        "en".into()
    }
}

/// The OS UI language tag (BCP-47), the egui equivalent of the web
/// face's navigator.language for locale resolution.
#[cfg(windows)]
fn system_locale_tag() -> Option<String> {
    use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;
    let mut buf = [0u16; 85]; // LOCALE_NAME_MAX_LENGTH
    let len = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
    if len > 0 {
        String::from_utf16(&buf[..(len - 1) as usize]).ok()
    } else {
        None
    }
}

#[cfg(not(windows))]
fn system_locale_tag() -> Option<String> {
    None
}


/// Registers a system CJK font as a glyph fallback so the Chinese UI
/// renders. Returns `false` when none is found (the UI falls back to
/// English strings). Only single-file `.ttf` fonts are probed — egui
/// cannot index `.ttc` collections.
/// Loads the SAME families the web face's font stack names, from the
/// OS: Segoe UI for Latin (the stack's first Windows-resident face),
/// Microsoft YaHei for CJK (the browser's zh fallback), Consolas for
/// the mono family. Glyph fallback then works like the browser's:
/// Latin renders in Segoe, Han glyphs fall through to YaHei. egui has
/// no weight selection yet (strong() keeps the regular face — no
/// worse than the bundled fonts), and FontData's face index picks the
/// right name out of the msyh TTC collection. Returns whether a
/// CJK-capable font landed: the Chinese UI is only offered when it
/// did. Anything missing falls through to the legacy SimHei probe and
/// finally egui's bundled fonts.
fn install_system_fonts(ctx: &Context) -> bool {
    let mut fonts = FontDefinitions::default();

    // (path, face index, registry name) — order matters: the family
    // list is a glyph-fallback chain, so Latin must precede CJK.
    const FACES: [(&str, u32, &str); 3] = [
        (r"C:\Windows\Fonts\segoeui.ttf", 0, "segoe-ui"),
        (r"C:\Windows\Fonts\msyh.ttc", 0, "ms-yahei"),
        (r"C:\Windows\Fonts\consola.ttf", 0, "consolas"),
    ];
    let mut loaded = std::collections::BTreeSet::new();
    for (path, index, name) in FACES {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        fonts.font_data.insert(
            name.to_string(),
            egui::FontData {
                font: bytes.into(),
                index,
                tweak: Default::default(),
            }
            .into(),
        );
        loaded.insert(name);
    }

    // Family chains: [Segoe UI, Microsoft YaHei, ..egui defaults] for
    // proportional text, [Consolas, Microsoft YaHei, ..] for mono —
    // the browser's fallback dance, one layer at a time.
    let chain = |fonts: &mut FontDefinitions, family: FontFamily, names: &[&str]| {
        let list = fonts.families.entry(family).or_default();
        for (position, name) in names.iter().enumerate() {
            if loaded.contains(name) {
                list.insert(position, name.to_string());
            }
        }
    };
    chain(
        &mut fonts,
        FontFamily::Proportional,
        &["segoe-ui", "ms-yahei"],
    );
    chain(&mut fonts, FontFamily::Monospace, &["consolas", "ms-yahei"]);
    let system_cjk = loaded.contains("ms-yahei");

    if system_cjk {
        ctx.set_fonts(fonts);
        return true;
    }

    // Legacy probe: no Segoe/YaHei (stripped-down Windows or another
    // OS) — the old SimHei-family CJK fallback still applies, so zh
    // stays offerable when any CJK face exists.
    for path in [
        r"C:\Windows\Fonts\simhei.ttf",
        r"C:\Windows\Fonts\deng.ttf",
        r"C:\Windows\Fonts\simfang.ttf",
        r"C:\Windows\Fonts\simkai.ttf",
    ] {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        fonts
            .font_data
            .insert("cjk".into(), egui::FontData::from_owned(bytes).into());
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .push("cjk".into());
        }
        ctx.set_fonts(fonts);
        return true;
    }
    false
}

/// The embedded product logo, decoded to an egui texture (like the
/// hikari title bar's icon prop). `None` when the manifest declares no
/// logo or the bytes do not decode.
// ── Lucide caption icons — the webview face's exact icon set ────────────
//
// HkTitleBar embeds lucide SVGs inline (lucide-vue-next at 14px,
// stroke-width 1.75, round caps). The egui face rasterizes the SAME
// path data (resvg) once at startup and tints the textures per state,
// so both faces draw pixel-identical glyphs from one source.

/// The lucide glyph bodies, verbatim from HkTitleBar.tsx / the lucide
/// set: minus, x, sun, moon.
mod lucide {
    pub(crate) const MINUS: &str = r#"<path d="M5 12h14"/>"#;
    pub(crate) const X: &str = r#"<path d="M6 6l12 12M18 6L6 18"/>"#;
    pub(crate) const SUN: &str = r#"<circle cx="12" cy="12" r="4"/><path d="M12 2v2"/><path d="M12 20v2"/><path d="m4.93 4.93 1.41 1.41"/><path d="m17.66 17.66 1.41 1.41"/><path d="M2 12h2"/><path d="M20 12h2"/><path d="m6.34 17.66-1.41 1.41"/><path d="m19.07 4.93-1.41 1.41"/>"#;
    pub(crate) const MOON: &str = r#"<path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"/>"#;
    pub(crate) const CHEVRON_DOWN: &str = r#"<path d="m6 9 6 6 6-6"/>"#;
    pub(crate) const FOLDER_OPEN: &str = r#"<path d="m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.54 6a2 2 0 0 1-1.95 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2"/>"#;
    pub(crate) const APP_WINDOW: &str = r#"<rect x="2" y="4" width="20" height="16" rx="2"/><path d="M10 4v4"/><path d="M2 8h20"/><path d="M6 4v4"/>"#;
    pub(crate) const HARD_DRIVE: &str = r#"<line x1="22" x2="2" y1="12" y2="12"/><path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z"/><line x1="6" x2="6.01" y1="16" y2="16"/><line x1="10" x2="10.01" y1="16" y2="16"/>"#;
    pub(crate) const ALERT_TRIANGLE: &str = r#"<path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 20h16a2 2 0 0 0 1.73-1"/><path d="M12 9v4"/><path d="M12 17h.01"/>"#;
}

/// The caption's four glyph textures (white strokes — tinted per state
/// at draw time, like `stroke="currentColor"`).
#[derive(Clone)]
struct CaptionIcons {
    minus: TextureHandle,
    x: TextureHandle,
    sun: TextureHandle,
    moon: TextureHandle,
    /// The select trigger's dropdown arrow (HkSelect's ChevronDown).
    chevron: TextureHandle,
    folder: TextureHandle,
    app_window: TextureHandle,
    hard_drive: TextureHandle,
    alert: TextureHandle,
}

impl CaptionIcons {
    fn load(ctx: &Context) -> Self {
        let render = |name: &str, body: &str| {
            let svg = format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" \
                 viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"#ffffff\" stroke-width=\"1.75\" \
                 stroke-linecap=\"round\" stroke-linejoin=\"round\">{body}</svg>"
            );
            // 28px raster drawn at 14 logical pt — crisp at 2× DPI.
            let image = render_svg(&svg, 28.0)
                .unwrap_or_else(|| panic!("lucide icon {name} must rasterize"));
            ctx.load_texture(format!("lucide-{name}"), image, egui::TextureOptions::LINEAR)
        };
        Self {
            minus: render("minus", lucide::MINUS),
            x: render("x", lucide::X),
            sun: render("sun", lucide::SUN),
            moon: render("moon", lucide::MOON),
            chevron: render("chevron-down", lucide::CHEVRON_DOWN),
            folder: render("folder-open", lucide::FOLDER_OPEN),
            app_window: render("app-window", lucide::APP_WINDOW),
            hard_drive: render("hard-drive", lucide::HARD_DRIVE),
            alert: render("alert-triangle", lucide::ALERT_TRIANGLE),
        }
    }
}

/// Rasterizes an SVG string at `px` square into an egui image.
fn render_svg(svg: &str, px: f32) -> Option<egui::ColorImage> {
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_str(svg, &opt).ok()?;
    let size = tree.size();
    let transform = resvg::tiny_skia::Transform::from_scale(
        px / size.width(),
        px / size.height(),
    );
    let mut pixmap = resvg::tiny_skia::Pixmap::new(px as u32, px as u32)?;
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    // tiny-skia stores premultiplied alpha; egui wants it straight.
    let pixels: Vec<u8> = pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [px as usize, px as usize],
        &pixels,
    ))
}

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

/// The DriveKind → i18n key mapping, identical to the web face's
/// list_drives serialization.
fn drive_kind_name(kind: shun::fs_probe::DriveKind) -> String {
    use shun::fs_probe::DriveKind;
    match kind {
        DriveKind::Removable => "removable",
        DriveKind::Fixed => "fixed",
        DriveKind::Network => "network",
        DriveKind::CdRom => "cdrom",
        DriveKind::RamDisk => "ramdisk",
        DriveKind::Unknown => "unknown",
    }
    .to_string()
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
/// The configure flow from the resolved pipeline (`shun-steps.json`):
/// language + location, then the pipeline's content steps and the
/// license at their declaration positions (license always present —
/// the pipeline resolves one even for license-less configs, an empty
/// documents list then renders an empty agreement pane).
fn wizard_pages() -> Vec<Page> {
    let pipeline: Vec<shun::config::ResolvedStep> =
        serde_json::from_str(include_str!(concat!(env!("OUT_DIR"), "/shun-steps.json")))
            .unwrap_or_default();
    let mut pages = vec![Page::Language, Page::Location];
    let mut license = false;
    for step in &pipeline {
        match step.kind {
            shun::config::StepKind::Content => pages.push(Page::Content {
                title: step.title.clone(),
                body: step.body.clone().unwrap_or_default(),
            }),
            shun::config::StepKind::License => {
                pages.push(Page::License);
                license = true;
            }
            _ => {}
        }
    }
    if !license {
        pages.push(Page::License);
    }
    pages
}

/// Fetches the manifest wallpaper's first IMAGE source off-thread
/// (ureq; video/pipeline sources have no egui renderer and stand down).
/// `None` on any failure — the face paints its token background.
fn spawn_wallpaper_fetcher(
    config: &ShunConfig,
) -> std::sync::mpsc::Receiver<Option<Vec<u8>>> {
    let (tx, rx) = std::sync::mpsc::channel();
    let url = config
        .shell
        .as_ref()
        .and_then(|shell| shell.theme.as_ref())
        .and_then(|theme| theme.wallpaper.as_ref())
        .and_then(|wallpaper| {
            wallpaper.sources.iter().find_map(|source| match source {
                shun::config::WallpaperSourceSpec::Image { image } => Some(image.clone()),
                _ => None,
            })
        });
    std::thread::spawn(move || {
        let Some(url) = url else {
            return;
        };
        let bytes = ureq::get(&url).call().ok().and_then(|resp| {
            let mut bytes = Vec::new();
            use std::io::Read;
            resp.into_reader()
                .take(64 * 1024 * 1024)
                .read_to_end(&mut bytes)
                .ok()
                .map(|_| bytes)
        });
        let _ = tx.send(bytes);
    });
    rx
}

fn desktop_policy_asks(config: &ShunConfig) -> bool {
    install_of(config)
        .map(|install| install.desktop_shortcut == shun::config::DesktopShortcutPolicy::Ask)
        .unwrap_or(false)
}

/// Same gate for the start-menu checkbox (`ShortcutPolicy` is the same
/// enum — the manifest may pin the menu shortcut to always/never).
fn menu_policy_asks(config: &ShunConfig) -> bool {
    install_of(config)
        .map(|install| install.start_menu_shortcut == shun::config::DesktopShortcutPolicy::Ask)
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
    /// The wizard locale code (one authored `LOCALES` entry).
    language: String,
    /// Whether a CJK-capable system font registered (informational —
    /// the face renders the OS font stack now).
    cjk_font: bool,
    texts: Texts,
    theme: Theme,
    /// The pinned light/dark state behind the title-bar toggle (the
    /// manifest's `user-adjustable`); `accent` rebuilds the tokens.
    dark_theme: bool,
    /// Set when the user toggles: the pinned mode wins for the session
    /// and the solar clock stops flipping it (hikari's `setMode`).
    mode_pinned: bool,
    /// The Windows location fix once it lands; `None` keeps the
    /// timezone estimate driving the solar clock.
    /// The solar clock's last verdict and tick time — re-evaluated
    /// every five minutes so dawn/dusk flip a `system`-mode face.
    /// The DWM round/shadow hint is applied once, on the first frame.
    dwm_rounded: bool,
    /// The install-location candidates + drive list — probed once from
    /// the same shun sources the web face's default_dir/list_drives
    /// commands ride (kind, writable, path) / (mount, kind, label).
    candidates: Vec<(String, bool, String)>,
    /// Optional attachments resolved against the payload (key, title,
    /// bundled?, size, picked?) — the location pane's block; picked
    /// non-bundled ones stream in right after the install in the worker.
    attachments: Vec<(String, String, bool, Option<u64>, bool)>,
    drives: Vec<(String, String, Option<String>)>,
    /// Live writability of the shown path, probed when it changes.
    dir_writable: Option<bool>,
    probed_dir: String,
    drive_open: bool,
    /// Build flavor for the location pane's identity line.
    flavor: String,
    accent: Option<[u8; 3]>,
    user_adjustable: bool,
    timeline_left: bool,
    logo: Option<TextureHandle>,
    /// The manifest's wallpaper (first IMAGE source), fetched
    /// off-thread after boot and painted as the pane's backdrop;
    /// video/pipeline sources stand down (no egui renderer — the
    /// degraded-face line), a failed fetch paints nothing.
    wallpaper: Option<TextureHandle>,
    wallpaper_rx: std::sync::mpsc::Receiver<Option<Vec<u8>>>,
    /// The lucide caption glyphs (loaded once; tinted per state).
    caption_icons: CaptionIcons,
    stage: Stage,
    mode: &'static str,
    dir: String,
    /// The wizard's answer to the `ask` desktop-shortcut policy
    /// (default checked, the NSIS convention).
    desktop_shortcut: bool,
    /// The wizard's answer to the `ask` start-menu-shortcut policy
    /// (default checked — the web done page's second checkbox).
    start_menu: bool,
    /// The done page's immediate-launch answer (default checked).
    launch_after: bool,
    /// The wizard's answer to the `ask` install-scope policy
    /// (default per-user).
    machine: bool,
    /// The resolved wizard pipeline (ordered steps, bodies inlined).
    /// License documents resolved per locale (`shun-license-docs.json`,
    /// shared with the web shell); the license step re-picks from this
    /// map when the language changes, falling back to `steps`.
    license_docs: std::collections::BTreeMap<String, Vec<shun::config::ResolvedLicenseDoc>>,
    /// The configure panes in walk order (language, location, the
    /// pipeline's content steps around the license, license).
    pages: Vec<Page>,
    /// Cursor into `pages` while on `Stage::Configure`.
    step: usize,
    /// The license checkbox (`license` steps gate progression on it).
    license_accepted: bool,
    /// The language step's HkSelect-style dropdown state.
    lang_combo_open: bool,
    /// Which license document the pane shows (multi-document licenses
    /// page through [`shun::config::ResolvedStep::licenses`]); reset
    /// whenever the wizard moves to another step.
    license_doc_index: usize,
    /// When the license step was entered — the web face's three-second
    /// minimum-read countdown runs from here.
    license_entered: Option<std::time::Instant>,
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
    /// The standalone uninstaller face (`/uninstall` without `--silent`,
    /// the ARP entry's GUI spelling): replaces the whole wizard with the
    /// web face's confirm → running → done/failed page, repair included.
    uninstall_mode: bool,
    /// Which action the uninstall page's run came from — repair relabels
    /// the running/done/failed copy (the web phases repairing/repaired/
    /// repair_failed).
    repairing: bool,
    outcome: Option<Outcome>,
    entry: Option<PathBuf>,
    receiver: Receiver<WorkerMsg>,
}

/// Shared control metrics for the egui face — one set of heights and
/// widths every pane draws with, so all products and steps size alike.
// Caption chrome, HkTitleBar parity: full-band-height 46pt plates in a
// flat 32pt band — the native Windows 11 caption proportion, identical
// to the webview face's SCSS (--hk-tb-height).
const CAPTION_W: f32 = 46.0;
const TITLEBAR_H: f32 = 32.0;
const CONTROL_H: f32 = 36.0;
const COMBO_W: f32 = 380.0;

impl FallbackApp {
    #[allow(clippy::too_many_arguments)]
    fn new(
        config: ShunConfig,
        payload: ArchivePayload,
        reason: FallbackReason,
        uninstall_mode: bool,
        language: String,
        cjk_font: bool,
        receiver: Receiver<WorkerMsg>,
        logo: Option<TextureHandle>,

        license_docs: std::collections::BTreeMap<String, Vec<shun::config::ResolvedLicenseDoc>>,
        wallpaper_rx: std::sync::mpsc::Receiver<Option<Vec<u8>>>,
        caption_icons: CaptionIcons,
        flavor: String,
    ) -> Self {
        let theme = resolve_theme(&config);
        let shell = config.shell.clone().unwrap_or_default();
        let mode = offered_modes(&config).first().copied().unwrap_or("local");
        // The initial location resolves exactly like the web face's
        // default_dir command: the first writable candidate from the
        // config-driven list, else the wizard's default.
        let probed = shun::wizard::location_defaults(&config.product.name);
        let dir = probed
            .iter()
            .find(|candidate| candidate.writable)
            .map(|candidate| candidate.path.clone())
            .unwrap_or_else(|| shun::wizard::default_location(&config.product.name));
        // The toggle state mirrors what resolve_theme resolved (the
        // solar clock's first-paint verdict counts for the session
        // until the user pins — or the clock itself flips it).
        let resolved_dark = !matches!(
            shell.theme.as_ref().and_then(|theme| theme.mode),
            Some(shun::config::ThemeMode::Light)
        ) || matches!(
            shell.theme.as_ref().and_then(|theme| theme.mode),
            Some(shun::config::ThemeMode::Dark)
        );
        let texts = wizard_strings().texts(&language);
        let attachments: Vec<(String, String, bool, Option<u64>, bool)> =
            shun::attachments::resolve(&config, &payload)
                .into_iter()
                .map(|a| (a.config.key, a.config.title, a.included, a.config.size, true))
                .collect();
        Self {
            dir,
            config,
            payload,
            reason,
            language,
            cjk_font,
            texts,
            dark_theme: resolved_dark,
            mode_pinned: false,
            dwm_rounded: false,
            candidates: probed
                .into_iter()
                .map(|c| (c.kind.to_string(), c.writable, c.path))
                .collect(),
            attachments,
            drives: shun::fs_probe::list_drives()
                .into_iter()
                .map(|d| {
                    (
                        d.mount.to_string_lossy().into_owned(),
                        drive_kind_name(d.kind),
                        d.label,
                    )
                })
                .collect(),
            dir_writable: None,
            probed_dir: String::new(),
            drive_open: false,
            flavor: flavor,
            theme,
            accent: shell.theme.as_ref().and_then(|theme| theme.accent),
            user_adjustable: shell
                .theme
                .as_ref()
                .and_then(|theme| theme.user_adjustable)
                .unwrap_or(false),
            // The side rail is the standard look (the web face renders
            // left too); an explicit `timeline = "top"` restores the
            // horizontal strip.
            timeline_left: shell.timeline != Some(shun::config::TimelineOrientation::Top),
            logo,
            wallpaper: None,
            wallpaper_rx,
            caption_icons,
            stage: Stage::Configure,
            pages: wizard_pages(),
            mode,
            desktop_shortcut: true,
            start_menu: true,
            launch_after: true,
            machine: false,
            license_docs,
            step: 0,
            license_accepted: false,
            license_entered: None,
            lang_combo_open: false,
            license_doc_index: 0,
            progress: None,
            overall: None,
            // The log drawer mirrors the web pane's defaults: collapsed
            // until opened (or forced by an error line), newest-first
            // unless the manifest pins `shell.log-order = "oldest"`.
            terminal: crate::terminal::Terminal::new(
                false,
                matches!(
                    shell.log_order,
                    Some(shun::config::LogOrder::Oldest)
                ),
            ),
            log_level: shell.log_level.unwrap_or(shun::config::LogVerbosity::All),
            phases_done: Vec::new(),
            phase_active: None,
            uninstalling: None,
            uninstall_mode,
            repairing: false,
            outcome: None,
            entry: None,
            receiver,
        }
    }

    /// Switches the wizard language: the TEXTS table, the license
    /// documents and the document pager all follow, and the choice is
    /// remembered for the next run — unless the selected mode is
    /// portable, in which case nothing leaves this process.
    /// Visual-debug hook (debug builds only): SHUN_DEBUG_STAGE =
    /// running|finished|failed forces the pane, SHUN_DEBUG_LOG_LINES
    /// fills sample log rows, SHUN_DEBUG_LOG_OPEN=1 opens the drawer —
    /// so `--screenshot` can capture states the wizard only reaches
    /// after interactive clicks (release builds carry none of it).
    #[cfg(debug_assertions)]
    fn debug_force_stage(&mut self) {
        if let Ok(step) = std::env::var("SHUN_DEBUG_STEP") {
            self.step = step.parse().unwrap_or(self.step);
        }
        let Ok(stage) = std::env::var("SHUN_DEBUG_STAGE") else {
            return;
        };
        let lines = std::env::var("SHUN_DEBUG_LOG_LINES")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(40);
        for i in 0..lines {
            // A realistic mix: file writes echo, script markers step,
            // completions ok (no errors - the capture is the happy run).
            let kind = match i % 9 {
                0 => crate::terminal::LineKind::Step,
                4 => crate::terminal::LineKind::Ok,
                _ => crate::terminal::LineKind::Echo,
            };
            let text = match i % 9 {
                0 => format!("running script installer/post-install.dk [{i:>2}]"),
                4 => format!("verified data/packs/chapter_{i:03}.json"),
                _ => format!("write data/packs/chapter_{i:03}.json"),
            };
            self.terminal.push(kind, text);
        }
        if std::env::var("SHUN_DEBUG_LOG_OPEN").map_or(false, |v| v == "1") {
            self.terminal.open = true;
        }
        match stage.as_str() {
            "running" => {
                self.stage = Stage::Running;
                self.progress =
                    Some(("Extracting chapter_042.pack".into(), Some(42)));
                self.overall = Some(42);
            }
            "finished" => {
                self.stage = Stage::Finished;
                self.outcome = Some(Outcome::InstallOk);
            }
            "failed" => {
                self.stage = Stage::Finished;
                self.outcome =
                    Some(Outcome::Failed("the extract step failed: archive truncated".into()));
            }
            _ => {}
        }
    }

    /// Rebuild the tokens + egui visuals for a light/dark flip — the
    /// one path both the caption toggle and the solar clock go through.
    fn apply_mode(&mut self, ctx: &Context, dark: bool) {
        self.dark_theme = dark;
        let accent = self.accent;
        let mut theme = if dark {
            Theme::dark(accent)
        } else {
            Theme::light(accent)
        };
        // The rebuilt tokens re-tint from the manifest's layers — a side
        // flip must not drop the configured background (the same
        // application the startup resolution runs).
        apply_theme_layers(
            &mut theme,
            &self.config.shell.clone().unwrap_or_default(),
        );
        self.theme = theme;
        ctx.set_visuals(if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
    }


    fn apply_language(&mut self, language: &str) {
        if self.language == language {
            return;
        }
        self.language = language.to_string();
        self.texts = wizard_strings().texts(language);
        // The documents switched under the pager — restart at doc 1.
        self.license_doc_index = 0;
        // Remember the choice unless this run is portable: a portable
        // copy writes no system state, so it stays in memory only.
        let _ = crate::remember_language(
            &crate::local_appdata(),
            &self.config.product.name,
            language.to_string(),
            self.mode == "portable",
        );
    }

    /// The install request the shared wizard driver consumes: mode,
    /// padded directory, and the wizard language (it rides into the flow
    /// as `SHUN_LANGUAGE` and the on-disk manifest). Shortcut answers
    /// deliberately ride OUT of the request — the flow creates none and
    /// the done page owns the choice, exactly like every other face.
    fn install_request(&self) -> Result<InstallRequest, String> {
        let dir = self.dir.trim().trim_end_matches('\\');
        if dir.is_empty() {
            return Err(self.texts.dir_empty.to_string());
        }
        Ok(InstallRequest {
            mode: self.mode.to_string(),
            dir: dir.to_string(),
            language: Some(self.language.clone()),
            machine: self.machine,
        })
    }

    /// Spawns the worker thread driving the flow. The egui context is
    /// cloned in so the worker can request repaints as events arrive.
    fn spawn_worker(&mut self, ctx: &Context, uninstalling: bool) {
        let raw = self.dir.clone();
        self.dir = nested_dir(&self.config, &raw);
        let request = match self.install_request() {
            Ok(request) => request,
            Err(err) => {
                self.outcome = Some(Outcome::Failed(err));
                self.stage = Stage::Finished;
                return;
            }
        };
        // Machine scope needs an elevated token; re-launch under UAC
        // carrying the resolved answers (headless) and exit this
        // instance — the elevated copy carries on.
        if let Err(err) = shun::wizard::elevation_context(
            &self.config,
            self.dir.trim(),
            self.mode == "portable",
            self.machine,
            Some(self.language.as_str()),
        )
        .and_then(|gate| {
            crate::ensure_elevated_for(
                &gate,
                self.mode,
                self.dir.trim(),
                self.desktop_shortcut,
                uninstalling,
            )
        }) {
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
        self.entry = install_of(&self.config)
            .and_then(|install| install.main_exe.clone())
            .map(|main| Path::new(self.dir.trim()).join(main));

        let (sender, receiver) = channel();
        self.receiver = receiver;
        let repaint = ctx.clone();
        let payload = self.payload.clone();
        let config = self.config.clone();
        let attachments = self.attachments.clone();
        std::thread::spawn(move || {
            // The shared driver, exactly what the webview and TUI faces
            // ride: writability gate, overwrite hygiene, the flow, the
            // stale-file sweep. Uninstall resolves the install dir this
            // process sits in (the ARP uninstaller's contract).
            let core = WizardCore::new(config, BTreeMap::new());
            let result = if uninstalling {
                shun::wizard::run_uninstall(&core).map_err(|e| e.to_string())
            } else {
                let install = shun::wizard::run_install(&core, &payload, &request, &mut |event| {
                    let _ = sender.send(WorkerMsg::Event(event.clone()));
                    repaint.request_repaint();
                })
                .map_err(|e| e.to_string());
                // Picked non-bundled attachments stream in right after
                // the payload (the web face's download pass, same
                // channel). A failed attachment fails the run — half an
                // install lies about what it delivered.
                install.and_then(|()| {
                    for (key, _, included, _, picked) in &attachments {
                        if *included || !*picked {
                            continue;
                        }
                        let Some(attachment) = core
                            .config
                            .attachments
                            .iter()
                            .find(|a| &a.key == key)
                        else {
                            continue;
                        };
                        let dir = request.dir.trim().trim_end_matches('\\');
                        shun::attachments::download(
                            attachment,
                            std::path::Path::new(dir),
                            &mut |event| {
                                let _ = sender.send(WorkerMsg::Event(event.clone()));
                                repaint.request_repaint();
                            },
                        )
                        .map_err(|e| e.to_string())?;
                    }
                    Ok(())
                })
            };
            let _ = sender.send(WorkerMsg::Done(result));
            repaint.request_repaint();
        });
    }

    /// The wallpaper's fetch landing: decode and upload once, then the
    /// backdrop paints every frame in update() before any panel.
    fn drain_wallpaper(&mut self, ctx: &Context) {
        if self.wallpaper.is_some() {
            return;
        }
        let Ok(bytes) = self.wallpaper_rx.try_recv() else {
            return;
        };
        let Some(bytes) = bytes else { return };
        let decoded = image::load_from_memory(&bytes)
            .ok()
            .map(|img| img.into_rgba8());
        if let Some(rgba) = decoded {
            let (w, h) = (rgba.width(), rgba.height());
            let texture = ctx.load_texture(
                "wallpaper",
                egui::ColorImage::from_rgba_unmultiplied(
                    [w as usize, h as usize],
                    rgba.as_raw(),
                ),
                egui::TextureOptions::default(),
            );
            self.wallpaper = Some(texture);
        }
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
                "desktop-shortcut-blocked" => self.texts.warn_desktop_blocked.clone(),
                "aumid-stamp-blocked" => self.texts.warn_aumid_blocked.clone(),
                _ => detail.clone(),
            };
            self.terminal
                .push(crate::terminal::LineKind::Error, format!("⚠ {text}"));
            return;
        }
        match record {
            FlowLog::FileWrite { path } => {
                let prefix = self.texts.log_write.clone();
                self.push_log(format!("{prefix} {}", path.display()));
            }
            FlowLog::FileReuse { path } => {
                let prefix = self.texts.log_reuse.clone();
                self.push_log(format!("{prefix} {}", path.display()));
            }
            FlowLog::ScriptBegin { name } => {
                let prefix = self.texts.script_begin.clone();
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

    fn hint(&self) -> String {
        self.texts
            .hint_local
            .replace("%PRODUCT%", &self.config.product.name)
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
/// Uninstall runs carry the uninstaller spelling — the same frame title
/// the webview face resolves (`卸载 {product}` et al).
pub fn window_title(config: &ShunConfig, uninstall: bool) -> String {
    // The same locale resolution the webview face applies to its frame
    // (saved wizard language → system locale → zh-Hans default); the
    // English literal below is the non-Windows floor.
    #[cfg(windows)]
    return crate::os_window_title(config, uninstall);
    #[cfg(not(windows))]
    return if uninstall {
        format!("Uninstall {}", config.product.name)
    } else {
        format!("{} Installer", config.product.name)
    };
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
    uninstall: bool,
    logo_kind: &str,
    logo_bytes: &[u8],
    license_docs: std::collections::BTreeMap<String, Vec<shun::config::ResolvedLicenseDoc>>,
) {
    let title = window_title(&config, uninstall);
    // DPI contract: the whole face is designed in logical points. The
    // ONLY place actual DPI enters is egui's pixels_per_point — eframe
    // picks the monitor scale once and applies it as a single uniform
    // scale when tessellating; layout code never reads the scale factor
    // (no PhysicalSize / scale math anywhere in this crate), so the
    // chrome keeps the designed proportions on every display.
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
            let font_found = install_system_fonts(&cc.egui_ctx);
            // Chrome copy is not content: dragging across headings and
            // labels must not paint selection bands (the path input
            // keeps its selection — only LABELS are gated here).
            cc.egui_ctx
                .style_mut(|style| style.interaction.selectable_labels = false);
            // Uniform control height across all egui widgets (combo,
            // buttons, text fields) — matches the web face's proportions.
            let mut style = (*cc.egui_ctx.style()).clone();
            style.spacing.interact_size.y = 36.0;
            style.spacing.button_padding = egui::vec2(12.0, 8.0);
            cc.egui_ctx.set_style(style);
            // Language: the remembered preference wins (the same
            // `installer-prefs.json` the web shell writes), then
            // `shell.language`, then the system locale — every one
            // validated against the authored locale list (now the web
            // face's full ten: the OS font stack covers them all).
            let strings = wizard_strings();
            let known = |code: &str| strings.is_known(code);
            let language = crate::load_prefs(&crate::local_appdata(), &config.product.name)
                .language
                .as_deref()
                .filter(|code| known(code))
                .or_else(|| {
                    config
                        .shell
                        .as_ref()
                        .and_then(|s| s.language.as_deref())
                        .filter(|pin| *pin != "auto" && known(pin))
                })
                .map(String::from)
                .unwrap_or_else(|| match system_locale_tag() {
                    Some(tag) => strings.resolve_system_locale(&tag),
                    None => strings.default.clone(),
                });
            let logo = load_logo(&cc.egui_ctx, logo_kind, logo_bytes);
            let (_sender, receiver) = channel::<WorkerMsg>();
            let wallpaper_rx = spawn_wallpaper_fetcher(&config);
            let caption_icons = CaptionIcons::load(&cc.egui_ctx);
            let mut app = FallbackApp::new(
                config,
                payload,
                reason,
                uninstall,
                language,
                font_found,
                receiver,
                logo,
                license_docs,
                wallpaper_rx,
                caption_icons,
                SHUN_FLAVOR.trim().to_string(),
            );
            #[cfg(debug_assertions)]
            app.debug_force_stage();
            Ok(Box::new(app))
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
    /// The hikari caption bar, drawn to HkTitleBar's spec so the egui
    /// face reads identical to the webview face: 32pt flat band, logo +
    /// 11pt semibold title + 10pt version subtitle left, full-height
    /// 46pt caption plates right (accent-tinted hover; close hovers
    /// red with a white glyph and rounds the window's top-right
    /// corner). The whole band outside the buttons drags the window.
    fn title_bar(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        let texts = self.texts.clone();
        let icons = self.caption_icons.clone();
        // Reserve one exact-height strip across the panel and lay both
        // halves out INSIDE it. Laying the caption cluster out directly
        // in the panel's ui let the bar's height depend on the panel's
        // previous-frame rect: egui's Center alignment measures
        // `set_min_height` from the centered cursor line, so the bar
        // crept north of 40pt and stayed there — a fixed point through
        // the panel's per-frame PanelState. A bounded strip renders the
        // same geometry every frame.
        let (strip, _) =
            ui.allocate_exact_size(vec2(ui.available_width(), TITLEBAR_H), Sense::hover());
        ui.allocate_new_ui(
            egui::UiBuilder::new()
                .max_rect(strip)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| {
                // HkTitleBar's left cluster: 12pt lead-in, 16pt icon,
                // 8pt gap, 11pt/600 title, then the version subtitle at
                // 10pt and 70% strength.
                ui.add_space(12.0);
                if let Some(logo) = &self.logo {
                    ui.add(egui::Image::from_texture(logo).fit_to_exact_size(Vec2::splat(16.0)));
                }
                ui.add_space(8.0);
                ui.label(
                    RichText::new(format!(
                        "{} {}",
                        self.config.product.name,
                        if self.uninstall_mode || self.uninstalling == Some(true) {
                            texts.titlebar_uninstall
                        } else {
                            texts.titlebar_installer
                        }
                    ))
                    .color(theme.text)
                    .size(11.0)
                    .strong(),
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new(format!("v{}", self.config.product.version))
                        .color(theme.text_secondary.gamma_multiply(0.7))
                        .size(10.0),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                // HkTitleBar's buttons are FLUSH — 46pt plates touching.
                // Zero egui's automatic item spacing or every pair of
                // plates grows an 8pt gap the web face doesn't have.
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                // Caption plates, HkTitleBar geometry: full-band-height
                // 46pt rectangles flush to the window edge, VECTOR
                // glyphs (egui's default fonts carry no caption
                // dingbats — strokes and discs render everywhere).
                // Hover rides the accent at 12%; the close plate turns
                // #e81123 with a white glyph and rounds the window's
                // top-right corner, exactly like the SCSS.
                enum CaptionIcon {
                    Minimize,
                    ThemeToggle,
                    Close,
                }
                let dark_now = self.dark_theme;
                let caption = move |ui: &mut egui::Ui, icon: CaptionIcon| -> egui::Response {
                let (rect, response) = ui.allocate_exact_size(
                    Vec2::new(CAPTION_W, TITLEBAR_H),
                    egui::Sense::click(),
                );
                let response = Self::hand(response);
                    let painter = ui.painter_at(rect);
                    let close = matches!(icon, CaptionIcon::Close);
                    let hovered = response.hovered();
                    let active = response.is_pointer_button_down_on();
                    // The SCSS's 0.12s background transition, immediate
                    // mode's way: animate the hover toward its fill and
                    // paint the fade. Pressed steps the wash up (and the
                    // close plate to its lighter #f1707a).
                    let hover_t = ui
                        .ctx()
                        .animate_bool_with_time(response.id.with("hover"), hovered, 0.12);
                    if hover_t > 0.0 || active {
                        let radius = if close {
                            egui::CornerRadius {
                                nw: 0,
                                ne: 8,
                                sw: 0,
                                se: 0,
                            }
                        } else {
                            CornerRadius::ZERO
                        };
                        let fill = if close {
                            if active {
                                Color32::from_rgb(0xf1, 0x70, 0x7a)
                            } else {
                                mix(
                                    theme.background,
                                    Color32::from_rgb(0xe8, 0x11, 0x23),
                                    hover_t,
                                )
                            }
                        } else {
                            mix(theme.background, theme.primary, 0.12 * hover_t + f32::from(active) * 0.08)
                        };
                        painter.rect_filled(rect, radius, fill);
                    }
                    let c = rect.center();
                    let icon_color = if close && (hovered || active) {
                        Color32::WHITE
                    } else if hovered || active {
                        theme.text
                    } else {
                        theme.text_secondary
                    };
                    // The lucide glyph texture (same path data the webview
                    // face renders), tinted like `stroke="currentColor"`.
                    let texture = match icon {
                        CaptionIcon::Minimize => &icons.minus,
                        CaptionIcon::Close => &icons.x,
                        CaptionIcon::ThemeToggle => {
                            if dark_now {
                                &icons.sun
                            } else {
                                &icons.moon
                            }
                        }
                    };
                    painter.image(
                        texture.id(),
                        egui::Rect::from_center_size(c, egui::vec2(14.0, 14.0)),
                        egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        icon_color,
                    );
                    response
                };

                // Theme toggle — manifest-gated (user-adjustable), the
                // sun/moon picked by the live mode. Not offered on the
                // standalone uninstall page (the web face renders no
                // custom action there either). Right-to-left row: emit
                // the CLOSE first so it lands right-most (the Windows
                // convention), minimize to its left, and the theme
                // toggle left-most of the cluster.
                let show_toggle = self.user_adjustable && !self.uninstall_mode;
                let close = caption(ui, CaptionIcon::Close);
                if close.clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                let minimize = caption(ui, CaptionIcon::Minimize);
                if minimize.clicked() {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }
                if show_toggle {
                    let r = caption(ui, CaptionIcon::ThemeToggle);
                    if r.clicked() {
                        // A manual toggle pins the side for the session
                        // (hikari's `setMode`): the solar clock stands
                        // down until the process restarts.
                        self.mode_pinned = true;
                        self.apply_mode(ui.ctx(), !self.dark_theme);
                    }
                }
                });
            },
        );
        // The drag zone must not overlap the caption buttons: egui
        // hit-tests later-registered widgets first, so a full-bar drag
        // rect registered after the button cluster sits ON TOP of it and
        // eats every click (buttons only responded at the window's outer
        // pixel). Shrink the drag rect to the cluster's left edge.
        let caption_count = if self.user_adjustable && !self.uninstall_mode {
            3
        } else {
            2
        };
        let mut drag_rect = strip;
        let cluster_left = drag_rect.right() - (CAPTION_W * caption_count as f32 + 2.0);
        drag_rect.set_right(cluster_left.max(drag_rect.left()));
        let drag = ui.interact(drag_rect, ui.id().with("titlebar-drag"), Sense::drag());
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
        // The fixed five steps, webview-face parity: done steps carry a
        // check in a filled circle, the active step its number in a
        // ring, pending steps their number in a muted ring.
        let mut labels: Vec<String> = self
            .pages
            .iter()
            .map(|page| match page {
                Page::Language => self.texts.step_language.to_owned(),
                Page::Location => self.texts.step_location.to_owned(),
                Page::License => self.texts.step_license.to_owned(),
                Page::Content { title, .. } => title.clone(),
            })
            .collect();
        labels.push(self.texts.step_install.to_owned());
        labels.push(self.texts.step_done.to_owned());
        let active_marker = match current {
            Stage::Configure => Some(self.step.min(labels.len() - 1)),
            _ => None,
        };
        let order_current = match current {
            Stage::Configure => self.step.min(labels.len() - 1),
            Stage::Running => labels.len() - 2,
            Stage::Finished => labels.len() - 1,
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

        let circle_d = 24.0f32;
        let connector_h = 24.0f32;
        let label_gap = 12.0f32;
        // HkTimeline's segment tone: the border at 30% over the page.
        let border_soft = mix(theme.background, theme.border, 0.3);
        // Block width: circle + gap + the widest localized label.
        // The web rail's label rides --text-sm (13px), not body size.
        let font = egui::FontId::proportional(13.0);
        let label_w = items
            .iter()
            .map(|(_, _, label)| {
                ui.painter()
                    .layout_no_wrap(label.clone(), font.clone(), theme.text)
                    .size()
                    .x
            })
            .fold(0.0f32, f32::max);
        let block_w = circle_d + label_gap + label_w;
        // The three HkTimeline node states, drawn per the SCSS: done =
        // primary fill with a white check; active = primary 15% wash
        // under a 2px primary ring with the primary number; pending =
        // surface fill under a 2px hairline ring with the muted number.
        let paint_node = |painter: &egui::Painter, center: egui::Pos2, active: bool, done: bool, number: usize| {
            let r = circle_d / 2.0;
            if done {
                painter.circle_filled(center, r, theme.primary);
                let stroke = Stroke::new(2.0_f32, theme.on_primary);
                painter.line_segment(
                    [pos2(center.x - 4.0, center.y + 0.5), pos2(center.x - 1.0, center.y + 3.5)],
                    stroke,
                );
                painter.line_segment(
                    [pos2(center.x - 1.0, center.y + 3.5), pos2(center.x + 4.5, center.y - 3.5)],
                    stroke,
                );
            } else if active {
                painter.circle_filled(center, r, mix(theme.background, theme.primary, 0.15));
                painter.circle_stroke(center, r - 1.0, Stroke::new(2.0_f32, theme.primary));
                painter.text(
                    center,
                    egui::Align2::CENTER_CENTER,
                    number.to_string(),
                    egui::FontId::proportional(12.0),
                    theme.primary,
                );
            } else {
                painter.circle_filled(center, r, theme.surface);
                painter.circle_stroke(center, r - 1.0, Stroke::new(2.0_f32, border_soft));
                painter.text(
                    center,
                    egui::Align2::CENTER_CENTER,
                    number.to_string(),
                    egui::FontId::proportional(12.0),
                    theme.text_secondary,
                );
            }
        };

        if vertical {
            // TOP-anchored at the unified heading position: the rail's
            // panel margin (48pt under the titlebar) puts the first
            // node's top at the same 80pt the pane's heading starts at
            // — no vertical centering, the rail reads as a header
            // element like the web face's. Horizontal stays centered.
            let top = 0.0;
            let left = ((ui.available_width() - block_w) / 2.0).max(0.0);
            let cx = left + circle_d / 2.0;

            let mut cursor_y = ui.cursor().top() + top;
            for (index, (active, done, label)) in items.iter().enumerate() {
                let cy = cursor_y + circle_d / 2.0;
                // Connector BEFORE the circle — it belongs to the row
                // above, so its color follows the previous step's status
                // (completed bonds ride the primary, everything else the
                // soft border), exactly like [data-el=connector].
                if index > 0 {
                    let prev_done = items[index - 1].1;
                    ui.painter().line_segment(
                        [
                            pos2(cx, cursor_y - connector_h + 2.0),
                            pos2(cx, cy - circle_d / 2.0 - 2.0),
                        ],
                        Stroke::new(2.0_f32, if prev_done { theme.primary } else { border_soft }),
                    );
                }
                paint_node(ui.painter(), pos2(cx, cy), *active, *done, index + 1);
                // Label beside the circle, vertically centered. Active
                // and completed labels read in the strong text tone;
                // pending ones stay muted.
                ui.painter().text(
                    pos2(left + circle_d + label_gap, cy),
                    egui::Align2::LEFT_CENTER,
                    label.as_str(),
                    font.clone(),
                    if *active || *done {
                        theme.text
                    } else {
                        theme.text_secondary
                    },
                );
                cursor_y = cy + circle_d / 2.0 + connector_h;
            }
        } else {
            // Horizontal strip: circle + label side by side per step.
            ui.horizontal(|ui| {
                for (index, (active, done, label)) in items.iter().enumerate() {
                    if index > 0 {
                        let prev_done = items[index - 1].1;
                        ui.label(RichText::new("——").color(if prev_done {
                            theme.primary
                        } else {
                            border_soft
                        }).small());
                        ui.add_space(6.0);
                    }
                    let cy = ui.cursor().top() + 12.0;
                    let cx = ui.cursor().left() + circle_d / 2.0;
                    paint_node(ui.painter(), pos2(cx, cy), *active, *done, index + 1);
                    // Reserve the circle's box, then the label beside it.
                    ui.allocate_exact_size(Vec2::new(circle_d, circle_d), egui::Sense::hover());
                    ui.label(
                        RichText::new(label.as_str())
                            .color(if *active || *done {
                                theme.text
                            } else {
                                theme.text_secondary
                            })
                            .size(14.0),
                    );
                }
            });
        }
    }

    /// The fallback-reason banner. This is the contract: a
    /// missing-environment install must say so.
    fn banner(&mut self, ui: &mut egui::Ui) {
        let theme = &self.theme;
        let text = match self.reason {
            FallbackReason::MissingWebview2 => self.texts.banner_missing.clone(),
            FallbackReason::ManualOverride => self.texts.banner_manual.clone(),
        };
        Frame::default()
            .fill(theme.warning_tint())
            .stroke(Stroke::new(1.0f32, theme.warning))
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(text).color(theme.warning).size(12.5));
            });
    }

    /// The configure pane: dispatches on the fixed wizard steps —
    /// language → location → license — the same progression the webview
    /// face renders.
    /// Clickables point: egui has no automatic hand cursor, so every
    /// interactive widget routes its response through here.
    fn hand(response: egui::Response) -> egui::Response {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    }

    /// Baseline-true icon alignment: measures the label's galley and
    /// centers the icon ON the x-height band (baseline minus half the
    /// icon), the same geometry CSS `align-items: center` produces for
    /// an inline SVG beside text. Shared by every icon+label control.
    fn icon_center_for(
        painter: &egui::Painter,
        label: &str,
        font: egui::FontId,
        row_center: egui::Pos2,
    ) -> egui::Pos2 {
        let galley = painter.layout_no_wrap(label.to_string(), font, Color32::WHITE);
        let row_top = row_center.y - galley.size().y / 2.0;
        // The first glyph's pos.y is the row's baseline (epaint
        // guarantees one baseline per TextFormat row).
        let baseline = galley
            .rows
            .first()
            .and_then(|row| row.glyphs.first())
            .map(|g| row_top + g.pos.y)
            .unwrap_or(row_center.y);
        // The x-height band's visual middle sits at roughly baseline
        // minus a third of the ascent — where a centered inline glyph
        // lands in CSS terms.
        let ascent = galley.size().y * 0.55;
        pos2(row_center.x, baseline - ascent * 0.42)
    }

    /// A hikari ghost button: borderless lucide icon + label with a
    /// soft hover wash (the quick-candidate/browse seat). The icon
    /// centers on the label's optical middle — the galley box includes
    /// descender space, so a raw box-center would sit the glyph high.
    /// Generic: any (icon, label) pair, any pane.
    fn ghost_icon_button(
        ui: &mut egui::Ui,
        theme: Theme,
        icons: &CaptionIcons,
        icon: &TextureHandle,
        label: &str,
        min_w: f32,
        h: f32,
    ) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(vec2(min_w, h), egui::Sense::click());
        let response = Self::hand(response);
        let painter = ui.painter_at(rect);
        let hover_t = ui
            .ctx()
            .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.12);
        if hover_t > 0.0 {
            painter.rect_filled(
                rect,
                CornerRadius::same(10),
                mix(theme.background, theme.text, 0.05 * hover_t),
            );
        }
        let font = egui::FontId::proportional(13.0);
        let galley = painter.layout_no_wrap(label.to_string(), font.clone(), theme.text);
        let content_w = 14.0 + 6.0 + galley.size().x;
        let left = rect.left() + ((rect.width() - content_w) / 2.0).max(0.0);
        let icon_center =
            Self::icon_center_for(&painter, label, font, pos2(left + 7.0, rect.center().y));
        painter.image(
            icon.id(),
            egui::Rect::from_center_size(icon_center, egui::vec2(14.0, 14.0)),
            egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            theme.text,
        );
        painter.galley(
            pos2(left + 20.0, rect.center().y - galley.size().y / 2.0),
            galley,
            theme.text,
        );
        response
    }

    /// A hikari quick-pick chip: lucide icon + mono label in a soft
    /// pill, one click target. Unwritable candidates dim and swap to
    /// the alert glyph; `accent` paints the steer-to highlight. The
    /// icon aligns to the label's optical middle (see
    /// [`Self::ghost_icon_button`]). Returns true on click.
    fn quick_chip(
        ui: &mut egui::Ui,
        theme: Theme,
        icons: &CaptionIcons,
        icon: &TextureHandle,
        label: &str,
        writable: bool,
        accent: bool,
    ) -> bool {
        let chip_w = 42.0 + label.len() as f32 * 7.5;
        let (rect, response) = ui.allocate_exact_size(egui::vec2(chip_w, 28.0), egui::Sense::click());
        let response = Self::hand(response);
        let painter = ui.painter_at(rect);
        if accent || response.hovered() {
            painter.rect_filled(
                rect,
                CornerRadius::same(8),
                mix(theme.background, theme.primary, if accent { 0.15 } else { 0.08 }),
            );
        }
        let font = egui::FontId::monospace(12.0);
        let galley = painter.layout_no_wrap(
            label.to_string(),
            font.clone(),
            if writable { theme.text } else { theme.text_secondary },
        );
        let icon_center =
            Self::icon_center_for(&painter, label, font, pos2(rect.left() + 15.0, rect.center().y));
        let icon_tint = if writable {
            theme.text
        } else {
            theme.text_secondary.gamma_multiply(0.55)
        };
        painter.image(
            icon.id(),
            egui::Rect::from_center_size(icon_center, egui::vec2(13.0, 13.0)),
            egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            icon_tint,
        );
        painter.galley(
            pos2(rect.left() + 26.0, rect.center().y - galley.size().y / 2.0),
            galley,
            if writable { theme.text } else { theme.text_secondary },
        );
        response.clicked()
    }

    /// The hikari checkbox, drawn round like the web face's: a filled
    /// accent disc with a white check when on, a hairline ring that
    /// tints toward the accent on hover when off. The whole row
    /// (plate + label) is one click target. `width` pins the row —
    /// `None` spans the pane (form rows), a width centers the answer
    /// inside the done page's hero.
    fn circle_checkbox(
        ui: &mut egui::Ui,
        theme: Theme,
        checked: &mut bool,
        label: &str,
        width: Option<f32>,
    ) {
        let on = *checked;
        // Default to the CONTENT's width (plate + gap + label), so the
        // checkbox centers as a unit under a centering parent — a
        // full-width row would pin the plate to the pane's left edge
        // and read off-center on every centered pane.
        let row_w = width.unwrap_or_else(|| {
            let galley = ui.painter().layout_no_wrap(
                label.to_owned(),
                egui::FontId::proportional(13.0),
                Color32::WHITE,
            );
            30.0 + galley.size().x + 4.0
        });
        let (row, response) = ui.allocate_exact_size(vec2(row_w, 22.0), egui::Sense::click());
        let response = Self::hand(response);
        // The plate sits 4pt off the row's left edge and the paint rect
        // widens 4pt to match: the ring's anti-aliased arc needs real
        // clearance or the clip shaves its left side flat.
        let painter = ui.painter_at(row.expand(4.0));
        let plate = egui::Rect::from_min_size(
            pos2(row.left() + 4.0, row.center().y - 9.0),
            egui::vec2(18.0, 18.0),
        );
        let c = plate.center();
        if on {
            painter.circle_filled(c, 9.0, theme.primary);
            let stroke = Stroke::new(1.8f32, theme.on_primary);
            painter.line_segment(
                [pos2(c.x - 3.6, c.y + 0.4), pos2(c.x - 1.0, c.y + 3.0)],
                stroke,
            );
            painter.line_segment(
                [pos2(c.x - 1.0, c.y + 3.0), pos2(c.x + 4.2, c.y - 2.8)],
                stroke,
            );
        } else {
            let ring = if response.hovered() {
                theme.primary
            } else {
                theme.border
            };
            painter.circle_stroke(c, 8.25, Stroke::new(1.5f32, ring));
        }
        painter.text(
            pos2(row.left() + 28.0, row.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(13.0),
            theme.text,
        );
        if response.clicked() {
            *checked = !on;
        }
    }

    fn configure_view(&mut self, ui: &mut egui::Ui) {
        match self.pages[self.step].clone() {
            Page::Language => self.language_view(ui),
            Page::Location => self.location_view(ui),
            Page::Content { title, body } => self.content_view(ui, title, body),
            Page::License => self.license_view(ui),
        }
    }

    /// A custom content step: the title + a document card with a
    /// minimal markdown renderer (headings, lists, quotes, plain
    /// paragraphs — emphasis marks strip, the egui face carries no
    /// rich-text engine).
    fn content_view(&mut self, ui: &mut egui::Ui, title: String, body: String) {
        let theme = self.theme;
        ui.label(
            RichText::new(title)
                .strong()
                .size(22.0)
                .color(theme.text),
        );
        ui.add_space(10.0);
        Frame::default()
            .fill(theme.surface)
            .stroke(Stroke::new(1.0f32, theme.border))
            .inner_margin(Margin::same(12))
            .corner_radius(CornerRadius::same(10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::ScrollArea::vertical()
                    .id_salt(ui.id().with("content-doc"))
                    .max_height(300.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        for raw in body.lines() {
                            let line = raw.trim_start();
                            let (text, size, strong, indent, muted) = if let Some(h) =
                                line.strip_prefix("# ")
                            {
                                (h.trim(), 16.0, true, 0.0, false)
                            } else if let Some(h) = line.strip_prefix("## ") {
                                (h.trim(), 14.0, true, 0.0, false)
                            } else if let Some(h) = line.strip_prefix("### ") {
                                (h.trim(), 13.0, true, 0.0, false)
                            } else if let Some(item) =
                                line.strip_prefix("- ").or_else(|| line.strip_prefix("* "))
                            {
                                (item.trim(), 12.5, false, 16.0, false)
                            } else if let Some(q) = line.strip_prefix("> ") {
                                (q.trim(), 12.5, false, 8.0, true)
                            } else {
                                (line, 12.5, false, 0.0, false)
                            };
                            let plain = text.replace("**", "").replace('*', "").replace('`', "");
                            let color = if muted { theme.text_secondary } else { theme.text };
                            ui.horizontal(|ui| {
                                ui.add_space(indent);
                                let mut text = RichText::new(plain).size(size).color(color);
                                if strong {
                                    text = text.strong();
                                }
                                ui.label(text);
                            });
                            ui.add_space(2.0);
                        }
                    });
            });
    }

    fn language_view(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        let texts = self.texts.clone();
        let icons = self.caption_icons.clone();
        let current = self.language.clone();
        // Every authored locale, named in its own script — the same
        // option list the web face's picker shows (the system font
        // stack carries all ten, so no font gate on the offer).
        let labels = &wizard_strings().labels;
        let options = wizard_strings().options();

        // The unified fixed-origin column: heading, sub and picker all
        // start at the pane's left inset — no centering, no offsets.
        let block_w = COMBO_W;
        {
            ui.add_space(2.0);
            ui.label(
                RichText::new(texts.lang_heading)
                    .strong()
                    .size(22.0)
                    .color(theme.text),
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new(texts.lang_sub)
                    .size(13.0)
                    .color(theme.text_secondary),
            );
            ui.add_space(24.0);
                // HkSelect trigger parity: a 44pt surface row with a
                // hairline border (text at 14%), 10pt radius, the
                // selection CENTERED like the SCSS's text-align, and the
                // lucide chevron at the inline end. Hover lifts the
                // border to the primary; open pins it.
                let combo_h = 44.0f32;
                let border_idle = mix(theme.background, theme.text, 0.14);
                let (rect, resp) =
                    ui.allocate_exact_size(egui::vec2(block_w, combo_h), egui::Sense::click());
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                let painter = ui.painter_at(rect);
                let open = self.lang_combo_open;
                let hover_t = ui
                    .ctx()
                    .animate_bool_with_time(resp.id.with("hover"), resp.hovered() || open, 0.12);
                let border = mix(border_idle, theme.primary, hover_t);
                painter.rect_filled(rect, CornerRadius::same(10), theme.surface);
                painter.rect_stroke(
                    rect,
                    CornerRadius::same(10),
                    Stroke::new(1.0, border),
                    egui::StrokeKind::Middle,
                );
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    labels.get(&current).cloned().unwrap_or(current.clone()),
                    egui::FontId::proportional(14.0),
                    theme.text,
                );
                painter.image(
                    icons.chevron.id(),
                    egui::Rect::from_center_size(
                        pos2(rect.right() - 24.0, rect.center().y),
                        egui::vec2(16.0, 16.0),
                    ),
                    egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    theme.text_secondary,
                );
                if resp.clicked() {
                    self.lang_combo_open = !open;
                }
                let mut picked: Option<String> = None;
                if self.lang_combo_open {
                    let mut popup_rect = rect;
                    egui::Area::new(resp.id.with("popup"))
                        .order(egui::Order::Foreground)
                        .fixed_pos(rect.left_bottom() + egui::vec2(0.0, 4.0))
                        .show(ui.ctx(), |ui| {
                            let popup = egui::Frame::default()
                                .fill(theme.surface)
                                .stroke(Stroke::new(1.0, border_idle))
                                .shadow(egui::Shadow {
                                    offset: [0, 8],
                                    blur: 24,
                                    color: Color32::from_black_alpha(40),
                                    ..Default::default()
                                })
                                .corner_radius(CornerRadius::same(10))
                                .inner_margin(Margin::same(6))
                                .show(ui, |ui| {
                                    ui.set_width(block_w - 12.0);
                                    // HkSelect's capped panel: the list
                                    // scrolls internally past six rows
                                    // instead of stacking to the pane's
                                    // full height.
                                    egui::ScrollArea::vertical()
                                        .id_salt(resp.id.with("popup-scroll"))
                                        .max_height(6.0 * 30.0)
                                        .show(ui, |ui| {
                                    for (code, autonym) in labels.iter() {
                                        let language = code.clone();
                                        let selected = language == current;
                                        let (row, row_resp) = ui.allocate_exact_size(
                                            egui::vec2(ui.available_width(), 30.0),
                                            egui::Sense::click(),
                                        );
                                        if row_resp.hovered() {
                                            ui.ctx()
                                                .set_cursor_icon(egui::CursorIcon::PointingHand);
                                        }
                                        let rp = ui.painter_at(row);
                                        if selected || row_resp.hovered() {
                                            rp.rect_filled(
                                                row,
                                                CornerRadius::same(8),
                                                mix(
                                                    theme.background,
                                                    theme.primary,
                                                    if selected { 0.12 } else { 0.08 },
                                                ),
                                            );
                                        }
                                        rp.text(
                                            row.center(),
                                            egui::Align2::CENTER_CENTER,
                                            autonym,
                                            egui::FontId::proportional(14.0),
                                            if selected {
                                                theme.primary
                                            } else {
                                                theme.text
                                            },
                                        );
                                        if row_resp.clicked() {
                                            picked = Some(language);
                                        }
                                    }
                                        });
                                });
                            popup_rect = popup.response.rect;
                        });
                    // Click anywhere outside the trigger and the popup
                    // closes — the HkSelect behavior.
                    let hover_pos = ui.input(|i| i.pointer.hover_pos());
                    let outside = ui.input(|i| i.pointer.any_click())
                        && !resp.clicked()
                        && hover_pos.map_or(true, |p| {
                            !popup_rect.contains(p) && !rect.contains(p)
                        });
                    if outside {
                        self.lang_combo_open = false;
                    }
                }
                if let Some(language) = picked {
                    self.lang_combo_open = false;
                    self.apply_language(&language);
                }
        }
    }

    /// The location pane — product hero + the install-directory row
    /// (badge, field, browse) and its hint. The mode cards are gone:
    /// these products install per-user locally, exactly what the
    /// webview face shows.
    fn location_view(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        let texts = self.texts.clone();
        let icons = self.caption_icons.clone();

        // HkSelect-style row metric + the shared hairline border tone.
        let border_idle = mix(theme.background, theme.text, 0.14);

        // One start-aligned block for the whole pane: heading, sub and
        // the path field all start at the column's fixed origin. The
        // sub rides the manifest's product name via %PRODUCT%.
        ui.label(
            RichText::new(texts.location_heading)
                .strong()
                .size(22.0)
                .color(theme.text),
        );
        ui.add_space(10.0);
        ui.label(
            RichText::new(
                texts.location_sub.replace("%PRODUCT%", &self.config.product.name),
            )
            .size(13.5)
            .color(theme.text_secondary),
        );
        ui.add_space(18.0);
        ui.label(
            RichText::new(texts.dir_label)
                .size(13.0)
                .color(theme.text_secondary),
        );

        // ── The path field row: [drive chip | mono path] + browse ──
        let browse_w = 96.0f32;
        let row_w = ui.available_width();
        let field_w = row_w - browse_w - 16.0;
        let field_h = 44.0f32;
        let mut chip_clicked = false;
        let mut chip_rect_out = egui::Rect::ZERO;
        ui.allocate_ui_with_layout(
            egui::vec2(row_w, field_h),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
            ui.spacing_mut().item_spacing.x = 16.0;
            let (field_rect, _) =
                ui.allocate_exact_size(egui::vec2(field_w, field_h), egui::Sense::hover());
        let painter = ui.painter_at(field_rect);
        painter.rect_filled(field_rect, CornerRadius::same(10), theme.surface);
        let field_border = if self.drive_open {
            theme.primary
        } else {
            border_idle
        };
        painter.rect_stroke(
            field_rect,
            CornerRadius::same(10),
            Stroke::new(1.0, field_border),
            egui::StrokeKind::Middle,
        );

        // Drive chip (PathField's HkAffixPicker): the mount shows once on
        // the chip; picking one rewrites the path in place, keeping the
        // typed remainder (an emptied box falls back to the root).
        let matched_mount = self
            .drives
            .iter()
            .find(|(m, _, _)| {
                self.dir
                    .get(..m.len())
                    .map_or(false, |prefix| prefix.eq_ignore_ascii_case(m))
            })
            .map(|(m, _, _)| m.clone());
        let mount = matched_mount.clone().unwrap_or_else(|| {
            self.drives
                .first()
                .map(|(m, _, _)| m.clone())
                .unwrap_or_default()
        });
        let chip_w = (40.0 + mount.len() as f32 * 9.0).min(field_w * 0.4);
        let chip_rect = egui::Rect::from_min_size(
            pos2(field_rect.left() + 6.0, field_rect.top() + 6.0),
            egui::vec2(chip_w, field_h - 12.0),
        );
        let chip_resp = ui
            .allocate_new_ui(
                egui::UiBuilder::new()
                    .max_rect(chip_rect)
                    .layout(Layout::left_to_right(Align::Center)),
                |ui| {
                    let (chip, chip_resp) =
                        ui.allocate_exact_size(chip_rect.size(), egui::Sense::click());
                    if chip_resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    let hover_t = ui.ctx().animate_bool_with_time(
                        chip_resp.id.with("hover"),
                        chip_resp.hovered() || self.drive_open,
                        0.12,
                    );
                    if hover_t > 0.0 {
                        ui.painter_at(chip).rect_filled(
                            chip,
                            CornerRadius::same(8),
                            mix(theme.background, theme.primary, 0.10 * hover_t),
                        );
                    }
                    ui.painter().text(
                        pos2(chip.left() + 12.0, chip.center().y + 1.5),
                        egui::Align2::LEFT_CENTER,
                        &mount,
                        egui::FontId::monospace(13.0),
                        theme.text,
                    );
                    ui.painter().image(
                        icons.chevron.id(),
                        egui::Rect::from_center_size(
                            pos2(chip.right() - 14.0, chip.center().y - 1.0),
                            egui::vec2(12.0, 12.0),
                        ),
                        egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        theme.text_secondary,
                    );
                    chip_clicked = chip_resp.clicked();
                    chip_rect_out = chip;
                    chip_resp
                },
            )
            .inner;
        if chip_clicked {
            self.drive_open = !self.drive_open;
        }

        // The mono path remainder inside the field. Baseline-true
        // centering: egui's TextEdit draws from its rect's TOP edge (+
        // its own margin), so a full-height edit rect left the text
        // riding high in the 44pt row - the edit gets a text-line rect
        // centered on the field's own center instead.
        let line_h = ui
            .painter()
            .layout_no_wrap("Ag".to_owned(), egui::FontId::monospace(13.0), Color32::WHITE)
            .size()
            .y;
        let edit_h = (line_h + 6.0).min(field_rect.height());
        let edit_left = chip_rect.right() + 10.0;
        let edit_right = field_rect.right() - 12.0;
        let input_rect = egui::Rect::from_center_size(
            pos2((edit_left + edit_right) / 2.0, field_rect.center().y),
            vec2(edit_right - edit_left, edit_h),
        );
        let rest = self
            .dir
            .strip_prefix(mount.as_str())
            .unwrap_or(&self.dir)
            .trim_start_matches(['\\', '/'])
            .to_string();
        let mut rest_edit = rest.clone();
        ui.allocate_new_ui(
            egui::UiBuilder::new().max_rect(input_rect),
            |ui| {
                TextEdit::singleline(&mut rest_edit)
                    .frame(false)
                    // The line box centers mathematically at margin 3,
                    // but mono glyphs render optically high in it —
                    // 5/1 shifts the run down the 2pt the eye expects.
                    .margin(egui::Margin {
                        left: 0,
                        right: 0,
                        top: 7,
                        bottom: 0,
                    })
                    .desired_width(input_rect.width())
                    .text_color(theme.text)
                    .font(egui::FontId::monospace(13.0))
                    .show(ui);
            },
        );
        if rest_edit != rest {
            self.dir = format!("{mount}{rest_edit}");
        }

        // Browse: the shared ghost button — borderless, icon and label
        // on one optical line, and the row's spacing keeps it off the
        // field.
        let browse_resp = Self::ghost_icon_button(
            ui,
            theme,
            &icons,
            &icons.folder,
            texts.browse.as_str(),
            browse_w,
            field_h,
        );
        if browse_resp.clicked() {
            if let Some(picked) =
                rfd::FileDialog::new().set_title(texts.browse_title).pick_folder()
            {
                self.dir = nested_dir(&self.config, &picked.to_string_lossy());
            }
        }
            },
        );

        // Hint + the unwritable warning + the product/flavor line.
        ui.add_space(12.0);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 90.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.label(
                    RichText::new(texts.target_hint_local)
                        .size(13.0)
                        .color(theme.text_secondary),
                );
                if self.dir_writable == Some(false) {
                    ui.add_space(6.0);
                    let has_writable = self
                        .candidates
                        .iter()
                        .any(|(_, writable, _)| *writable);
                    let warn = if has_writable {
                        texts.warn_unwritable.clone()
                    } else {
                        texts.warn_no_writable.clone()
                    };
                    ui.label(RichText::new(warn).size(13.0).color(theme.error));
                }
                ui.add_space(6.0);
                let flavor_label = match self.flavor.as_str() {
                    "full-webview2" => texts.flavor_full_webview2.as_str(),
                    "full" => texts.flavor_full.as_str(),
                    other => other,
                };
                ui.label(
                    RichText::new(format!(
                        "{} {} · {}",
                        self.config.product.name, self.config.product.version, flavor_label
                    ))
                    .size(13.0)
                    .color(theme.text_secondary),
                );
                // The flash-target notice (web `.wizard-target__
                // flash-hint`): a declared Flash target reads as a
                // pending post-install step.
                if self
                    .config
                    .targets
                    .iter()
                    .any(|t| matches!(t, TargetConfig::Flash(_)))
                {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(texts.flash_hint.as_str())
                            .size(12.0)
                            .color(theme.text_tertiary),
                    );
                }
                // The optional-components block (web `.wizard-
                // attachments`): one checkbox row per declared
                // attachment; bundled ones lock checked with the badge.
                if !self.attachments.is_empty() {
                    ui.add_space(14.0);
                    ui.label(
                        RichText::new(texts.attach_title.as_str())
                            .size(13.0)
                            .color(theme.text_secondary),
                    );
                    ui.add_space(6.0);
                    for index in 0..self.attachments.len() {
                        let (key, title, included, size, _) = self.attachments[index].clone();
                        ui.horizontal(|ui| {
                            if included {
                                // Locked-on: the same plate, a static
                                // check, no click target.
                                let (rect, _) = ui.allocate_exact_size(
                                    Vec2::new(18.0, 18.0),
                                    egui::Sense::hover(),
                                );
                                let painter = ui.painter_at(rect.expand(4.0));
                                let c = rect.center();
                                painter.circle_filled(c, 9.0, theme.primary);
                                let stroke = Stroke::new(1.8f32, theme.on_primary);
                                painter.line_segment(
                                    [pos2(c.x - 3.6, c.y + 0.4), pos2(c.x - 1.0, c.y + 3.0)],
                                    stroke,
                                );
                                painter.line_segment(
                                    [pos2(c.x - 1.0, c.y + 3.0), pos2(c.x + 4.2, c.y - 2.8)],
                                    stroke,
                                );
                                ui.add_space(12.0);
                            } else {
                                let picked = &mut self.attachments[index].4;
                                Self::circle_checkbox(ui, theme, picked, "", None);
                                ui.add_space(12.0);
                                let _ = key;
                            }
                            ui.label(
                                RichText::new(title).size(13.0).color(theme.text),
                            );
                            ui.add_space(10.0);
                            let meta = if included {
                                texts.attach_bundled.clone()
                            } else {
                                size.map(|bytes| {
                                    format!("{:.1} MB", bytes as f32 / 1_048_576.0)
                                })
                                .unwrap_or_default()
                            };
                            ui.label(
                                RichText::new(meta).size(12.0).color(theme.text_tertiary),
                            );
                        });
                        ui.add_space(6.0);
                    }
                }
            },
        );

        // The drive picker popup, dropping below the chip like
        // HkSelectPanel: mount left, localized kind (+ volume label)
        // right; the current mount carries a soft wash.
        if self.drive_open {
            let mut popup_rect = chip_rect_out;
            egui::Area::new(egui::Id::new("drive-picker"))
                .order(egui::Order::Foreground)
                .fixed_pos(chip_rect_out.left_bottom() + egui::vec2(0.0, 4.0))
                .show(ui.ctx(), |ui| {
                    let popup = egui::Frame::default()
                        .fill(theme.surface)
                        .stroke(Stroke::new(1.0, border_idle))
                        .shadow(egui::Shadow {
                            offset: [0, 8],
                            blur: 24,
                            color: Color32::from_black_alpha(40),
                            ..Default::default()
                        })
                        .corner_radius(CornerRadius::same(10))
                        .inner_margin(Margin::same(6))
                        .show(ui, |ui| {
                            ui.set_width(field_w - 12.0);
                            // Two-level picker (user direction): each
                            // drive is a group header; its config-driven
                            // default candidates nest beneath it. Click
                            // either — both land on pad_root_dir, the
                            // typed remainder survives a drive switch.
                            egui::ScrollArea::vertical()
                                .id_salt(egui::Id::new("drive-picker-scroll"))
                                .max_height(9.0 * 30.0)
                                .show(ui, |ui| {
                            for (mount, kind, label) in self.drives.clone() {
                                let kind_label = match kind.as_str() {
                                    "removable" => texts.kind_removable.clone(),
                                    "fixed" => texts.kind_fixed.clone(),
                                    "network" => texts.kind_network.clone(),
                                    "cdrom" => texts.kind_cdrom.clone(),
                                    "ramdisk" => texts.kind_ramdisk.clone(),
                                    _ => texts.kind_unknown.clone(),
                                };
                                let meta = match label.as_deref() {
                                    Some(l) if !l.is_empty() => {
                                        format!("{kind_label} · {l}")
                                    }
                                    _ => kind_label.to_string(),
                                };
                                let drive_active = self.dir.starts_with(&mount);
                                let (row, row_resp) = ui.allocate_exact_size(
                                    egui::vec2(ui.available_width(), 30.0),
                                    egui::Sense::click(),
                                );
                                if row_resp.hovered() {
                                    ui.ctx()
                                        .set_cursor_icon(egui::CursorIcon::PointingHand);
                                }
                                let rp = ui.painter_at(row);
                                if row_resp.hovered() || drive_active {
                                    rp.rect_filled(
                                        row,
                                        CornerRadius::same(8),
                                        mix(theme.background, theme.primary, 0.08),
                                    );
                                }
                                rp.text(
                                    pos2(row.left() + 10.0, row.center().y + 1.0),
                                    egui::Align2::LEFT_CENTER,
                                    &mount,
                                    egui::FontId::monospace(13.0),
                                    if drive_active {
                                        theme.primary
                                    } else {
                                        theme.text
                                    },
                                );
                                rp.text(
                                    pos2(row.right() - 10.0, row.center().y - 0.5),
                                    egui::Align2::RIGHT_CENTER,
                                    meta,
                                    egui::FontId::proportional(11.0),
                                    theme.text_secondary,
                                );
                                let switch_drive = |dir: &mut String, mount: &str| {
                                    // PathField's contract: strip the old
                                    // prefix, keep the typed remainder.
                                    let old_mount = self
                                        .drives
                                        .iter()
                                        .find(|(m, _, _)| dir.starts_with(m))
                                        .map(|(m, _, _)| m.clone())
                                        .unwrap_or_default();
                                    let rest = dir
                                        .strip_prefix(old_mount.as_str())
                                        .map(|r| r.trim_start_matches(['\\', '/']))
                                        .unwrap_or("")
                                        .to_string();
                                    *dir = format!("{mount}{rest}");
                                };
                                if row_resp.clicked() {
                                    switch_drive(&mut self.dir, &mount);
                                    self.probed_dir.clear();
                                }
                                // The drive's config-driven defaults,
                                // indented beneath the group header.
                                for (ckind, cwritable, cpath) in self.candidates.clone() {
                                    if !cpath
                                        .get(..mount.len())
                                        .map_or(false, |prefix| {
                                            prefix.eq_ignore_ascii_case(&mount)
                                        })
                                    {
                                        continue;
                                    }
                                    let clabel = match ckind.as_str() {
                                        "appdata" => "AppData".to_string(),
                                        "program-files" => "Program Files".to_string(),
                                        _ => cpath.clone(),
                                    };
                                    let (crow, crow_resp) = ui.allocate_exact_size(
                                        egui::vec2(ui.available_width(), 30.0),
                                        egui::Sense::click(),
                                    );
                                    if crow_resp.hovered() {
                                        ui.ctx().set_cursor_icon(
                                            egui::CursorIcon::PointingHand,
                                        );
                                    }
                                    let icon = if cwritable {
                                        match ckind.as_str() {
                                            "appdata" => &icons.app_window,
                                            _ => &icons.hard_drive,
                                        }
                                    } else {
                                        &icons.alert
                                    };
                                    let icon_tint = if cwritable {
                                        theme.text
                                    } else {
                                        theme.text_secondary.gamma_multiply(0.55)
                                    };
                                    let crp = ui.painter_at(crow);
                                    if crow_resp.hovered() {
                                        crp.rect_filled(
                                            crow,
                                            CornerRadius::same(8),
                                            mix(theme.background, theme.primary, 0.08),
                                        );
                                    }
                                    // Baseline-true pairing: the icon rides
                                    // the label's own baseline (the shared
                                    // helper), one consistent 10pt gap to
                                    // its right.
                                    let icon_center = Self::icon_center_for(
                                        &crp,
                                        &clabel,
                                        egui::FontId::monospace(12.0),
                                        pos2(crow.left() + 24.0, crow.center().y),
                                    );
                                    crp.image(
                                        icon.id(),
                                        egui::Rect::from_center_size(
                                            icon_center,
                                            egui::vec2(13.0, 13.0),
                                        ),
                                        egui::Rect::from_min_max(
                                            pos2(0.0, 0.0),
                                            pos2(1.0, 1.0),
                                        ),
                                        icon_tint,
                                    );
                                    crp.text(
                                        pos2(crow.left() + 42.0, crow.center().y),
                                        egui::Align2::LEFT_CENTER,
                                        &clabel,
                                        egui::FontId::monospace(12.0),
                                        if cwritable {
                                            theme.text
                                        } else {
                                            theme.text_secondary
                                        },
                                    );
                                    if crow_resp.clicked() {
                                        self.dir = shun::wizard::pad_root_dir(
                                            &self.config,
                                            &cpath,
                                        );
                                        self.probed_dir.clear();
                                        self.drive_open = false;
                                    }
                                }
                            }
                                });
                        });
                    popup_rect = popup.response.rect;
                });
            let hover_pos = ui.input(|i| i.pointer.hover_pos());
            let outside = ui.input(|i| i.pointer.any_click())
                && !chip_clicked
                && hover_pos.map_or(true, |p| {
                    !popup_rect.contains(p) && !chip_rect_out.contains(p)
                });
            if outside {
                self.drive_open = false;
            }
        }

        // Live writability probe on change (drives the warning and the
        // accent candidate) — the same probe the web face rides.
        if self.probed_dir != self.dir {
            self.probed_dir = self.dir.clone();
            self.dir_writable = Some(shun::fs_probe::is_dir_writable(std::path::Path::new(
                self.dir.trim(),
            )));
        }
    }

    /// The license pane: the agreement documents (paged when several
    /// resolve) with an accept checkbox gating all of them.
    fn license_view(&mut self, ui: &mut egui::Ui) {
        let theme = &self.theme;
        let texts = self.texts.clone();
        // Snapshot the documents so the ui closures below can mutate
        // wizard state freely (the step borrow would otherwise span the
        // checkbox and pager). The documents follow the wizard language:
        // the per-locale map (shun-license-docs.json) first, then the
        // locale-less pipeline resolution as the fallback.
        let docs: Vec<(Option<String>, String)> = {
            let localized = self
                .license_docs
                .get(&self.language)
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

        // The web face's license step leads with its title + sub
        // (license.title / license.sub), left-aligned like every pane.
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 90.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.label(
                    RichText::new(texts.license_title.as_str())
                        .strong()
                        .size(22.0)
                        .color(theme.text),
                );
                ui.add_space(10.0);
                ui.label(
                    RichText::new(texts.license_sub.as_str())
                        .size(13.5)
                        .color(theme.text_secondary),
                );
            },
        );
        ui.add_space(8.0);
        Frame::default()
            .fill(theme.surface)
            .stroke(Stroke::new(1.0f32, theme.border))
            .inner_margin(Margin::same(12))
            .corner_radius(CornerRadius::same(10))
            .show(ui, |ui| {
                // Documents read left-aligned regardless of the pane's
                // centered text alignment — like the webview card.
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("license-body")
                        .auto_shrink([false, false])
                        .max_height(ui.available_height() - 44.0)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            if let Some(title) = title {
                                ui.label(
                                    RichText::new(title).strong().size(15.0).color(theme.text),
                                );
                                ui.add_space(6.0);
                            }
                            ui.label(
                                RichText::new(body).size(12.5).color(theme.text_secondary),
                            );
                        });
                });
            });
        // Pager for multi-document licenses: [<] left, the position
        // indicator centered between, [>] right (disabled at the ends).
        if total > 1 {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let pager_button = |ui: &mut egui::Ui, glyph: &str, enabled: bool| {
                    Self::hand(ui.add_enabled(
                        enabled,
                        Button::new(RichText::new(glyph).size(13.0).color(theme.text_secondary))
                            .fill(theme.surface)
                            .stroke(Stroke::new(1.0f32, theme.border))
                            .corner_radius(CornerRadius::same(6))
                            .min_size(Vec2::new(44.0, 24.0)),
                    ))
                };
                if pager_button(ui, "[<]", index > 0).clicked() {
                    self.license_doc_index = index - 1;
                }
                let indicator = format!(
                    "{}/{} {}",
                    index + 1,
                    total,
                    title.unwrap_or(texts.step_license.as_str())
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
        Self::circle_checkbox(ui, self.theme, &mut self.license_accepted, self.texts.license_agree.as_str(), None);
    }

    /// Progress view (the "install" pane), the web layout verbatim: the
    /// progress block centers in the space ABOVE, the log strip pins to
    /// the pane's bottom edge at full pane width (a hairline's gap
    /// between them). The strip's height is reserved BEFORE the block
    /// centers, so the two can never overlap or push past the window.
    fn running_view(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        let uninstalling = self.uninstalling == Some(true);

        let strip_h = self.terminal.height_hint();
        // The unified fixed-origin column: the progress block starts at
        // the pane's top-left inset; the log strip pins to the pane's
        // bottom edge. Nothing centers, nothing drifts.
        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), 140.0),
            Layout::top_down(Align::LEFT),
            |ui| {
            if let Some(logo) = &self.logo {
                ui.add(egui::Image::from_texture(logo).fit_to_exact_size(Vec2::splat(56.0)));
                ui.add_space(16.0);
            }
            ui.label(
                RichText::new(if uninstalling {
                    self.texts.uninstalling.clone()
                } else {
                    self.config.product.name.clone()
                })
                .strong()
                .size(18.0)
                .color(theme.text),
            );
            ui.add_space(16.0);
            if uninstalling {
                // Uninstalling has no payload phases to weigh — the
                // indeterminate sweep, like the web page's loading bar.
                let t = ui.input(|i| i.time) as f32;
                let sweep = (t * 0.9).sin() * 0.5 + 0.5;
                ui.ctx().request_repaint();
                let bar = egui::ProgressBar::new(sweep.clamp(0.02, 0.98))
                    .fill(theme.primary)
                    .corner_radius(CornerRadius::same(8));
                ui.add(bar.desired_width(320.0).desired_height(8.0));
            } else {
                let percent = self.overall.unwrap_or(0);
                let bar = egui::ProgressBar::new(f32::from(percent) / 100.0)
                    .show_percentage()
                    .fill(theme.primary)
                    .corner_radius(CornerRadius::same(8));
                ui.add(bar.desired_width(320.0).desired_height(16.0));
            }
            ui.add_space(10.0);
            // The live step under the bar — the one thing actually
            // happening (the percent headline rides the bar itself).
            let step = self
                .progress
                .as_ref()
                .map(|(step, _)| step.clone())
                .unwrap_or_else(|| self.texts.installing.clone());
            ui.label(RichText::new(step).size(12.0).color(theme.text_tertiary));
        });
        // Pin the strip to the pane's bottom edge: fill the gap between
        // the block and the strip with exactly the space that remains.
        let gap = ui.max_rect().bottom() - strip_h - 18.0 - ui.cursor().top();
        ui.add_space(gap.max(0.0));
        // The web logs block: hairline separator above the strip, then
        // 10pt of air before the header bar.
        let (sep, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
        ui.painter()
            .rect_filled(sep, CornerRadius::ZERO, theme.border);
        ui.add_space(10.0);
        self.log_view(ui);
    }

    /// Result view (the "done" pane or the failure alert).
    fn finished_view(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        let texts = self.texts.clone();
        let desktop_asks = desktop_policy_asks(&self.config);
        ui.add_space(8.0);
        // Clone the outcome out of self: the pane below mutates the
        // desktop-shortcut answer while painting.
        let outcome = self.outcome.clone();
        match outcome.as_ref().expect("Finished implies an outcome") {
            Outcome::InstallOk | Outcome::UninstallOk => {
                let (title, path) = match outcome.as_ref().expect("checked above") {
                    Outcome::UninstallOk => (self.texts.done_uninstall.clone(), None),
                    _ => (
                        self.texts.done_title.clone(),
                        Some(self.dir.trim().trim_end_matches('\\').to_string()),
                    ),
                };
                // The done hero, left-aligned at the column origin like
                // every pane: green ring check, the success headline in
                // the success color, the install path in monospace, then
                // the hint and the shortcut answers.
                ui.add_space(16.0);
                {
                    Self::hero_check(ui, &theme);
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(title)
                            .strong()
                            .size(18.0)
                            .color(theme.success),
                    );
                    if let Some(path) = path {
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(path)
                                .size(13.0)
                                .color(theme.text_secondary)
                                .font(egui::FontId::monospace(13.0)),
                        );
                    }
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(self.hint())
                            .size(12.0)
                            .color(theme.text_tertiary),
                    );
                    // The done-page answers — the shared driver's flow
                    // creates nothing, so the finish button applies
                    // them. Three checkboxes stack left-aligned inside a
                    // fixed-width block that centers as a unit (the web
                    // `.wizard-done__shortcuts`); a portable run keeps
                    // only the launch answer (its shortcuts point
                    // nowhere).
                    if matches!(outcome.as_ref(), Some(Outcome::InstallOk)) {
                        ui.add_space(12.0);
                        let product = self.config.product.name.clone();
                        let with_product =
                            |raw: &str| -> String { raw.replace("%PRODUCT%", &product) };
                        let menu_asks = menu_policy_asks(&self.config);
                        let portable = self.mode == "portable";
                        // The answers stack left-aligned at the origin —
                        // the web `.wizard-done__shortcuts` block; a
                        // portable run keeps only the launch answer (its
                        // shortcuts point nowhere).
                        if menu_asks && !portable {
                            Self::circle_checkbox(
                                ui,
                                theme,
                                &mut self.start_menu,
                                with_product(&texts.menu_shortcut).as_str(),
                                None,
                            );
                            ui.add_space(8.0);
                        }
                        if desktop_asks && !portable {
                            Self::circle_checkbox(
                                ui,
                                theme,
                                &mut self.desktop_shortcut,
                                with_product(&texts.desktop_shortcut).as_str(),
                                None,
                            );
                            ui.add_space(8.0);
                        }
                        Self::circle_checkbox(
                            ui,
                            theme,
                            &mut self.launch_after,
                            with_product(&texts.launch_after).as_str(),
                            None,
                        );
                    }
                }
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
        // The done page shows the log ONLY on the failure variant (the
        // web failure pane keeps the trail beside the retry action);
        // success is clean — no log chrome on the celebration page.
        if matches!(self.outcome, Some(Outcome::Failed(_))) {
            ui.add_space(8.0);
            self.log_view(ui);
        }
    }

    fn log_view(&mut self, ui: &mut egui::Ui) {
        use shun::config::LogVerbosity;
        if self.log_level == LogVerbosity::Off {
            return;
        }
        let theme = &self.theme;
        self.terminal.render(ui, theme, self.texts.log.as_str());
    }

    /// The done hero's check ring (hikari's CheckCircle2 at 56pt) — the
    /// success glyph of the finished pane AND the uninstall page.
    fn hero_check(ui: &mut egui::Ui, theme: &Theme) {
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(56.0, 56.0), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        let c = rect.center();
        let stroke = Stroke::new(3.0f32, theme.success);
        painter.circle_stroke(c, 24.0, stroke);
        painter.line_segment(
            [pos2(c.x - 9.0, c.y + 1.0), pos2(c.x - 2.5, c.y + 8.0)],
            stroke,
        );
        painter.line_segment(
            [pos2(c.x - 2.5, c.y + 8.0), pos2(c.x + 10.0, c.y - 7.0)],
            stroke,
        );
    }

    /// The failure hero's cross ring (hikari's XCircle at 56pt) — the
    /// uninstall page's failure glyph.
    fn hero_cross(ui: &mut egui::Ui, theme: &Theme) {
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(56.0, 56.0), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        let c = rect.center();
        let stroke = Stroke::new(3.0f32, theme.error);
        painter.circle_stroke(c, 24.0, stroke);
        painter.line_segment(
            [pos2(c.x - 9.0, c.y - 9.0), pos2(c.x + 9.0, c.y + 9.0)],
            stroke,
        );
        painter.line_segment(
            [pos2(c.x + 9.0, c.y - 9.0), pos2(c.x - 9.0, c.y + 9.0)],
            stroke,
        );
    }

    /// Spawns the uninstall page's worker. Uninstall drives the shared
    /// uninstall (the ARP contract — the install dir this process sits
    /// in); repair re-runs the local delivery over that same dir, the
    /// web face's `start_install { mode: "local", dir: current_dir }`
    /// verbatim (no nesting pass, no shortcut finish).
    fn spawn_uninstall_worker(&mut self, ctx: &Context, repair: bool) {
        self.stage = Stage::Running;
        self.progress = None;
        self.phases_done.clear();
        self.phase_active = None;
        self.terminal.clear();
        self.overall = None;
        self.uninstalling = Some(!repair);
        self.repairing = repair;
        self.entry = None;

        let (sender, receiver) = channel();
        self.receiver = receiver;
        let repaint = ctx.clone();
        let payload = self.payload.clone();
        let config = self.config.clone();
        std::thread::spawn(move || {
            let core = WizardCore::new(config, BTreeMap::new());
            let result = if repair {
                let dir = shun::wizard::current_exe_dir()
                    .map(|dir| dir.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let request = InstallRequest {
                    mode: "local".into(),
                    dir,
                    language: None,
                    machine: false,
                };
                shun::wizard::run_install(&core, &payload, &request, &mut |event| {
                    let _ = sender.send(WorkerMsg::Event(event.clone()));
                    repaint.request_repaint();
                })
                .map_err(|e| e.to_string())
            } else {
                shun::wizard::run_uninstall(&core).map_err(|e| e.to_string())
            };
            let _ = sender.send(WorkerMsg::Done(result));
            repaint.request_repaint();
        });
    }

    /// The standalone uninstaller page — the web face's uninstall pane,
    /// state for state: confirm (cancel · repair · danger uninstall) →
    /// indeterminate running (the repair variant relabels) → done/failed
    /// hero with a single close. The pane carries every action; the
    /// wizard's rail, footer and degradation banner are suppressed.
    fn uninstall_view(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        let texts = self.texts.clone();
        // %PRODUCT% substitution — the same token the web face's
        // withProduct() resolves everywhere.
        let product = self.config.product.name.clone();
        let with_product = |raw: &str| raw.replace("%PRODUCT%", &product);

        // hikari button analogs, content-sized: ghost = hairline box
        // with muted text, solid = filled plate (the confirm row's
        // uninstall rides the error channel).
        let ghost = |ui: &mut egui::Ui, label: &str| {
            Self::hand(ui.add(
                Button::new(RichText::new(label).size(13.0).color(theme.text_secondary))
                    .fill(Color32::TRANSPARENT)
                    .stroke(Stroke::new(1.0f32, theme.border))
                    .corner_radius(CornerRadius::same(8))
                    .min_size(Vec2::new(0.0, 32.0)),
            ))
            .clicked()
        };
        let solid = |ui: &mut egui::Ui, label: &str, fill: Color32| {
            Self::hand(ui.add(
                Button::new(RichText::new(label).strong().size(13.5).color(theme.on_primary))
                    .fill(fill)
                    .corner_radius(CornerRadius::same(8))
                    .min_size(Vec2::new(0.0, 32.0)),
            ))
            .clicked()
        };

        match self.stage {
            // Confirm: heading + sub, then the action row.
            Stage::Configure => {
                ui.label(
                    RichText::new(with_product(&texts.un_heading))
                        .strong()
                        .size(22.0)
                        .color(theme.text),
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new(with_product(&texts.un_sub))
                        .size(13.5)
                        .color(theme.text_secondary),
                );
                ui.add_space(28.0);
                // The action row, left-aligned at the column origin —
                // the unified fixed-origin rule (the web row is
                // flex-start now too). Content-sized ghost/solid plates.
                ui.horizontal(|ui| {
                    if ghost(ui, texts.un_cancel.as_str()) {
                        std::process::exit(0);
                    }
                    if ghost(ui, with_product(&texts.un_repair).as_str()) {
                        self.spawn_uninstall_worker(ui.ctx(), true);
                    }
                    if solid(
                        ui,
                        with_product(&texts.uninstall).as_str(),
                        theme.error,
                    ) {
                        self.spawn_uninstall_worker(ui.ctx(), false);
                    }
                });
            }
            // Running: the indeterminate sweep — shun's uninstall emits
            // no percents, and the web page keeps its loading bar
            // indeterminate for the repair too.
            Stage::Running => {
                if let Some(logo) = &self.logo {
                    ui.add(egui::Image::from_texture(logo).fit_to_exact_size(Vec2::splat(56.0)));
                    ui.add_space(16.0);
                }
                let t = ui.input(|i| i.time) as f32;
                let sweep = (t * 0.9).sin() * 0.5 + 0.5;
                ui.ctx().request_repaint();
                let bar = egui::ProgressBar::new(sweep.clamp(0.02, 0.98))
                    .fill(theme.primary)
                    .corner_radius(CornerRadius::same(8));
                ui.add(bar.desired_width(320.0).desired_height(8.0));
                ui.add_space(14.0);
                ui.label(
                    RichText::new(if self.repairing {
                        with_product(&texts.un_repairing)
                    } else {
                        with_product(&texts.uninstalling)
                    })
                    .size(13.0)
                    .color(theme.text_secondary),
                );
            }
            // Terminal: the success/failure hero plus a close. The
            // repair variant relabels (repaired / repair failed).
            Stage::Finished => {
                let outcome = self.outcome.clone();
                match outcome.as_ref().expect("Finished implies an outcome") {
                    Outcome::InstallOk | Outcome::UninstallOk => {
                        Self::hero_check(ui, &theme);
                        ui.add_space(12.0);
                        let title = if self.repairing {
                            texts.done_repair.clone()
                        } else {
                            texts.done_uninstall.clone()
                        };
                        ui.label(
                            RichText::new(title).strong().size(18.0).color(theme.success),
                        );
                        ui.add_space(20.0);
                        if solid(ui, texts.un_close.as_str(), theme.primary) {
                            std::process::exit(0);
                        }
                    }
                    Outcome::Failed(err) => {
                        Self::hero_cross(ui, &theme);
                        ui.add_space(12.0);
                        let title = if self.repairing {
                            texts.failed_repair.clone()
                        } else {
                            texts.failed_uninstall.clone()
                        };
                        ui.label(RichText::new(title).strong().size(18.0).color(theme.error));
                        ui.add_space(8.0);
                        ui.label(RichText::new(err.clone()).size(12.5).color(theme.error));
                        ui.add_space(20.0);
                        if solid(ui, texts.un_close.as_str(), theme.primary) {
                            std::process::exit(0);
                        }
                    }
                }
            }
        }
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
        // No separator here: the side panel's own divider already draws
        // the line above the footer — a second one read as a double rule.
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
                let on_last_step =
                    self.pages[self.step].is_license() && self.step + 1 == self.pages.len();
                let step_blocked = self.step_blocked();
                // The license step's minimum-read countdown (web
                // parity): the primary stays disabled for the first
                // three seconds of the step, relabeling each second.
                let countdown = if configuring && on_last_step {
                    match self.license_entered {
                        Some(entered) => {
                            3u32.saturating_sub(entered.elapsed().as_secs() as u32)
                        }
                        None => 0,
                    }
                } else {
                    0
                };
                if countdown > 0 {
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
                }
                let (label, enabled) = match self.stage {
                    Stage::Configure if !on_last_step => (self.texts.next.clone(), !step_blocked),
                    Stage::Configure => (
                        if countdown > 0 {
                            self.texts
                                .agree_install_wait
                                .replace("{N}", &countdown.to_string())
                        } else {
                            self.texts.agree_install.clone()
                        },
                        !step_blocked && countdown == 0,
                    ),
                    Stage::Running => (
                        match self.uninstalling {
                            Some(true) => self.texts.uninstalling.clone(),
                            _ => self.texts.installing.clone(),
                        },
                        false,
                    ),
                    Stage::Finished => match self.outcome.as_ref() {
                        Some(Outcome::Failed(_)) => (self.texts.retry.clone(), true),
                        _ => (self.texts.finish.clone(), true),
                    },
                };
                let action = match (self.stage, self.outcome.as_ref()) {
                    (Stage::Configure, _) => Some((false, false)),
                    (Stage::Running, _) => None,
                    (Stage::Finished, Some(Outcome::Failed(_))) => Some((false, true)),
                    (Stage::Finished, _) => Some((false, false)),
                };
                if let Some((uninstalling, _)) = action {
                    if Self::hand(ui
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
                            .min_size(Vec2::new(0.0, 30.0)),
                        ))
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
                                    // answers (the shared driver's flow
                                    // creates none): apply them through
                                    // the same finish path every face
                                    // uses, then close. A portable run
                                    // carries no shortcut answers —
                                    // nothing to register — but its
                                    // launch answer still applies.
                                    if matches!(self.outcome, Some(Outcome::InstallOk)) {
                                        let portable = self.mode == "portable";
                                        let core =
                                            WizardCore::new(self.config.clone(), BTreeMap::new());
                                        let _ = shun::wizard::apply_finish(
                                            &core,
                                            self.dir.trim(),
                                            (!portable).then_some(self.desktop_shortcut),
                                            (!portable).then_some(self.start_menu),
                                            self.launch_after,
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
                                // Arriving on the license step arms its
                                // minimum-read countdown.
                                if self.pages[self.step].is_license() {
                                    self.license_entered =
                                        Some(std::time::Instant::now());
                                }
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
                        .min_size(Vec2::new(0.0, 30.0)),
                    );
                }

                // Ghost buttons to the left of the primary: back while
                // walking steps; uninstall on the last configure step and
                // after a successful install; open-folder on success.
                // hikari's secondary actions are bare text — no border
                // box — with the muted color carrying the hierarchy.
                let ghost = |ui: &mut egui::Ui, label: &str| {
                    Self::hand(ui.add(
                        Button::new(RichText::new(label).size(13.0).color(theme.text_secondary))
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::new(1.0f32, theme.border))
                            .corner_radius(CornerRadius::same(8))
                            .min_size(Vec2::new(0.0, 30.0)),
                    ))
                    .clicked()
                };
                if configuring {
                    // An INSTALLER'S configure steps never offer
                    // uninstall — there is nothing installed yet. The
                    // action appears on the done page (and the
                    // standalone uninstaller face) only.
                    if self.step > 0 && ghost(ui, self.texts.back.as_str()) {
                        self.license_doc_index = 0;
                        if self.pages[self.step].is_license() {
                            self.license_entered = None;
                        }
                        self.step -= 1;
                    }
                } else if self.stage == Stage::Finished
                    && matches!(self.outcome, Some(Outcome::InstallOk))
                {
                    // 打开安装目录 ONLY. No uninstall next to the finish
                    // button — the flow just succeeded and the ARP entry
                    // (or the standalone uninstaller page) owns removal;
                    // offering it here read as "uninstall what you just
                    // installed".
                    if ghost(ui, self.texts.open_dir.as_str()) {
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
        // The license pane gates progression on the accept box.
        self.pages[self.step].is_license() && !self.license_accepted
    }
}

impl eframe::App for FallbackApp {
    fn update(&mut self, ctx: &Context, frame: &mut eframe::Frame) {
        self.drain_worker();
        self.drain_wallpaper(ctx);
        // Frameless windows lose BOTH the rounded corners and the
        // shadow until DWMWCP_ROUND lands. Apply it here — our OWN
        // window handle, not a title lookup (both faces share one
        // title, so a title poll would round the other instance's
        // window).
        if !self.dwm_rounded {
            self.dwm_rounded = true;
            #[cfg(windows)]
            apply_dwm_rounding(frame);
        }
        let theme = self.theme;

        // The wallpaper backdrop (first IMAGE source): painted cover-
        // style over the full window behind every panel, tinted down so
        // content keeps its contrast (the web backdrop's overlay look).
        if let Some(texture) = &self.wallpaper {
            let screen = ctx.screen_rect();
            let painter = ctx.layer_painter(egui::LayerId::background());
            let scale = (screen.width() / texture.size_vec2().x)
                .max(screen.height() / texture.size_vec2().y);
            let size = texture.size_vec2() * scale;
            painter.image(
                texture.id(),
                egui::Rect::from_center_size(screen.center(), size),
                egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                // The backdrop sits at ~35% strength over the token
                // background — full-bleed wallpaper under an installer's
                // dense text needs the dimming to stay legible.
                Color32::from_white_alpha(89),
            );
        }

        // ── Caption bar (frameless chrome): one flat HkTitleBar band.
        egui::TopBottomPanel::top("titlebar")
            .frame(Frame::default().fill(theme.surface).inner_margin(Margin::ZERO))
            .show(ctx, |ui| {
                self.title_bar(ui);
            });

        // ── Footer: live step + nav. The standalone uninstall page
        // carries its actions in the pane itself — no footer, exactly
        // like the web face's uninstall layout.
        if !self.uninstall_mode {
            egui::TopBottomPanel::bottom("footer")
                .frame(
                    Frame::default()
                        .fill(theme.background)
                        .inner_margin(Margin::symmetric(20, 6)),
                )
                .show(ctx, |ui| {
                    self.footer(ui, ctx);
                });
        }

        // ── Optional left rail (`shell.timeline = "left"`). The
        // uninstall page has no step rail to draw.
        let timeline_left = self.timeline_left;
        if timeline_left && !self.uninstall_mode {
            egui::SidePanel::left("timeline")
                .exact_width(200.0)
                .frame(
                    Frame::default()
                        // The brightness split, web-face direction: the
                        // rail sits a shade DARKER than the pane's page
                        // background (the old surface fill inverted it);
                        // a manifest `rail-background` overrides the
                        // split outright.
                        .fill(theme.rail_bg_override.unwrap_or_else(|| {
                            mix(theme.background, theme.text, 0.04)
                        }))
                        // Top inset 54: the first node's CENTER lands
                        // at 32+54+12 = 98pt — the heading's first-line
                        // center (the heading tops out at 32+52=84).
                        .inner_margin(Margin {
                            left: 16,
                            right: 16,
                            top: 54,
                            bottom: 16,
                        }),
                )
                .show(ctx, |ui| {
                    self.timeline(ui, true);
                });
        }

        // ── Content pane.
        egui::CentralPanel::default()
            .frame(
                Frame::default()
                    // The pane layer: the token background, or the
                    // manifest's `pane-background` when it declares one.
                    // ZERO margin — the content rect below owns ALL the
                    // insets, so the cross-face numbers are exact (the
                    // web pane's 48/40px padding, measured from the same
                    // rail edge).
                    .fill(theme.pane_bg_override.unwrap_or(theme.background))
                    .inner_margin(Margin::ZERO),
            )
            .show(ctx, |ui| {
                if !timeline_left && !self.uninstall_mode {
                    self.timeline(ui, false);
                    ui.add_space(10.0);
                }
                // The degradation banner is information, not decoration:
                // show it only when the runtime forced the fallback (no
                // WebView2). A deliberate --no-webview launch needs no
                // warning about itself, and the uninstall page mirrors
                // the web uninstaller (which never banners).
                if self.reason == FallbackReason::MissingWebview2 && !self.uninstall_mode {
                    self.banner(ui);
                    ui.add_space(12.0);
                }
                // THE unified content origin (user direction: both faces
                // left-align everything at a FIXED inset — no per-page
                // centering, so the heading parks at the same spot on
                // every page and the distance from the pane's left edge
                // never varies with content). 48/48 lateral matches the
                // web pane's padding; PAD_T is 40 + the 12px titlebar
                // clearance the web pane carries in its padding-top calc,
                // so both headings land 84pt under the window top.
                const PAD_L: f32 = 48.0;
                const PAD_T: f32 = 52.0;
                const PAD_R: f32 = 48.0;
                let outer = ui.max_rect();
                let content = egui::Rect::from_min_max(
                    pos2(outer.left() + PAD_L, outer.top() + PAD_T),
                    pos2(outer.right() - PAD_R, outer.bottom()),
                );
                ui.allocate_new_ui(
                    egui::UiBuilder::new()
                        .max_rect(content)
                        .layout(Layout::top_down(Align::LEFT)),
                    |ui| {
                        ui.set_width(content.width());
                        // The standalone uninstaller page replaces the
                        // wizard panes entirely (one flow, every face).
                        if self.uninstall_mode {
                            self.uninstall_view(ui);
                            return;
                        }
                        match self.stage {
                            Stage::Configure => self.configure_view(ui),
                            Stage::Running => self.running_view(ui),
                            Stage::Finished => self.finished_view(ui),
                        }
                    },
                );
            });
    }
}

/// Windows 11 DWM corner rounding for the frameless window — the
/// system shadow rides on it. Takes the frame so the handle is OUR
/// window: the caption title is shared by every face instance, so a
/// title lookup would round a neighbor.
#[cfg(windows)]
fn apply_dwm_rounding(frame: &eframe::Frame) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = frame.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(win) = handle.as_raw() else {
        return;
    };
        const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
        const DWMWCP_ROUND: u32 = 2;
        // SAFETY: plain dwmapi call with our own window handle and a
        // 4-byte attribute.
        unsafe {
            windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute(
                win.hwnd.get() as *mut core::ffi::c_void,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &DWMWCP_ROUND as *const u32 as *const core::ffi::c_void,
                4,
            );
        }
}
