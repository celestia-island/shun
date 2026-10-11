//! Panic-free diagnostics for a GUI-subsystem binary — stderr AND a
//! per-run log file.
//!
//! `eprintln!` panics when the process has no stderr handle — which is the
//! normal state for a `windows_subsystem = "windows"` binary launched by
//! double-click: `GetStdHandle` yields NULL, the write fails, and std's
//! `print_to` turns that into `panic!("failed printing to stderr")` (found by
//! verification round 2, 2026-10-06; the DWM border-colour diagnostic would
//! have hit it on every Windows 10 launch, since that attribute does not
//! exist there). Everything the shell wants to say while the window may not
//! exist yet goes through [`diag`], which writes best-effort and swallows
//! the error instead.
//!
//! Every line ALSO lands in a per-run file —
//! `%LOCALAPPDATA%\shun\<product>\setup-logs\<timestamp>.log` (created
//! lazily on first write; failures are swallowed, the file is a
//! diagnostic, not a dependency). A double-clicked installer's console
//! is gone the moment the window opens, and the field reports that
//! matter (why the WebView2 runtime did not install) lived entirely in
//! that vanishing stream; the file is what the operator can hand back.

use std::io::Write;

/// The log file's open handle, once per process.
static LOG_FILE: std::sync::OnceLock<Option<std::fs::File>> = std::sync::OnceLock::new();

/// Opens (or declines) the run's log file: the first caller wins, and
/// a failure (no LOCALAPPDATA, read-only dir) disables the sink for
/// the run — diagnostics must never take the installer down.
fn log_file() -> Option<&'static std::fs::File> {
    LOG_FILE
        .get_or_init(|| {
            let name = option_env!("SHUN_PRODUCT_LOG_NAME").unwrap_or("installer");
            let dir = std::env::var_os("LOCALAPPDATA")?;
            let dir = std::path::Path::new(&dir)
                .join("shun")
                .join(name)
                .join("setup-logs");
            std::fs::create_dir_all(&dir).ok()?;
            let stamp = local_stamp();
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(dir.join(format!("{stamp}.log")))
                .ok()?;
            let _ = writeln!(
                file,
                "── shun setup log · {} ──",
                std::env::current_exe()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default()
            );
            Some(file)
        })
        .as_ref()
}

/// Local wall clock as `YYYYMMDD-HHMMSS` for the file name. Windows
/// reads GetLocalTime; elsewhere UTC (a filename aid, not a clock).
#[cfg(windows)]
fn local_stamp() -> String {
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    let mut now = SYSTEMTIME {
        wYear: 0,
        wMonth: 0,
        wDayOfWeek: 0,
        wDay: 0,
        wHour: 0,
        wMinute: 0,
        wSecond: 0,
        wMilliseconds: 0,
    };
    // SAFETY: plain call writing into our own zeroed struct.
    unsafe { GetLocalTime(&mut now) };
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        now.wYear, now.wMonth, now.wDay, now.wHour, now.wMinute, now.wSecond
    )
}

#[cfg(not(windows))]
fn local_stamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("utc-{secs}")
}

/// Local clock as `HH:MM:SS.mmm` for line prefixes (the field reports
/// that matter are TIMING-sensitive: a UAC consent that never surfaces
/// reads as a gap between two stamps).
#[cfg(windows)]
fn clock() -> String {
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    let mut now = SYSTEMTIME {
        wYear: 0,
        wMonth: 0,
        wDayOfWeek: 0,
        wDay: 0,
        wHour: 0,
        wMinute: 0,
        wSecond: 0,
        wMilliseconds: 0,
    };
    // SAFETY: plain call writing into our own zeroed struct.
    unsafe { GetLocalTime(&mut now) };
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        now.wHour, now.wMinute, now.wSecond, now.wMilliseconds
    )
}

#[cfg(not(windows))]
fn clock() -> String {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let secs = (ms / 1000) % 86_400;
    format!(
        "utc {:02}:{:02}:{:02}.{:03}",
        secs / 3600,
        (secs / 60) % 60,
        secs % 60,
        ms % 1000
    )
}

/// Best-effort line to stderr AND the run's log file: console launches
/// still see it, console-less launches keep it in the file, and nobody
/// panics.
pub fn diag(args: std::fmt::Arguments<'_>) {
    let line = format!("[{}] {args}", clock());
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr, "{line}");
    if let Some(mut file) = log_file() {
        let _ = writeln!(file, "{line}");
        let _ = file.flush();
    }
}

/// `diag(format_args!(...))` with the usual `format!` ergonomics.
#[macro_export]
macro_rules! diag {
    ($($arg:tt)*) => {
        $crate::diag::diag(format_args!($($arg)*))
    };
}
