//! The running-instance guard for the Windows install and uninstall
//! lanes: exact-image-path detection and graceful-then-forced shutdown
//! through the Windows Restart Manager.
//!
//! Path discrimination is the point. The guard this module replaces
//! (`taskkill /F /IM <file name>`) killed every process carrying the
//! image name on the machine and never checked the kill landed — the
//! Windows Restart Manager instead matches processes by the exact
//! executable path, asks nicely first ([`RmShutdown`] delivers
//! `WM_ENDSESSION` to windowed applications and console CTRL events so
//! a well-behaved app exits through its own save-state path), and
//! reports exactly which processes survived, so a lane can fail loudly
//! instead of silently shipping a half-removed or stale install. The
//! registered resource is always the FULL path of the previous
//! install's executable: a bare file name registers against nothing
//! and every query silently answers zero processes (the same lesson
//! the clash-verge-rev NSIS installer learned the hard way — its
//! pre-2.5.10 checks passed a bare exe name to the Restart Manager and
//! the running-instance prompt never fired).

use std::path::Path;

/// One process the Restart Manager matched for a registered file.
#[derive(Debug, Clone, PartialEq)]
pub struct RunningInstance {
    /// The process id, for the lane's failure messages.
    pub pid: u32,

    /// The registered application display name — the process's
    /// description as the Restart Manager sees it.
    pub app_name: String,
}

/// The processes whose executable image IS `exe` (an exact path match,
/// not a name match): empty when none. `Err` when the Restart Manager
/// session could not be established or the file registration failed —
/// the callers degrade to a warning instead of guessing, and the swap's
/// own retry/merge lanes remain the tolerance for what the guard cannot
/// see.
pub fn running_instances(exe: &Path) -> std::io::Result<Vec<RunningInstance>> {
    running_instances_impl(exe)
}

/// Shuts the processes running from `exe` down: the Restart Manager's
/// graceful close first, the [`windows_sys::Win32::System::RestartManager::RmForceShutdown`]
/// action terminating whatever ignores it. Returns the survivors — an
/// empty list means every instance stopped and the file is free.
pub fn stop_instances(exe: &Path) -> std::io::Result<Vec<RunningInstance>> {
    stop_instances_impl(exe)
}

#[cfg(windows)]
fn running_instances_impl(exe: &Path) -> std::io::Result<Vec<RunningInstance>> {
    with_session(exe, query)
}

#[cfg(windows)]
fn stop_instances_impl(exe: &Path) -> std::io::Result<Vec<RunningInstance>> {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::RestartManager::{RmForceShutdown, RmShutdown};

    // Nothing running: no shutdown call, nothing to verify.
    if running_instances(exe)?.is_empty() {
        return Ok(Vec::new());
    }
    with_session(exe, |handle| {
        // SAFETY: a valid session handle; `None` = no per-process
        // status callback. `RmForceShutdown` layers forced termination
        // on top of the graceful pass for whatever ignores the close
        // (windows-sys types the action flag as i32 against the API's
        // u32, hence the cast).
        let code = unsafe { RmShutdown(handle, RmForceShutdown as u32, None) };
        if code != ERROR_SUCCESS {
            return Err(os_error(code));
        }
        Ok(())
    })?;
    // The authoritative answer comes from a fresh query, not the
    // shutdown session: a process is gone exactly when nothing matches
    // the path anymore.
    running_instances(exe)
}

#[cfg(not(windows))]
fn running_instances_impl(_exe: &Path) -> std::io::Result<Vec<RunningInstance>> {
    Ok(Vec::new())
}

#[cfg(not(windows))]
fn stop_instances_impl(_exe: &Path) -> std::io::Result<Vec<RunningInstance>> {
    // POSIX never locks a running executable against replacement, so
    // there is no instance to stop.
    Ok(Vec::new())
}

/// Opens a Restart Manager session over the single resource `exe` (its
/// FULL path — see the module docs), runs `body` with the handle, and
/// always ends the session.
#[cfg(windows)]
fn with_session<T>(exe: &Path, body: impl FnOnce(u32) -> std::io::Result<T>) -> std::io::Result<T> {
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::RestartManager::{
        CCH_RM_SESSION_KEY, RmEndSession, RmRegisterResources, RmStartSession,
    };

    let path: Vec<u16> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut handle = 0u32;
    // The session key buffer is CCH_RM_SESSION_KEY plus its NUL — a
    // 32-wchar buffer makes RmStartSession fail with a buffer error and
    // every query silently answers "nothing running" at the caller.
    let mut key = [0u16; CCH_RM_SESSION_KEY as usize + 1];
    // SAFETY: both buffers outlive the call — the handle is written
    // once and the session key buffer is `CCH_RM_SESSION_KEY + 1`
    // wchars, exactly what the API fills.
    let code = unsafe { RmStartSession(&mut handle, 0, key.as_mut_ptr()) };
    if code != ERROR_SUCCESS {
        return Err(os_error(code));
    }
    let result = (|| {
        let paths = [path.as_ptr()];
        // SAFETY: the path array is null-terminated and outlives the
        // call; no applications or services ride the registration —
        // the file alone is what the processes are matched against.
        let code = unsafe {
            RmRegisterResources(
                handle,
                1,
                paths.as_ptr(),
                0,
                std::ptr::null(),
                0,
                std::ptr::null(),
            )
        };
        if code != ERROR_SUCCESS {
            return Err(os_error(code));
        }
        body(handle)
    })();
    // SAFETY: the handle came from the successful start above and has
    // not been ended yet; the session ends either way.
    unsafe { RmEndSession(handle) };
    result
}

/// Lists the processes whose image the session's registered resource
/// matches, oldest first (`RmGetList` order).
#[cfg(windows)]
fn query(handle: u32) -> std::io::Result<Vec<RunningInstance>> {
    use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS};
    use windows_sys::Win32::System::RestartManager::{RM_PROCESS_INFO, RmGetList};

    // Two-phase `RmGetList`: probe for the needed capacity, then fill.
    // The matched set can grow between the phases (an instance racing
    // in), which surfaces as another MORE_DATA on the fill — retry with
    // the refreshed capacity, bounded. Two API quirks the probe run
    // pinned down: the probe's reasons pointer must be REAL even with
    // a null list (a null there is ERROR_INVALID_PARAMETER), and the
    // array capacity rides `pnProcInfo` (in/out), not the needed-count
    // out-parameter.
    for _ in 0..8 {
        let mut needed = 0u32;
        let mut written = 0u32;
        let mut probe_reasons = 0u32;
        // SAFETY: the counts are written, and the null affected-apps
        // list beside a real reasons slot is the documented probe shape.
        let code = unsafe {
            RmGetList(
                handle,
                &mut needed,
                &mut written,
                std::ptr::null_mut(),
                &mut probe_reasons,
            )
        };
        if code == ERROR_SUCCESS {
            return Ok(Vec::new());
        }
        if code != ERROR_MORE_DATA {
            return Err(os_error(code));
        }
        let mut infos: Vec<RM_PROCESS_INFO> = vec![unsafe { std::mem::zeroed() }; needed as usize];
        let mut reasons = vec![0u32; needed as usize];
        let mut capacity = needed;
        // SAFETY: the list has room for `capacity` entries and the
        // reasons buffer one flag per entry, as the probe demanded; the
        // call receives the capacity through `capacity` and reports the
        // written count back through the same slot.
        let code = unsafe {
            RmGetList(
                handle,
                &mut needed,
                &mut capacity,
                infos.as_mut_ptr(),
                reasons.as_mut_ptr(),
            )
        };
        if code == ERROR_MORE_DATA {
            // The set grew between the phases — reallocate and retry.
            continue;
        }
        if code != ERROR_SUCCESS {
            return Err(os_error(code));
        }
        infos.truncate(capacity as usize);
        return Ok(infos.iter().map(RunningInstance::from_info).collect());
    }
    Err(std::io::Error::other(
        "the Restart Manager process list kept growing between queries",
    ))
}

/// A Restart Manager error code as an [`std::io::Error`] — the raw OS
/// code, so the lane messages carry the API's own wording.
#[cfg(windows)]
fn os_error(code: u32) -> std::io::Error {
    std::io::Error::from_raw_os_error(code as i32)
}

#[cfg(windows)]
impl RunningInstance {
    /// Reads one `RM_PROCESS_INFO`: the process id and the
    /// NUL-terminated application name.
    fn from_info(info: &windows_sys::Win32::System::RestartManager::RM_PROCESS_INFO) -> Self {
        let len = info
            .strAppName
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(info.strAppName.len());
        Self {
            pid: info.Process.dwProcessId,
            app_name: String::from_utf16_lossy(&info.strAppName[..len]),
        }
    }
}

#[cfg(test)]
mod tests {
    // The only test below is Windows-only (the RM_PROCESS_INFO shape
    // is a Windows type), so the import follows the same gate — an
    // ungated import is an unused-import error on the Linux lane.
    #[cfg(windows)]
    use super::RunningInstance;

    /// The display name survives the lossy UTF-16 round trip; the pid
    /// passes through untouched. Pure struct plumbing — the API shape
    /// is exercised against a live process in
    /// `tests/stop_running_instance.rs`.
    #[cfg(windows)]
    #[test]
    fn display_name_reads_through_utf16() {
        let mut info: windows_sys::Win32::System::RestartManager::RM_PROCESS_INFO =
            unsafe { std::mem::zeroed() };
        info.Process.dwProcessId = 4242;
        for (slot, unit) in info.strAppName.iter_mut().zip("Shun Demo".encode_utf16()) {
            *slot = unit;
        }
        let parsed = RunningInstance::from_info(&info);
        assert_eq!(parsed.pid, 4242);
        assert_eq!(parsed.app_name, "Shun Demo");
    }
}
