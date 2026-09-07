//! The flow model: a delivery run is a sequence of steps streaming events
//! to the shell UI.

use serde::Serialize;

/// Events a delivery flow emits; the shell renders these directly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "event", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum FlowEvent {
    /// The flow validated its inputs and is about to start.
    Started,

    /// A named step progressed; `percent` is `None` for indeterminate steps.
    Progress {
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
