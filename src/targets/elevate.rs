//! Machine-scope elevation helpers (Windows).
//!
//! A machine-wide install writes HKLM, the all-users Start Menu, and the
//! public desktop — surfaces that need an elevated token. The shell
//! stays `asInvoker` and re-launches *itself* elevated (the standard
//! bootstrapper pattern) when the resolved scope is `machine` and the
//! current process is not elevated; the UAC consent is the one prompt
//! the user sees, and only for the install mode that genuinely needs
//! it. Per-user installs never touch this path.

use crate::error::ShunError;

/// Whether the current process runs with an elevated token.
#[cfg(windows)]
pub fn is_elevated() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{
        GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut returned = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut _ as *mut core::ffi::c_void,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        ) != 0;
        CloseHandle(token);
        ok && elevation.TokenIsElevated != 0
    }
}

/// Machine scope is Windows-only; the per-user Unix backends never ask,
/// so elevation on Unix is reported as absent.
#[cfg(not(windows))]
pub fn is_elevated() -> bool {
    false
}

/// Re-launches the current executable elevated with `args` (the runas
/// verb — Windows shows the UAC consent). Returns `Ok(true)` when the
/// elevated process spawned (the caller should exit; the relaunched
/// copy carries on) and `Ok(false)` when the user declined the prompt.
#[cfg(windows)]
pub fn relaunch_elevated(args: &str) -> Result<bool, ShunError> {
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let Some(exe) = std::env::current_exe().ok() else {
        return Err(ShunError::Config(
            "machine scope needs a re-launchable executable".into(),
        ));
    };
    let verb: Vec<u16> = "runas".encode_utf16().chain(Some(0)).collect();
    let file: Vec<u16> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
    let parameters: Vec<u16> = args.encode_utf16().chain(Some(0)).collect();
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            parameters.as_ptr(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // ShellExecuteW returns a value > 32 on success.
    Ok(result as usize > 32)
}

#[cfg(not(windows))]
pub fn relaunch_elevated(_args: &str) -> Result<bool, ShunError> {
    Err(ShunError::Unsupported(
        "machine-scope elevation (windows only)",
    ))
}
