//! shun — a flow-driven payload delivery runtime.
//!
//! shun packages the *delivery* half of shipping desktop software: an
//! embeddable payload, a declarative flow (choose a mode, choose a target,
//! stream progress), and pluggable targets — an [`targets::install`] target
//! that performs direct per-platform registration (Windows: ARP entry,
//! shortcuts, Explorer verbs; Linux: `.desktop` launchers; macOS: `.app`
//! bundles) plus a portable mode that writes no system state at all, and a
//! [`targets::flash`] target that writes images to block devices with
//! post-write verification.
//!
//! The runtime shell (`shell/`, a Tauri front-end built on the celestia
//! hikari design system) and the build CLI both consume this crate; one
//! config document drives the three of them — see
//! [`config::ShunConfig`].
//!
//! # Status
//!
//! `0.2.x`: the config schema, flow model, payload pipeline, and install
//! target are exercised by three real consumers — the WoWSP installer
//! shell, shittim-chest local, and the evernight image flasher (which
//! lands the flash backend's block-device write path). APIs track the
//! three consumers between minor versions.

pub mod config;
pub mod env_probe;
pub mod error;
pub mod flow;
// Install-target filesystem probes: drive enumeration (fixed drives
// included, unlike the flash backend's removable-only list), writability
// probing, and conventional install-root candidates.
pub mod fs_probe;
pub mod msix;
pub mod payload;
// The online payload source rides the optional `ureq` dependency; without
// the feature the crate still builds for offline consumers (found by the
// evernight flasher integrating with default-features = false).
#[cfg(feature = "online")]
pub mod payload_online;
// Optional attachments (asset packs fetched at install time) download
// through the online pipeline, so they share the feature gate.
#[cfg(feature = "online")]
pub mod attachments;
#[cfg(feature = "online")]
pub mod license_sysl;
pub mod targets;
// Update watch: mirror sources probed in order, with the declared files
// resolved under the first reachable one. Probes and fetches ride the
// same optional `ureq` dependency as the online payload source.
#[cfg(feature = "online")]
pub mod update;

pub mod wizard;

pub use error::ShunError;
