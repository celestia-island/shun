//! The flow model: a delivery run is a sequence of steps streaming events
//! to the shell UI.
//!
//! Progress is **multi-phase**: an online installer streams the payload
//! (download) while decoding and extracting it (extract) and checking each
//! entry against the manifest (verify). Events carry their phase so the UI
//! can render one bar per active phase.

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
