//! Optional payload attachments — named companion resources (asset packs)
//! declared in the delivery manifest and fetchable at install time.
//!
//! The manifest (`[[package.metadata.shun.attachments]`) declares each
//! attachment with a dest prefix and an online archive URL. Full builds
//! pack the attachment into the installer payload, where its entries
//! already sit under the dest prefix — nothing to download. Lite builds
//! embed only the declaration, and the runtime shell offers
//! [`download`] as the way to fetch the resource instead, streamed
//! through the same download → decode → verify pipeline as online
//! installers. Requires the `online` feature.

use std::path::Path;

use crate::ShunError;
use crate::config::{AttachmentConfig, ShunConfig};
use crate::flow::FlowEvent;
use crate::payload::ArchivePayload;
#[cfg(feature = "online")]
use crate::payload_online::OnlinePayload;

/// One manifest-declared attachment resolved against the embedded payload.
pub struct ResolvedAttachment {
    /// The manifest declaration.
    pub config: AttachmentConfig,

    /// True when the embedded payload already carries the dest subtree
    /// (full build) — nothing to download.
    pub included: bool,
}

/// Resolves every declared attachment against the embedded payload.
pub fn resolve(config: &ShunConfig, payload: &ArchivePayload) -> Vec<ResolvedAttachment> {
    config
        .attachments
        .iter()
        .map(|config| ResolvedAttachment {
            included: payload.has_prefix(&config.dest),
            config: config.clone(),
        })
        .collect()
}

/// Streams the attachment's online archive into `<install_dir>/<dest>`.
/// The archive's payload root IS the dest content — pack the attachment
/// directory directly (`shun pack models/ models.shun`), no staging
/// prefix needed — and every entry is verified against the archive's own
/// manifest, so extraction lands the files exactly where full builds
/// place them.
#[cfg(feature = "online")]
pub fn download(
    attachment: &AttachmentConfig,
    install_dir: &Path,
    on_event: &mut dyn FnMut(FlowEvent),
) -> Result<(), ShunError> {
    let dest = install_dir.join(&attachment.dest);
    OnlinePayload::new(attachment.online.url.clone()).extract(&dest, on_event)
}
