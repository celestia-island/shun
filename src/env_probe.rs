//! Environment probing for delivery shells: which interface faces the
//! runtime can actually present. The four faces (webview GUI, egui GUI,
//! TUI, CLI) all live behind one wizard model; these probes decide —
//! together with the operator's flags and the manifest's face list —
//! which face renders it.
//!
//! - [`gui_available`]: is there a desktop to put a window on?
//! - [`stdout_is_tty`]: is the process attached to an interactive
//!   terminal (the TUI's requirement)?
//! - [`attach_parent_console`]: best-effort console attach for
//!   GUI-subsystem binaries launched from a terminal, so `--no-gui`
//!   (and help output) actually reach that terminal.

/// Whether this process can create windows on an interactive desktop.
///
/// Windows: the process's window station carries `WSF_VISIBLE` — false
/// for session-0 services and headless contexts, which is exactly the
/// population that must fall back to the TUI/CLI faces. Unix: a display
/// server is reachable through `DISPLAY` or `WAYLAND_DISPLAY`.
#[cfg(windows)]
pub fn gui_available() -> bool {
    use windows_sys::Win32::System::StationsAndDesktops::{
        GetProcessWindowStation, GetUserObjectInformationW, UOI_FLAGS, USEROBJECTFLAGS,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::WSF_VISIBLE;

    // SAFETY: read-only queries against the process's own window station;
    // the flags struct is a plain out-buffer.
    unsafe {
        let station = GetProcessWindowStation();
        if station.is_null() {
            return false;
        }
        let mut flags = USEROBJECTFLAGS {
            fInherit: 0,
            fReserved: 0,
            dwFlags: 0,
        };
        let ok = GetUserObjectInformationW(
            station,
            UOI_FLAGS,
            &mut flags as *mut USEROBJECTFLAGS as *mut core::ffi::c_void,
            std::mem::size_of::<USEROBJECTFLAGS>() as u32,
            std::ptr::null_mut(),
        );
        ok != 0 && (flags.dwFlags & WSF_VISIBLE as u32) != 0
    }
}

/// Unix branch of [`gui_available`]: a display server is reachable.
#[cfg(not(windows))]
pub fn gui_available() -> bool {
    std::env::var_os("DISPLAY")
        .map(|v| !v.is_empty())
        .unwrap_or(false)
        || std::env::var_os("WAYLAND_DISPLAY")
            .map(|v| !v.is_empty())
            .unwrap_or(false)
}

/// Whether stdout is an interactive terminal (the TUI renders onto it;
/// non-TTY means `--no-gui` resolves to the help text instead).
#[cfg(windows)]
pub fn stdout_is_tty() -> bool {
    use windows_sys::Win32::System::Console::{GetConsoleMode, GetStdHandle, STD_OUTPUT_HANDLE};

    // SAFETY: GetStdHandle queries a handle; GetConsoleMode only succeeds
    // for real console handles, which is the whole probe.
    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        if handle.is_null() {
            return false;
        }
        let mut mode: u32 = 0;
        GetConsoleMode(handle, &mut mode) != 0
    }
}

/// Unix branch of [`stdout_is_tty`].
#[cfg(not(windows))]
pub fn stdout_is_tty() -> bool {
    // SAFETY: isatty on a valid fd constant.
    unsafe { libc::isatty(1) == 1 }
}

/// Attaches the parent process's console — and wires the standard
/// handles onto it — so a GUI-subsystem binary launched from a terminal
/// can print (help text, CLI output) and run the TUI on it. Best-effort
/// by design: a false return just means no console — the caller falls
/// back to GUI or exits quietly. Windows-only; a no-op elsewhere.
#[cfg(windows)]
pub fn attach_parent_console() -> bool {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE,
        STD_OUTPUT_HANDLE, SetStdHandle,
    };

    // SAFETY: plain kernel32 calls with constant ids and our own handles.
    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            return false;
        }
        // AttachConsole does NOT wire the standard handles: a
        // GUI-subsystem process still holds null std handles, so
        // GetConsoleMode (the TTY probe) fails and stdout writes go
        // nowhere. Reopen the console devices and install them — the
        // classic recipe for "GUI exe that behaves when launched from a
        // terminal".
        // Only fill MISSING handles: a process launched with inherited
        // pipes keeps them (rewiring would hijack piped stdout into the
        // console); the double-click / `cmd /k` case carries null
        // handles that need the console devices.
        let wire = |name: &[u16], std_id: u32| {
            if !GetStdHandle(std_id).is_null() {
                return;
            }
            let handle = CreateFileW(
                name.as_ptr(),
                // GENERIC_READ | GENERIC_WRITE (windows-sys keeps these
                // as root constants in some feature sets; the raw values
                // are stable ABI).
                0x8000_0000u32 | 0x4000_0000u32,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            );
            if handle != INVALID_HANDLE_VALUE {
                SetStdHandle(std_id, handle);
            }
        };
        let conin: Vec<u16> = "CONIN$\0".encode_utf16().collect();
        let conout: Vec<u16> = "CONOUT$\0".encode_utf16().collect();
        wire(&conin, STD_INPUT_HANDLE);
        wire(&conout, STD_OUTPUT_HANDLE);
        wire(&conout, STD_ERROR_HANDLE);
        true
    }
}

/// Unix branch of [`attach_parent_console`]: nothing to attach.
#[cfg(not(windows))]
pub fn attach_parent_console() -> bool {
    true
}

/// A face the runtime can render, resolved from probes + flags + the
/// manifest's face list. Kept as a plain enum so the resolver stays a
/// pure function (unit-testable without touching the real environment).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiCapabilities {
    /// A desktop exists (windows can be created).
    pub gui: bool,
    /// The WebView2 runtime is usable (Windows; always true elsewhere).
    pub webview2: bool,
    /// stdout is an interactive terminal.
    pub tty: bool,
}

impl UiCapabilities {
    /// Live probe of the process environment.
    pub fn probe() -> Self {
        Self {
            gui: gui_available(),
            webview2: crate::env_probe::webview2_available(),
            tty: stdout_is_tty(),
        }
    }
}

/// Detects a usable WebView2 runtime the way the WebView2 loader falls
/// back to: the Evergreen EdgeUpdate registry entries, per-machine
/// (native and WOW6432Node views) and per-user. The fixed-version
/// strategy (an explicit `WEBVIEW2_BROWSER_EXECUTABLE_FOLDER`) wins when
/// it points at a real directory, same as the loader. `false` means the
/// tauri face cannot come up and the egui face takes over.
///
/// REGISTRY ≠ LOADER: a leftover `pv` value (or an uninstalled runtime)
/// can make this probe say "present" while the actual loader —
/// `GetAvailableCoreWebView2BrowserVersionString`, what tauri/wry gate
/// webview creation on — disagrees, and tauri would die on its English
/// "Could not find the WebView2 Runtime" box. The shell therefore
/// re-checks with `wry::webview_version()` before picking the webview
/// face (see the shell's `webview2_loader_ready`); this probe stays the
/// cheap, dependency-free signal for the other consumers.
#[cfg(windows)]
pub fn webview2_available() -> bool {
    // The fixed-version override only wins when it points at a real
    // directory — the loader would fail on a stale path, and a blanket
    // `is_some` misreads broken setups as "runtime present".
    if std::env::var_os("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER")
        .is_some_and(|folder| std::path::Path::new(&folder).is_dir())
    {
        return true;
    }
    // Test override: forces the egui degradation path on machines that
    // DO have the runtime, so the auto-fallback stays regression-tested.
    if std::env::var_os("SHUN_FORCE_FALLBACK").is_some() {
        return false;
    }
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};

    const CLIENT: &str = r"Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
    // The Evergreen runtime registers under all three hives depending on
    // install scope and bitness; missing the native HKLM view (the old
    // probe) misdetected per-machine installs as "no runtime".
    let probes = [
        (
            HKEY_LOCAL_MACHINE,
            format!(r"SOFTWARE\WOW6432Node\{CLIENT}"),
        ),
        (HKEY_LOCAL_MACHINE, format!(r"SOFTWARE\{CLIENT}")),
        (HKEY_CURRENT_USER, format!(r"SOFTWARE\{CLIENT}")),
    ];
    probes.iter().any(|(root, path)| {
        RegKey::predef(*root)
            .open_subkey_with_flags(path, KEY_READ)
            .and_then(|key| key.get_value::<String, _>("pv"))
            .is_ok_and(|version| !version.is_empty() && version != "0.0.0.0")
    })
}

/// Non-Windows platforms always have their system webview; the tauri
/// face never auto-degrades there (`--no-webview` still forces egui).
#[cfg(not(windows))]
pub fn webview2_available() -> bool {
    std::env::var_os("SHUN_FORCE_FALLBACK").is_none()
}

/// The host's UI language as one of the fleet's locale spellings
/// (`en` / `es` / `fr` / `ja` / `ko` / `ru` / `zh-Hans` / `zh-Hant`).
///
/// The terminal face starts on this language instead of a hardcoded
/// `en`: Windows reads the user's default UI language (LANGID), other
/// hosts normalize `LC_ALL` / `LC_CTYPE` / `LANG`. `None` means the
/// host says nothing usable — callers fall back to `en`.
pub fn system_locale() -> Option<&'static str> {
    #[cfg(windows)]
    {
        // SAFETY: a plain per-user preference query (no handles).
        let langid = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
        Some(map_langid(langid))
    }
    #[cfg(not(windows))]
    {
        let raw = ["LC_ALL", "LC_CTYPE", "LANG"].iter().find_map(|name| {
            std::env::var(name)
                .ok()
                .filter(|value| !value.trim().is_empty())
        })?;
        locale_from_tag(&raw)
    }
}

/// Maps a Windows LANGID (`0x0SSLL`, primary language in the low ten bits,
/// sublanguage above) to the fleet spelling. Anything outside the eight
/// documented languages resolves to `en`.
#[cfg(windows)]
fn map_langid(langid: u16) -> &'static str {
    let primary = langid & 0x03FF;
    let sub = (langid >> 10) & 0x3F;
    match primary {
        0x04 => match sub {
            // Traditional / Hong Kong / Macau.
            0x01 | 0x03 | 0x05 => "zh-Hant",
            // Simplified / Singapore.
            _ => "zh-Hans",
        },
        0x0A => "es",
        0x0C => "fr",
        0x11 => "ja",
        0x12 => "ko",
        0x19 => "ru",
        _ => "en",
    }
}

/// Normalizes an environment locale (`zh_CN.UTF-8`, `zh-TW`, `fr_FR@euro`)
/// to the fleet spelling; unknown languages yield `None`.
#[cfg(not(windows))]
fn locale_from_tag(raw: &str) -> Option<&'static str> {
    let tag = raw.trim().to_ascii_lowercase();
    if tag.is_empty() || tag == "c" || tag == "posix" {
        return None;
    }
    let language = tag.split(['_', '-', '.', '@']).next().unwrap_or("");
    let traditional =
        tag.contains("tw") || tag.contains("hk") || tag.contains("mo") || tag.contains("hant");
    Some(match language {
        "zh" => {
            if traditional {
                "zh-Hant"
            } else {
                "zh-Hans"
            }
        }
        "es" => "es",
        "fr" => "fr",
        "ja" => "ja",
        "ko" => "ko",
        "ru" => "ru",
        "en" => "en",
        _ => return None,
    })
}

#[cfg(test)]
mod locale_tests {
    use super::*;

    #[test]
    #[cfg(windows)]
    fn langids_map_to_the_fleet_spellings() {
        assert_eq!(map_langid(0x0804), "zh-Hans", "PRC simplified");
        assert_eq!(map_langid(0x0404), "zh-Hant", "Taiwan traditional");
        assert_eq!(map_langid(0x0C04), "zh-Hant", "Hong Kong");
        assert_eq!(map_langid(0x1004), "zh-Hans", "Singapore");
        assert_eq!(map_langid(0x0411), "ja");
        assert_eq!(map_langid(0x0412), "ko");
        assert_eq!(map_langid(0x0419), "ru");
        assert_eq!(map_langid(0x040A), "es");
        assert_eq!(map_langid(0x040C), "fr");
        assert_eq!(map_langid(0x0409), "en");
        assert_eq!(map_langid(0x0415), "en", "unknown languages fall back");
    }

    #[test]
    #[cfg(not(windows))]
    fn environment_tags_normalize() {
        assert_eq!(locale_from_tag("zh_CN.UTF-8"), Some("zh-Hans"));
        assert_eq!(locale_from_tag("zh_TW.UTF-8"), Some("zh-Hant"));
        assert_eq!(locale_from_tag("zh-HK"), Some("zh-Hant"));
        assert_eq!(locale_from_tag("ja_JP.UTF-8"), Some("ja"));
        assert_eq!(locale_from_tag("ko_KR"), Some("ko"));
        assert_eq!(locale_from_tag("ru_RU.UTF-8"), Some("ru"));
        assert_eq!(locale_from_tag("es_ES.UTF-8"), Some("es"));
        assert_eq!(locale_from_tag("fr_FR@euro"), Some("fr"));
        assert_eq!(locale_from_tag("en_US.UTF-8"), Some("en"));
        assert_eq!(locale_from_tag("C"), None);
        assert_eq!(locale_from_tag("POSIX"), None);
        assert_eq!(locale_from_tag(""), None);
        assert_eq!(locale_from_tag("de_DE.UTF-8"), None);
    }
}
