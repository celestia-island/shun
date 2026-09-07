//! Online payload source — the web-installer story.
//!
//! Streams a payload archive straight from its release URL through the
//! whole pipeline in **one pass**: download bytes → zstd decode → tar
//! entries → SHA-256 verify → write. Download and extract progress run
//! concurrently (multi-phase), and each entry is verified against the
//! manifest as it streams by. Requires the `online` feature.

use std::cell::Cell;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use sha2::{Digest, Sha256};

use crate::error::ShunError;
use crate::flow::{FlowEvent, FlowPhase};
use crate::payload::{MANIFEST_PATH, PayloadEntry};

/// A payload archive fetched from its release URL at install time.
#[derive(Debug, Clone)]
pub struct OnlinePayload {
    url: String,
}

impl OnlinePayload {
    /// Points the payload at a packed archive URL (e.g. a GitHub Releases
    /// asset of a `*.shun` package).
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into() }
    }

    /// Streams download → decode → extract → verify into `dest`.
    ///
    /// The manifest must be the archive's first entry (as produced by
    /// [`pack_directory`](crate::payload::pack_directory)), so verification
    /// data arrives before the files it describes.
    pub fn extract(
        &self,
        dest: &Path,
        on_event: &mut dyn FnMut(FlowEvent),
    ) -> Result<(), ShunError> {
        let response = ureq::get(&self.url)
            .call()
            .map_err(|e| ShunError::Config(format!("download failed: {e}")))?;

        let download_total: Option<u64> = response
            .header("Content-Length")
            .and_then(|value| value.parse().ok());

        let counter = Rc::new(Cell::new(0u64));
        let mut counted = CountingReader::new(response.into_reader(), Rc::clone(&counter));
        let decoder = zstd::stream::Decoder::new(&mut counted)
            .map_err(|e| ShunError::Config(format!("payload decode init: {e}")))?;
        let mut archive = tar::Archive::new(decoder);
        let mut entries = archive.entries()?;

        // The manifest is the first entry of every shun archive; entries
        // after it are verified against it as they stream by.
        let mut first = entries
            .next()
            .ok_or_else(|| ShunError::MissingEntry(PathBuf::from(MANIFEST_PATH)))??;
        let mut manifest_bytes = Vec::new();
        first.read_to_end(&mut manifest_bytes)?;
        if !manifest_bytes.starts_with(b"[") {
            return Err(ShunError::MissingEntry(PathBuf::from(MANIFEST_PATH)));
        }
        let manifest: Vec<PayloadEntry> = serde_json::from_slice(&manifest_bytes)
            .map_err(|e| ShunError::Config(format!("manifest parse: {e}")))?;
        let manifest_by_path: HashMap<String, &PayloadEntry> = manifest
            .iter()
            .map(|entry| (entry.path.to_string_lossy().into_owned(), entry))
            .collect();

        let extract_total = u64::max(PayloadEntry::total_bytes(&manifest), 1);
        let mut extracted: u64 = 0;

        for entry in entries.by_ref() {
            let mut entry = entry?;
            let path = entry.path()?.to_path_buf();
            if path == Path::new(MANIFEST_PATH) {
                continue;
            }

            let spec = manifest_by_path
                .get(path.to_string_lossy().as_ref())
                .copied()
                .ok_or_else(|| ShunError::MissingEntry(path.clone()))?;

            let mut bytes = Vec::with_capacity(spec.size as usize);
            entry.read_to_end(&mut bytes)?;
            if bytes.len() as u64 != spec.size || sha256_hex(&bytes) != spec.sha256 {
                return Err(ShunError::Config(format!(
                    "payload integrity check failed for {}",
                    path.display()
                )));
            }

            let target = dest.join(&path);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&target, &bytes)?;

            extracted += bytes.len() as u64;

            // Multi-phase progress: network and local delivery move at the
            // same time, so report both while either is still running.
            let download_percent =
                download_total.map(|total| ((counter.get() * 100 / total).min(100)) as u8);
            on_event(FlowEvent::Progress {
                phase: FlowPhase::Download,
                step: format!("Downloading {}", self.url),
                percent: download_percent,
            });
            on_event(FlowEvent::Progress {
                phase: FlowPhase::Extract,
                step: format!("Extracting {}", path.display()),
                percent: Some((extracted * 100 / extract_total).min(100) as u8),
            });
        }

        // Drain the remaining stream through the decode pipeline (skipping
        // any tail entries) so the download phase finishes at 100%.
        for rest in entries.by_ref() {
            let mut entry = rest?;
            std::io::copy(&mut entry, &mut std::io::sink())?;
        }
        on_event(FlowEvent::Progress {
            phase: FlowPhase::Download,
            step: "Downloaded".to_string(),
            percent: Some(100),
        });
        Ok(())
    }
}

/// Reader wrapper counting bytes pulled through the decode pipeline.
struct CountingReader<R: Read> {
    inner: R,
    count: Rc<Cell<u64>>,
}

impl<R: Read> CountingReader<R> {
    fn new(inner: R, count: Rc<Cell<u64>>) -> Self {
        Self { inner, count }
    }
}

impl<R: Read> Read for CountingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.count.set(self.count.get() + n as u64);
        Ok(n)
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
