//! Done-page shortcut application: the one place the user's shortcut
//! choices take effect. The install flow itself creates none (the
//! manifest pins both launcher policies to `never`), so the wizard's
//! final confirmation — and the headless run's conventional defaults —
//! land here.
//!
//! Windows-only surface (`.lnk` + Explorer notification); other
//! platforms have no Start-menu/desktop metaphor yet.

use std::path::{Path, PathBuf};

/// Applies one set of shortcut choices: creates or removes the
/// requested `.lnk`s and shell-notifies the changed surfaces. Both
/// operations run even when one fails — the combined error is returned,
/// and a shortcut failure never invalidates the completed install.
pub fn apply_shortcut_choices(
    aumid: &str,
    main_exe: &str,
    desktop: Option<bool>,
    menu: Option<bool>,
    dir: &str,
) -> Result<(), String> {
    let dir = dir.trim().trim_end_matches('\\').to_string();
    if dir.is_empty() {
        return Err("the install directory must not be empty".into());
    }
    let exe = Path::new(&dir).join(main_exe);
    if !exe.is_file() {
        return Err(format!("{main_exe} not found in the install directory"));
    }

    let mut changed = Vec::new();
    let mut failures = Vec::new();
    if let Some(want) = menu {
        let link = start_menu_link(main_exe);
        if let Err(e) = apply_shortcut(&link, &exe, want, aumid, &mut changed) {
            failures.push(format!("start-menu shortcut: {e}"));
        }
    }
    if let Some(want) = desktop {
        if let Err(e) = apply_shortcut(&desktop_link(main_exe), &exe, want, aumid, &mut changed) {
            failures.push(format!("desktop shortcut: {e}"));
        }
    }
    notify_shell_change(&changed);
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

/// The `.lnk` file name for a main executable: the stem with an `.lnk`
/// suffix.
fn link_name(main_exe: &str) -> String {
    let stem = main_exe.strip_suffix(".exe").unwrap_or(main_exe);
    format!("{stem}.lnk")
}

/// The Start-menu entry path (per-user Programs folder).
fn start_menu_link(main_exe: &str) -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    base.join(r"Microsoft\Windows\Start Menu\Programs")
        .join(link_name(main_exe))
}

/// The desktop entry path (known folder, USERPROFILE fallback).
fn desktop_link(main_exe: &str) -> PathBuf {
    use windows_sys::Win32::UI::Shell::{FOLDERID_Desktop, SHGetKnownFolderPath};

    // SAFETY: standard known-folder query; the returned PIDL string is
    // copied out and freed.
    unsafe {
        let mut path = std::ptr::null_mut();
        let hr = SHGetKnownFolderPath(&FOLDERID_Desktop, 0, std::ptr::null_mut(), &mut path);
        if hr == 0 && !path.is_null() {
            let mut len = 0usize;
            while *path.add(len) != 0 {
                len += 1;
            }
            let wide = std::slice::from_raw_parts(path, len);
            let s = String::from_utf16_lossy(wide);
            windows_sys::Win32::System::Com::CoTaskMemFree(path.cast());
            return PathBuf::from(s).join(link_name(main_exe));
        }
    }
    let base = std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    base.join("Desktop").join(link_name(main_exe))
}

/// Creates or removes one `.lnk`, recording the path in `changed` when
/// the filesystem actually changes. Creation stamps the app's AUMID on
/// the link — best-effort: a failed stamp only warns.
fn apply_shortcut(
    link: &Path,
    exe: &Path,
    want: bool,
    aumid: &str,
    changed: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if want {
        if link.is_file() {
            return Ok(());
        }
        if let Some(parent) = link.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("create dir: {e}"))?;
        }
        mslnk::ShellLink::new(exe)
            .and_then(|l| l.create_lnk(link))
            .map_err(|e| format!("create shortcut: {e}"))?;
        if let Err(e) = crate::targets::aumid::stamp(link, aumid) {
            eprintln!("shun: AUMID stamp on {}: {e}", link.display());
        }
        changed.push(link.to_path_buf());
    } else {
        match std::fs::remove_file(link) {
            Ok(()) => changed.push(link.to_path_buf()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("remove shortcut: {e}")),
        }
    }
    Ok(())
}

/// Tells Explorer the shortcut surfaces changed — a per-path update
/// event plus one association-level refresh.
fn notify_shell_change(paths: &[PathBuf]) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::{
        SHCNE_ASSOCCHANGED, SHCNE_UPDATEITEM, SHCNF_PATH, SHChangeNotify,
    };

    // SAFETY: plain shell-notification calls with wide-string payloads.
    unsafe {
        for path in paths {
            let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            SHChangeNotify(
                SHCNE_UPDATEITEM as i32,
                SHCNF_PATH,
                wide.as_ptr().cast(),
                std::ptr::null(),
            );
        }
        SHChangeNotify(
            SHCNE_ASSOCCHANGED as i32,
            0,
            std::ptr::null(),
            std::ptr::null(),
        );
    }
}
