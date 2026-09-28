//! Done-page shortcut application: the one place the user's shortcut
//! choices take effect. The install flow itself creates none (the
//! shared wizard driver pins both launcher answers to "no"), so the
//! wizard's final confirmation — and the headless run's conventional
//! defaults — land here.
//!
//! Link naming follows the ONE rule the whole surface shares: the
//! product name stem ([`crate::targets::install::shortcut_stem`]), the
//! same name the registration pass and the uninstall sweep use. Decline
//! also removes the legacy executable-stem links older shells wrote, so
//! an upgraded install cannot leave a stale differently-named copy.

use std::path::{Path, PathBuf};

use crate::targets::install::shortcut_stem;

/// Applies one set of shortcut choices: creates or removes the
/// requested `.lnk`s and shell-notifies the changed surfaces. Both
/// operations run even when one fails — the combined error is returned,
/// and a shortcut failure never invalidates the completed install.
pub fn apply_shortcut_choices(
    aumid: &str,
    product: &str,
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
    let stem = shortcut_stem(product);
    // The legacy `.lnk` name (executable stem) pre-unification shells
    // wrote; only swept on removal, never created anymore. A name that
    // matches the primary modulo case is the same file on Windows.
    let primary_name = format!("{stem}.lnk");
    let legacy_name = link_name(main_exe);
    let legacy = (!legacy_name.eq_ignore_ascii_case(&primary_name)).then_some(legacy_name);

    let mut changed = Vec::new();
    let mut failures = Vec::new();
    if let Some(want) = menu {
        let link = start_menu_link(&stem);
        if let Err(e) = apply_shortcut(
            &link,
            legacy.as_deref().map(start_menu_link_stem),
            &exe,
            want,
            aumid,
            &mut changed,
        ) {
            failures.push(format!("start-menu shortcut: {e}"));
        }
    }
    // The desktop shortcut is a convenience and degrades like the
    // registration pass's: AV/EDR policies commonly deny `.lnk` creation
    // on the desktop specifically (fake-shortcut / ransomware
    // protection) — warn, never fail the finished install.
    if let Some(want) = desktop {
        let link = desktop_link(&stem);
        if let Err(e) = apply_shortcut(
            &link,
            legacy.as_deref().map(desktop_link_stem),
            &exe,
            want,
            aumid,
            &mut changed,
        ) {
            eprintln!("shun: desktop-shortcut-blocked: {e}");
        }
    }
    notify_shell_change(&changed);
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

/// The `.lnk` file name for a main executable: the FILE-name stem with
/// an `.lnk` suffix — a payload-relative `bin/app.exe` names the link
/// `app.lnk`, not `bin/app.lnk` (the pre-unification naming — kept
/// only for the legacy sweep).
fn link_name(main_exe: &str) -> String {
    let file = Path::new(main_exe)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| main_exe.to_string());
    let stem = file.strip_suffix(".exe").unwrap_or(&file);
    format!("{stem}.lnk")
}

/// The Start-menu entry path for a link file name (per-user Programs
/// folder).
fn start_menu_link_stem(link: &str) -> PathBuf {
    start_menu_dir().join(link)
}

/// The Start-menu entry path for a sanitized product stem.
fn start_menu_link(stem: &str) -> PathBuf {
    start_menu_link_stem(&format!("{stem}.lnk"))
}

/// The per-user Start-menu `Programs` base directory.
fn start_menu_dir() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
        .join(r"Microsoft\Windows\Start Menu\Programs")
}

/// The desktop entry path for a link file name (known folder).
fn desktop_link_stem(link: &str) -> PathBuf {
    desktop_dir().join(link)
}

/// The desktop entry path for a sanitized product stem.
fn desktop_link(stem: &str) -> PathBuf {
    desktop_link_stem(&format!("{stem}.lnk"))
}

/// The known-folder desktop directory (USERPROFILE fallback).
fn desktop_dir() -> PathBuf {
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
            return PathBuf::from(s);
        }
    }
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
        .join("Desktop")
}

/// Creates or removes one `.lnk`, recording the path in `changed` when
/// the filesystem actually changes. Creation stamps the app's AUMID on
/// the link — best-effort: a failed stamp only warns. Both directions
/// also delete the legacy `link` (the executable-stem name older
/// shells wrote) when it names a different file, so an upgraded install
/// never leaves a stale differently-named copy behind.
fn apply_shortcut(
    link: &Path,
    legacy: Option<PathBuf>,
    exe: &Path,
    want: bool,
    aumid: &str,
    changed: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if want {
        if !link.is_file() {
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
        }
        // Best-effort sweep: the legacy link may not exist at all, and a
        // failure to remove it must not fail the creation.
        if let Some(legacy) = legacy
            && std::fs::remove_file(&legacy).is_ok()
        {
            changed.push(legacy);
        }
    } else {
        match std::fs::remove_file(link) {
            Ok(()) => changed.push(link.to_path_buf()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("remove shortcut: {e}")),
        }
        if let Some(legacy) = legacy {
            // Best-effort sweep: the legacy link may not exist at all,
            // and a failure to remove it must not mask the primary one.
            if std::fs::remove_file(&legacy).is_ok() {
                changed.push(legacy);
            }
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
