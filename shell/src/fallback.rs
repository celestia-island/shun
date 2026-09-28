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
        Some(shun::config::ThemeMode::System) => {
            let (lat, lng) = solar::timezone_estimate();
            if solar::prefers_light(lat, lng) {
                Theme::light(accent)
            } else {
                Theme::dark(accent)
            }
        }
        Some(shun::config::ThemeMode::Dark) | None => Theme::dark(accent),
    };
    // The manifest's background tints the tokens: solid applies as-is, a
    // gradient mixes to its midpoint (egui paints flat fills). Panes and
    // the rail then derive from it, so the whole face shifts with the
    // manifest's theme.
    let background = shell
        .theme
        .as_ref()
        .and_then(|theme| theme.background.as_ref());
    match background {
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
        // Wallpapers have no egui renderer — the face stays on the
        // token background (the banner already says "no effects").
        _ => {}
    }
    theme
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

// ── The solar theme clock — ported 1:1 from hikari's useSolarTime.ts ────
//
// `system` theme mode resolves day/night from the SUN, not the OS
// preference: the sun's altitude at the device's coordinates decides
// (day → light, dusk/night → dark). Coordinates cascade like hikari's:
// the Windows location service fix when one lands, else the first-paint
// estimate (hikari's default latitude + the timezone-offset longitude).
// The clock re-evaluates every five minutes, so dawn and dusk flip the
// face while it sits open.
mod solar {
    const DEG: f64 = core::f64::consts::PI / 180.0;
    const RAD: f64 = 180.0 / core::f64::consts::PI;

    fn to_julian_date(unix_ms: i64) -> f64 {
        unix_ms as f64 / 86_400_000.0 + 2_440_587.5
    }

    fn greenwich_sidereal_time(jd: f64) -> f64 {
        let t = (jd - 2_451_545.0) / 36_525.0;
        let mut theta = (280.460_618_37
            + 360.985_647_366_29 * (jd - 2_451_545.0)
            + 0.000_387_933 * t * t
            - t * t * t / 38_710_000.0)
            % 360.0;
        if theta < 0.0 {
            theta += 360.0;
        }
        theta
    }

    /// Sun declination (radians) and right ascension (degrees).
    fn sun_equatorial(jd: f64) -> (f64, f64) {
        let t = (jd - 2_451_545.0) / 36_525.0;
        let l0 = (280.466_46 + 36_000.769_83 * t) % 360.0;
        let m = ((357.529_11 + 35_999.050_29 * t) % 360.0) * DEG;
        let c = (1.914_6 - 0.004_817 * t) * m.sin() + (0.019_993 - 0.000_101 * t) * (2.0 * m).sin();
        let mut sun_lon = (l0 + c) % 360.0;
        if sun_lon < 0.0 {
            sun_lon += 360.0;
        }
        let omega = (125.04 - 1_934.136 * t) * DEG;
        let lambda = sun_lon * DEG - 0.005_69 * DEG - 0.004_78 * DEG * omega.sin();
        let epsilon = (23.439_291 - 0.013_004 * t) * DEG;
        let decl = (epsilon.sin() * lambda.sin()).asin();
        let ra = (epsilon.cos() * lambda.sin()).atan2(lambda.cos());
        (decl, ra * RAD)
    }

    /// The sun's altitude in degrees at the given coordinates right now.
    pub(crate) fn solar_altitude(lat_deg: f64, lng_deg: f64) -> f64 {
        let unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let jd = to_julian_date(unix_ms);
        let (decl, ra) = sun_equatorial(jd);
        let mut ha = greenwich_sidereal_time(jd) + lng_deg - ra;
        ha = (((ha + 180.0) % 360.0) + 360.0) % 360.0 - 180.0;
        let (lat_r, ha_r) = (lat_deg * DEG, ha * DEG);
        (lat_r.sin() * decl.sin() + lat_r.cos() * decl.cos() * ha_r.cos()).asin() * RAD
    }

    /// hikari's bands: day above +6°, civil twilight in between, night
    /// below −6°. Light only in full day — matching `useTheme`'s
    /// `resolveEffectiveMode` (day → light, dusk/night → dark).
    pub(crate) fn prefers_light(lat_deg: f64, lng_deg: f64) -> bool {
        solar_altitude(lat_deg, lng_deg) > 6.0
    }

    /// First-paint estimate: hikari's default latitude plus the
    /// timezone-offset longitude (offset minutes / 4 = degrees).
    pub(crate) fn timezone_estimate() -> (f64, f64) {
        (31.23, f64::from(local_utc_offset_minutes()) / 4.0)
    }

    /// Windows location service fix resolved on a worker thread — the
    /// native slot for hikari's geolocation provider. `None` when the
    /// service is off, permission is denied, or no fix lands in time.
    #[cfg(windows)]
    pub(crate) fn spawn_fix_resolver() -> std::sync::mpsc::Receiver<Option<(f64, f64)>> {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(winrt_fix());
        });
        rx
    }

    #[cfg(windows)]
    fn winrt_fix() -> Option<(f64, f64)> {
        use std::sync::mpsc;
        use std::time::Duration;
        // The WinRT call needs an initialized apartment; the UI thread
        // keeps its own — init MTA here.
        unsafe {
            windows_sys::Win32::System::Com::CoInitializeEx(
                std::ptr::null(),
                windows_sys::Win32::System::Com::COINIT_MULTITHREADED as u32,
            );
        }
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let fix = (|| -> Option<(f64, f64)> {
                use windows::Devices::Geolocation::Geolocator;
                let op = Geolocator::new().ok()?.GetGeopositionAsync().ok()?;
                let position = op.get().ok()?;
                let basic = position
                    .Coordinate()
                    .ok()?
                    .Point()
                    .ok()?
                    .Position()
                    .ok()?;
                Some((basic.Latitude, basic.Longitude))
            })();
            let _ = tx.send(fix);
        });
        // A wedged service must not hold the theme clock hostage: the
        // worker leaks (one short-lived stack), the face keeps the
        // timezone estimate.
        rx.recv_timeout(Duration::from_secs(4)).ok().flatten()
    }

    #[cfg(not(windows))]
    pub(crate) fn spawn_fix_resolver() -> std::sync::mpsc::Receiver<Option<(f64, f64)>> {
        let (_, rx) = std::sync::mpsc::channel();
        rx
    }

    /// Minutes east of UTC (DST-adjusted) for the longitude estimate.
    #[cfg(windows)]
    fn local_utc_offset_minutes() -> i32 {
        use windows_sys::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
        unsafe {
            let mut tz: TIME_ZONE_INFORMATION = std::mem::zeroed();
            // 1 = standard time active, 2 = daylight; the active bias
            // rides along. Bias is minutes WEST (UTC = local + Bias).
            let state = GetTimeZoneInformation(&mut tz);
            let mut bias = tz.Bias;
            if state == 2 {
                bias += tz.DaylightBias;
            } else if state == 1 {
                bias += tz.StandardBias;
            }
            -bias
        }
    }

    #[cfg(not(windows))]
    fn local_utc_offset_minutes() -> i32 {
        480 // UTC+8 — the estimate's honest default
    }
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
    dir_label: String,
    browse: String,
    browse_title: String,
    quick_title: String,
    dir_empty: String,
    desktop_shortcut: String,
    hint_local: String,
    hint_portable: String,
    location_sub: String,
    target_hint_local: String,
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
    geo: Option<(f64, f64)>,
    geo_rx: Receiver<Option<(f64, f64)>>,
    /// The solar clock's last verdict and tick time — re-evaluated
    /// every five minutes so dawn/dusk flip a `system`-mode face.
    solar_dark: Option<bool>,
    last_solar_tick: std::time::Instant,
    /// The DWM round/shadow hint is applied once, on the first frame.
    dwm_rounded: bool,
    /// The install-location candidates + drive list — probed once from
    /// the same shun sources the web face's default_dir/list_drives
    /// commands ride (kind, writable, path) / (mount, kind, label).
    candidates: Vec<(String, bool, String)>,
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
    /// The lucide caption glyphs (loaded once; tinted per state).
    caption_icons: CaptionIcons,
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
    /// The language step's HkSelect-style dropdown state.
    lang_combo_open: bool,
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
        language: String,
        cjk_font: bool,
        receiver: Receiver<WorkerMsg>,
        logo: Option<TextureHandle>,

        license_docs: std::collections::BTreeMap<String, Vec<shun::config::ResolvedLicenseDoc>>,
        geo_rx: Receiver<Option<(f64, f64)>>,
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
            geo: None,
            geo_rx,
            solar_dark: None,
            last_solar_tick: std::time::Instant::now(),
            dwm_rounded: false,
            candidates: probed
                .into_iter()
                .map(|c| (c.kind.to_string(), c.writable, c.path))
                .collect(),
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
            caption_icons,
            stage: Stage::Configure,
            mode,
            desktop_shortcut: true,
            machine: false,
            license_docs,
            step: 0,
            license_accepted: false,
            lang_combo_open: false,
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
    /// Rebuild the tokens + egui visuals for a light/dark flip — the
    /// one path both the caption toggle and the solar clock go through.
    fn apply_mode(&mut self, ctx: &Context, dark: bool) {
        self.dark_theme = dark;
        self.theme = if dark {
            Theme::dark(self.accent)
        } else {
            Theme::light(self.accent)
        };
        ctx.set_visuals(if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
    }

    /// The theme clock (hikari's `useTheme`): while the manifest leaves
    /// the mode on `system` and the user hasn't pinned a side, the sun
    /// decides — re-evaluated when the location fix lands and every
    /// five minutes after, so dawn and dusk flip the face live.
    fn tick_solar_clock(&mut self, ctx: &Context) {
        let mut got_fix = false;
        while let Ok(fix) = self.geo_rx.try_recv() {
            self.geo = fix;
            got_fix = true;
        }
        let pinned = matches!(
            self.config
                .shell
                .as_ref()
                .and_then(|t| t.theme.as_ref())
                .and_then(|t| t.mode),
            Some(shun::config::ThemeMode::Light) | Some(shun::config::ThemeMode::Dark)
        ) || self.mode_pinned;
        let due = self.last_solar_tick.elapsed().as_secs() >= 300;
        if pinned || (!got_fix && !due) {
            return;
        }
        self.last_solar_tick = std::time::Instant::now();
        let (lat, lng) = self.geo.unwrap_or_else(solar::timezone_estimate);
        let dark = !solar::prefers_light(lat, lng);
        if self.solar_dark != Some(dark) {
            self.solar_dark = Some(dark);
            self.apply_mode(ctx, dark);
        }
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
        std::thread::spawn(move || {
            // The shared driver, exactly what the webview and TUI faces
            // ride: writability gate, overwrite hygiene, the flow, the
            // stale-file sweep. Uninstall resolves the install dir this
            // process sits in (the ARP uninstaller's contract).
            let core = WizardCore::new(config, BTreeMap::new());
            let result = if uninstalling {
                shun::wizard::run_uninstall(&core).map_err(|e| e.to_string())
            } else {
                shun::wizard::run_install(&core, &payload, &request, &mut |event| {
                    let _ = sender.send(WorkerMsg::Event(event.clone()));
                    repaint.request_repaint();
                })
                .map_err(|e| e.to_string())
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
            // The theme clock's geolocation fix — resolved off-thread so
            // a slow or unavailable location service never delays the
            // first frame (the estimate covers until it lands).
            let geo_rx = solar::spawn_fix_resolver();
            let caption_icons = CaptionIcons::load(&cc.egui_ctx);
            Ok(Box::new(FallbackApp::new(
                config,
                payload,
                reason,
                language,
                font_found,
                receiver,
                logo,
                license_docs,
                geo_rx,
                caption_icons,
                SHUN_FLAVOR.trim().to_string(),
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
                        if self.uninstalling == Some(true) {
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
                // sun/moon picked by the live mode. Right-to-left row:
                // emit the CLOSE first so it lands right-most (the
                // Windows convention), minimize to its left, and the
                // theme toggle left-most of the cluster.
                let close = caption(ui, CaptionIcon::Close);
                if close.clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                let minimize = caption(ui, CaptionIcon::Minimize);
                if minimize.clicked() {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }
                if self.user_adjustable {
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
        let caption_count = if self.user_adjustable { 3 } else { 2 };
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
            // Deterministic centering, both axes: computed vertical
            // offset for the fixed-metric block, computed left offset
            // for the block width (egui's aligns nest unreliably through
            // the pane's fixed-width block).
            let total = items.len() as f32 * circle_d + (items.len() as f32 - 1.0) * connector_h;
            let top = ((ui.available_height() - total) / 2.0).max(0.0);
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

    /// A group label above a chip row (hikari's micro-headers): muted
    /// secondary text at the small size. Generic across panes.
    fn section_label(ui: &mut egui::Ui, theme: Theme, text: &str) {
        ui.label(
            RichText::new(text)
                .size(12.0)
                .color(theme.text_secondary),
        );
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
        let row_w = width.unwrap_or(ui.available_width());
        let (row, response) = ui.allocate_exact_size(vec2(row_w, 22.0), egui::Sense::click());
        let response = Self::hand(response);
        let painter = ui.painter_at(row);
        let plate = egui::Rect::from_min_size(
            pos2(row.left(), row.center().y - 9.0),
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
            pos2(row.left() + 26.0, row.center().y),
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
        match self.step {
            0 => self.language_view(ui),
            1 => self.location_view(ui),
            _ => self.license_view(ui),
        }
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

        // Vertical centering, deterministic: measure the parts, offset by
        // (available - content) / 2, then draw with a fixed-width block
        // for the horizontal centering. (egui's main-align Center
        // silently no-ops for sized-to-content content in this nesting.)
        let heading_font = egui::FontId::proportional(22.0);
        let heading_h = ui
            .painter()
            .layout_no_wrap(texts.lang_heading.to_string(), heading_font, theme.text)
            .size()
            .y;
        let sub_h = 20.0f32;
        let combo_h = CONTROL_H;
        let content_h = heading_h + 8.0 + sub_h + 24.0 + combo_h;
        let avail_h = ui.available_height();
        ui.add_space(((avail_h - content_h) / 2.0).max(0.0));

        let block_w = COMBO_W;
        let block_left = ((ui.available_width() - block_w) / 2.0).max(0.0);
        // The block centers horizontally in the pane (the webview face's
        // language step centers its picker the same way); its CONTENT is
        // left-aligned inside.
        ui.allocate_ui_with_layout(
            egui::vec2(block_w, content_h),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.add_space(2.0);
                let _ = block_left;
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
                                    for (code, autonym) in labels.iter() {
                                        let language = code.clone();
                                        let selected = language == current;
                                        let (row, row_resp) = ui.allocate_exact_size(
                                            egui::vec2(ui.available_width(), 34.0),
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
            },
        );
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

        // One start-aligned block for the whole pane: the manifest's
        // centered step alignment would otherwise re-center individual
        // rows (the field row drifted under the browse button, the chip
        // wrap spread apart). The sub rides the manifest's product name
        // via %PRODUCT%.
        ui.add_space(4.0);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), ui.available_height()),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
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
            },
        );
        ui.add_space(8.0);

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
                        pos2(chip.left() + 12.0, chip.center().y),
                        egui::Align2::LEFT_CENTER,
                        &mount,
                        egui::FontId::monospace(13.0),
                        theme.text,
                    );
                    ui.painter().image(
                        icons.chevron.id(),
                        egui::Rect::from_center_size(
                            pos2(chip.right() - 14.0, chip.center().y),
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

        // The mono path remainder inside the field.
        let input_rect = egui::Rect::from_min_max(
            pos2(chip_rect.right() + 10.0, field_rect.top() + 6.0),
            pos2(field_rect.right() - 12.0, field_rect.bottom() - 6.0),
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

        // Quick candidates: the config-driven chips — kind → icon,
        // unwritable dims and swaps to the alert glyph, the accent
        // highlights the first writable candidate while the current
        // path fails its probe (all web-face behaviors). The group
        // carries the shared 常用路径-style heading (i18n quick_title).
        ui.add_space(14.0);
        Self::section_label(ui, theme, texts.quick_title.as_str());
        let first_writable = self
            .candidates
            .iter()
            .find(|(_, writable, _)| *writable)
            .map(|(_, _, path)| path.clone());
        let accent_target = (self.dir_writable == Some(false))
            .then(|| first_writable.clone())
            .flatten();
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 70.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                for (kind, writable, path) in self.candidates.clone() {
                    let accent = accent_target.as_deref() == Some(path.as_str());
                    let label = match kind.as_str() {
                        "appdata" => "AppData".to_string(),
                        "program-files" => "Program Files".to_string(),
                        _ => path.clone(),
                    };
                    let icon = if writable {
                        match kind.as_str() {
                            "appdata" => &icons.app_window,
                            _ => &icons.hard_drive,
                        }
                    } else {
                        &icons.alert
                    };
                    let clicked = Self::quick_chip(
                        ui,
                        theme,
                        &icons,
                        icon,
                        &label,
                        writable,
                        accent,
                    );
                    if clicked {
                        self.dir = shun::wizard::pad_root_dir(&self.config, &path);
                        self.probed_dir.clear();
                    }
                }
                });
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
                    let warn = if first_writable.is_some() {
                        texts.warn_unwritable
                    } else {
                        texts.warn_no_writable
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
                                let (row, row_resp) = ui.allocate_exact_size(
                                    egui::vec2(ui.available_width(), 34.0),
                                    egui::Sense::click(),
                                );
                                if row_resp.hovered() {
                                    ui.ctx()
                                        .set_cursor_icon(egui::CursorIcon::PointingHand);
                                }
                                let rp = ui.painter_at(row);
                                if row_resp.hovered() || self.dir.starts_with(&mount) {
                                    rp.rect_filled(
                                        row,
                                        CornerRadius::same(8),
                                        mix(theme.background, theme.primary, 0.08),
                                    );
                                }
                                rp.text(
                                    pos2(row.left() + 10.0, row.center().y),
                                    egui::Align2::LEFT_CENTER,
                                    &mount,
                                    egui::FontId::monospace(13.0),
                                    theme.text,
                                );
                                rp.text(
                                    pos2(row.right() - 10.0, row.center().y),
                                    egui::Align2::RIGHT_CENTER,
                                    meta,
                                    egui::FontId::proportional(11.0),
                                    theme.text_secondary,
                                );
                                if row_resp.clicked() {
                                    // PathField's contract: strip the old
                                    // prefix, keep the typed remainder.
                                    let old_mount = self
                                        .drives
                                        .iter()
                                        .find(|(m, _, _)| self.dir.starts_with(m))
                                        .map(|(m, _, _)| m.clone())
                                        .unwrap_or_default();
                                    let rest = self
                                        .dir
                                        .strip_prefix(old_mount.as_str())
                                        .map(|r| r.trim_start_matches(['\\', '/']))
                                        .unwrap_or("");
                                    self.dir = format!("{mount}{rest}");
                                }
                            }
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
                self.texts.uninstalling.clone()
            } else {
                self.texts.installing_percent.clone()
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
                // hikari's done hero: a centered column — green ring
                // check, the success headline in the success color, the
                // install path in monospace, then the hint and the
                // shortcut answers.
                ui.add_space(16.0);
                ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(56.0, 56.0), egui::Sense::hover());
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
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(format!("✓ {title}"))
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
                    // The done-page shortcut answer — the shared driver's
                    // flow creates none, so the finish button applies it.
                    // Offered only after a real (non-portable) install:
                    // an uninstall has nothing to point a shortcut at.
                    if desktop_asks
                        && matches!(outcome.as_ref(), Some(Outcome::InstallOk))
                        && self.mode != "portable"
                    {
                        ui.add_space(12.0);
                        Self::circle_checkbox(
                            ui,
                            theme,
                            &mut self.desktop_shortcut,
                            texts.desktop_shortcut.as_str(),
                            Some(320.0),
                        );
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
            self.texts.log.as_str(),
            self.texts.log_expand.as_str(),
            self.texts.log_collapse.as_str(),
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
                let on_last_step = self.step >= 2;
                let step_blocked = self.step_blocked();
                let (label, enabled) = match self.stage {
                    Stage::Configure if !on_last_step => (self.texts.next.clone(), !step_blocked),
                    Stage::Configure => (self.texts.install.clone(), !step_blocked),
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
                            .min_size(Vec2::new(112.0, 30.0)),
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
                                    // answer (the shared driver's flow
                                    // creates none): apply it through the
                                    // same finish path every face uses,
                                    // then close. Uninstalls and portable
                                    // runs never apply shortcuts — there
                                    // is nothing to point at.
                                    if matches!(self.outcome, Some(Outcome::InstallOk))
                                        && self.mode != "portable"
                                    {
                                        let core =
                                            WizardCore::new(self.config.clone(), BTreeMap::new());
                                        let _ = shun::wizard::apply_finish(
                                            &core,
                                            self.dir.trim(),
                                            Some(self.desktop_shortcut),
                                            Some(true),
                                            false,
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
                // hikari's secondary actions are bare text — no border
                // box — with the muted color carrying the hierarchy.
                let ghost = |ui: &mut egui::Ui, label: &str| {
                    Self::hand(ui.add(
                        Button::new(RichText::new(label).size(13.0).color(theme.text_secondary))
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::NONE)
                            .corner_radius(CornerRadius::same(8))
                            .min_size(Vec2::new(88.0, 30.0)),
                    ))
                    .clicked()
                };
                if configuring {
                    if self.step > 0 && ghost(ui, self.texts.back.as_str()) {
                        self.license_doc_index = 0;
                        self.step -= 1;
                    }
                    if on_last_step && ghost(ui, self.texts.uninstall.as_str()) {
                        self.spawn_worker(ctx, true);
                    }
                } else if self.stage == Stage::Finished
                    && matches!(self.outcome, Some(Outcome::InstallOk))
                {
                    if ghost(ui, self.texts.uninstall.as_str()) {
                        self.spawn_worker(ctx, true);
                    }
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
        // Step 2 is the license: progression gates on the accept box.
        self.step >= 2 && !self.license_accepted
    }
}

impl eframe::App for FallbackApp {
    fn update(&mut self, ctx: &Context, frame: &mut eframe::Frame) {
        self.drain_worker();
        self.tick_solar_clock(ctx);
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

        // ── Caption bar (frameless chrome): one flat HkTitleBar band.
        egui::TopBottomPanel::top("titlebar")
            .frame(Frame::default().fill(theme.surface).inner_margin(Margin::ZERO))
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
                        // The brightness split, web-face direction: the
                        // rail sits a shade DARKER than the pane's page
                        // background (the old surface fill inverted it).
                        .fill(mix(theme.background, theme.text, 0.04))
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
                // The degradation banner is information, not decoration:
                // show it only when the runtime forced the fallback (no
                // WebView2). A deliberate --no-webview launch needs no
                // warning about itself.
                if self.reason == FallbackReason::MissingWebview2 {
                    self.banner(ui);
                    ui.add_space(12.0);
                }
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
