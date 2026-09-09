//! Payload packaging: the build CLI packs an app directory into a
//! compressed archive that the runtime shell embeds (single-file installer)
//! or carries alongside itself, then extracts with real progress.
//!
//! Archive format: a zstd-compressed tar whose first entry is
//! [`MANIFEST_PATH`], a JSON `Vec<PayloadEntry>` describing every carried
//! file (size + SHA-256 for post-extraction verification).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::error::ShunError;
use crate::flow::{FlowEvent, FlowLog, FlowPhase};

/// Manifest entry name inside the payload archive.
pub const MANIFEST_PATH: &str = "shun-manifest.json";

/// Manifest describing one entry of a shun payload archive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PayloadEntry {
    /// Archive-relative path, also the on-disk path under the target
    /// directory.
    pub path: PathBuf,

    /// Uncompressed size in bytes (progress accounting).
    pub size: u64,

    /// SHA-256 of the uncompressed bytes (verify after extraction).
    pub sha256: String,
}

impl PayloadEntry {
    /// Sum of all entry sizes (progress + ARP `EstimatedSize` accounting).
    pub fn total_bytes(entries: &[PayloadEntry]) -> u64 {
        entries.iter().map(|e| e.size).sum()
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// Packs a directory into a shun payload archive (zstd-compressed tar).
///
/// The archive carries `MANIFEST_PATH` (JSON [`PayloadEntry`] list) as its
/// first entry followed by every regular file under `source_dir`, at paths
/// relative to it. Empty directories are not represented.
pub fn pack_directory(source_dir: &Path) -> Result<Vec<u8>, ShunError> {
    let mut files: Vec<PathBuf> = WalkDir::new(source_dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().to_path_buf())
        .collect();
    files.sort();

    let mut entries: Vec<PayloadEntry> = Vec::with_capacity(files.len());
    let mut contents: HashMap<PathBuf, Vec<u8>> = HashMap::with_capacity(files.len());
    for path in &files {
        let relative = path
            .strip_prefix(source_dir)
            .map_err(|e| ShunError::Config(format!("payload root walk drift: {e}")))?
            .to_path_buf();
        // The tar format requires forward-slash names; normalize here so
        // the header and the manifest agree on every platform.
        let name = relative.to_string_lossy().replace('\\', "/");
        let normalized = PathBuf::from(&name);
        let bytes = std::fs::read(path)?;
        entries.push(PayloadEntry {
            path: normalized.clone(),
            size: bytes.len() as u64,
            sha256: sha256_hex(&bytes),
        });
        contents.insert(normalized, bytes);
    }

    let manifest = serde_json::to_vec(&entries)
        .map_err(|e| ShunError::Config(format!("manifest serialize: {e}")))?;

    let mut tar_bytes: Vec<u8> = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut tar_bytes);
        let mut header = tar::Header::new_gnu();
        header.set_size(manifest.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, MANIFEST_PATH, manifest.as_slice())?;
        for entry in &entries {
            let bytes = &contents[&entry.path];
            let mut header = tar::Header::new_gnu();
            header.set_size(entry.size);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(
                &mut header,
                entry.path.to_string_lossy().as_ref(),
                bytes.as_slice(),
            )?;
        }
        builder.finish()?;
    }

    Ok(zstd::stream::encode_all(&tar_bytes[..], 19)?)
}

/// Read-side of a shun payload: a manifest plus streamed extraction.
///
/// Implementations: [`ArchivePayload`] reads packed archives; the shell
/// later adds an embedded (`include_bytes!`) source for single-file
/// installers.
pub trait PayloadSource {
    /// The manifest of entries carried by this payload.
    fn manifest(&self) -> &[PayloadEntry];

    /// Extract every entry under `dest`, emitting progress events as bytes
    /// hit the disk.
    fn extract(&self, dest: &Path, on_event: &mut dyn FnMut(FlowEvent)) -> Result<(), ShunError>;
}

/// Read-side of a shun payload archive: manifest plus streamed extraction
/// with SHA-256 verification and progress events.
///
/// Cloning is cheap-ish (the file set is fully materialized in memory) and
/// extraction is repeatable.
#[derive(Clone)]
pub struct ArchivePayload {
    entries: Vec<PayloadEntry>,
    files: HashMap<PathBuf, Vec<u8>>,
}

impl ArchivePayload {
    /// Decodes archive bytes produced by [`pack_directory`].
    pub fn from_bytes(archive: &[u8]) -> Result<Self, ShunError> {
        let tar_bytes = zstd::stream::decode_all(archive)?;
        let mut archive_reader = tar::Archive::new(&tar_bytes[..]);

        let mut manifest: Option<Vec<PayloadEntry>> = None;
        let mut files: HashMap<PathBuf, Vec<u8>> = HashMap::new();
        for entry in archive_reader.entries()? {
            let mut entry = entry?;
            let path = entry.path()?.to_path_buf();
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut bytes)?;
            if path == Path::new(MANIFEST_PATH) {
                manifest = Some(
                    serde_json::from_slice(&bytes)
                        .map_err(|e| ShunError::Config(format!("manifest parse: {e}")))?,
                );
            } else {
                files.insert(path, bytes);
            }
        }

        let entries =
            manifest.ok_or_else(|| ShunError::MissingEntry(PathBuf::from(MANIFEST_PATH)))?;
        Ok(Self { entries, files })
    }

    /// Extracts only the entries under `prefix`, preserving their archive
    /// paths under `dest`. The single-copy bootstrap path: a shell carrying
    /// a fixed-version WebView2 runtime inside its payload stages just
    /// that subtree for its own UI without unpacking the rest. Returns
    /// the number of bytes written (reused entries do not count).
    pub fn extract_prefix(
        &self,
        dest: &Path,
        prefix: &Path,
        on_event: &mut dyn FnMut(FlowEvent),
    ) -> Result<u64, ShunError> {
        let selected: Vec<&PayloadEntry> = self
            .entries
            .iter()
            .filter(|entry| entry.path.starts_with(prefix))
            .collect();
        if selected.is_empty() {
            return Err(ShunError::MissingEntry(prefix.to_path_buf()));
        }
        let selected_sizes: Vec<u64> = selected.iter().map(|entry| entry.size).collect();
        let total = u64::max(selected_sizes.iter().sum(), 1);
        let mut done: u64 = 0;
        let mut written: u64 = 0;
        for entry in selected {
            let reused = self.stage_entry(entry, dest, &mut done, total, on_event)?;
            if !reused {
                written += entry.size;
            }
        }
        Ok(written)
    }

    /// Verifies one entry against the in-memory bytes and writes it to
    /// `dest` — unless the destination already holds byte-identical
    /// content, in which case it is reused as-is (idempotent installs,
    /// pre-staged shared copies). Returns `true` when the entry was
    /// reused rather than written.
    fn stage_entry(
        &self,
        entry: &PayloadEntry,
        dest: &Path,
        done: &mut u64,
        total: u64,
        on_event: &mut dyn FnMut(FlowEvent),
    ) -> Result<bool, ShunError> {
        let bytes = self
            .files
            .get(&entry.path)
            .ok_or_else(|| ShunError::MissingEntry(entry.path.clone()))?;
        if sha256_hex(bytes) != entry.sha256 || bytes.len() as u64 != entry.size {
            return Err(ShunError::Config(format!(
                "payload integrity check failed for {}",
                entry.path.display()
            )));
        }

        let target = dest.join(&entry.path);
        let reused = target_is_identical(&target, entry)?;
        if !reused {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&target, bytes)?;
        }

        *done += entry.size;
        let step = format!(
            "{} {}",
            if reused { "Reusing" } else { "Extracting" },
            entry.path.display()
        );
        on_event(FlowEvent::Log {
            record: if reused {
                FlowLog::FileReuse {
                    path: entry.path.clone(),
                }
            } else {
                FlowLog::FileWrite {
                    path: entry.path.clone(),
                }
            },
        });
        on_event(FlowEvent::Progress {
            phase: FlowPhase::Extract,
            step,
            percent: Some((*done * 100 / total).min(100) as u8),
        });
        Ok(reused)
    }
}

/// `true` when `target` exists with exactly `entry`'s size and SHA-256 —
/// streamed, so multi-hundred-MB runtime folders verify without loading
/// into memory.
fn target_is_identical(target: &Path, entry: &PayloadEntry) -> Result<bool, ShunError> {
    let Ok(meta) = std::fs::metadata(target) else {
        return Ok(false);
    };
    if !meta.is_file() || meta.len() != entry.size {
        return Ok(false);
    }
    let mut file = std::fs::File::open(target)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    Ok(hex == entry.sha256)
}

impl PayloadSource for ArchivePayload {
    fn manifest(&self) -> &[PayloadEntry] {
        &self.entries
    }

    fn extract(&self, dest: &Path, on_event: &mut dyn FnMut(FlowEvent)) -> Result<(), ShunError> {
        let total = u64::max(PayloadEntry::total_bytes(&self.entries), 1);
        let mut done: u64 = 0;

        for entry in &self.entries {
            self.stage_entry(entry, dest, &mut done, total, on_event)?;
        }

        Ok(())
    }
}

/// Extracts without progress reporting (convenience for tests and tools).
pub fn extract_all(source: &dyn PayloadSource, dest: &Path) -> Result<(), ShunError> {
    source.extract(dest, &mut |_| {})
}
