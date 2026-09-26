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

/// Attaches the parent process's console so a GUI-subsystem binary
/// launched from a terminal can print (help text, CLI output) into it.
/// Best-effort by design: a false return just means no console — the
/// caller falls back to GUI or exits quietly. Windows-only; a no-op
/// elsewhere (unix GUI subsystems keep their console anyway).
#[cfg(windows)]
pub fn attach_parent_console() -> bool {
    use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};

    // SAFETY: plain kernel32 call with a constant parent id.
    unsafe { AttachConsole(ATTACH_PARENT_PROCESS) != 0 }
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
/// strategy (an explicit `WEBVIEW2_BROWSER_EXECUTABLE_FOLDER`) always
/// wins, same as the loader. `false` means the tauri face cannot come
/// up and the egui face takes over.
#[cfg(windows)]
pub fn webview2_available() -> bool {
    if std::env::var_os("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER").is_some() {
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
    if std::env::var_os("SHUN_FORCE_FALLBACK").is_some() {
        return false;
    }
    true
}
