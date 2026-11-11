//! The runtime stamp loader: prefer the container appended to this exe
//! over the compile-time embeds.
//!
//! A GENERIC shell (built with `SHUN_SHELL_PRESET=generic`) carries
//! placeholder embeds and is turned into a product's installer by
//! `shun build --shell`, which appends the stamp container and writes
//! the trailer — THIS is where the runtime finds everything the
//! compile-time path used to bake in: config, wizard steps, per-locale
//! license documents, logo, flavor marker and payload.
//!
//! The container lives as a byte range inside this exe's own file, so
//! the payload is memory-mapped and handed to the payload extractor as
//! a slice — the same lazily-paged behavior `include_bytes!` had, with
//! no giant allocation and no copy.
//!
//! Two layouts are recognized:
//!
//! - **unsigned** — `[exe][container][trailer]`, the trailer occupying
//!   the final 64 bytes;
//! - **Authenticode-signed** — `[exe][container][trailer][certificate]`:
//!   signing appends the WIN_CERTIFICATE block AFTER the trailer, so the
//!   trailer is looked up just ahead of the certificate range declared by
//!   the PE Security directory.
//!
//! A PRESENT but corrupt container (or a signed exe missing its trailer)
//! is fatal, never a silent fallback: installing the placeholder (demo)
//! data instead of the product's would be the worst possible degradation.
//! A file with neither trailer nor signature falls back — the compile-
//! time embeds stand (the local demo and product builds).

use std::io::{Read, Seek, SeekFrom};
use std::sync::OnceLock;

use shun::stamp::{ContainerIndex, StampMeta, TRAILER_LEN, entry, parse_tail, verify_container};

/// The stamped data this run should embed, when the exe carries a
/// container.
#[derive(Debug)]
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
    load_from(&exe)
}

/// Load the stamp container from an explicit exe path — the testable
/// core of [`load`] (`load` points it at `current_exe`).
pub(crate) fn load_from(path: &std::path::Path) -> Result<Option<StampData>, String> {
    let mut file =
        std::fs::File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let total_len = file
        .metadata()
        .map_err(|e| format!("metadata {}: {e}", path.display()))?
        .len();

    // Primary layout: the trailer occupying the final bytes (unsigned).
    if let Some(data) = try_trailer_at(&mut file, total_len)? {
        return Ok(Some(data));
    }

    // Signed layout: Authenticode appends the certificate AFTER the
    // trailer — the trailer ends where the certificate begins, and the
    // certificate range is declared by the PE Security directory.
    let header = read_header(&mut file)?;
    if let Some((cert_off, cert_size)) = security_directory_range(&header)? {
        if cert_size > 0 && cert_off + cert_size == total_len {
            if let Some(data) = try_trailer_at(&mut file, cert_off)? {
                return Ok(Some(data));
            }
            return Err(
                "the signed exe carries no stamp trailer ahead of its certificate — corrupt stamp"
                    .into(),
            );
        }
    }

    Ok(None)
}

/// Try to load a container whose trailer occupies the 64 bytes ending at
/// `trailer_end`. Absent magic → no container here; magic with a broken
/// trailer → fatal.
fn try_trailer_at(file: &mut std::fs::File, trailer_end: u64) -> Result<Option<StampData>, String> {
    if trailer_end < TRAILER_LEN as u64 {
        return Ok(None);
    }
    file.seek(SeekFrom::Start(trailer_end - TRAILER_LEN as u64))
        .map_err(|e| format!("seek: {e}"))?;
    let mut tail = [0u8; TRAILER_LEN];
    file.read_exact(&mut tail)
        .map_err(|e| format!("read trailer: {e}"))?;
    if &tail[..8] != shun::stamp::MAGIC {
        return Ok(None); // no stamp here
    }
    let trailer = parse_tail(&tail, trailer_end)?;
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
            .map(&*file)
    }
    .map_err(|e| format!("map stamp container: {e}"))?;
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
        logo_kind,
        logo_bytes,
        payload,
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

/// The PE Security directory (data directory index 4) as a byte range in
/// the FILE — VirtualAddress is a file offset for this one directory —
/// or None when the image is unsigned or too foreign to parse.
fn security_directory_range(header: &[u8]) -> Result<Option<(u64, u64)>, String> {
    if header.len() < 0x40 || &header[..2] != b"MZ" {
        return Ok(None);
    }
    let lfanew = u32_le(header, 0x3c) as usize;
    if lfanew + 4 + 20 > header.len() || &header[lfanew..lfanew + 4] != b"PE\0\0" {
        return Ok(None);
    }
    let opt = lfanew + 24;
    if opt + 2 > header.len() {
        return Ok(None);
    }
    let magic = u16_le(header, opt);
    // Data directories start after the fixed optional-header part:
    // 96 bytes in for PE32, 112 for PE32+.
    let dd = opt
        + match magic {
            0x10b => 96,
            0x20b => 112,
            _ => return Ok(None),
        };
    // The Security entry is index 4: (file offset, size).
    if dd + 5 * 8 > header.len() {
        return Ok(None);
    }
    let off = u32_le(header, dd + 4 * 8) as u64;
    let size = u32_le(header, dd + 4 * 8 + 4) as u64;
    if size == 0 {
        return Ok(None);
    }
    Ok(Some((off, size)))
}

/// The first 4 KiB — enough for the DOS header, the PE signature, and
/// the optional header's data directories on every image we stamp.
fn read_header(file: &mut std::fs::File) -> Result<Vec<u8>, String> {
    file.seek(SeekFrom::Start(0))
        .map_err(|e| format!("seek: {e}"))?;
    let mut header = vec![0u8; 4096];
    let mut read = 0usize;
    while read < header.len() {
        let n = file
            .read(&mut header[read..])
            .map_err(|e| format!("read header: {e}"))?;
        if n == 0 {
            break;
        }
        read += n;
    }
    header.truncate(read);
    Ok(header)
}

fn u16_le(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(bytes[at..at + 2].try_into().expect("bounds checked"))
}

fn u32_le(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().expect("bounds checked"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use shun::stamp::{StampMeta, StampParts, build_trailer};

    fn stamp_parts() -> StampParts {
        StampParts {
            config_json: r#"{"product":{"name":"SmokeTest"}}"#.into(),
            steps_json: "[]".into(),
            license_docs_json: "{}".into(),
            logo_bytes: vec![1, 2, 3],
            logo_kind: "png".into(),
            payload_archive: b"payload-bytes".to_vec(),
            meta: StampMeta {
                shun_version: "0.6.33".into(),
                product_name: "SmokeTest".into(),
                product_version: "1.2.3".into(),
                variant: Some("lite".into()),
                flavor: "lite".into(),
                log_name: "SmokeTest".into(),
            },
        }
    }

    fn unique_path(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("shun-loader-test-{}-{}", std::process::id(), name));
        p
    }

    #[test]
    fn stamped_file_loads_back_through_the_container() {
        let path = unique_path("ok.exe");
        let parts = stamp_parts();
        let container = parts.pack_container().expect("pack");
        let mut data = b"MZfake shell".to_vec();
        let trailer = build_trailer(data.len() as u64, &container);
        data.extend_from_slice(&container);
        data.extend_from_slice(&trailer);
        std::fs::write(&path, &data).expect("write stamped fixture");

        let loaded = load_from(&path)
            .expect("load succeeds")
            .expect("container present");
        assert_eq!(loaded.config_json, r#"{"product":{"name":"SmokeTest"}}"#);
        assert_eq!(loaded.steps_json, "[]");
        assert_eq!(loaded.logo_bytes, &[1, 2, 3]);
        assert_eq!(loaded.logo_kind, "png");
        assert_eq!(loaded.payload, b"payload-bytes");
        assert_eq!(loaded.flavor, Some("lite"));
        assert_eq!(loaded.log_name, Some("SmokeTest"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_signed_exe_reads_the_trailer_ahead_of_its_certificate() {
        // Signed layout: Authenticode appends the WIN_CERTIFICATE after
        // the trailer; the loader must step over the certificate using
        // the PE Security directory. The fixture hand-builds a fake
        // PE32+ header with the Security directory filled to point at a
        // token certificate appended past the trailer.
        let path = unique_path("signed.exe");
        let parts = stamp_parts();
        let container = parts.pack_container().expect("pack");

        let mut data = vec![0u8; 0x400];
        data[0..2].copy_from_slice(b"MZ");
        let lfanew = 0x200usize;
        data[0x3c..0x40].copy_from_slice(&(lfanew as u32).to_le_bytes());
        data[lfanew..lfanew + 4].copy_from_slice(b"PE\0\0");
        let opt = lfanew + 24;
        data[opt..opt + 2].copy_from_slice(&0x20bu16.to_le_bytes()); // PE32+
        let dd = opt + 112; // data directories (PE32+)

        let trailer = build_trailer(data.len() as u64, &container);
        data.extend_from_slice(&container);
        data.extend_from_slice(&trailer);

        // Certificates start 8-byte aligned.
        let padded = data.len().div_ceil(8) * 8;
        let mut cert = vec![0u8; 40];
        let cert_len = cert.len() as u32;
        cert[0..4].copy_from_slice(&cert_len.to_le_bytes()); // dwLength
        while data.len() < padded {
            data.push(0);
        }
        let cert_off = data.len() as u64;
        data.extend_from_slice(&cert);

        // Security directory entry (index 4): file offset + size.
        data[dd + 32..dd + 36].copy_from_slice(&(cert_off as u32).to_le_bytes());
        data[dd + 36..dd + 40].copy_from_slice(&cert_len.to_le_bytes());
        std::fs::write(&path, &data).expect("write signed fixture");

        let loaded = load_from(&path)
            .expect("load succeeds")
            .expect("container found across the certificate");
        assert_eq!(loaded.payload, b"payload-bytes");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_corrupt_container_is_fatal_not_a_silent_fallback() {
        let path = unique_path("corrupt.exe");
        let parts = stamp_parts();
        let container = parts.pack_container().expect("pack");
        let mut data = b"MZfake shell".to_vec();
        let trailer = build_trailer(data.len() as u64, &container);
        data.extend_from_slice(&container);
        data.extend_from_slice(&trailer);
        // flip one container byte: the sha256 must stop the load
        let idx = data.len() - 64 - 10;
        data[idx] ^= 0xff;
        std::fs::write(&path, &data).unwrap();
        let err = load_from(&path).expect_err("corrupt container is fatal");
        assert!(err.contains("sha256"), "unexpected error: {err}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_broken_trailer_after_magic_is_fatal_too() {
        let path = unique_path("badrev.exe");
        let mut data = b"MZfake shell".to_vec();
        let mut tail = [0u8; TRAILER_LEN];
        tail[..8].copy_from_slice(shun::stamp::MAGIC);
        tail[8..12].copy_from_slice(&99u32.to_le_bytes()); // unknown rev
        data.extend_from_slice(&tail);
        std::fs::write(&path, &data).unwrap();
        let err = load_from(&path).expect_err("a magic-bearing broken trailer is fatal");
        assert!(err.contains("rev 99"), "unexpected error: {err}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn no_trailer_falls_back_to_none() {
        let path = unique_path("plain.exe");
        std::fs::write(&path, b"MZ plain build").unwrap();
        let loaded = load_from(&path).expect("load succeeds");
        assert!(loaded.is_none(), "a plain exe carries no container");
        let _ = std::fs::remove_file(&path);
    }
}
