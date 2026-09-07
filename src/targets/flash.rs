//! Flash target: write an image to a block device, verify, done — the
//! evernight image flasher rides on this surface. Block-device writing
//! (volume unmount, elevation, post-write verification) lands in a later
//! iteration; v0 ships the trait surface plus a demo-grade Windows
//! enumeration backend for the flasher UI.

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

/// Windows backend enumerating removable logical drives — demo-grade (the
/// evernight flasher replaces this with physical-drive enumeration and raw
/// writes), but enough to drive a flasher UI end to end.
#[derive(Debug, Clone, Copy, Default)]
pub struct LogicalDrives;

#[cfg(windows)]
const DRIVE_REMOVABLE: u32 = 2;

#[cfg(windows)]
impl FlashTarget for LogicalDrives {
    fn list_devices(&self) -> Result<Vec<FlashDevice>, ShunError> {
        use windows_sys::Win32::Storage::FileSystem::{
            GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives,
        };

        fn wide(root: &str) -> [u16; 4] {
            let mut out = [0u16; 4];
            for (slot, ch) in out.iter_mut().zip(root.encode_utf16()) {
                *slot = ch;
            }
            out
        }

        let masks = unsafe { GetLogicalDrives() };
        if masks == 0 {
            return Err(ShunError::Io(std::io::Error::last_os_error()));
        }

        let mut devices = Vec::new();
        for i in 0..26u32 {
            if masks & (1 << i) == 0 {
                continue;
            }
            let letter = (b'A' + i as u8) as char;
            let root = format!("{letter}:\\");
            let root_w = wide(&root);
            if unsafe { GetDriveTypeW(root_w.as_ptr()) } != DRIVE_REMOVABLE {
                continue;
            }
            let mut total: u64 = 0;
            let size = unsafe {
                GetDiskFreeSpaceExW(
                    root_w.as_ptr(),
                    std::ptr::null_mut(),
                    &mut total,
                    std::ptr::null_mut(),
                )
            };
            devices.push(FlashDevice {
                id: root,
                label: format!("Removable drive ({letter}:)"),
                size: if size != 0 { total } else { 0 },
                removable: true,
            });
        }
        Ok(devices)
    }

    fn write(
        &self,
        _device: &FlashDevice,
        _image: &Path,
        _on_event: &mut dyn FnMut(FlowEvent),
    ) -> Result<(), ShunError> {
        Err(ShunError::Unsupported(
            "block-device write (lands with the evernight flasher)",
        ))
    }
}

#[cfg(not(windows))]
impl FlashTarget for LogicalDrives {
    fn list_devices(&self) -> Result<Vec<FlashDevice>, ShunError> {
        Err(ShunError::Unsupported(
            "logical drive enumeration (windows only)",
        ))
    }

    fn write(
        &self,
        _device: &FlashDevice,
        _image: &Path,
        _on_event: &mut dyn FnMut(FlowEvent),
    ) -> Result<(), ShunError> {
        Err(ShunError::Unsupported(
            "block-device write (lands with the evernight flasher)",
        ))
    }
}
