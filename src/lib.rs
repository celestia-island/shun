//! shun — a flow-driven payload delivery runtime.
//!
//! shun packages the *delivery* half of shipping desktop software: an
//! embeddable payload, a declarative flow (choose a mode, choose a target,
//! stream progress), and pluggable targets — an [`targets::install`] target
//! that performs NSIS-like registration (ARP, uninstaller, shortcuts, deep
//! links) plus a portable mode that writes no registry at all, and a
//! [`targets::flash`] target that writes images to block devices with
//! post-write verification.
//!
//! The runtime shell (a Tauri front-end built on the celestia hikari design
//! system) and the build CLI land in later iterations; this crate is the
//! contract they both consume. One config document drives both — see
//! [`config::ShunConfig`].
//!
//! # Status
//!
//! Pre-release scaffolding (`0.0.x`): the config schema and flow model are
//! settling against three real consumers — the WoWSP installer shell,
//! shittim-chest local, and the evernight image flasher.

pub mod config;
pub mod error;
pub mod flow;
pub mod payload;
pub mod payload_online;
pub mod targets;

pub use error::ShunError;
