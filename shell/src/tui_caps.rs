//! What the host terminal can actually render — probed once at TUI startup.
//!
//! The face adapts instead of assuming: a Windows Terminal pane gets colors,
//! unicode borders, mouse and inline images; a legacy console without VT
//! support degrades to mono ASCII. Every decision comes from a pure
//! [ProbeInput] snapshot so the matrix stays unit-testable, and the
//! SHUN_TUI_* overrides exist for the ConPTY harness (the e2e driver forces
//! each degradation lane instead of hoping the host matches).

/// Which terminal family the process is talking to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TermKind {
    WindowsTerminal,
    ConEmu,
    VsCode,
    Mintty,
    Xterm,
    /// A Windows console host reached without a modern-terminal marker.
    LegacyConsole,
    /// Unknown but a terminal.
    Unknown,
}

impl TermKind {
    pub fn label(self) -> &'static str {
        match self {
            TermKind::WindowsTerminal => "Windows Terminal",
            TermKind::ConEmu => "ConEmu",
            TermKind::VsCode => "VS Code",
            TermKind::Mintty => "mintty",
            TermKind::Xterm => "xterm",
            TermKind::LegacyConsole => "console",
            TermKind::Unknown => "terminal",
        }
    }
}

/// How many colors the face may use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ColorDepth {
    Mono,
    Basic16,
    Ansi256,
    TrueColor,
}

impl ColorDepth {
    pub fn label(self) -> &'static str {
        match self {
            ColorDepth::Mono => "mono",
            ColorDepth::Basic16 => "16 colors",
            ColorDepth::Ansi256 => "256 colors",
            ColorDepth::TrueColor => "truecolor",
        }
    }
}

/// Inline-image support. Half blocks need only colors, so they are the
/// universal floor; dedicated protocols (kitty / sixel / iTerm2 inline
/// images) can grow into this enum without moving the call sites.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageSupport {
    None,
    HalfBlocks,
}

/// The resolved capability set.
#[derive(Clone, Debug)]
pub struct TerminalCaps {
    pub kind: TermKind,
    pub color: ColorDepth,
    pub unicode: bool,
    pub mouse: bool,
    pub images: ImageSupport,
}

impl TerminalCaps {
    /// One-line self-description for the footer (also the e2e assertion hook).
    pub fn summary(&self) -> String {
        let mut parts = vec![
            self.kind.label().to_string(),
            self.color.label().to_string(),
        ];
        parts.push(if self.unicode {
            "unicode".into()
        } else {
            "ascii".into()
        });
        if self.mouse {
            parts.push("mouse".into());
        }
        if self.images == ImageSupport::HalfBlocks {
            parts.push("images".into());
        }
        parts.join(" · ")
    }
}

/// The environment/platform facts the classifier needs — captured once in
/// [probe], built by hand in tests.
#[derive(Clone, Debug)]
pub struct ProbeInput {
    /// Variable name → value (lossy UTF-8; lookups are case-insensitive).
    pub env: Vec<(String, String)>,
    pub windows: bool,
    /// Whether stdout is a Windows console at all.
    pub console: bool,
    /// Whether VT processing ended up enabled on the console (Windows).
    pub vt: bool,
}

impl ProbeInput {
    fn var(&self, name: &str) -> Option<&str> {
        self.env
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    fn flag(&self, name: &str) -> bool {
        self.var(name).is_some_and(|v| !v.is_empty())
    }
}

/// Pure classifier — the whole decision matrix lives here.
pub fn classify(input: &ProbeInput) -> TerminalCaps {
    let term = input.var("term").unwrap_or_default().to_ascii_lowercase();
    let colorterm = input
        .var("colorterm")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let term_program = input
        .var("term_program")
        .unwrap_or_default()
        .to_ascii_lowercase();

    let kind = if input.flag("wt_session") {
        TermKind::WindowsTerminal
    } else if term.eq_ignore_ascii_case("xterm-256color") && input.flag("msystem") {
        TermKind::Mintty
    } else if input.flag("conemuansi") {
        TermKind::ConEmu
    } else if term_program == "vscode" {
        TermKind::VsCode
    } else if term.contains("xterm") || term.contains("screen") || term.contains("tmux") {
        TermKind::Xterm
    } else if input.windows && input.console {
        TermKind::LegacyConsole
    } else {
        TermKind::Unknown
    };

    // Baseline color depth from the environment. An explicit mono request
    // (NO_COLOR / TERM=dumb) is final — the modern-terminal upgrade below
    // must not undo it.
    let forced_mono = input.flag("no_color") || term == "dumb";
    let mut color = if forced_mono {
        ColorDepth::Mono
    } else if colorterm.contains("truecolor") || colorterm.contains("24bit") {
        ColorDepth::TrueColor
    } else if term.contains("256color") {
        ColorDepth::Ansi256
    } else if input.windows {
        // Modern conhost exposes VT processing; without it only the classic
        // 16-color console API remains (which we do not drive directly).
        if input.vt {
            ColorDepth::TrueColor
        } else {
            ColorDepth::Mono
        }
    } else if term.is_empty() {
        ColorDepth::Mono
    } else {
        ColorDepth::Basic16
    };

    // A modern Windows terminal is truecolor even when the caller's
    // environment carries no COLORTERM (Windows Terminal does not set it).
    if input.windows
        && input.vt
        && !forced_mono
        && matches!(
            kind,
            TermKind::WindowsTerminal | TermKind::ConEmu | TermKind::VsCode | TermKind::Mintty
        )
        && color < ColorDepth::TrueColor
    {
        color = ColorDepth::TrueColor;
    }

    // Unicode box drawing: UTF-8 locales and modern Windows consoles have it;
    // dumb / legacy-mono / explicit ASCII requests do not.
    let locale_utf8 = ["lc_all", "lc_ctype", "lang"].iter().any(|name| {
        input.var(name).is_some_and(|v| {
            let v = v.to_ascii_lowercase();
            v.contains("utf-8") || v.contains("utf8")
        })
    });
    let mut unicode = if input.windows {
        input.vt || input.flag("wt_session")
    } else if term == "dumb" {
        false
    } else {
        locale_utf8 || !term.is_empty()
    };

    let mut mouse = (input.windows && input.console) || (!input.windows && term != "dumb");
    let mut images = if color == ColorDepth::TrueColor {
        ImageSupport::HalfBlocks
    } else {
        ImageSupport::None
    };

    // ── Explicit overrides (accessibility flags and the e2e harness) ──
    if let Some(force) = input.var("shun_tui_color") {
        color = match force.to_ascii_lowercase().as_str() {
            "mono" | "none" | "off" | "0" => ColorDepth::Mono,
            "16" | "basic" => ColorDepth::Basic16,
            "256" => ColorDepth::Ansi256,
            "true" | "truecolor" | "24" => ColorDepth::TrueColor,
            _ => color,
        };
        images = if color == ColorDepth::TrueColor {
            ImageSupport::HalfBlocks
        } else {
            ImageSupport::None
        };
    }
    if let Some(force) = input.var("shun_tui_unicode") {
        unicode = !matches!(
            force.trim().to_ascii_lowercase().as_str(),
            "0" | "off" | "false" | "no"
        );
    }
    if let Some(force) = input.var("shun_tui_mouse") {
        mouse = !matches!(
            force.trim().to_ascii_lowercase().as_str(),
            "0" | "off" | "false" | "no"
        );
    }
    if let Some(force) = input.var("shun_tui_images") {
        let wants = !matches!(
            force.trim().to_ascii_lowercase().as_str(),
            "0" | "off" | "false" | "no"
        );
        images = if wants && color >= ColorDepth::Ansi256 {
            ImageSupport::HalfBlocks
        } else {
            ImageSupport::None
        };
    }

    TerminalCaps {
        kind,
        color,
        unicode,
        mouse,
        images,
    }
}

/// Probe the real terminal (environment + the Windows console VT mode).
pub fn probe() -> TerminalCaps {
    let env: Vec<(String, String)> = std::env::vars_os()
        .map(|(k, v)| {
            (
                k.to_string_lossy().into_owned(),
                v.to_string_lossy().into_owned(),
            )
        })
        .collect();

    #[cfg(windows)]
    let (console, vt) = windows_console_vt();
    #[cfg(not(windows))]
    let (console, vt) = (false, false);

    classify(&ProbeInput {
        env,
        windows: cfg!(windows),
        console,
        vt,
    })
}

/// Whether stdout is a console, and whether VT processing is on (enabled here
/// when the host supports it — the TUI needs it for colors and cursor work).
#[cfg(windows)]
fn windows_console_vt() -> (bool, bool) {
    use windows_sys::Win32::System::Console::{
        ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleMode, GetStdHandle, STD_OUTPUT_HANDLE,
        SetConsoleMode,
    };

    // SAFETY: plain kernel32 calls on the process's own stdout handle.
    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        if handle.is_null() {
            return (false, false);
        }
        let mut mode = 0u32;
        if GetConsoleMode(handle, &mut mode) == 0 {
            return (false, false);
        }
        if mode & ENABLE_VIRTUAL_TERMINAL_PROCESSING != 0 {
            return (true, true);
        }
        let enabled = SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING) != 0;
        (true, enabled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(env: &[(&str, &str)], windows: bool, console: bool, vt: bool) -> ProbeInput {
        ProbeInput {
            env: env
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            windows,
            console,
            vt,
        }
    }

    #[test]
    fn windows_terminal_gets_truecolor_unicode_mouse_and_images() {
        let caps = classify(&input(
            &[("WT_SESSION", "abc-123"), ("LANG", "zh_CN.UTF-8")],
            true,
            true,
            true,
        ));
        assert_eq!(caps.kind, TermKind::WindowsTerminal);
        assert_eq!(caps.color, ColorDepth::TrueColor);
        assert!(caps.unicode);
        assert!(caps.mouse);
        assert_eq!(caps.images, ImageSupport::HalfBlocks);
        assert!(caps.summary().contains("Windows Terminal"));
    }

    #[test]
    fn legacy_console_without_vt_degrades_to_mono_ascii() {
        let caps = classify(&input(&[], true, true, false));
        assert_eq!(caps.kind, TermKind::LegacyConsole);
        assert_eq!(caps.color, ColorDepth::Mono);
        assert!(!caps.unicode);
        assert_eq!(caps.images, ImageSupport::None);
        // The console still reports mouse events even in mono.
        assert!(caps.mouse);
    }

    #[test]
    fn no_color_and_dumb_term_force_mono() {
        for env in [
            vec![("NO_COLOR", "1"), ("WT_SESSION", "x")],
            vec![("TERM", "dumb")],
        ] {
            let caps = classify(&input(&env, true, true, true));
            assert_eq!(caps.color, ColorDepth::Mono, "env {env:?}");
            assert_eq!(caps.images, ImageSupport::None);
        }
    }

    #[test]
    fn xterm_256_and_truecolor_follow_the_environment() {
        let c256 = classify(&input(&[("TERM", "xterm-256color")], false, false, false));
        assert_eq!(c256.kind, TermKind::Xterm);
        assert_eq!(c256.color, ColorDepth::Ansi256);
        assert!(c256.mouse);
        assert_eq!(c256.images, ImageSupport::None);
        assert!(c256.unicode, "xterm carries box drawing");

        let true_color = classify(&input(
            &[("TERM", "xterm-256color"), ("COLORTERM", "truecolor")],
            false,
            false,
            false,
        ));
        assert_eq!(true_color.color, ColorDepth::TrueColor);
        assert_eq!(true_color.images, ImageSupport::HalfBlocks);
    }

    #[test]
    fn overrides_win_only_where_they_apply() {
        let base = [("WT_SESSION", "x"), ("SHUN_TUI_COLOR", "mono")];
        let caps = classify(&input(&base, true, true, true));
        assert_eq!(caps.color, ColorDepth::Mono);
        assert_eq!(caps.images, ImageSupport::None);

        let mouse_off = classify(&input(
            &[("WT_SESSION", "x"), ("SHUN_TUI_MOUSE", "0")],
            true,
            true,
            true,
        ));
        assert!(!mouse_off.mouse);

        let ascii = classify(&input(
            &[("WT_SESSION", "x"), ("SHUN_TUI_UNICODE", "off")],
            true,
            true,
            true,
        ));
        assert!(!ascii.unicode);

        let images_on_256 = classify(&input(
            &[("TERM", "xterm-256color"), ("SHUN_TUI_IMAGES", "1")],
            false,
            false,
            false,
        ));
        assert_eq!(images_on_256.images, ImageSupport::HalfBlocks);
    }

    #[test]
    fn vs_code_and_conemu_are_recognized() {
        let vs = classify(&input(&[("TERM_PROGRAM", "vscode")], true, true, true));
        assert_eq!(vs.kind, TermKind::VsCode);
        assert_eq!(vs.color, ColorDepth::TrueColor);

        let conemu = classify(&input(&[("ConEmuANSI", "ON")], true, true, true));
        assert_eq!(conemu.kind, TermKind::ConEmu);
    }
}
