//! Enumerates flash-candidate devices via the shun flash target surface.
//!
//! ```text
//! cargo run --example demo_flash
//! ```
//!
//! Writing is intentionally not wired up: block-device writes land with the
//! evernight flasher.

use shun::targets::flash::{FlashTarget, LogicalDrives};

fn main() {
    match LogicalDrives.list_devices() {
        Ok(devices) if devices.is_empty() => {
            println!("no removable drives found");
        }
        Ok(devices) => {
            println!("{:<14} {:>14}  LABEL", "ID", "SIZE (B)");
            for device in devices {
                println!("{:<14} {:>14}  {}", device.id, device.size, device.label);
            }
        }
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(1);
        }
    }
}
