//! Pluggable delivery targets.
//!
//! Every target consumes the same flow model ([`crate::flow`]) and payload
//! surface ([`crate::payload`]) but lands the payload differently:
//! [`install`] performs direct registration (Windows: ARP + shortcuts +
//! Explorer verbs; Linux: `.desktop` via [`freedesktop`]; macOS: `.app`
//! bundles via [`macos`]) or skips it for portable mode, [`flash`] writes
//! images to block devices.

pub mod flash;
pub mod freedesktop;
pub mod install;
pub mod macos;
pub mod plist;

/// Machine-scope elevation helpers (self-relaunch via runas).
pub mod elevate;

#[cfg(windows)]
pub mod aumid;
