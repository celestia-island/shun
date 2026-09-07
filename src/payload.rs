//! Payload packaging: the build CLI packs an app directory into a
//! compressed archive that the runtime shell embeds (single-file installer)
//! or carries alongside itself, then extracts with real progress.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::ShunError;
use crate::flow::FlowEvent;

/// Manifest describing one entry of a shun payload archive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PayloadEntry {
    /// Archive-relative path, also the on-disk path under the target
    /// directory.
    pub path: PathBuf,

    /// Uncompressed size in bytes (progress accounting).
    pub size: u64,

    /// SHA-256 of the uncompressed bytes (verify after extraction).
    pub sha256: String,
}

/// Read-side of a shun payload: a manifest plus streamed extraction.
///
/// Implementations: the build CLI embeds a zstd-compressed archive into the
/// shell binary (`include_bytes!`) for single-file installers; a directory
/// source pairs the shell with a sidecar payload during development.
pub trait PayloadSource {
    /// The manifest of entries carried by this payload.
    fn manifest(&self) -> &[PayloadEntry];

    /// Extract every entry under `dest`, emitting progress events as bytes
    /// hit the disk.
    fn extract(&self, dest: &Path, on_event: &mut dyn FnMut(FlowEvent)) -> Result<(), ShunError>;
}
