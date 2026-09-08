//! Pluggable delivery targets.
//!
//! Every target consumes the same flow model ([`crate::flow`]) and payload
//! surface ([`crate::payload`]) but lands the payload differently:
//! [`install`] performs direct Windows registration (or skips it for portable
//! mode), [`flash`] writes images to block devices.

pub mod flash;
pub mod install;
