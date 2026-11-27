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
    // Per-entry Unix mode, carried for the executable bit only: tar's
    // `set_mode` decides what the extracted file can run as on POSIX.
    let mut modes: HashMap<PathBuf, u32> = HashMap::with_capacity(files.len());
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
        modes.insert(normalized.clone(), source_exec_mode(path));
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
            header.set_mode(modes.get(&entry.path).copied().unwrap_or(0o644));
            header.set_cksum();
            builder.append_data(
                &mut header,
                entry.path.to_string_lossy().as_ref(),
                bytes.as_slice(),
            )?;
        }
        builder.finish()?;
    }

    encode_tar(tar_bytes)
}

/// The Unix mode a payload entry is packed with: the source file's
/// permission bits (minus the setuid/setgid/sticky trio, which an
/// installer must never reproduce), or `0o644` when the source cannot
/// be read — notably a Windows build host, whose files have no Unix
/// mode, and whose payload therefore still packs as plain data.
#[cfg(unix)]
fn source_exec_mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|meta| meta.permissions().mode() & 0o777)
        .unwrap_or(0o644)
}

/// Windows has no Unix permission bits: every entry packs as `0o644`.
#[cfg(not(unix))]
fn source_exec_mode(_path: &Path) -> u32 {
    0o644
}

/// Compress a packed tar with zstd.
///
/// Level and worker count are environment-tunable so payload-heavy build
/// hosts can trade installer size for pack speed without a code change:
/// `SHUN_ZSTD_LEVEL` (default 19), `SHUN_ZSTD_WORKERS` (default: one
/// worker per CPU core; requires the `zstdmt` feature — with it disabled
/// the encoder stays single-threaded).
fn encode_tar(tar_bytes: Vec<u8>) -> Result<Vec<u8>, ShunError> {
    let level = std::env::var("SHUN_ZSTD_LEVEL")
        .ok()
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(19);
    let workers = std::env::var("SHUN_ZSTD_WORKERS")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .or_else(|| {
            std::thread::available_parallelism()
                .ok()
                .map(|n| n.get() as u32)
        })
        .unwrap_or(1);

    let mut encoder = zstd::stream::Encoder::new(Vec::new(), level)
        .map_err(|e| ShunError::Config(format!("zstd encoder: {e}")))?;
    encoder
        .multithread(workers)
        .map_err(|e| ShunError::Config(format!("zstd workers: {e}")))?;
    std::io::copy(&mut tar_bytes.as_slice(), &mut encoder)
        .map_err(|e| ShunError::Config(format!("zstd compress: {e}")))?;
    encoder
        .finish()
        .map_err(|e| ShunError::Config(format!("zstd finish: {e}")))
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
    /// Per-entry Unix mode from the archive headers (the executable bit
    /// the extractor restores); absent entries fall back to `0o644`.
    modes: HashMap<PathBuf, u32>,
}

/// A read-only cursor over the archive that reports consumption as a
/// 0..1 fraction — the decode-progress feed of
/// [`ArchivePayload::from_bytes_with_progress`].
struct CountingReader<'a> {
    buf: &'a [u8],
    pos: u64,
    total: u64,
    on_progress: &'a mut dyn FnMut(f32),
}

impl std::io::Read for CountingReader<'_> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let n = std::cmp::min(out.len(), self.buf.len());
        out[..n].copy_from_slice(&self.buf[..n]);
        self.buf = &self.buf[n..];
        self.pos += n as u64;
        (self.on_progress)((self.pos as f32 / self.total.max(1) as f32).min(1.0));
        Ok(n)
    }
}

/// Audit shun-R1-1: whether a manifest/archive entry path may be joined
/// under the install dir. Rejects absolute paths, Windows prefixes, and
/// any `..` component — `Path::join` would otherwise let a crafted
/// payload (or a poisoned manifest) write or delete outside the install
/// directory. Shared by the extract paths, the online variant, and the
/// uninstall sweep (wizard's stale-file cleanup uses it too).
///
/// The post-install companion launch
/// (`install.post-install-launch`) deliberately does NOT share this
/// predicate: its declaration is authored config, not archive content,
/// so it uses the stricter [`path_anchors_outside`] instead.
pub(crate) fn entry_path_is_unsafe(path: &Path) -> bool {
    path.components().any(|c| {
        matches!(
            c,
            std::path::Component::Prefix(_)
                | std::path::Component::RootDir
                | std::path::Component::ParentDir
        )
    })
}

/// Whether a path spells an anchor OUTSIDE the payload on any host: an
/// absolute path, a drive prefix (`C:\…`, `C:tool`, `C:/…`), or a UNC
/// / device lead-in (`\\server\share`, `//server/share`).
///
/// Stricter than [`entry_path_is_unsafe`] because `Path::is_absolute`
/// and friends answer for the CURRENT host only: a `C:\…` string is an
/// ordinary relative FILE NAME on POSIX and would sail through a
/// payload-relative check there, while the same manifest ships to every
/// platform. The declarations that consume it
/// (`install.post-install-launch`) are authored configuration, so the
/// permissive reading is the wrong one — refuse the spelling everywhere.
pub(crate) fn path_anchors_outside(path: &Path) -> bool {
    if path.is_absolute() || path.has_root() {
        return true;
    }
    let raw = path.to_string_lossy();
    let bytes = raw.as_bytes();
    // A drive prefix: one letter then `:`.
    let drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    // A UNC / device lead-in: the separator pair Windows accepts.
    let unc = raw.starts_with("\\\\") || raw.starts_with("//");
    drive || unc
}

/// Applies a packed entry's permission bits to the extracted file.
///
/// A packed POSIX entry carries its source permission bits in the archive
/// (see [`source_exec_mode`]); the payload's own data files are `0o644`
/// and stay `0o644`, but a program the product intends to RUN — a
/// launcher, a first-run setup helper — must land executable, or the
/// delivery produces a file nobody can start (the post-install handoff's
/// whole premise). Windows hosts have no mode bits to set and skip this.
///
/// The mode is applied VERBATIM: the packer already normalized it (the
/// setuid/setgid/sticky trio is stripped there, and a non-executable
/// source stays non-executable), so the extract lane needs no second
/// opinion — an `| 0o111` here would be unreachable guard code whose loss
/// no test could observe.
#[cfg(unix)]
fn apply_packed_mode(target: &Path, mode: Option<u32>) {
    use std::os::unix::fs::PermissionsExt;
    let Some(mode) = mode else { return };
    if let Ok(metadata) = std::fs::metadata(target) {
        let mut permissions = metadata.permissions();
        permissions.set_mode(mode & 0o777);
        let _ = std::fs::set_permissions(target, permissions);
    }
}

/// Windows: mode bits are not a thing here.
#[cfg(not(unix))]
fn apply_packed_mode(_target: &Path, _mode: Option<u32>) {}

impl ArchivePayload {
    /// An empty archive — the placeholder a UI holds before its init
    /// worker delivers the decoded one (the splash phase).
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
            files: HashMap::new(),
            modes: HashMap::new(),
        }
    }

    /// Decodes archive bytes produced by [`pack_directory`].
    pub fn from_bytes(archive: &[u8]) -> Result<Self, ShunError> {
        Self::from_bytes_with_progress(archive, &mut |_| {})
    }

    /// The streaming twin of [`from_bytes`]: the zstd stage runs over a
    /// counting reader, so `on_progress` receives the 0..1 fraction of
    /// COMPRESSED bytes consumed as the decode crawls — the feed a
    /// startup splash renders while a multi-hundred-MB payload unfolds
    /// (the buffered `decode_all` twin is opaque and peak-RAM twice
    /// over). The tar pass after it is fast and unreported.
    pub fn from_bytes_with_progress(
        archive: &[u8],
        on_progress: &mut dyn FnMut(f32),
    ) -> Result<Self, ShunError> {
        let reader = CountingReader {
            buf: archive,
            pos: 0,
            total: archive.len() as u64,
            on_progress,
        };
        let decoder = zstd::stream::read::Decoder::new(reader)?;
        let mut archive_reader = tar::Archive::new(decoder);

        let mut manifest: Option<Vec<PayloadEntry>> = None;
        let mut files: HashMap<PathBuf, Vec<u8>> = HashMap::new();
        let mut modes: HashMap<PathBuf, u32> = HashMap::new();
        for entry in archive_reader.entries()? {
            let mut entry = entry?;
            let path = entry.path()?.to_path_buf();
            let mode = entry.header().mode().ok();
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut bytes)?;
            if path == Path::new(MANIFEST_PATH) {
                manifest = Some(
                    serde_json::from_slice(&bytes)
                        .map_err(|e| ShunError::Config(format!("manifest parse: {e}")))?,
                );
            } else {
                if let Some(mode) = mode {
                    modes.insert(path.clone(), mode);
                }
                files.insert(path, bytes);
            }
        }

        let entries =
            manifest.ok_or_else(|| ShunError::MissingEntry(PathBuf::from(MANIFEST_PATH)))?;
        Ok(Self {
            entries,
            files,
            modes,
        })
    }

    /// The carried bytes of one archive path, pre-extraction — the
    /// script-hook runner's source (`prepare` hooks run before anything
    /// is written to disk).
    pub fn read_file(&self, path: &Path) -> Option<&[u8]> {
        self.files.get(path).map(|bytes| bytes.as_slice())
    }

    /// The archive manifest — every carried entry, in pack order. A
    /// read-only view for callers that scan the payload without
    /// extracting (e.g. the Evergreen installer discovery in
    /// [`crate::webview2`]).
    pub fn entries(&self) -> &[PayloadEntry] {
        &self.entries
    }

    /// Whether any carried entry sits under `prefix` — the shipped-pack
    /// check behind optional attachments (a full build carries the dest
    /// subtree, a lite build does not).
    pub fn has_prefix(&self, prefix: &Path) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.path.starts_with(prefix))
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

        // Audit shun-R1-1: fail-closed on traversal entries — a crafted
        // payload must not write outside the install directory.
        if entry_path_is_unsafe(&entry.path) {
            return Err(ShunError::Config(format!(
                "payload entry path escapes the install directory: {}",
                entry.path.display()
            )));
        }
        let target = dest.join(&entry.path);
        let reused = target_is_identical(&target, entry)?;
        if !reused {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            replace_file(&target, bytes)?;
            apply_packed_mode(&target, self.modes.get(&entry.path).copied());
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

/// Write `bytes` over `target`, surviving the one Windows upgrade trap
/// that matters: a RUNNING executable cannot be opened for write
/// (sharing violation), but it CAN be renamed. An upgrade install over a
/// live app therefore parks the old file aside under a `.shun-old-<ts>`
/// sibling name first, writes the new one in its place, and then tries
/// to delete the parked copy (the delete of a still-running image fails
/// harmlessly — the leftover is swept by the next install's pre-clean).
fn replace_file(target: &Path, bytes: &[u8]) -> Result<(), ShunError> {
    if target.exists() {
        // Sweep any parked copies from earlier upgrades before parking a
        // fresh one (bounded: same-directory siblings, prefix-matched).
        if let Some(dir) = target.parent() {
            if let Ok(read_dir) = std::fs::read_dir(dir) {
                let prefix = format!(
                    "{}.shun-old-",
                    target
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default()
                );
                for stale in read_dir.flatten() {
                    let name = stale.file_name().to_string_lossy().into_owned();
                    if name.starts_with(&prefix) {
                        let _ = std::fs::remove_file(stale.path());
                    }
                }
            }
        }
        let parked = target.with_file_name(format!(
            "{}.shun-old-{}",
            target
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::rename(target, &parked)?;
        match std::fs::write(target, bytes) {
            Ok(()) => {
                // The parked copy only survives when the old image is
                // still running — its deletion then fails, by design.
                let _ = std::fs::remove_file(&parked);
                Ok(())
            }
            Err(e) => {
                // Put the old file back so a failed write does not leave
                // the destination without its previous content.
                let _ = std::fs::rename(&parked, target);
                Err(e.into())
            }
        }
    } else {
        std::fs::write(target, bytes)?;
        Ok(())
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

#[cfg(test)]
mod traversal_guard_tests {
    use super::*;

    /// Hand-craft a minimal ustar archive (manifest.json first, then one
    /// regular file at exactly `hostile_path`) — tar::Builder refuses to
    /// append traversal names, and the real attacker hand-crafts the
    /// bytes the same way.
    fn hostile_archive(hostile_path: &str, content: &[u8]) -> Vec<u8> {
        let manifest_entries = vec![PayloadEntry {
            path: std::path::PathBuf::from(hostile_path),
            size: content.len() as u64,
            sha256: sha256_hex(content),
        }];
        let manifest_json = serde_json::to_vec(&manifest_entries).expect("manifest serialize");

        let mut tar_bytes = Vec::new();
        for (name, body) in [
            (MANIFEST_PATH, manifest_json.as_slice()),
            (hostile_path, content),
        ] {
            let mut header = [0u8; 512];
            header[..name.len()].copy_from_slice(name.as_bytes());
            header[100..108].copy_from_slice(b"0000644\0");
            header[108..116].copy_from_slice(b"0000000\0");
            header[116..124].copy_from_slice(b"0000000\0");
            header[124..136].copy_from_slice(format!("{:011o}\0", body.len()).as_bytes());
            header[136..148].copy_from_slice(b"00000000000\0");
            header[156] = b'0'; // regular file
            header[257..263].copy_from_slice(b"ustar\0");
            header[263..265].copy_from_slice(b"00");
            // The checksum sums the whole header with the chksum field
            // itself counted as eight spaces.
            header[148..156].copy_from_slice(b"        ");
            let sum: u32 = header.iter().map(|&b| b as u32).sum();
            header[148..156].copy_from_slice(format!("{:06o}\0 ", sum).as_bytes());
            tar_bytes.extend_from_slice(&header);
            tar_bytes.extend_from_slice(body);
            let pad = (512 - body.len() % 512) % 512;
            tar_bytes.extend(std::iter::repeat_n(0u8, pad));
        }
        tar_bytes.extend(std::iter::repeat_n(0u8, 1024)); // footer
        zstd::stream::encode_all(&tar_bytes[..], 3).expect("zstd encode")
    }

    /// Audit shun-R1-1: an archive entry with a parent-dir escape must be
    /// refused at extraction (the old code joined it verbatim and wrote
    /// outside the install directory).
    #[test]
    fn extract_refuses_parent_dir_escape() {
        let archive = hostile_archive("../../escaped.txt", b"payload");
        let payload = ArchivePayload::from_bytes(&archive).unwrap();

        let dest = std::env::temp_dir().join("shun-traversal-test");
        std::fs::create_dir_all(&dest).unwrap();
        let before = std::fs::read_dir(&dest).unwrap().count();

        let out = extract_all(&payload, &dest);
        assert!(out.is_err(), "traversal entry must be refused");
        let escaped = dest.parent().unwrap().join("escaped.txt");
        assert!(!escaped.exists(), "no file may land outside dest");
        let after = std::fs::read_dir(&dest).unwrap().count();
        assert_eq!(after, before, "no files written inside dest either");
    }

    /// Audit shun-R1-1: an ABSOLUTE archive entry replaces the join base
    /// entirely (Path::join semantics) — must be refused the same way.
    #[test]
    fn extract_refuses_absolute_entry() {
        let archive = hostile_archive("/etc/shun-escaped", b"payload");
        let payload = ArchivePayload::from_bytes(&archive).unwrap();
        let dest = std::env::temp_dir().join("shun-absolute-test");
        std::fs::create_dir_all(&dest).unwrap();
        let out = extract_all(&payload, &dest);
        assert!(out.is_err(), "absolute entry must be refused");
    }
}

#[cfg(test)]
mod replace_file_tests {
    use super::*;

    #[test]
    fn replace_file_overwrites_a_differing_target_and_cleans_up() {
        let dir = std::env::temp_dir().join("shun-replace-file-test");
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("app.exe");
        std::fs::write(&target, b"old image").unwrap();

        replace_file(&target, b"new image").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new image");
        // The parked sibling must be gone after a successful write.
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            assert!(!name.contains(".shun-old-"), "leftover parked file: {name}");
        }
    }

    #[test]
    fn replace_file_sweeps_parked_copies_from_earlier_upgrades() {
        let dir = std::env::temp_dir().join("shun-replace-sweep-test");
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("app.exe");
        std::fs::write(&target, b"old image").unwrap();
        // A parked copy an earlier upgrade could not delete (the old
        // image was still running at the time).
        let parked = dir.join("app.exe.shun-old-1234567890");
        std::fs::write(&parked, b"ancient image").unwrap();

        replace_file(&target, b"newer image").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"newer image");
        assert!(!parked.exists(), "the stale parked copy must be swept");
    }

    #[test]
    fn replace_file_writes_a_missing_target_directly() {
        let dir = std::env::temp_dir().join("shun-replace-missing-test");
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("fresh.exe");
        replace_file(&target, b"first image").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"first image");
    }
}

#[cfg(test)]
mod path_anchor_tests {
    use super::*;

    /// The strict anchor reads the SPELLING, not the host: a Windows drive
    /// prefix or a UNC lead-in is refused on POSIX too (where `Path`
    /// would read both as ordinary relative names) — the declaration
    /// ships to every platform, so the permissive reading is the wrong
    /// one.
    #[test]
    fn foreign_anchors_are_refused_on_every_host() {
        for refused in [
            "/bin/sh",
            "/usr/bin/env",
            r"C:\Windows\System32\cmd.exe",
            "C:/Windows/System32/cmd.exe",
            "C:provision.exe",
            r"\\server\share\provision.exe",
            "//server/share/provision.exe",
        ] {
            assert!(
                path_anchors_outside(Path::new(refused)),
                "{refused:?} anchors outside the payload"
            );
        }
        for relative in [
            "bin/provision.exe",
            "tool",
            "./bin/provision",
            "a/b/c",
            "...",
        ] {
            assert!(
                !path_anchors_outside(Path::new(relative)),
                "{relative:?} stays inside the payload"
            );
        }
        // The traversal component stays the OTHER guard's job.
        assert!(!path_anchors_outside(Path::new("bin/../../provision")));
        assert!(entry_path_is_unsafe(Path::new("bin/../../provision")));
    }
}

#[cfg(test)]
mod executable_mode_tests {
    use super::*;

    /// A payload file the SOURCE marks executable must land executable
    /// after pack → extract: the post-install handoff's whole premise is
    /// a program the delivery itself put there and can start. Data files
    /// (no execute bit) stay plain, and a setuid/setgid/sticky source
    /// never reproduces them.
    #[cfg(unix)]
    #[test]
    fn a_packed_executable_stays_executable_after_extraction() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        let stage = tempfile::tempdir().unwrap();
        let source = stage.path().join("src");
        std::fs::create_dir_all(source.join("bin")).unwrap();
        let program = source.join("bin").join("companion");
        std::fs::write(&program, b"#!/bin/sh\necho pair\n").unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let data = source.join("readme.txt");
        std::fs::write(&data, b"plain data").unwrap();
        std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o644)).unwrap();

        let archive = pack_directory(&source).unwrap();
        let payload = ArchivePayload::from_bytes(&archive).unwrap();
        let dest = stage.path().join("out");
        payload.extract(&dest, &mut |_| {}).unwrap();

        let packed = std::fs::metadata(dest.join("bin").join("companion")).unwrap();
        assert_eq!(
            packed.mode() & 0o777,
            0o755,
            "the packed executable bit survives extraction"
        );
        let plain = std::fs::metadata(dest.join("readme.txt")).unwrap();
        assert_eq!(plain.mode() & 0o111, 0, "data files stay non-executable");
        assert_eq!(
            plain.mode() & 0o777,
            0o644,
            "the packed mode is applied verbatim — no extractor-side promotion"
        );

        // A setuid source packs its plain permission bits only — an
        // installer must never deliver a privileged image.
        let privileged = source.join("bin").join("helper");
        std::fs::write(&privileged, b"#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&privileged, std::fs::Permissions::from_mode(0o4755)).unwrap();
        let archive = pack_directory(&source).unwrap();
        let payload = ArchivePayload::from_bytes(&archive).unwrap();
        let dest = stage.path().join("out2");
        payload.extract(&dest, &mut |_| {}).unwrap();
        let helper = std::fs::metadata(dest.join("bin").join("helper")).unwrap();
        assert_eq!(
            helper.mode() & 0o7000,
            0,
            "setuid/setgid/sticky bits are never reproduced"
        );
        assert_eq!(helper.mode() & 0o111, 0o111, "the execute bit still lands");
    }
}
