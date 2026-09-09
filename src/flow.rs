//! The flow model: a delivery run is a sequence of steps streaming events
//! to the shell UI.
//!
//! Progress is **multi-phase**: an online installer streams the payload
//! (download) while decoding and extracting it (extract) and checking each
//! entry against the manifest (verify). Events carry their phase so the UI
//! can render one bar per active phase.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The delivery phase a progress event belongs to. Phases may overlap —
/// an online installer downloads and extracts at the same time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum FlowPhase {
    /// The flow is validating inputs. Default for flow-level events.
    #[default]
    Prepare,
    /// Transfering payload bytes from the network.
    Download,
    /// Writing decoded payload bytes to the target.
    Extract,
    /// Verifying extracted bytes against the manifest.
    Verify,
    /// Registering the install (ARP, shortcuts, uninstaller).
    Register,
}

/// A terminal log record: what the flow actually did, one line at a
/// time. The installer's collapsible output pane (and headless
/// consoles) render these; verbosity is filtered at consumption, the
/// flow always emits everything.
///
/// Two families, matching the two log sources: **file operations**
/// from the payload pipeline (every write, every adopted identical
/// copy), and **script activity** — one line per completed instruction
/// of a mounted runner script (`justfile`-style duckscript), plus the
/// begin marker and streamed output of a Python-VM script (complex
/// tasks). The runners land as their own integration; the event
/// surface is final.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "log", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum FlowLog {
    /// A payload file was written to disk.
    FileWrite { path: PathBuf },

    /// An identical payload file already existed and was adopted
    /// instead of rewritten.
    FileReuse { path: PathBuf },

    /// A mounted script began running; `name` carries the bare script
    /// name (no path, no extension).
    ScriptBegin { name: String },

    /// One line of a running script's output (stdout/stderr).
    ScriptLine { name: String, line: String },

    /// One instruction of a runner script completed; `command` is the
    /// instruction's own spelling.
    CommandDone { command: String },

    /// A non-fatal degradation the user should see (a security policy
    /// denied the desktop shortcut, the AUMID stamp, ...). Warnings
    /// bypass the file/script family filter — only `off` hides them.
    /// `code` is a stable identifier shells map to localized text
    /// (`desktop-shortcut-blocked`, `aumid-stamp-blocked`, ...);
    /// `detail` carries the raw error for the fallback.
    Warning { code: String, detail: String },
}

impl FlowLog {
    /// Whether this record belongs to the script family (as opposed to
    /// the file-operation family) — the verbosity filter buckets on it.
    pub fn is_script(&self) -> bool {
        matches!(
            self,
            FlowLog::ScriptBegin { .. } | FlowLog::ScriptLine { .. } | FlowLog::CommandDone { .. }
        )
    }
}

/// Events a delivery flow emits; the shell renders these directly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "event", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum FlowEvent {
    /// The flow validated its inputs and is about to start.
    Started,

    /// A named step progressed; `percent` is `None` for indeterminate steps.
    Progress {
        /// Delivery phase the step belongs to.
        phase: FlowPhase,
        /// Human-readable step label.
        step: String,
        /// Completion percentage, `None` while indeterminate.
        percent: Option<u8>,
    },

    /// A terminal log line — the flow's actual activity stream.
    Log { record: FlowLog },

    /// The payload landed at its target.
    Completed,

    /// The flow failed; the shell shows the message and a retry affordance.
    Failed {
        /// Human-readable failure message.
        message: String,
    },
}

/// A delivery flow: validates inputs, streams [`FlowEvent`]s, and delivers
/// the payload to its target. Implemented per target kind — the install
/// flow performs registration, the flash flow writes and verifies a block
/// device.
pub trait Flow {
    /// Run the flow to completion, forwarding events to `on_event`.
    fn run(&self, on_event: &mut dyn FnMut(FlowEvent)) -> Result<(), crate::error::ShunError>;
}
