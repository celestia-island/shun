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
        ShunError::MissingEntry(_) => (FailureKind::MissingExecutable, None, None),
        _ => (FailureKind::Other, None, None),
    }
}

/// Maps a raw OS error code to the narrow reason. Kept public because the
/// mapping is the contract between the classifier and the guide topics.
///
/// The table covers BOTH platforms' raw codes in one arm list — the
/// numbers overlap harmlessly (`ENOSPC` and `ERROR_DISK_FULL` are both
/// 28 and mean the same thing; the Winsock block is far from the Unix
/// one), so one match serves every host this crate compiles for.
pub fn classify_os_error(code: i32) -> FailureKind {
    match code {
        ERROR_APPLICATION_CONTROL_POLICY_BLOCKED | ERROR_ACCESS_DISABLED_BY_POLICY => {
            FailureKind::AppControlBlocked
        }
        // Access denied (Win), privilege not held (Win), EPERM / EACCES /
        // EROFS (Unix).
        5 | 1314 | 1 | 13 | 30 => FailureKind::Permissions,
        // Disk full (Win), handle-disk-full (Win), ENOSPC (Unix).
        28 | 39 => FailureKind::DiskFull,
        // Sharing violation + user-mapped file (Win), ETXTBSY (Unix).
        32 | 1224 | 26 => FailureKind::FileInUse,
        // Filename-exceeds-range + bad pathname (Win), ENAMETOOLONG (Unix).
        206 | 161 | 36 => FailureKind::PathTooLong,
        // The Winsock block: timed out / refused / unreachable /
        // aborted / reset; the WinINet block: name resolution, cannot
        // connect, timeout. Network failures usually arrive as strings
        // (ureq), so these arms serve the rare io-borne path.
        10060 | 10061 | 10051 | 10053 | 10054 | 12002 | 12007 | 12029 => FailureKind::Network,
        _ => FailureKind::Io,
    }
}

fn help_topic_for(kind: FailureKind) -> Option<String> {
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
    // report spelled it 0x11C7).
    if message.contains("0x11C7") {
        return (
            FailureKind::AppControlBlocked,
            Some(ERROR_APPLICATION_CONTROL_POLICY_BLOCKED),
        );
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
            return Some(OsErrorHit {
                code: digits.parse().ok()?,
                end: start + digits.len(),
            });
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
        assert!(failure.help_topic.is_none());
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
    fn os_codes_map_to_the_failure_families_across_platforms() {
        let cases: [(i32, FailureKind); 12] = [
            (5, FailureKind::Permissions),   // ERROR_ACCESS_DENIED
            (13, FailureKind::Permissions),  // EACCES
            (30, FailureKind::Permissions),  // EROFS
            (28, FailureKind::DiskFull),     // ERROR_DISK_FULL / ENOSPC
            (39, FailureKind::DiskFull),     // ERROR_HANDLE_DISK_FULL
            (32, FailureKind::FileInUse),    // ERROR_SHARING_VIOLATION
            (26, FailureKind::FileInUse),    // ETXTBSY
            (1224, FailureKind::FileInUse),  // ERROR_USER_MAPPED_FILE
            (206, FailureKind::PathTooLong), // ERROR_FILENAME_EXCEDNAV
            (36, FailureKind::PathTooLong),  // ENAMETOOLONG
            (10061, FailureKind::Network),   // WSAECONNREFUSED
            (12029, FailureKind::Network),   // ERROR_INTERNET_CANNOT_CONNECT
        ];
        for (code, kind) in cases {
            assert_eq!(classify_os_error(code), kind, "code {code}");
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
        // The shape run_install actually produces on a denied write.
        let (kind, code) = classify_message("io error: Access is denied. (os error 5)");
        assert_eq!(kind, FailureKind::Permissions);
        assert_eq!(code, Some(5));
        // And the ETXTBSY / in-use arm.
        let (kind, code) =
            classify_message("io error: Extract of WoWSP.exe failed: Text file busy (os error 26)");
        assert_eq!(kind, FailureKind::FileInUse);
        assert_eq!(code, Some(26));
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
    fn multiple_os_codes_walk_forward_and_the_narrower_one_wins() {
        // The first code (2, ENOENT) is plain IO; the classifier keeps
        // walking and the denied write further on is the informative one.
        let (kind, code) =
            classify_message("io error: absent (os error 2) during write to X (os error 5)");
        assert_eq!(kind, FailureKind::Permissions);
        assert_eq!(code, Some(5));
    }
}
