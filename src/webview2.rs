//! Evergreen offline-installer delivery for the WebView2 runtime.
//!
//! The `evergreen-installer` strategy (config) carries Microsoft's
//! standalone Evergreen installer inside the payload, under the
//! `webview2/` prefix. On a machine the loader reports as runtime-less,
//! initialization only decodes and stages that subtree into the
//! per-user cache — the runtime install happens at the INSTALL step:
//! wizard runs ask first (a consent callout) and run the installer
//! with its own window, headless runs pass `/silent /install` under
//! the `silent-install` knob. A declined consent or a failed run just
//! leaves the runtime missing — the ladder degrades to the egui face
//! instead of dying on a message box.
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
//! The download is only automated for the networked source (below) —
//! `shun build` packs whatever the declared payload directory carries.
//! Fetch a carried installer from
//! <https://developer.microsoft.com/microsoft-edge/webview2/> (or the
//! Evergreen fwlink) into the payload before building. The prefix is
//! matched case-sensitively (`webview2/`, lowercase — the same
//! `starts_with` the staging `extract_prefix` rides, so discovery and
//! extraction can never diverge); packagers naming the directory
//! `WebView2/` silently opt out of the whole feature.
//!
//! # The networked source
//!
//! A manifest whose `evergreen-installer` declares `download-url` and
//! whose payload carries no installer resolves to
//! [`EvergreenSource::Download`]: the runtime shell (which owns the
//! HTTP fetch — this crate stays offline-safe) downloads Microsoft's
//! Evergreen installer from that URL inside the same install-step
//! acquire, behind the same consent (headless keeps the silent path).
//! The `silent-install` knob gates carried and downloaded alike, so
//! disabling it opts out of both. The intended value is a permanent
//! permalink (the Evergreen bootstrapper's fwlink) — a few MB that
//! fetches the actual runtime itself.
//!
//! # The integrity gate
//!
//! A carried installer is already digested: it rides the payload
//! archive, whose per-entry SHA-256 the decoder checks before anything
//! stages it ([`crate::payload`]). A DOWNLOAD has no such cover — the
//! bytes arrive from the network and the install step runs them inside
//! the flow, elevated through UAC — so the networked source carries its
//! own pin and every fetch ends in [`verify_evergreen_download`]:
//!
//! * the manifest's `download-sha256` wins when it pins one (the
//!   per-product override, and the only option for mirrors);
//! * otherwise [`EVERGREEN_PERMALINK_SHA256`] pins the permalinks this
//!   crate documents;
//! * a URL with neither is REFUSED ([`EvergreenDigestError::Unpinned`]).
//!
//! Nothing runs on a mismatch or an unpinned URL: the acquire reports a
//! loud refusal, deletes the staged file, and the install step degrades
//! to its usual "runtime still missing" row. The gate is fail-closed on
//! purpose — a silent fallback back to "run whatever arrived" would
//! reinstate exactly the hole it closes.

use std::path::Path;

use sha2::{Digest, Sha256};

use crate::config::ShunConfig;
use crate::payload::ArchivePayload;

/// Payload-relative directory carrying the Evergreen offline installer.
pub const EVERGREEN_PREFIX: &str = "webview2";

/// File-name prefix of the Evergreen offline installer inside the
/// payload prefix (compared case-insensitively — Windows resolves paths
/// that way and Microsoft's own casing has drifted between releases).
const EVERGREEN_INSTALLER_STEM: &str = "microsoftedgewebview2runtimeinstaller";

/// Whether the payload carries an Evergreen offline installer under the
/// [`EVERGREEN_PREFIX`] subtree — the "actually carried" gate for the
/// install-step runtime acquire and the missing-runtime warning.
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

/// The per-user cache the installer subtree stages into at init — the
/// install step runs it from there; the same one-copy cache the
/// fixed-version bootstrap uses, scoped per product.
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
    stage_evergreen_with_progress(payload, cache, &mut |_| {})
}

/// The reporting twin of [`stage_evergreen`]: `on_percent` receives the
/// extraction's own 0-100 as the runtime subtree unfolds (the feed a
/// startup splash renders while ~127 MB leave the archive).
pub fn stage_evergreen_with_progress(
    payload: &ArchivePayload,
    cache: &Path,
    on_percent: &mut dyn FnMut(u8),
) -> Option<std::path::PathBuf> {
    let entry = evergreen_entry(payload)?;
    let mut sink = |event: crate::flow::FlowEvent| {
        if let crate::flow::FlowEvent::Progress {
            percent: Some(percent),
            ..
        } = event
        {
            on_percent(percent);
        }
    };
    if payload
        .extract_prefix(cache, Path::new(EVERGREEN_PREFIX), &mut sink)
        .is_err()
    {
        return None;
    }
    let staged = cache.join(&entry);
    staged.is_file().then_some(staged)
}

/// Where the install step can get an Evergreen installer on a
/// runtime-less machine — the ONE decision every caller (install-step
/// acquire, consent ask, fallback banner copy) renders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvergreenSource {
    /// The payload carries an installer under the `webview2/` prefix;
    /// the boxed path is its archive-relative entry. Staged into the
    /// per-user cache at init, run at the install step behind the
    /// consent ask (headless: behind the `silent-install` knob).
    Carried(std::path::PathBuf),

    /// Nothing carried, but the manifest declares `download-url` and the
    /// `silent-install` knob is on: fetch the installer from this URL at
    /// install time through the same acquire (the same consent ask;
    /// headless runs it silently).
    Download(String),

    /// Nothing to install from — the missing-runtime warnings and the
    /// Microsoft download page are the only story.
    None,
}

/// Resolves the install-step runtime source for `config` against
/// `payload`. The carried installer always wins (offline first); the
/// networked source answers only when the strategy is
/// `evergreen-installer`, the payload carries nothing, `download-url`
/// is set AND `silent-install` is on — one knob, both sources, so a
/// product disabling silent installs opts out of networked ones too.
pub fn evergreen_source(config: &ShunConfig, payload: &ArchivePayload) -> EvergreenSource {
    if let Some(entry) = evergreen_entry(payload) {
        return EvergreenSource::Carried(entry);
    }
    if config.webview2_silent_install() {
        if let Some(url) = config.webview2_download_url() {
            return EvergreenSource::Download(url.to_string());
        }
    }
    EvergreenSource::None
}

/// Digest pins for the Evergreen permalinks this crate documents, keyed
/// by the permalink as published (lookups normalize: see
/// [`normalize_permalink`]). The manifest's `download-sha256` always
/// wins over this table; a URL with no row here and no manifest pin is
/// refused rather than trusted.
///
/// # Provenance
///
/// Both rows were fetched over HTTPS on 2026-11-16 and hashed locally
/// (`curl -L <permalink> -o file && sha256sum file`):
///
/// | permalink | resolves to | bytes | sha256 |
/// | --- | --- | --- | --- |
/// | `?linkid=2124703` | `MicrosoftEdgeWebview2Setup.exe` — the small bootstrapper | 1 897 688 | `cf2f2445…082e` |
/// | `?linkid=2124701` | `MicrosoftEdgeWebView2RuntimeInstallerX64.exe` — the standalone installer | 215 116 512 | `d3cbe8fe…f9c3` |
///
/// # Bumping (read before chasing a `sha256 mismatch` in a setup log)
///
/// These fwlinks are PERMALINKS: Microsoft republishes them — same URL,
/// different bytes — every time the Evergreen runtime ships a version.
/// A pin is therefore a dated fact, not a constant property of the URL.
/// When the gate starts refusing one of these rows, bump it:
///
/// 1. re-fetch the permalink, `sha256sum` the file it lands on;
/// 2. replace that row's digest and update the date above;
/// 3. bump the crate version like any other behavior change — shipped
///    installers embed the pin at compile time.
///
/// Nothing is silently trusted in the meantime: a stale row refuses the
/// download (fail-closed, loud log row, runtime reported missing) until
/// a build carries the new digest. A product that would rather not ride
/// Microsoft's schedule points `download-url` at its own mirror and
/// pins the bytes with `download-sha256` in the manifest.
const EVERGREEN_PERMALINK_SHA256: &[(&str, &str)] = &[
    (
        "https://go.microsoft.com/fwlink/?linkid=2124703",
        "cf2f2445fef0c5c14e372045f84998771eeb8bb01d59be4afd00692fa160082e",
    ),
    (
        "https://go.microsoft.com/fwlink/?linkid=2124701",
        "d3cbe8fedb104282d745fff106993efead51ae94f5595a4e75804446a974f9c3",
    ),
];

/// Why a downloaded Evergreen installer was REFUSED. Every variant means
/// the same thing at the call site: do not run this file. Nothing in
/// this crate executes a download that did not pass
/// [`verify_evergreen_download`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvergreenDigestError {
    /// No pin covers the URL — neither the manifest's `download-sha256`
    /// nor a compile-time permalink row.
    Unpinned {
        /// The `download-url` that nothing pins.
        url: String,
    },
    /// A pin exists but is blank (`download-sha256 = ""`).
    EmptyPin,
    /// A pin exists but can never be a sha256 (not 64 hex characters).
    MalformedPin {
        /// The pin as the manifest spelled it.
        pinned: String,
    },
    /// The downloaded bytes hash to something other than the pin.
    Mismatch {
        /// The normalized pinned digest.
        expected: String,
        /// The digest the downloaded bytes actually hash to.
        actual: String,
    },
    /// The staged file could not be read back for hashing.
    Unreadable {
        /// The staged path, for the log row.
        path: String,
        /// The I/O error's own message.
        detail: String,
    },
}

impl std::fmt::Display for EvergreenDigestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unpinned { url } => write!(
                f,
                "no sha256 pin covers the Evergreen download URL {url} — declare \
                 `download-sha256` in the manifest (an unverified download never runs)"
            ),
            Self::EmptyPin => write!(
                f,
                "the Evergreen download pin is blank — nothing is trusted, nothing runs"
            ),
            Self::MalformedPin { pinned } => write!(
                f,
                "the Evergreen download pin {pinned:?} is not a 64-character hex sha256"
            ),
            Self::Mismatch { expected, actual } => write!(
                f,
                "the downloaded Evergreen installer does not match its pin — \
                 expected sha256 {expected}, got {actual}"
            ),
            Self::Unreadable { path, detail } => write!(
                f,
                "could not read the staged Evergreen installer {path} for hashing: {detail}"
            ),
        }
    }
}

impl std::error::Error for EvergreenDigestError {}

/// Normalizes a permalink for the pin table: trim, lowercase, drop a
/// trailing `/` on either side of the query and the optional `p/` segment
/// the fwlink publisher also accepts (`…/fwlink/p/?LinkId=…`), and keep
/// the query otherwise verbatim (lowercasing is what collapses
/// `LinkId`/`linkid`).
fn normalize_permalink(url: &str) -> String {
    let lowered = url.trim().to_lowercase();
    let (base, query) = match lowered.split_once('?') {
        Some((base, query)) => (base, Some(query)),
        None => (lowered.as_str(), None),
    };
    let base = base.trim_end_matches('/');
    let base = base.strip_suffix("/p").unwrap_or(base);
    match query {
        Some(query) => format!("{base}?{}", query.trim_end_matches('/')),
        None => base.to_string(),
    }
}

/// The compile-time pin for `url`, when it is one of the documented
/// permalinks (see [`EVERGREEN_PERMALINK_SHA256`]).
fn permalink_sha256(url: &str) -> Option<&'static str> {
    let wanted = normalize_permalink(url);
    EVERGREEN_PERMALINK_SHA256
        .iter()
        .find(|(permalink, _)| normalize_permalink(permalink) == wanted)
        .map(|(_, digest)| *digest)
}

/// The digest a downloaded installer must hash to: the manifest's
/// `download-sha256` when it pins one (always wins — that is the mirror
/// and override story), else the compile-time row for a documented
/// permalink. Returned as SPELLED (padding, casing and an optional
/// `sha256:` label included) — [`check_evergreen_pin`] and
/// [`verify_download`] normalize it. `None` means NOTHING pins the URL,
/// and the caller must refuse to run the download
/// ([`verify_evergreen_download`] does).
pub fn pinned_download_sha256<'a>(config: &'a ShunConfig, url: &str) -> Option<&'a str> {
    config
        .webview2_download_sha256()
        .or_else(|| permalink_sha256(url))
}

/// Normalizes a pin to lowercase hex, accepting the same `sha256:`
/// label other packaging tools write. `Err` when the value can never
/// name a sha256 — an unreadable pin must refuse, not pass.
fn normalize_pin(pinned: &str) -> Result<String, EvergreenDigestError> {
    let trimmed = pinned.trim();
    if trimmed.is_empty() {
        return Err(EvergreenDigestError::EmptyPin);
    }
    let lowered = trimmed.to_lowercase();
    let hex = lowered.strip_prefix("sha256:").unwrap_or(&lowered).trim();
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(EvergreenDigestError::MalformedPin {
            pinned: trimmed.to_string(),
        });
    }
    Ok(hex.to_string())
}

/// Hex-encodes a digest (the same spelling [`crate::payload`] uses for
/// its per-entry SHA-256, so a pin copied out of a payload manifest is
/// byte-comparable with this one).
fn hex_digest(digest: impl AsRef<[u8]>) -> String {
    let bytes = digest.as_ref();
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// The pure integrity gate: `bytes` must hash to `expected_hex`.
///
/// `expected_hex` may carry a `sha256:` label and any casing (see
/// [`normalize_pin`]); an empty or malformed pin is an error, never a
/// pass. The streaming twin over an on-disk download is
/// [`verify_download_file`].
pub fn verify_download(bytes: &[u8], expected_hex: &str) -> Result<(), EvergreenDigestError> {
    let expected = normalize_pin(expected_hex)?;
    let actual = hex_digest(Sha256::digest(bytes));
    if actual != expected {
        return Err(EvergreenDigestError::Mismatch { expected, actual });
    }
    Ok(())
}

/// The streaming twin of [`verify_download`] for the shell's on-disk
/// download: hashes `path` in 64 KiB chunks. The standalone installer is
/// over 200 MB, so the gate must never need it in memory.
pub fn verify_download_file(path: &Path, expected_hex: &str) -> Result<(), EvergreenDigestError> {
    // Reject a pin that can never match before touching the disk.
    let expected = normalize_pin(expected_hex)?;
    let unreadable = |detail: String| EvergreenDigestError::Unreadable {
        path: path.display().to_string(),
        detail,
    };
    let mut file = std::fs::File::open(path).map_err(|err| unreadable(err.to_string()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = std::io::Read::read(&mut file, &mut buffer)
            .map_err(|err| unreadable(err.to_string()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let actual = hex_digest(hasher.finalize());
    if actual != expected {
        return Err(EvergreenDigestError::Mismatch { expected, actual });
    }
    Ok(())
}

/// The pre-fetch half of [`verify_evergreen_download`]: proves a pin
/// actually covers `url`, and returns the normalized digest it must hash
/// to. The shell calls it BEFORE the transfer — an unpinned or malformed
/// pin can never pass the gate afterwards, so the hundreds of megabytes
/// (or the few, for the bootstrapper) must never move at all.
pub fn check_evergreen_pin(config: &ShunConfig, url: &str) -> Result<String, EvergreenDigestError> {
    let pinned =
        pinned_download_sha256(config, url).ok_or_else(|| EvergreenDigestError::Unpinned {
            url: url.to_string(),
        })?;
    normalize_pin(pinned)
}

/// Verifies a downloaded installer ALREADY ON DISK against whatever
/// pins its URL — the one call the shell makes between the fetch and
/// the run, and the reason no downloaded installer can reach
/// [`run_evergreen`] unverified.
pub fn verify_evergreen_download(
    config: &ShunConfig,
    url: &str,
    path: &Path,
) -> Result<(), EvergreenDigestError> {
    let pinned = check_evergreen_pin(config, url)?;
    verify_download_file(path, &pinned)
}

/// Runs the Evergreen offline installer silently. The installer's
/// manifest requires elevation for the machine-wide install, so an os
/// error 740 relaunches through the UAC consent (PowerShell
/// `Start-Process -Verb RunAs -Wait`); every child is started with
/// `CREATE_NO_WINDOW` so no console ever flashes in front of the
/// wizard. Callers decide success by re-probing the loader afterwards —
/// the exit code alone cannot distinguish "installed" from "already
/// present, nothing to do" — and MUST treat a non-zero exit as a
/// failure to retry elevated (the downloaded BOOTSTRAPPER runs
/// asInvoker: unelevated it exits non-zero on access denied without
/// ever surfacing the 740 spawn gate; see [`run_evergreen_elevated`]).
///
/// Provenance is the CALLER'S: a carried installer is covered by the
/// payload archive's per-entry digests, and a downloaded one only
/// reaches this call after [`verify_evergreen_download`] accepted it —
/// this function executes the path it is handed, by design (it has no
/// digest to compare against).
///
/// Non-Windows platforms have no WebView2 story; the call is a no-op
/// error so callers can stay unconditional.
#[cfg(windows)]
pub fn run_evergreen(installer: &Path, silent: bool) -> std::io::Result<std::process::ExitStatus> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    // `silent` picks the contract: the GUI runs pass FALSE so the
    // installer's OWN window shows (its progress, its elevation
    // consent — the user sees the real thing); the headless track
    // passes TRUE — `--silent` promises no UI at all.
    let mut command = std::process::Command::new(installer);
    if silent {
        command.args(["/silent", "/install"]);
    } else {
        command.arg("/install");
    }
    command.creation_flags(CREATE_NO_WINDOW).status()
}

/// Relaunches the Evergreen installer through the UAC consent
/// (`Start-Process -Verb RunAs -Wait`). The elevated-retry half of
/// [`run_evergreen_silent`]'s 740 gate, callable directly: the
/// downloaded bootstrapper runs asInvoker and FAILS (non-zero exit)
/// unelevated without tripping the 740 spawn gate, so the caller
/// retries on a failed status, not only on a failed spawn.
#[cfg(windows)]
pub fn run_evergreen_elevated(
    installer: &Path,
    silent: bool,
) -> std::io::Result<std::process::ExitStatus> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let args: Vec<&str> = if silent {
        vec!["/silent", "/install"]
    } else {
        vec!["/install"]
    };
    // Single-quote PowerShell literals; a quote inside the literal
    // (profile paths like `O'Brien`) escapes by doubling. The argument
    // list is fixed flags, so only the path needs the treatment — and
    // the flags must COMMA-join into one -ArgumentList array: a space
    // join passes two positional parameters and PowerShell rejects the
    // command outright (verified: ParameterBindingException, exit 1).
    let exe = installer.display().to_string().replace('\'', "''");
    // -PassThru + exit: Start-Process -Wait alone returns POWERSHELL's
    // own exit code (0) whatever the elevated installer did — the
    // child's code must be carried out explicitly or a failed silent
    // install reads as success (the sandbox's silent skip).
    let script = format!(
        "$p = Start-Process -FilePath '{exe}' -ArgumentList '{}' -Verb RunAs -PassThru -Wait; exit $p.ExitCode",
        args.join("','")
    );
    std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
}

#[cfg(not(windows))]
pub fn run_evergreen(
    _installer: &Path,
    _silent: bool,
) -> std::io::Result<std::process::ExitStatus> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "the Evergreen installer only exists on Windows",
    ))
}

#[cfg(not(windows))]
pub fn run_evergreen_elevated(
    _installer: &Path,
    _silent: bool,
) -> std::io::Result<std::process::ExitStatus> {
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

    /// A minimal evergreen config, the way the manifest spells it.
    fn evergreen_config(download_url: Option<&str>, silent_install: bool) -> ShunConfig {
        evergreen_config_pinned(download_url, silent_install, None)
    }

    /// The same config with the manifest's `download-sha256` pin set.
    fn evergreen_config_pinned(
        download_url: Option<&str>,
        silent_install: bool,
        download_sha256: Option<&str>,
    ) -> ShunConfig {
        ShunConfig {
            script: None,
            variants: None,
            product: crate::config::ProductIdentity {
                name: "ShunDemo".into(),
                version: "0.1.0".into(),
                publisher: None,
                logo: None,
            },
            payload: None,
            webview2: Some(crate::config::Webview2Strategy::EvergreenInstaller {
                silent_install: Some(silent_install),
                warn_missing: None,
                download_url: download_url.map(str::to_string),
                download_sha256: download_sha256.map(str::to_string),
            }),
            targets: Vec::new(),
            shell: None,
            source: None,
            update: None,
            attachments: Vec::new(),
            license_sysl: None,
            license: None,
            license_locales: Default::default(),
            licenses: Vec::new(),
            custom_steps: Vec::new(),
            steps: None,
            signing: None,
            msix: None,
        }
    }

    #[test]
    fn carried_runtime_wins_over_the_download_url() {
        let dir = std::env::temp_dir().join(format!("shun-wv2-src-carried-{}", std::process::id()));
        let inner = dir.join(EVERGREEN_PREFIX);
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(
            inner.join("MicrosoftEdgeWebView2RuntimeInstallerX64.exe"),
            b"MZ",
        )
        .unwrap();
        let payload = packed(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        let config = evergreen_config(
            Some("https://go.microsoft.com/fwlink/?linkid=2124701"),
            true,
        );

        assert_eq!(
            evergreen_source(&config, &payload),
            EvergreenSource::Carried(
                Path::new("webview2/MicrosoftEdgeWebView2RuntimeInstallerX64.exe").to_path_buf()
            )
        );
    }

    #[test]
    fn download_source_needs_url_and_the_silent_knob() {
        let dir = std::env::temp_dir().join(format!("shun-wv2-src-dl-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("app.exe"), b"MZ").unwrap();
        let payload = packed(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        let url = "https://go.microsoft.com/fwlink/?linkid=2124701";

        // Both armed → the download source.
        let config = evergreen_config(Some(url), true);
        assert_eq!(
            evergreen_source(&config, &payload),
            EvergreenSource::Download(url.to_string())
        );
        // The shared knob OFF opts out of the networked source too.
        let declined = evergreen_config(Some(url), false);
        assert_eq!(evergreen_source(&declined, &payload), EvergreenSource::None);
        // No URL → nothing to install from, whatever the knob says.
        let offline = evergreen_config(None, true);
        assert_eq!(evergreen_source(&offline, &payload), EvergreenSource::None);
    }

    /// sha256 of `b"evergreen-installer-bytes"`, computed with
    /// `sha256sum` on the same literal.
    const TEST_BYTES_SHA256: &str =
        "303cf195bd4bdded50e46dba9b36aa780e4e05c2e77a5c4e026a7421f1a9c2bd";

    #[test]
    fn a_download_must_match_its_pinned_sha256() {
        let bytes = b"evergreen-installer-bytes";
        // The pin matches (and an uppercase spelling / a `sha256:` label
        // normalize onto the same digest).
        assert_eq!(verify_download(bytes, TEST_BYTES_SHA256), Ok(()));
        assert_eq!(
            verify_download(bytes, &TEST_BYTES_SHA256.to_uppercase()),
            Ok(())
        );
        assert_eq!(
            verify_download(bytes, &format!("sha256:{TEST_BYTES_SHA256}")),
            Ok(())
        );
        assert_eq!(
            verify_download(
                bytes,
                &format!("  sha256:{}  ", TEST_BYTES_SHA256.to_uppercase())
            ),
            Ok(())
        );
        // A mismatch names both digests so the log row is actionable.
        let other = "5cea7ca2004304231309e1e0b9d7722e42904d2e7feaf6488cf43c171ee77722";
        assert_eq!(
            verify_download(bytes, other),
            Err(EvergreenDigestError::Mismatch {
                expected: other.to_string(),
                actual: TEST_BYTES_SHA256.to_string(),
            })
        );
        // A one-byte change flips the verdict — the gate hashes the
        // whole file, not a prefix.
        let mut tampered = bytes.to_vec();
        tampered.push(b'!');
        assert!(matches!(
            verify_download(&tampered, TEST_BYTES_SHA256),
            Err(EvergreenDigestError::Mismatch { .. })
        ));
    }

    #[test]
    fn an_empty_or_malformed_pin_refuses_instead_of_passing() {
        let bytes = b"evergreen-installer-bytes";
        assert_eq!(
            verify_download(bytes, ""),
            Err(EvergreenDigestError::EmptyPin)
        );
        assert_eq!(
            verify_download(bytes, "   "),
            Err(EvergreenDigestError::EmptyPin)
        );
        // Short, non-hex and over-long pins can never name a sha256.
        for pin in [
            "abc",
            "zz".repeat(32).as_str(),
            "0".repeat(63).as_str(),
            &"0".repeat(65),
        ] {
            let err = verify_download(bytes, pin).expect_err("must refuse");
            assert!(
                matches!(err, EvergreenDigestError::MalformedPin { .. }),
                "unexpected error for {pin:?}: {err:?}"
            );
        }
        // A blank manifest pin means "nothing pinned", not "everything
        // passes": the accessor drops it and the source stays refused.
        let blank =
            evergreen_config_pinned(Some("https://example.invalid/wv2.exe"), true, Some("  "));
        assert_eq!(blank.webview2_download_sha256(), None);
        assert_eq!(
            pinned_download_sha256(&blank, "https://example.invalid/wv2.exe"),
            None
        );
    }

    #[test]
    fn permalink_pins_cover_the_documented_fwlinks() {
        // The published spellings, the `p/` form and a stray trailing
        // slash all resolve to the same pinned row.
        let bootstrapper = "cf2f2445fef0c5c14e372045f84998771eeb8bb01d59be4afd00692fa160082e";
        let standalone = "d3cbe8fedb104282d745fff106993efead51ae94f5595a4e75804446a974f9c3";
        let config = evergreen_config(
            Some("https://go.microsoft.com/fwlink/?linkid=2124703"),
            true,
        );
        for url in [
            "https://go.microsoft.com/fwlink/?linkid=2124703",
            "https://go.microsoft.com/fwlink/p/?LinkId=2124703",
            "HTTPS://go.microsoft.com/fwlink/?linkid=2124703/",
            " https://go.microsoft.com/fwlink/?LinkId=2124703 ",
        ] {
            assert_eq!(
                pinned_download_sha256(&config, url),
                Some(bootstrapper),
                "{url}"
            );
        }
        assert_eq!(
            pinned_download_sha256(&config, "https://go.microsoft.com/fwlink/?linkid=2124701"),
            Some(standalone)
        );
        // Any other URL — a mirror, the resolved CDN GUID link — is
        // unpinned until the manifest says otherwise.
        for url in [
            "https://example.invalid/MicrosoftEdgeWebview2Setup.exe",
            "https://msedge.sf.dl.delivery.mp.microsoft.com/filestreamingservice/files/068be74d-a831-494f-8f57-c173756d5a33/MicrosoftEdgeWebview2Setup.exe",
        ] {
            assert_eq!(pinned_download_sha256(&config, url), None, "{url}");
        }
        // The manifest pin always wins over the table (a mirror of the
        // same bytes, or a deliberate re-pin).
        let override_config = evergreen_config_pinned(
            Some("https://go.microsoft.com/fwlink/?linkid=2124703"),
            true,
            Some(TEST_BYTES_SHA256),
        );
        assert_eq!(
            pinned_download_sha256(
                &override_config,
                "https://go.microsoft.com/fwlink/?linkid=2124703"
            ),
            Some(TEST_BYTES_SHA256)
        );
    }

    #[test]
    fn the_staged_file_gate_matches_the_in_memory_one() {
        let dir = std::env::temp_dir().join(format!("shun-wv2-digest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let staged = dir.join("MicrosoftEdgeWebview2Setup.exe");
        std::fs::write(&staged, b"evergreen-installer-bytes").unwrap();

        assert_eq!(verify_download_file(&staged, TEST_BYTES_SHA256), Ok(()));
        let other = "5cea7ca2004304231309e1e0b9d7722e42904d2e7feaf6488cf43c171ee77722";
        assert_eq!(
            verify_download_file(&staged, other),
            Err(EvergreenDigestError::Mismatch {
                expected: other.to_string(),
                actual: TEST_BYTES_SHA256.to_string(),
            })
        );
        assert_eq!(
            verify_download_file(&staged, ""),
            Err(EvergreenDigestError::EmptyPin)
        );
        // A missing file is a refusal, never a pass.
        assert!(matches!(
            verify_download_file(&dir.join("absent.exe"), TEST_BYTES_SHA256),
            Err(EvergreenDigestError::Unreadable { .. })
        ));

        // The end-to-end call the shell makes: the manifest pin verifies
        // the staged file, and an unpinned URL refuses it outright.
        let pinned = evergreen_config_pinned(
            Some("https://example.invalid/wv2.exe"),
            true,
            Some(TEST_BYTES_SHA256),
        );
        assert_eq!(
            verify_evergreen_download(&pinned, "https://example.invalid/wv2.exe", &staged),
            Ok(())
        );
        // A pin that can never be a sha256 refuses before the file is
        // even read — the pre-fetch check the shell runs first.
        let malformed = evergreen_config_pinned(
            Some("https://example.invalid/wv2.exe"),
            true,
            Some("not-a-digest"),
        );
        assert_eq!(
            check_evergreen_pin(&malformed, "https://example.invalid/wv2.exe"),
            Err(EvergreenDigestError::MalformedPin {
                pinned: "not-a-digest".to_string(),
            })
        );
        let unpinned = evergreen_config(Some("https://example.invalid/wv2.exe"), true);
        assert_eq!(
            check_evergreen_pin(&unpinned, "https://example.invalid/wv2.exe"),
            Err(EvergreenDigestError::Unpinned {
                url: "https://example.invalid/wv2.exe".to_string(),
            })
        );
        assert_eq!(
            verify_evergreen_download(&unpinned, "https://example.invalid/wv2.exe", &staged),
            Err(EvergreenDigestError::Unpinned {
                url: "https://example.invalid/wv2.exe".to_string(),
            })
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
