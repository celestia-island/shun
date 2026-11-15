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

/// Which half of the done-page confirmation failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FinishStage {
    /// The shortcut choices (`apply_shortcut_choices`).
    Shortcuts,
    /// The immediate-launch answer (`launch_installed`).
    Launch,
}

/// Why the confirmation failed, at the granularity the UI branches on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FailureKind {
    /// Windows application-control policy refused the process creation.
    AppControlBlocked,
    /// The configured `main_exe` is not present in the install directory.
    MissingExecutable,
    /// A filesystem / process-creation error that carries an OS code.
    Io,
    /// Everything else (shortcut composition errors, non-I/O failures).
    Other,
}

/// The help topic a face can open for this failure, if one is authored.
///
/// The guide documents live with the installer shell (both faces embed the
/// same per-locale markdown); the core crate only names the topic so it
/// carries no UI assets.
pub const HELP_TOPIC_APP_CONTROL: &str = "app-control";

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
pub fn classify_os_error(code: i32) -> FailureKind {
    match code {
        ERROR_APPLICATION_CONTROL_POLICY_BLOCKED | ERROR_ACCESS_DISABLED_BY_POLICY => {
            FailureKind::AppControlBlocked
        }
        _ => FailureKind::Io,
    }
}

fn help_topic_for(kind: FailureKind) -> Option<String> {
    match kind {
        FailureKind::AppControlBlocked => Some(HELP_TOPIC_APP_CONTROL.to_string()),
        _ => None,
    }
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
            assert_eq!(failure.help_topic.as_deref(), Some(HELP_TOPIC_APP_CONTROL));
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
}
