//! Structured finish-failure classification, shared by every install face.
//!
//! The done page's confirmation can fail in two places — applying the
//! shortcut choices, or launching the freshly installed application — and
//! one of those failures is not the installer's fault: Windows
//! application-control policy (Smart App Control on consumer Windows,
//! App Control for Business / AppLocker on managed machines) refuses
//! `CreateProcess` with `ERROR_APPLICATION_CONTROL_POLICY_BLOCKED`
//! (0x11C7 / raw OS error 4551) before any of our code runs.
//!
//! Rendering that as the raw `io error: … (os error 4551)` string loses the
//! one fact the user needs (an OS policy blocked the launch, the install
//! itself succeeded), and every face would have to re-parse the text to
//! recover it. This module classifies the underlying [`ShunError`] once and
//! names the help topic the UI can open next to the message.
//!
//! Both the web face (through the Tauri `finish_install` command) and the
//! egui face (in process) consume the same [`FinishFailure`], so the two
//! surfaces cannot drift on what "blocked by policy" means.

use serde::{Deserialize, Serialize};

use crate::error::ShunError;

/// `ERROR_APPLICATION_CONTROL_POLICY_BLOCKED` (0x11C7): Windows refused to
/// create the process because application-control policy does not trust
/// the binary. Seen on Windows 11 Smart App Control (`os error 4551`).
pub const ERROR_APPLICATION_CONTROL_POLICY_BLOCKED: i32 = 4551;

/// `ERROR_ACCESS_DISABLED_BY_POLICY` (0x4EC): the legacy "blocked by
/// policy" code still surfaced by AppLocker / WDAC paths and group-policy
/// launchers. It shares the application-control remedy, so it shares the
/// help topic.
pub const ERROR_ACCESS_DISABLED_BY_POLICY: i32 = 1260;

/// Which half of the flow failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FinishStage {
    /// The shortcut choices (`apply_shortcut_choices`).
    Shortcuts,
    /// The immediate-launch answer (`launch_installed`).
    Launch,
    /// The install flow itself (extraction, registration, downloads) —
    /// the failed-install landing page's classification.
    Install,
}

/// Why the confirmation failed, at the granularity the UI branches on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FailureKind {
    /// Windows application-control policy refused the process creation.
    AppControlBlocked,
    /// The configured `main_exe` is not present in the install directory.
    MissingExecutable,
    /// The OS refused a privileged operation (access denied, read-only
    /// target, elevation required).
    Permissions,
    /// The target drive ran out of space.
    DiskFull,
    /// A file the install must replace is held open by a running process.
    FileInUse,
    /// Windows' path-length restriction rejected a target path.
    PathTooLong,
    /// A download could not reach its source (network down, mirror
    /// unreachable, DNS failure).
    Network,
    /// A filesystem / process-creation error that carries an OS code but
    /// matches no narrower family.
    Io,
    /// Everything else (shortcut composition errors, non-I/O failures).
    Other,
}

/// A classified finish failure — the payload the web face receives as the
/// command error and the egui face drives its toast from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinishFailure {
    /// Which step failed (shortcuts vs launch).
    pub stage: FinishStage,
    /// The narrow reason the UI branches on.
    pub kind: FailureKind,
    /// The human-readable message (`ShunError`'s display text today).
    pub message: String,
    /// The raw OS error code when one exists (`Some(4551)` for an
    /// application-control block).
    pub os_error: Option<i32>,
    /// The guide topic to open, if one is authored for this failure.
    pub help_topic: Option<String>,
}

impl FinishFailure {
    /// Classifies a [`ShunError`] raised under `stage`.
    pub fn from_error(stage: FinishStage, error: &ShunError) -> Self {
        let (kind, os_error, help_topic) = classify_error(error);
        Self {
            stage,
            kind,
            message: error.to_string(),
            os_error,
            help_topic,
        }
    }

    /// A failure that carries no [`ShunError`] (a blocking task that
    /// panicked, a shortcut helper returning a bare string).
    pub fn other(stage: FinishStage, message: impl Into<String>) -> Self {
        Self {
            stage,
            kind: FailureKind::Other,
            message: message.into(),
            os_error: None,
            help_topic: None,
        }
    }

    /// Classifies a bare OS error code (the `raw_os_error` of the failed
    /// process creation), exposed for tests and for callers that only have
    /// the code.
    pub fn from_os_error(stage: FinishStage, code: i32, message: impl Into<String>) -> Self {
        let kind = classify_os_error(code);
        Self {
            stage,
            kind,
            message: message.into(),
            os_error: Some(code),
            help_topic: help_topic_for(kind),
        }
    }

    /// Classifies a flow error that arrives as text under `stage` — the
    /// string-shaped arm the install lane surfaces through `to_string()`.
    pub fn from_message(stage: FinishStage, message: impl Into<String>) -> Self {
        let message = message.into();
        let (kind, os_error) = classify_message(&message);
        Self {
            stage,
            kind,
            message,
            os_error,
            help_topic: help_topic_for(kind),
        }
    }

    /// Whether the UI should offer the "what do I do now" guide action.
    pub fn has_help(&self) -> bool {
        self.help_topic.is_some()
    }
}

fn classify_error(error: &ShunError) -> (FailureKind, Option<i32>, Option<String>) {
    match error {
        ShunError::Io(io) => match io.raw_os_error() {
            Some(code) => {
                let kind = classify_os_error(code);
                (kind, Some(code), help_topic_for(kind))
            }
            None => (FailureKind::Io, None, None),
        },
        ShunError::MissingEntry(_) => {
            let kind = FailureKind::MissingExecutable;
            (kind, None, help_topic_for(kind))
        }
        _ => (FailureKind::Other, None, None),
    }
}

/// ENAMETOOLONG's number: 36 on Linux, 63 on Darwin (where 36 is
/// EINPROGRESS — no family, plain Io). One constant, two consumers: the
/// match arms in [`classify_os_error`] below and the platform test.
#[cfg(all(not(windows), not(target_os = "macos")))]
pub(crate) const ENAMETOOLONG: i32 = 36;
#[cfg(target_os = "macos")]
pub(crate) const ENAMETOOLONG: i32 = 63;

/// The Unix network errnos, per platform: refused / timed out /
/// net unreachable / host unreachable / host down. Linux and Darwin
/// number them differently, and network failures usually arrive as
/// strings (ureq's composed English), so this table serves the io-borne
/// path — and the platform test reads the SAME constant, so the arm and
/// its coverage cannot drift apart. (What the constant CANNOT prove is
/// that each number is the right errno for its family — that mapping is
/// pinned by the Windows-negative probe and human review against the
/// vendor tables.)
#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) const UNIX_NETWORK_ERRNOS: [i32; 5] = [101, 110, 111, 112, 113];
#[cfg(target_os = "macos")]
pub(crate) const UNIX_NETWORK_ERRNOS: [i32; 5] = [61, 60, 51, 64, 65];

/// Maps a raw OS error code to the narrow reason. Kept public because the
/// mapping is the contract between the classifier and the guide topics.
///
/// The Windows and Unix code spaces are kept APART, not merged: their
/// numbers collide with different meanings (28 is `ERROR_OUT_OF_PAPER`
/// on Windows but `ENOSPC` on Unix; 13 is `ERROR_INVALID_DATA` on
/// Windows but `EACCES` on Unix), and a merged table misroutes real
/// Windows errors onto Unix remedies. The Unix arms are `cfg`-gated so
/// each platform's classifier only knows its own codes; the Winsock
/// block is Windows-only by nature, and network failures usually arrive
/// as strings anyway ([`classify_message`]).
pub fn classify_os_error(code: i32) -> FailureKind {
    match code {
        ERROR_APPLICATION_CONTROL_POLICY_BLOCKED | ERROR_ACCESS_DISABLED_BY_POLICY => {
            FailureKind::AppControlBlocked
        }
        #[cfg(windows)]
        // Access denied; privilege not held.
        5 | 1314 => FailureKind::Permissions,
        #[cfg(windows)]
        // ERROR_DISK_FULL; ERROR_HANDLE_DISK_FULL.
        112 | 39 => FailureKind::DiskFull,
        #[cfg(windows)]
        // Sharing violation; user-mapped file.
        32 | 1224 => FailureKind::FileInUse,
        #[cfg(windows)]
        // Filename-exceeds-range; bad pathname.
        206 | 161 => FailureKind::PathTooLong,
        #[cfg(windows)]
        // Winsock: timed out / refused / unreachable / aborted / reset.
        // WinINet: timeout / name resolution / cannot connect.
        10060 | 10061 | 10051 | 10053 | 10054 | 12002 | 12007 | 12029 => FailureKind::Network,
        #[cfg(not(windows))]
        // EPERM / EACCES / EROFS (the same numbers on Linux and Darwin).
        1 | 13 | 30 => FailureKind::Permissions,
        #[cfg(not(windows))]
        // ENOSPC.
        28 => FailureKind::DiskFull,
        #[cfg(not(windows))]
        // ETXTBSY.
        26 => FailureKind::FileInUse,
        // ENAMETOOLONG — the per-platform constant above.
        #[cfg(not(windows))]
        ENAMETOOLONG => FailureKind::PathTooLong,
        // The Unix network errnos — the per-platform table above.
        #[cfg(unix)]
        code if UNIX_NETWORK_ERRNOS.contains(&code) => FailureKind::Network,
        _ => FailureKind::Io,
    }
}

/// The kind → guide-topic table. `pub(crate)` because the shell's
/// `classify_error` command and the faces re-derive the same pairing —
/// one table, no drift.
pub(crate) fn help_topic_for(kind: FailureKind) -> Option<String> {
    match kind {
        FailureKind::AppControlBlocked => Some(crate::help::TOPIC_APP_CONTROL.to_string()),
        FailureKind::Permissions => Some(crate::help::TOPIC_PERMISSIONS.to_string()),
        FailureKind::DiskFull => Some(crate::help::TOPIC_DISK_SPACE.to_string()),
        FailureKind::FileInUse => Some(crate::help::TOPIC_FILE_IN_USE.to_string()),
        FailureKind::PathTooLong => Some(crate::help::TOPIC_PATH_TOO_LONG.to_string()),
        FailureKind::Network => Some(crate::help::TOPIC_NETWORK.to_string()),
        FailureKind::MissingExecutable => Some(crate::help::TOPIC_MISSING_FILES.to_string()),
        FailureKind::Io | FailureKind::Other => None,
    }
}

/// Classifies a failure that arrives as TEXT — the install flow's shape:
/// `run_install` / `download_attachment` surface their errors through
/// `to_string()`, which preserves the raw OS code (`io error: … (os
/// error 13)`) but not the `io::Error` itself, and network failures come
/// out of ureq as composed English. This is the lossy-but-honest arm:
/// every marker it matches is one the flow actually produces.
pub fn classify_message(message: &str) -> (FailureKind, Option<i32>) {
    // A raw OS code in the text wins — `os error 13` is exact.
    let mut best: Option<(FailureKind, i32)> = None;
    let mut rest = message;
    while let Some(at) = find_os_error(rest) {
        let code = at.code;
        let kind = classify_os_error(code);
        if !matches!(kind, FailureKind::Io) {
            best = Some((kind, code));
            break;
        }
        rest = &rest[at.end..];
    }
    if let Some((kind, code)) = best {
        return (kind, Some(code));
    }
    // Application-control also travels as its hex form (the WoWSP field
    // report spelled it 0x11C7). Token-bounded and case-insensitive: a
    // longer hex value that merely CONTAINS the digits (a script hook
    // printing `fault at 0x11C7F00A`) must not classify as the block.
    //
    // Scanned over CHARS of the original string — a lowercased COPY
    // cannot be reused for slicing, because lowercasing changes byte
    // lengths (İ U+0130 lowers to three bytes) and the offsets would
    // land out of bounds or mid-char.
    let pattern: Vec<char> = "0x11c7".chars().collect();
    let chars: Vec<char> = message.chars().collect();
    let bounded = |c: Option<char>| c.map(|c| !c.is_ascii_hexdigit()).unwrap_or(true);
    let mut index = 0usize;
    while index + pattern.len() <= chars.len() {
        if chars[index..index + pattern.len()]
            .iter()
            .zip(pattern.iter())
            .all(|(c, p)| c.to_ascii_lowercase() == *p)
        {
            let before = index.checked_sub(1).and_then(|i| chars.get(i)).copied();
            let after = chars.get(index + pattern.len()).copied();
            if bounded(before) && bounded(after) {
                return (
                    FailureKind::AppControlBlocked,
                    Some(ERROR_APPLICATION_CONTROL_POLICY_BLOCKED),
                );
            }
        }
        index += 1;
    }
    // Download / transport failures (ureq's composed English): the words
    // the transport actually prints, matched lowercase against a
    // lowercased copy.
    let lowered = message.to_lowercase();
    const NETWORK_MARKERS: [&str; 8] = [
        "connection closed",
        "connection refused",
        "connection reset",
        "timed out",
        "timeout",
        "dns error",
        "name or service not known",
        "unreachable",
    ];
    if NETWORK_MARKERS
        .iter()
        .any(|marker| lowered.contains(marker))
    {
        return (FailureKind::Network, None);
    }
    (FailureKind::Io, None)
}

/// The next `os error <n>` occurrence in `text` (decimal), as
/// (parsed code, end offset) so a message carrying several codes walks
/// forward instead of looping.
fn find_os_error(text: &str) -> Option<OsErrorHit> {
    let needle = "os error ";
    let mut search_from = 0;
    while let Some(rel) = text[search_from..].find(needle) {
        let start = search_from + rel + needle.len();
        let digits: String = text[start..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if !digits.is_empty() {
            if let Ok(code) = digits.parse::<i32>() {
                return Some(OsErrorHit {
                    code,
                    end: start + digits.len(),
                });
            }
            // A value past i32 does not abort the walk: skip this hit
            // and keep scanning (a message may carry a wider code and a
            // real one further on).
        }
        search_from = start;
    }
    None
}

struct OsErrorHit {
    code: i32,
    end: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_control_codes_are_blocked_with_the_guide_topic() {
        for code in [
            ERROR_APPLICATION_CONTROL_POLICY_BLOCKED,
            ERROR_ACCESS_DISABLED_BY_POLICY,
        ] {
            let failure = FinishFailure::from_os_error(FinishStage::Launch, code, "blocked");
            assert_eq!(failure.kind, FailureKind::AppControlBlocked, "code {code}");
            assert_eq!(failure.os_error, Some(code));
            assert_eq!(
                failure.help_topic.as_deref(),
                Some(crate::help::TOPIC_APP_CONTROL)
            );
            assert!(failure.has_help());
        }
    }

    #[test]
    fn other_io_codes_stay_plain_io_without_a_topic() {
        let failure = FinishFailure::from_os_error(FinishStage::Launch, 2, "not found");
        assert_eq!(failure.kind, FailureKind::Io);
        assert_eq!(failure.os_error, Some(2));
        assert!(failure.help_topic.is_none());
        assert!(!failure.has_help());
    }

    #[test]
    fn io_errors_classify_through_the_os_code() {
        let error = ShunError::Io(std::io::Error::from_raw_os_error(
            ERROR_APPLICATION_CONTROL_POLICY_BLOCKED,
        ));
        let failure = FinishFailure::from_error(FinishStage::Launch, &error);
        assert_eq!(failure.kind, FailureKind::AppControlBlocked);
        assert_eq!(
            failure.os_error,
            Some(ERROR_APPLICATION_CONTROL_POLICY_BLOCKED)
        );
        assert!(
            failure.message.starts_with("io error:"),
            "the raw message stays available: {}",
            failure.message
        );
        assert_eq!(failure.stage, FinishStage::Launch);
    }

    #[test]
    fn missing_entry_is_its_own_kind() {
        let error = ShunError::MissingEntry(std::path::PathBuf::from("bin/app.exe"));
        let failure = FinishFailure::from_error(FinishStage::Launch, &error);
        assert_eq!(failure.kind, FailureKind::MissingExecutable);
        // The kind's table row names the missing-files article (an AV
        // quarantine is the usual cause) — topic and kind agree.
        assert_eq!(
            failure.help_topic.as_deref(),
            Some(crate::help::TOPIC_MISSING_FILES)
        );
        assert!(failure.has_help());
    }

    #[test]
    fn the_wire_shape_is_camel_case_with_kebab_kinds() {
        let failure = FinishFailure::from_os_error(
            FinishStage::Launch,
            ERROR_APPLICATION_CONTROL_POLICY_BLOCKED,
            "io error: blocked",
        );
        let json = serde_json::to_value(&failure).expect("serializes");
        assert_eq!(json["stage"], "launch");
        assert_eq!(json["kind"], "app-control-blocked");
        assert_eq!(json["osError"], 4551);
        assert_eq!(json["helpTopic"], "app-control");
        assert_eq!(json["message"], "io error: blocked");
        // Round-trip: the egui face drives the same struct from the same
        // source, so a wire-only change would break this arm first.
        let back: FinishFailure = serde_json::from_value(json).expect("deserializes");
        assert_eq!(back, failure);
    }

    #[test]
    fn the_other_constructor_carries_no_help() {
        let failure = FinishFailure::other(FinishStage::Shortcuts, "boom");
        assert_eq!(failure.kind, FailureKind::Other);
        assert_eq!(failure.os_error, None);
        assert!(!failure.has_help());
    }

    #[test]
    fn os_codes_map_to_the_failure_families_per_platform() {
        // Windows and Unix code spaces are deliberately separate (28 is
        // ERROR_OUT_OF_PAPER on Windows but ENOSPC on Unix) — each host
        // classifies only its OWN codes, and the colliding numbers must
        // land on the host's meaning (usually plain Io).
        if cfg!(windows) {
            let cases: [(i32, FailureKind); 10] = [
                (5, FailureKind::Permissions),    // ERROR_ACCESS_DENIED
                (1314, FailureKind::Permissions), // ERROR_PRIVILEGE_NOT_HELD
                (112, FailureKind::DiskFull),     // ERROR_DISK_FULL
                (39, FailureKind::DiskFull),      // ERROR_HANDLE_DISK_FULL
                (32, FailureKind::FileInUse),     // ERROR_SHARING_VIOLATION
                (1224, FailureKind::FileInUse),   // ERROR_USER_MAPPED_FILE
                (206, FailureKind::PathTooLong),  // ERROR_FILENAME_EXCEDNAV
                (161, FailureKind::PathTooLong),  // ERROR_BAD_PATHNAME
                (10061, FailureKind::Network),    // WSAECONNREFUSED
                (12029, FailureKind::Network),    // ERROR_INTERNET_CANNOT_CONNECT
            ];
            for (code, kind) in cases {
                assert_eq!(classify_os_error(code), kind, "windows code {code}");
            }
            // The Unix-only numbers mean nothing on Windows — plain Io,
            // never a Unix remedy.
            for code in [1, 13, 26, 28, 30, 36] {
                assert_eq!(
                    classify_os_error(code),
                    FailureKind::Io,
                    "unix code {code} on windows"
                );
            }
        } else {
            // ENAMETOOLONG and the network set come from the SAME
            // constants the match arm reads — the test cannot describe a
            // different table than the classifier implements.
            let net = UNIX_NETWORK_ERRNOS;
            let cases: [(i32, FailureKind); 6] = [
                (1, FailureKind::Permissions),  // EPERM
                (13, FailureKind::Permissions), // EACCES
                (30, FailureKind::Permissions), // EROFS
                (28, FailureKind::DiskFull),    // ENOSPC
                (26, FailureKind::FileInUse),   // ETXTBSY
                (ENAMETOOLONG, FailureKind::PathTooLong),
            ];
            for (code, kind) in cases {
                assert_eq!(classify_os_error(code), kind, "unix code {code}");
            }
            for (code, kind) in net.iter().zip(std::iter::repeat(FailureKind::Network)) {
                assert_eq!(classify_os_error(*code), kind, "unix network code {code}");
            }
            // The OTHER Unix flavor's ENAMETOOLONG number means nothing
            // here (Darwin's 36 is EINPROGRESS; Linux's 63 is unknown).
            let other_enametoolang: i32 = if cfg!(target_os = "macos") { 36 } else { 63 };
            assert_eq!(classify_os_error(other_enametoolang), FailureKind::Io);
            // The Windows-only numbers mean nothing on Unix — plain Io,
            // never a Windows remedy. This is the only guard that catches
            // a Windows number creeping into UNIX_NETWORK_ERRNOS (the
            // table test iterates the const, so a wrong entry would
            // otherwise verify itself).
            if cfg!(unix) {
                for code in [5, 1314, 39, 32, 1224, 206, 161, 10061, 12029] {
                    assert_eq!(
                        classify_os_error(code),
                        FailureKind::Io,
                        "windows code {code} on unix"
                    );
                }
                // And the table carries no duplicates: a typo'd repeat
                // (round-2's 111→110 probe) fails here instead of
                // silently testing one code twice.
                let mut sorted = UNIX_NETWORK_ERRNOS;
                sorted.sort_unstable();
                assert!(
                    sorted.windows(2).all(|pair| pair[0] < pair[1]),
                    "UNIX_NETWORK_ERRNOS has no duplicates: {sorted:?}"
                );
            }
        }
        for kind in [
            FailureKind::AppControlBlocked,
            FailureKind::Permissions,
            FailureKind::DiskFull,
            FailureKind::FileInUse,
            FailureKind::PathTooLong,
            FailureKind::Network,
            FailureKind::MissingExecutable,
        ] {
            let topic = help_topic_for(kind).expect("family carries a topic");
            assert!(
                crate::help::TOPICS.contains(&topic.as_str()),
                "{:?}: {}",
                kind,
                topic
            );
        }
        // Codes with no family stay plain IO without a topic.
        assert_eq!(classify_os_error(2), FailureKind::Io);
        assert!(help_topic_for(FailureKind::Io).is_none());
    }

    #[test]
    fn install_stage_is_a_first_class_wire_value() {
        let failure = FinishFailure::from_os_error(FinishStage::Install, 28, "disk full");
        let json = serde_json::to_value(&failure).expect("serializes");
        assert_eq!(json["stage"], "install");
        assert_eq!(json["kind"], "disk-full");
        assert_eq!(json["helpTopic"], "disk-space");
    }

    #[test]
    fn flow_messages_classify_through_their_embedded_os_code() {
        // The shape run_install actually produces on a denied write —
        // the host's own access-denied code (5 = ERROR_ACCESS_DENIED on
        // Windows, 13 = EACCES on Unix).
        let denied: i32 = if cfg!(windows) { 5 } else { 13 };
        let (kind, code) =
            classify_message(&format!("io error: Access is denied. (os error {denied})"));
        assert_eq!(kind, FailureKind::Permissions);
        assert_eq!(code, Some(denied));
        // And the in-use arm (32 = sharing violation on Windows,
        // 26 = ETXTBSY on Unix).
        let in_use: i32 = if cfg!(windows) { 32 } else { 26 };
        let (kind, code) = classify_message(&format!(
            "io error: Extract of WoWSP.exe failed: file is busy (os error {in_use})"
        ));
        assert_eq!(kind, FailureKind::FileInUse);
        assert_eq!(code, Some(in_use));
    }

    #[test]
    fn transport_wording_classifies_as_network() {
        for message in [
            "download failed: Network connect timed out: Connection refused (os error 111)",
            "fetch https://example.invalid/mirror: Dns Error: Failed to lookup address information",
            "download failed: connection closed before message completed",
        ] {
            let (kind, _) = classify_message(message);
            assert_eq!(kind, FailureKind::Network, "{message}");
        }
    }

    #[test]
    fn plain_errors_stay_unclassified() {
        let (kind, code) = classify_message("the configuration declares no install target");
        assert_eq!(kind, FailureKind::Io);
        assert_eq!(code, None);
    }

    #[test]
    fn the_hex_form_of_the_block_code_classifies() {
        let (kind, code) = classify_message("CreateProcess failed: 0x11C7 before any output");
        assert_eq!(kind, FailureKind::AppControlBlocked);
        assert_eq!(code, Some(ERROR_APPLICATION_CONTROL_POLICY_BLOCKED));
    }

    #[test]
    fn the_hex_form_is_token_bounded_and_case_insensitive() {
        // Standalone, either case: both classify.
        for message in ["CreateProcess failed: 0x11C7", "blocked: 0x11c7 (?)"] {
            let (kind, code) = classify_message(message);
            assert_eq!(kind, FailureKind::AppControlBlocked, "{message}");
            assert_eq!(code, Some(ERROR_APPLICATION_CONTROL_POLICY_BLOCKED));
        }
        // A longer hex value that merely CONTAINS the digits must not —
        // script-hook errors embed arbitrary script text.
        let (kind, code) = classify_message("script prepare: fault at 0x11C7F00A in module two");
        assert_ne!(
            kind,
            FailureKind::AppControlBlocked,
            "0x11C7F00A is not the block"
        );
        assert_eq!(code, None);
        // Bounded by sentence punctuation too.
        let (kind, code) = classify_message("blocked (see 0x11c7.)");
        assert_eq!(kind, FailureKind::AppControlBlocked);
        assert_eq!(code, Some(ERROR_APPLICATION_CONTROL_POLICY_BLOCKED));
        // And the scan never panics on multi-byte case folding around a
        // hit (the lowercased length of İ differs from the original).
        let (kind, _) = classify_message("C:\\Users\\\u{0130}brahim\\app.exe blocked 0x11C7");
        assert_eq!(kind, FailureKind::AppControlBlocked);
    }

    #[test]
    fn multiple_os_codes_walk_forward_and_the_narrower_one_wins() {
        // The first code (2, ENOENT) is plain IO; the classifier keeps
        // walking and the denied write further on is the informative one
        // (the host's own access-denied code).
        let denied: i32 = if cfg!(windows) { 5 } else { 13 };
        let (kind, code) = classify_message(&format!(
            "io error: absent (os error 2) during write to X (os error {denied})"
        ));
        assert_eq!(kind, FailureKind::Permissions);
        assert_eq!(code, Some(denied));
    }

    #[test]
    fn the_os_error_walk_survives_an_overflowing_code() {
        // A value past i32 skips one hit without aborting the walk: the
        // real code further on still wins.
        let denied: i32 = if cfg!(windows) { 5 } else { 13 };
        let (kind, code) = classify_message(&format!(
            "io error: first (os error 99999999999) then denied (os error {denied})"
        ));
        assert_eq!(kind, FailureKind::Permissions);
        assert_eq!(code, Some(denied));
    }
}
