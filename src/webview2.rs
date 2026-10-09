//! Evergreen offline-installer delivery for the WebView2 runtime.
//!
//! The `evergreen-installer` strategy (config) carries Microsoft's
//! standalone Evergreen installer inside the payload, under the
//! `webview2/` prefix. On a machine the loader reports as runtime-less,
//! the shell stages that subtree into its per-user cache and runs the
//! installer silently (`/silent /install`) BEFORE the face ladder
//! resolves — a successful run upgrades the machine in place and the
//! webview face stays reachable; a declined elevation or a failed run
//! degrades to the egui face instead of dying on a message box.
//!
//! Discovery is by prefix + file-name pattern, not an explicit path, so
//! the payload layout stays a packaging concern:
//!
//! ```text
//! payload/
//!   webview2/
//!     MicrosoftEdgeWebView2RuntimeInstallerX64.exe
//! ```
//!
//! The download itself is NOT automated — `shun build` packs whatever
//! the declared payload directory carries. Fetch the installer from
//! <https://developer.microsoft.com/microsoft-edge/webview2/> (or the
//! Evergreen fwlink) into the payload before building. The prefix is
//! matched case-sensitively (`webview2/`, lowercase — the same
//! `starts_with` the staging `extract_prefix` rides, so discovery and
//! extraction can never diverge); packagers naming the directory
//! `WebView2/` silently opt out of the whole feature.

use std::path::Path;

use crate::payload::ArchivePayload;

/// Payload-relative directory carrying the Evergreen offline installer.
pub const EVERGREEN_PREFIX: &str = "webview2";

/// File-name prefix of the Evergreen offline installer inside the
/// payload prefix (compared case-insensitively — Windows resolves paths
/// that way and Microsoft's own casing has drifted between releases).
const EVERGREEN_INSTALLER_STEM: &str = "microsoftedgewebview2runtimeinstaller";

/// Whether the payload carries an Evergreen offline installer under the
/// [`EVERGREEN_PREFIX`] subtree — the "actually carried" gate for the
/// silent-install bootstrap and the missing-runtime warning.
///
/// Carry exactly ONE architecture's installer: discovery takes the
/// first stem match in pack order, so an X64+ARM64 pair runs whichever
/// packed first (a wrong-arch run fails gracefully and degrades, but
/// silently).
pub fn evergreen_carried(payload: &ArchivePayload) -> bool {
    evergreen_entry(payload).is_some()
}

/// The archive-relative installer path, when the payload carries one.
fn evergreen_entry(payload: &ArchivePayload) -> Option<std::path::PathBuf> {
    payload
        .entries()
        .iter()
        .map(|entry| &entry.path)
        .find(|path| {
            path.starts_with(Path::new(EVERGREEN_PREFIX))
                && path.file_name().is_some_and(|name| {
                    name.to_string_lossy()
                        .to_lowercase()
                        .starts_with(EVERGREEN_INSTALLER_STEM)
                })
                && path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
        })
        .cloned()
}

/// The per-user cache the installer subtree stages into before the
/// silent run — the same one-copy cache the fixed-version bootstrap
/// uses, scoped per product.
pub fn evergreen_cache(product: &str) -> Option<std::path::PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .map(|local| local.join("shun").join(product).join("webview2"))
}

/// Stages the carried installer into `cache` and returns the extracted
/// executable path. `None` when the payload carries no installer or the
/// staging write fails — callers treat that as "not carried" and keep
/// going (the face ladder degrades to egui).
pub fn stage_evergreen(payload: &ArchivePayload, cache: &Path) -> Option<std::path::PathBuf> {
    let entry = evergreen_entry(payload)?;
    if payload
        .extract_prefix(cache, Path::new(EVERGREEN_PREFIX), &mut |_| {})
        .is_err()
    {
        return None;
    }
    let staged = cache.join(&entry);
    staged.is_file().then_some(staged)
}

/// Runs the Evergreen offline installer silently. The installer's
/// manifest requires elevation for the machine-wide install, so an os
/// error 740 relaunches through the UAC consent (PowerShell
/// `Start-Process -Verb RunAs -Wait`); every child is started with
/// `CREATE_NO_WINDOW` so no console ever flashes in front of the
/// wizard. Callers decide success by re-probing the loader afterwards —
/// the exit code alone cannot distinguish "installed" from "already
/// present, nothing to do".
///
/// Non-Windows platforms have no WebView2 story; the call is a no-op
/// error so callers can stay unconditional.
#[cfg(windows)]
pub fn run_evergreen_silent(installer: &Path) -> std::io::Result<std::process::ExitStatus> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let args = ["/silent", "/install"];
    match std::process::Command::new(installer)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .status()
    {
        Ok(status) => Ok(status),
        Err(err) if err.raw_os_error() == Some(740) => {
            // Single-quote PowerShell literals; a quote inside the
            // literal (profile paths like `O'Brien`) escapes by
            // doubling. The argument list is fixed flags, so only the
            // path needs the treatment — and the flags must COMMA-join
            // into one -ArgumentList array: a space join passes two
            // positional parameters and PowerShell rejects the command
            // outright (verified: ParameterBindingException, exit 1).
            let exe = installer.display().to_string().replace('\'', "''");
            let script = format!(
                "Start-Process -FilePath '{exe}' -ArgumentList '{}' -Verb RunAs -Wait",
                args.join("','")
            );
            std::process::Command::new("powershell")
                .args(["-NoProfile", "-Command", &script])
                .creation_flags(CREATE_NO_WINDOW)
                .status()
        }
        Err(err) => Err(err),
    }
}

#[cfg(not(windows))]
pub fn run_evergreen_silent(_installer: &Path) -> std::io::Result<std::process::ExitStatus> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "the Evergreen installer only exists on Windows",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payload::pack_directory;

    /// Packs a scratch tree and decodes it back — the same roundtrip the
    /// shell build performs.
    fn packed(dir: &Path) -> ArchivePayload {
        ArchivePayload::from_bytes(&pack_directory(dir).expect("packs")).expect("decodes")
    }

    #[test]
    fn evergreen_is_found_under_the_payload_prefix() {
        let dir = std::env::temp_dir().join(format!("shun-wv2-found-{}", std::process::id()));
        let inner = dir.join(EVERGREEN_PREFIX);
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(
            inner.join("MicrosoftEdgeWebView2RuntimeInstallerX64.exe"),
            b"MZ",
        )
        .unwrap();
        let payload = packed(&dir);
        let _ = std::fs::remove_dir_all(&dir);

        assert!(evergreen_carried(&payload));
        assert_eq!(
            evergreen_entry(&payload),
            Some(Path::new("webview2/MicrosoftEdgeWebView2RuntimeInstallerX64.exe").to_path_buf())
        );
    }

    #[test]
    fn evergreen_match_is_case_insensitive_but_name_gated() {
        let dir = std::env::temp_dir().join(format!("shun-wv2-case-{}", std::process::id()));
        let inner = dir.join(EVERGREEN_PREFIX);
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(
            inner.join("MICROSOFTEDGEWEBVIEW2RUNTIMEINSTALLERARM64.EXE"),
            b"MZ",
        )
        .unwrap();
        // Same prefix, wrong name: the runtime's EULA, not an installer.
        std::fs::write(inner.join("license.txt"), b"eula").unwrap();
        let payload = packed(&dir);
        let _ = std::fs::remove_dir_all(&dir);

        assert!(evergreen_carried(&payload));
        assert_eq!(
            evergreen_entry(&payload)
                .unwrap()
                .to_string_lossy()
                .to_lowercase(),
            "webview2/microsoftedgewebview2runtimeinstallerarm64.exe"
        );
    }

    #[test]
    fn evergreen_outside_the_prefix_is_ignored() {
        let dir = std::env::temp_dir().join(format!("shun-wv2-outside-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("tools")).unwrap();
        std::fs::write(
            dir.join("tools/MicrosoftEdgeWebView2RuntimeInstallerX64.exe"),
            b"MZ",
        )
        .unwrap();
        let payload = packed(&dir);
        let _ = std::fs::remove_dir_all(&dir);

        assert!(!evergreen_carried(&payload));
    }

    #[test]
    fn evergreen_staging_extracts_the_installer() {
        let dir = std::env::temp_dir().join(format!("shun-wv2-stage-{}", std::process::id()));
        let inner = dir.join(EVERGREEN_PREFIX);
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(
            inner.join("MicrosoftEdgeWebView2RuntimeInstallerX64.exe"),
            b"MZ-payload",
        )
        .unwrap();
        let payload = packed(&dir);
        let _ = std::fs::remove_dir_all(&dir);

        let cache = std::env::temp_dir().join(format!("shun-wv2-cache-{}", std::process::id()));
        let staged = stage_evergreen(&payload, &cache);
        assert_eq!(
            staged.as_deref(),
            Some(
                cache
                    .join(EVERGREEN_PREFIX)
                    .join("MicrosoftEdgeWebView2RuntimeInstallerX64.exe")
                    .as_path()
            )
        );
        assert_eq!(
            std::fs::read(staged.unwrap()).unwrap(),
            b"MZ-payload".to_vec()
        );
        let _ = std::fs::remove_dir_all(&cache);
    }

    #[test]
    fn staging_without_a_carried_installer_is_none() {
        let dir = std::env::temp_dir().join(format!("shun-wv2-bare-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("app.exe"), b"MZ").unwrap();
        let payload = packed(&dir);
        let _ = std::fs::remove_dir_all(&dir);

        let cache =
            std::env::temp_dir().join(format!("shun-wv2-bare-cache-{}", std::process::id()));
        assert_eq!(stage_evergreen(&payload, &cache), None);
        assert!(!cache.join(EVERGREEN_PREFIX).exists());
    }
}
