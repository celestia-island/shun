//! The runtime stamp loader: prefer the container appended to this exe
//! over the compile-time embeds.
//!
//! A GENERIC shell (built with `SHUN_SHELL_PRESET=generic`) carries
//! placeholder embeds and a stamp trailer describing an empty container.
//! Once `shun build --shell` stamps a product's container in, THIS is
//! where the runtime finds everything the compile-time path used to
//! bake in: config, wizard steps, per-locale license documents, logo,
//! flavor marker and payload.
//!
//! The container lives as a byte range inside this exe's own file, so
//! the payload is memory-mapped and handed to the payload extractor as
//! a slice — the same lazily-paged behavior `include_bytes!` had, with
//! no giant allocation and no copy.
//!
//! A PRESENT but corrupt container is fatal, never a silent fallback:
//! installing the placeholder (demo) data instead of the product's
//! would be the worst possible degradation.

use std::io::{Read, Seek, SeekFrom};
use std::sync::OnceLock;

use shun::stamp::{ContainerIndex, StampMeta, TRAILER_LEN, entry, parse_tail, verify_container};

/// The stamped data this run should embed, when the exe carries a
/// container.
pub struct StampData {
    pub config_json: &'static str,
    pub steps_json: &'static str,
    pub license_docs_json: &'static str,
    pub logo_kind: &'static str,
    pub logo_bytes: &'static [u8],
    pub payload: &'static [u8],
    /// The variant's `SHUN_FLAVOR` marker (the wizard's identity line),
    /// when the stamp carried one.
    pub flavor: Option<&'static str>,
    /// Sanitized product name for the per-run setup log directory.
    pub log_name: Option<&'static str>,
}

static STAMP: OnceLock<Result<Option<StampData>, String>> = OnceLock::new();

/// Probe this exe for a stamp container. MUST run before anything reads
/// the embedded data (main calls this first); a corrupt container fails
/// the run loudly.
pub fn init() -> Result<(), String> {
    match STAMP.get_or_init(load) {
        Ok(_) => Ok(()),
        Err(e) => Err(e.clone()),
    }
}

/// The stamped data, when the exe carries a container.
pub fn stamp() -> Option<&'static StampData> {
    STAMP
        .get_or_init(load)
        .as_ref()
        .ok()
        .and_then(|s| s.as_ref())
}

fn load() -> Result<Option<StampData>, String> {
    let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let mut file = std::fs::File::open(&exe).map_err(|e| format!("open {}: {e}", exe.display()))?;
    let total_len = file
        .metadata()
        .map_err(|e| format!("metadata {}: {e}", exe.display()))?
        .len();
    if total_len < TRAILER_LEN as u64 {
        return Ok(None);
    }
    file.seek(SeekFrom::Start(total_len - TRAILER_LEN as u64))
        .map_err(|e| format!("seek {}: {e}", exe.display()))?;
    let mut tail = [0u8; TRAILER_LEN];
    file.read_exact(&mut tail)
        .map_err(|e| format!("read trailer {}: {e}", exe.display()))?;
    let trailer = match parse_tail(&tail, total_len) {
        Ok(t) => t,
        // No trailer magic: the compile-time embeds stand (a demo or
        // product build from before the stamp era).
        Err(_) => return Ok(None),
    };
    if trailer.len == 0 {
        return Ok(None); // a GENERIC shell, not yet stamped
    }

    // Map the container range off our own file. The image section keeps
    // the exe readable for the lifetime of the process, so this stays
    // valid as long as we leak it.
    let mmap = unsafe {
        memmap2::MmapOptions::new()
            .offset(trailer.offset)
            .len(trailer.len as usize)
            .map(&file)
    }
    .map_err(|e| format!("map stamp container in {}: {e}", exe.display()))?;
    let container: &'static [u8] = Box::leak(Box::new(mmap));
    verify_container(&trailer, container)?;
    let index = ContainerIndex::parse(container)?;

    let meta: StampMeta = serde_json::from_slice(
        index
            .data(container, entry::META)
            .ok_or("stamp container carries no meta entry")?,
    )
    .map_err(|e| format!("stamp meta decodes: {e}"))?;

    let config = str_entry(&index, container, entry::CONFIG)?;
    let steps = str_entry(&index, container, entry::STEPS)?;
    let license_docs = str_entry(&index, container, entry::LICENSE_DOCS)?;
    let logo_kind = str_entry(&index, container, entry::LOGO_KIND)?;
    let logo_bytes = index
        .data(container, entry::LOGO)
        .ok_or("stamp container carries no logo entry")?;
    let payload = index
        .data(container, entry::PAYLOAD)
        .ok_or("stamp container carries no payload entry")?;

    Ok(Some(StampData {
        config_json: config,
        steps_json: steps,
        license_docs_json: license_docs,
        logo_kind: logo_kind,
        logo_bytes: logo_bytes,
        payload: payload,
        flavor: leak_option_string(meta.flavor),
        log_name: leak_option_string(meta.log_name),
    }))
}

fn str_entry(
    index: &ContainerIndex,
    container: &'static [u8],
    name: &str,
) -> Result<&'static str, String> {
    let bytes = index
        .data(container, name)
        .ok_or_else(|| format!("stamp container carries no {name} entry"))?;
    std::str::from_utf8(bytes).map_err(|e| format!("stamp {name} is not UTF-8: {e}"))
}

fn leak_option_string(s: String) -> Option<&'static str> {
    if s.is_empty() {
        None
    } else {
        Some(Box::leak(s.into_boxed_str()))
    }
}
