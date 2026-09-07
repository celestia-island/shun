//! Errors surfaced by shun flows and backends.

use std::path::PathBuf;

/// Errors surfaced by shun flows and backends.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ShunError {
    /// The requested capability is not implemented on this platform yet.
    #[error("unsupported on this platform: {0}")]
    Unsupported(&'static str),

    /// The configuration document is invalid.
    #[error("invalid configuration: {0}")]
    Config(String),

    /// A payload entry is missing from the archive.
    #[error("payload entry not found: {0}")]
    MissingEntry(PathBuf),

    /// Filesystem I/O failed.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
