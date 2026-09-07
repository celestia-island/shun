//! Flash target: write an image to a block device, verify, done — the
//! evernight image flasher rides on this surface. Block-device writing
//! (volume unmount, elevation, post-write verification) lands in a later
//! iteration; v0 keeps the trait surface the flasher UI consumes.

use std::path::Path;

use crate::error::ShunError;
use crate::flow::FlowEvent;

/// A block device eligible as a flash target.
#[derive(Debug, Clone, PartialEq)]
pub struct FlashDevice {
    /// OS-specific identifier (`\\.\PhysicalDrive2`, `/dev/rdisk4`, ...).
    pub id: String,

    /// Human-readable label (vendor / model).
    pub label: String,

    /// Device size in bytes.
    pub size: u64,

    /// Removable media (USB sticks); non-removable devices require an
    /// explicit override in [`FlashConfig`](crate::config::FlashConfig).
    pub removable: bool,
}

/// Block-device write backend.
pub trait FlashTarget {
    /// Enumerate candidate devices.
    fn list_devices(&self) -> Result<Vec<FlashDevice>, ShunError>;

    /// Write `image` onto `device` and verify the written bytes, emitting
    /// progress events.
    fn write(
        &self,
        device: &FlashDevice,
        image: &Path,
        on_event: &mut dyn FnMut(FlowEvent),
    ) -> Result<(), ShunError>;
}
