//! Filesystem probes for would-be install targets: drive enumeration,
//! writability probing, and conventional install-root candidates.
//!
//! Unlike the [`crate::targets::flash`] backend (removable devices only),
//! this module lists *every* logical drive / mount root — an installer
//! wants to offer `D:\Games` just as much as `C:\`. The probes are
//! best-effort and side-effect free: [`is_dir_writable`] answers
//! strictly by creating and deleting a uniquely-named temp file beside
//! the target, never by touching the target itself.

use std::path::{Path, PathBuf};

/// The nature of a drive / mount root, as reported by the OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveKind {
    /// Removable media (USB sticks, card readers).
    Removable,
    /// Fixed internal storage.
    Fixed,
    /// A network share.
    Network,
    /// An optical drive.
    CdRom,
    /// A RAM-backed disk.
    RamDisk,
    /// The OS reports nothing usable (unformatted, undetermined, ...).
    Unknown,
}

/// One logical drive (Windows) or mount root (Unix).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveInfo {
    /// Windows: the drive root (`C:\`); Unix: the mount point.
    pub mount: PathBuf,
    /// Drive / filesystem kind.
    pub kind: DriveKind,
    /// Volume label, best-effort (`None` when the OS reports none —
    /// media-less drives, unformatted volumes, ...).
    pub label: Option<String>,
}

/// All logical drives / mount roots, fixed and otherwise (unlike the
/// flash target's removable-only list). Enumeration is best-effort:
/// an error yields an empty list.
pub fn list_drives() -> Vec<DriveInfo> {
    list_drives_impl()
}

/// Writability probe for a would-be install target. `dir` may not exist:
/// the probe walks up to the nearest existing ancestor and creates +
/// deletes a uniquely-named temp file there. A `true` answer means
/// [`std::fs::create_dir_all`] on `dir` should succeed and the directory
/// is writable. `dir` itself is never created; any error answers `false`.
pub fn is_dir_writable(dir: &Path) -> bool {
    // Walk up to the nearest existing ancestor — probing happens where
    // the filesystem already exists, `dir` itself stays untouched.
    let mut probe = dir;
    loop {
        if probe.exists() {
            break;
        }
        match probe.parent() {
            Some(parent) => probe = parent,
            None => return false,
        }
    }
    for _ in 0..PROBE_ATTEMPTS {
        let candidate = probe.join(probe_file_name());
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(_) => {
                // Best-effort cleanup: one retry rides out transient
                // locks (antivirus scanners hold fresh files briefly on
                // Windows). A leftover probe file is harmless — it is a
                // hidden, uniquely-named empty file — but try not to
                // litter.
                if std::fs::remove_file(&candidate).is_err() {
                    std::fs::remove_file(&candidate).ok();
                }
                return true;
            }
            // Name collision (absurd, but the name is only probablistic
            // unique): retry with a fresh one.
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return false,
        }
    }
    false
}

/// Returns the first candidate accepted by [`is_dir_writable`].
pub fn first_writable(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates
        .iter()
        .find(|candidate| is_dir_writable(candidate))
        .cloned()
}

/// Conventional install-root candidates in fallback priority order.
///
/// Windows: `%LOCALAPPDATA%\<product>`, `%ProgramFiles%\<product>`, then
/// `X:\<product>` for every fixed drive (`C:` first). Unix:
/// `${XDG_DATA_HOME|~/.local/share}/<product>`, then `/opt/<product>`.
///
/// `product` is joined verbatim and may carry subdirectories
/// (`"Programs/WoWSP"`). Note the `PathBuf::join` semantics: an
/// *absolute* `product` replaces the base entirely, so pass a relative
/// product path. An empty `product` is rejected with an empty list.
///
/// Environment values that are set but empty are treated as unset.
pub fn install_dir_candidates(product: &str) -> Vec<PathBuf> {
    if product.is_empty() {
        return Vec::new();
    }
    #[cfg(windows)]
    {
        let local = std::env::var_os("LOCALAPPDATA");
        let program_files = std::env::var_os("ProgramFiles");
        let fixed_drives: Vec<DriveInfo> = list_drives()
            .into_iter()
            .filter(|drive| drive.kind == DriveKind::Fixed)
            .collect();
        candidates_from_windows(
            local.as_deref(),
            program_files.as_deref(),
            &fixed_drives,
            product,
        )
    }
    #[cfg(not(windows))]
    {
        let data_home = std::env::var_os("XDG_DATA_HOME");
        let home = std::env::var_os("HOME");
        candidates_from_unix(data_home.as_deref(), home.as_deref(), product)
    }
}

/// Treats a set-but-empty environment value like an unset one (an empty
/// `XDG_DATA_HOME` would otherwise degrade into CWD-relative garbage).
fn non_empty(value: Option<&std::ffi::OsStr>) -> Option<&std::ffi::OsStr> {
    value.filter(|value| !value.is_empty())
}

/// Assembles the Windows candidate list from the given inputs; split out
/// of [`install_dir_candidates`] so the env-value handling is unit-testable
/// without mutating the live environment.
#[cfg(windows)]
fn candidates_from_windows(
    local_appdata: Option<&std::ffi::OsStr>,
    program_files: Option<&std::ffi::OsStr>,
    fixed_drives: &[DriveInfo],
    product: &str,
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(local) = non_empty(local_appdata) {
        candidates.push(PathBuf::from(local).join(product));
    }
    if let Some(program_files) = non_empty(program_files) {
        candidates.push(PathBuf::from(program_files).join(product));
    }
    candidates.extend(fixed_drives.iter().map(|drive| drive.mount.join(product)));
    candidates
}

/// The Unix counterpart of [`candidates_from_windows`] (`data-home` =
/// `XDG_DATA_HOME`, `home` = the `~/.local/share` fallback).
#[cfg(not(windows))]
fn candidates_from_unix(
    data_home: Option<&std::ffi::OsStr>,
    home: Option<&std::ffi::OsStr>,
    product: &str,
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let base = non_empty(data_home)
        .map(PathBuf::from)
        .or_else(|| non_empty(home).map(|home| PathBuf::from(home).join(".local/share")));
    if let Some(base) = base {
        candidates.push(base.join(product));
    }
    candidates.push(PathBuf::from("/opt").join(product));
    candidates
}

/// How many times the writability probe retries on a name collision.
const PROBE_ATTEMPTS: usize = 3;

/// A per-process, per-call unique file name for the writability probe.
fn probe_file_name() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or(0);
    format!(".shun-write-probe-{}-{nanos}", std::process::id())
}

// ── Windows: logical drives via the same FFI surface as the flash
//    backend, plus GetVolumeInformationW for the labels ─────────────────

/// `GetDriveTypeW` drive-type constants, mirrored locally like the flash
/// backend does (`windows-sys` keeps them behind the
/// `Win32_System_WindowsProgramming` feature).
#[cfg(windows)]
const DRIVE_REMOVABLE: u32 = 2;
#[cfg(windows)]
const DRIVE_FIXED: u32 = 3;
#[cfg(windows)]
const DRIVE_REMOTE: u32 = 4;
#[cfg(windows)]
const DRIVE_CDROM: u32 = 5;
#[cfg(windows)]
const DRIVE_RAMDISK: u32 = 6;

#[cfg(windows)]
fn list_drives_impl() -> Vec<DriveInfo> {
    use windows_sys::Win32::Storage::FileSystem::{
        GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
    };

    /// `root` UTF-16 + NUL, shaped for the drive-root FFI calls (`C:\`).
    fn wide(root: &str) -> [u16; 4] {
        let mut out = [0u16; 4];
        for (slot, ch) in out.iter_mut().zip(root.encode_utf16()) {
            *slot = ch;
        }
        out
    }

    let mask = unsafe { GetLogicalDrives() };
    if mask == 0 {
        return Vec::new();
    }

    let mut drives = Vec::new();
    for i in 0..26u32 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let letter = (b'A' + i as u8) as char;
        let root = format!("{letter}:\\");
        let root_w = wide(&root);
        let kind = match unsafe { GetDriveTypeW(root_w.as_ptr()) } {
            DRIVE_REMOVABLE => DriveKind::Removable,
            DRIVE_FIXED => DriveKind::Fixed,
            DRIVE_REMOTE => DriveKind::Network,
            DRIVE_CDROM => DriveKind::CdRom,
            DRIVE_RAMDISK => DriveKind::RamDisk,
            _ => DriveKind::Unknown,
        };
        // Volume label, best-effort: media-less or unformatted drives
        // simply report none. Network drives are skipped on purpose — a
        // disconnected SMB mapping can block inside
        // `GetVolumeInformationW` for seconds, and this enumeration runs
        // on the caller's thread ([`install_dir_candidates`] calls it
        // while sizing install defaults).
        let mut name = [0u16; 64];
        let label = if kind != DriveKind::Network
            && unsafe {
                GetVolumeInformationW(
                    root_w.as_ptr(),
                    name.as_mut_ptr(),
                    name.len() as u32,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    0,
                )
            } != 0
        {
            let end = name.iter().position(|&unit| unit == 0).unwrap_or(64);
            Some(String::from_utf16_lossy(&name[..end]))
        } else {
            None
        };
        drives.push(DriveInfo {
            mount: PathBuf::from(root),
            kind,
            label: label.filter(|label| !label.is_empty()),
        });
    }
    drives
}

// ── Linux: /proc/mounts, parsed with std only ───────────────────────────

#[cfg(all(unix, target_os = "linux"))]
fn list_drives_impl() -> Vec<DriveInfo> {
    use std::collections::HashSet;

    /// Filesystem types mapped to [`DriveKind::Network`] (pragmatic,
    /// not exhaustive).
    const NETWORK_FS_TYPES: [&str; 10] = [
        "nfs",
        "nfs4",
        "cifs",
        "smbfs",
        "sshfs",
        "fuse.sshfs",
        "ncpfs",
        "9p",
        "ceph",
        "glusterfs",
    ];
    /// Memory-backed filesystems.
    const RAM_FS_TYPES: [&str; 2] = ["tmpfs", "ramfs"];
    /// Kernel/proc pseudo-filesystems — not install targets.
    const PSEUDO_FS_TYPES: [&str; 16] = [
        "proc",
        "sysfs",
        "devtmpfs",
        "devpts",
        "mqueue",
        "cgroup",
        "cgroup2",
        "pstore",
        "bpf",
        "debugfs",
        "tracefs",
        "securityfs",
        "configfs",
        "fusectl",
        "autofs",
        "overlay",
    ];

    let Ok(raw) = std::fs::read_to_string("/proc/mounts") else {
        return Vec::new();
    };
    let mut drives = Vec::new();
    let mut seen = HashSet::new();
    for line in raw.lines() {
        let mut fields = line.split_whitespace();
        let (Some(device), Some(mount), Some(fstype)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let mount = decode_mount(mount);
        if !Path::new(&mount).is_absolute() || !seen.insert(mount.clone()) {
            continue;
        }
        let kind = if NETWORK_FS_TYPES.contains(&fstype) {
            DriveKind::Network
        } else if PSEUDO_FS_TYPES.contains(&fstype) {
            continue;
        } else if RAM_FS_TYPES.contains(&fstype) {
            DriveKind::RamDisk
        } else if device.starts_with("/dev/") {
            DriveKind::Fixed
        } else {
            DriveKind::Unknown
        };
        drives.push(DriveInfo {
            mount: PathBuf::from(mount),
            kind,
            label: None,
        });
    }
    drives
}

/// Decodes the octal escapes `/proc/mounts` uses for whitespace and
/// backslash in mount points (`\040` = space, `\011` = tab,
/// `\012` = newline, `\134` = backslash).
#[cfg(all(unix, target_os = "linux"))]
fn decode_mount(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' && index + 3 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&raw[index + 1..index + 4], 8) {
                out.push(byte);
                index += 4;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

// ── Other unix: pragmatic fallback — the POSIX root, fixed, no label ────

#[cfg(all(unix, not(target_os = "linux")))]
fn list_drives_impl() -> Vec<DriveInfo> {
    vec![DriveInfo {
        mount: PathBuf::from("/"),
        kind: DriveKind::Fixed,
        label: None,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_drives_is_populated() {
        // Mild cross-platform assertion: /proc/mounts fills on Linux,
        // the fallback root on other unix, real drives on Windows.
        assert!(!list_drives().is_empty());
    }

    /// Windows-only sanity: the fixed system drive is enumerated with
    /// its mount root present.
    #[test]
    #[cfg(windows)]
    fn list_drives_contains_a_fixed_mounted_drive() {
        let drives = list_drives();
        assert!(
            drives
                .iter()
                .any(|drive| drive.kind == DriveKind::Fixed && drive.mount.is_dir()),
            "expected a fixed drive with an existing root, got {drives:?}"
        );
    }

    #[test]
    fn probe_accepts_existing_and_not_yet_existing_dirs() {
        let dir = tempfile::tempdir().unwrap();
        assert!(is_dir_writable(dir.path()));
        let missing = dir.path().join("deep").join("nested").join("target");
        assert!(is_dir_writable(&missing));
        // The probe never creates the directory itself.
        assert!(!missing.exists());
    }

    #[test]
    fn probe_rejects_paths_under_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("occupied");
        std::fs::write(&file, b"payload").unwrap();
        assert!(!is_dir_writable(&file.join("below")));
        assert!(!is_dir_writable(&file));
        assert!(!file.join("below").exists());
    }

    #[test]
    fn probe_leaves_no_leftover_files() {
        let dir = tempfile::tempdir().unwrap();

        // Success path: the probe file is created, then deleted again.
        assert!(is_dir_writable(dir.path()));

        // Failure path (under a file): the probe never lands anywhere.
        let file = dir.path().join("occupied");
        std::fs::write(&file, b"payload").unwrap();
        assert!(!is_dir_writable(&file.join("below")));

        let leftovers: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(".shun-write-probe-"))
            .collect();
        assert!(leftovers.is_empty(), "probe leftovers: {leftovers:?}");
    }

    #[test]
    fn probe_terminates_on_the_filesystem_root() {
        // The walk-up loop must stop at the root (its parent is itself)
        // instead of looping forever; the probe answers promptly either
        // way (a bare root usually answers false — writing there needs
        // privileges — and only the return, not the verdict, is pinned).
        #[cfg(windows)]
        let root = Path::new("C:\\");
        #[cfg(not(windows))]
        let root = Path::new("/");
        let _ = is_dir_writable(root);
    }

    #[test]
    fn first_writable_picks_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let occupied = dir.path().join("occupied");
        std::fs::write(&occupied, b"payload").unwrap();
        let good = dir.path().join("good");

        assert_eq!(first_writable(&[occupied.join("below")]), None);
        assert_eq!(
            first_writable(&[occupied.join("below"), good.clone()]),
            Some(good.clone())
        );
        assert_eq!(
            first_writable(&[good, dir.path().join("other")]),
            Some(dir.path().join("good"))
        );
    }

    #[test]
    fn candidates_carry_the_product_segment_verbatim() {
        let candidates = install_dir_candidates("Programs/WoWSP");
        assert!(
            candidates.iter().all(|c| c.ends_with("Programs/WoWSP")),
            "{candidates:?}"
        );
    }

    #[test]
    fn candidates_reject_an_empty_product() {
        assert!(install_dir_candidates("").is_empty());
    }

    /// Empty environment values behave like unset ones (exercised on the
    /// pure helper — mutating the live env from a test would race the
    /// parallel suites).
    #[test]
    #[cfg(windows)]
    fn candidates_treat_empty_env_like_unset() {
        use std::ffi::OsStr;
        let fixed = vec![DriveInfo {
            mount: PathBuf::from("D:\\"),
            kind: DriveKind::Fixed,
            label: None,
        }];

        // An empty LOCALAPPDATA is skipped; ProgramFiles and the fixed
        // drive still contribute, in priority order.
        let candidates = candidates_from_windows(
            Some(OsStr::new("")),
            Some(OsStr::new("C:\\Program Files")),
            &fixed,
            "App",
        );
        assert_eq!(
            candidates,
            vec![
                PathBuf::from("C:\\Program Files\\App"),
                PathBuf::from("D:\\App")
            ]
        );

        // Nothing set at all: only the fixed-drive candidates remain.
        assert_eq!(
            candidates_from_windows(None, None, &fixed, "App"),
            vec![PathBuf::from("D:\\App")]
        );
    }

    #[test]
    #[cfg(windows)]
    fn candidates_cover_the_conventional_windows_roots() {
        let candidates = install_dir_candidates("Programs/WoWSP");
        assert!(
            candidates.len() >= 2,
            "LOCALAPPDATA + ProgramFiles at least, got {candidates:?}"
        );
        if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
            assert!(
                candidates[0].starts_with(&local),
                "{:?} should sit under {:?}",
                candidates[0],
                local
            );
        }
        if let Some(program_files) = std::env::var_os("ProgramFiles").map(PathBuf::from) {
            assert!(
                candidates
                    .iter()
                    .any(|candidate| candidate.starts_with(&program_files)),
                "no ProgramFiles candidate in {candidates:?}"
            );
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn decode_mount_expands_octal_escapes() {
        assert_eq!(decode_mount("/mnt/with\\040space"), "/mnt/with space");
        assert_eq!(decode_mount("/mnt/with\\011tab"), "/mnt/with\ttab");
        assert_eq!(
            decode_mount("/mnt/with\\134backslash"),
            "/mnt/with\\backslash"
        );
        assert_eq!(decode_mount("/plain/mount"), "/plain/mount");
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn decode_mount_keeps_a_truncated_escape() {
        // A truncated escape at the end of the string is kept verbatim
        // instead of panicking on a short slice.
        assert_eq!(decode_mount("/mnt/trailing\\"), "/mnt/trailing\\");
        assert_eq!(decode_mount("/mnt/trailing\\04"), "/mnt/trailing\\04");
        assert_eq!(decode_mount("/mnt/bogus\\099tail"), "/mnt/bogus\\099tail");
    }
}
