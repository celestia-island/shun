//! The stamp container: a product-neutral shell binary made
//! product-specific by appending data, not by recompiling.
//!
//! A generic installer shell is stampable in two shapes: a PLAIN local
//! build (`SHUN_SHELL_PRESET=generic cargo build` — no trailer: build
//! scripts run before the linker, so nothing can have appended one yet)
//! and the published release asset (whose release job bakes a fixed
//! 64-byte trailer describing an EMPTY container). `shun build --shell
//! <exe>` accepts both, patches the exe's Windows version resource and
//! icon to the product's identity (the OS's own `BeginUpdateResourceW`
//! editor — see [`stamp_win`]), appends a container holding everything the
//! shell's compile-time embedding used to carry, and writes the trailer.
//! The shell runtime prefers the container when one is present and falls
//! back to the compile-time constants otherwise — so a plain `cargo
//! build` of the demo shell keeps working unchanged.
//!
//! # Container format (rev 1)
//!
//! `[shell exe][container][trailer 64B]` — the trailer:
//!
//! ```text
//! 0..8    magic b"SHUNSTMP"
//! 8..12   format rev (u32 LE) — 1
//! 12..16  flags (u32 LE) — 0
//! 16..24  container offset from file start (u64 LE)
//! 24..32  container length (u64 LE)
//! 32..64  sha256(container)
//! ```
//!
//! The container itself is a plain (uncompressed) tar with fixed short
//! entry names, so the payload entry — the multi-hundred-MB zstd archive
//! `shun pack` already produces — stays a byte range: the shell runtime
//! can memory-map the exe and hand the range to the payload extractor
//! without a copy, the same lazily-paged behavior `include_bytes!` had.
//!
//! The manifest-to-parts resolution lives here too ([`resolve_parts`],
//! [`resolved_config`]): the shell's `build.rs` and the stamp CLI drive
//! the SAME transforms, so the two embedding paths cannot drift.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The trailer magic.
pub const MAGIC: &[u8; 8] = b"SHUNSTMP";
/// The container format this crate reads and writes.
pub const FORMAT_REV: u32 = 1;
/// Fixed trailer size: magic(8) + rev(4) + flags(4) + offset(8) + len(8)
/// + sha256(32).
pub const TRAILER_LEN: usize = 64;

/// The wizard locales the license documents resolve for — the shell's
/// i18n tables. The shell build and the stamp CLI must agree with the
/// shell's own locale list; a locale added to the shell's tables without
/// adding it here degrades to the base document for that locale, never to
/// a build error.
pub const SHELL_LOCALES: [&str; 8] = ["en", "zh-Hans", "zh-Hant", "ja", "ko", "fr", "ru", "es"];

// ── trailer ─────────────────────────────────────────────────────────────

/// A parsed trailer: where the container starts, how long it is, and the
/// digest its content must verify against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trailer {
    pub offset: u64,
    pub len: u64,
    pub sha256: [u8; 32],
}

impl Trailer {
    /// The trailer a GENERIC (never-stamped) shell ships with: magic and
    /// format present, zero-length container.
    pub fn generic() -> [u8; TRAILER_LEN] {
        let mut t = [0u8; TRAILER_LEN];
        t[..8].copy_from_slice(MAGIC);
        t[8..12].copy_from_slice(&FORMAT_REV.to_le_bytes());
        t[16..24].copy_from_slice(&0u64.to_le_bytes());
        t[24..32].copy_from_slice(&0u64.to_le_bytes());
        t[32..64].copy_from_slice(&Sha256::digest([]));
        t
    }
}

/// Parse and structurally verify the trailer occupying the last
/// `TRAILER_LEN` bytes of `exe`.
pub fn parse_trailer(exe: &[u8]) -> Result<Trailer, String> {
    if exe.len() < TRAILER_LEN {
        return Err(format!(
            "too short for a stamp trailer: {} bytes",
            exe.len()
        ));
    }
    let mut tail = [0u8; TRAILER_LEN];
    tail.copy_from_slice(&exe[exe.len() - TRAILER_LEN..]);
    parse_tail(&tail, exe.len() as u64)
}

/// Parse a trailer from the final `TRAILER_LEN` bytes of a file of
/// `total_len` bytes — the runtime loader reads only the tail of its own
/// exe ([`parse_trailer`] wraps this for whole-file slices).
pub fn parse_tail(tail: &[u8; TRAILER_LEN], total_len: u64) -> Result<Trailer, String> {
    if total_len < TRAILER_LEN as u64 {
        return Err(format!("too short for a stamp trailer: {total_len} bytes"));
    }
    let t = &tail[..];
    if &t[..8] != MAGIC {
        return Err("no stamp trailer magic — not a shun shell binary".into());
    }
    let rev = u32::from_le_bytes(t[8..12].try_into().unwrap());
    if rev != FORMAT_REV {
        return Err(format!(
            "stamp container format rev {rev} unsupported — this build understands rev {FORMAT_REV}"
        ));
    }
    let flags = u32::from_le_bytes(t[12..16].try_into().unwrap());
    if flags != 0 {
        return Err(format!("unknown trailer flags {flags:#x}"));
    }
    let offset = u64::from_le_bytes(t[16..24].try_into().unwrap());
    let len = u64::from_le_bytes(t[24..32].try_into().unwrap());
    let mut sha256 = [0u8; 32];
    sha256.copy_from_slice(&t[32..64]);
    if len == 0 {
        // A GENERIC shell: no container exists, so there is nothing to
        // abut — the trailer simply occupies the final bytes.
        return Ok(Trailer {
            offset,
            len,
            sha256,
        });
    }
    let end = offset.saturating_add(len);
    if end != total_len - TRAILER_LEN as u64 {
        return Err(format!(
            "container [{offset}, {end}) does not abut the trailer at {} — corrupt stamp",
            total_len - TRAILER_LEN as u64
        ));
    }
    Ok(Trailer {
        offset,
        len,
        sha256,
    })
}

/// Verify a container's bytes against a parsed trailer.
pub fn verify_container(trailer: &Trailer, container: &[u8]) -> Result<(), String> {
    if container.len() as u64 != trailer.len {
        return Err(format!(
            "container length {} != trailer's {}",
            container.len(),
            trailer.len
        ));
    }
    let got = Sha256::digest(container);
    if got.as_slice() != trailer.sha256 {
        return Err("container sha256 mismatch — the stamped exe is corrupt".into());
    }
    Ok(())
}

/// Build the trailer for `container` placed at `offset`.
pub fn build_trailer(offset: u64, container: &[u8]) -> [u8; TRAILER_LEN] {
    let mut t = [0u8; TRAILER_LEN];
    t[..8].copy_from_slice(MAGIC);
    t[8..12].copy_from_slice(&FORMAT_REV.to_le_bytes());
    t[16..24].copy_from_slice(&offset.to_le_bytes());
    t[24..32].copy_from_slice(&(container.len() as u64).to_le_bytes());
    t[32..64].copy_from_slice(&Sha256::digest(container));
    t
}

// ── container ───────────────────────────────────────────────────────────

/// Fixed container entry names, in pack order. The payload rides last so
/// streaming readers can stop before it.
pub mod entry {
    pub const CONFIG: &str = "shun-config.json";
    pub const STEPS: &str = "shun-steps.json";
    pub const LICENSE_DOCS: &str = "shun-license-docs.json";
    pub const LOGO: &str = "shun-logo.bin";
    pub const LOGO_KIND: &str = "shun-logo-kind.txt";
    pub const META: &str = "shun-meta.json";
    pub const PAYLOAD: &str = "shun-payload.bin";
}

/// Pack the container: a tar with fixed short names. The payload is
/// ALREADY the compressed `shun pack` archive — recompressing would spend
/// CPU to shrink nothing, so the container tar stores it as-is.
pub fn pack_container(entries: &[(&str, &[u8])]) -> Result<Vec<u8>, String> {
    let mut builder = tar::Builder::new(Vec::new());
    for &(name, data) in entries.iter() {
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(0);
        builder
            .append_data(&mut header, name, data)
            .map_err(|e| format!("container entry {name}: {e}"))?;
    }
    builder
        .into_inner()
        .map_err(|e| format!("container finalize: {e}"))
}

/// A zero-copy view of a parsed container: entry name → byte range into
/// the container slice the index was parsed from.
#[derive(Debug, Clone)]
pub struct ContainerIndex {
    ranges: BTreeMap<String, (u64, u64)>,
}

impl ContainerIndex {
    /// Walk a tar container. Entries are looked up by exact name; the
    /// format never emits long names (fixed short entry names), and a
    /// pax/GNU long-name extension header is a format error rather than
    /// something to interpret.
    pub fn parse(container: &[u8]) -> Result<Self, String> {
        let mut ranges = BTreeMap::new();
        let mut pos = 0usize;
        while pos + 512 <= container.len() {
            let header = &container[pos..pos + 512];
            if header.iter().all(|&b| b == 0) {
                break; // end-of-archive zero block
            }
            // Both ustar and gnu headers carry the magic "ustar" at 257.
            if &header[257..262] != b"ustar" {
                return Err(format!(
                    "container entry at {pos}: not a tar header (long-name formats unsupported)"
                ));
            }
            let name_end = header[..100].iter().position(|&b| b == 0).unwrap_or(100);
            let name = String::from_utf8_lossy(&header[..name_end]).into_owned();
            let size = parse_size(&header[124..136])
                .map_err(|e| format!("container entry '{name}': {e}"))?;
            let data = (pos + 512) as u64;
            let padded = size.div_ceil(512) * 512;
            if data + size > container.len() as u64 {
                return Err(format!("container entry '{name}': truncated payload"));
            }
            ranges.insert(name, (data, size));
            pos = (data + padded) as usize;
        }
        Ok(Self { ranges })
    }

    /// The byte range of one entry.
    pub fn range(&self, name: &str) -> Option<(u64, u64)> {
        self.ranges.get(name).copied()
    }

    /// Slice one entry out of `container`.
    pub fn data<'a>(&self, container: &'a [u8], name: &str) -> Option<&'a [u8]> {
        self.range(name)
            .map(|(off, len)| &container[off as usize..(off + len) as usize])
    }
}

/// Tar header size field: octal ASCII, or GNU base-256 (high bit of the
/// first byte set) for sizes that outgrow eleven octal digits.
fn parse_size(bytes: &[u8]) -> Result<u64, String> {
    if let Some(&first) = bytes.first() {
        if first & 0x80 != 0 {
            let mut v: u64 = (first & 0x7f) as u64;
            for &b in &bytes[1..] {
                v = (v << 8) | b as u64;
            }
            return Ok(v);
        }
    }
    let s = String::from_utf8_lossy(bytes);
    let trimmed = s.trim_matches(char::from(0)).trim();
    if trimmed.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(trimmed, 8).map_err(|e| format!("bad octal size '{trimmed}': {e}"))
}

// ── parts resolution (shared with the shell's build.rs) ─────────────────

/// Identity + provenance facts carried in the container's meta entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StampMeta {
    /// The shun crate version that stamped the container.
    pub shun_version: String,
    pub product_name: String,
    pub product_version: String,
    /// The named `[[variant]]` applied, if any.
    pub variant: Option<String>,
    /// The `SHUN_FLAVOR` marker the variant env carried (the wizard's
    /// identity line; empty when the manifest declares none).
    pub flavor: String,
    /// Sanitized product name for the per-run setup log directory (the
    /// same transform the compile-time `SHUN_PRODUCT_LOG_NAME` used).
    pub log_name: String,
}

/// Everything a stamped shell embeds — the exact set the compile-time
/// build.rs path writes into OUT_DIR.
pub struct StampParts {
    pub config_json: String,
    pub steps_json: String,
    pub license_docs_json: String,
    pub logo_bytes: Vec<u8>,
    pub logo_kind: String,
    pub payload_archive: Vec<u8>,
    pub meta: StampMeta,
}

impl StampParts {
    /// Pack the container. The meta entry serializes here — the one part
    /// that is not a field slice — so the method (not a bare entries list)
    /// is the container's single entry point.
    pub fn pack_container(&self) -> Result<Vec<u8>, String> {
        let meta = self.meta_bytes();
        let entries: [(&str, &[u8]); 7] = [
            (entry::CONFIG, self.config_json.as_bytes()),
            (entry::STEPS, self.steps_json.as_bytes()),
            (entry::LICENSE_DOCS, self.license_docs_json.as_bytes()),
            (entry::LOGO, &self.logo_bytes),
            (entry::LOGO_KIND, self.logo_kind.as_bytes()),
            (entry::META, &meta),
            (entry::PAYLOAD, &self.payload_archive),
        ];
        pack_container(&entries)
    }

    fn meta_bytes(&self) -> Vec<u8> {
        serde_json::to_vec_pretty(&self.meta).expect("stamp meta serializes")
    }
}

/// Product identity for the Windows version resource / icon stamped
/// alongside the container.
#[derive(Debug, Clone)]
pub struct StampIdentity {
    pub company: String,
    /// The Explorer / UAC file description (e.g. "WoWSP Installer").
    pub description: String,
    /// Internal name: the artifact stem.
    pub internal_name: String,
    pub version: String,
    /// The product logo re-encoded as a multi-size ICO (None: the generic
    /// shell's own icon stays).
    pub ico: Option<Vec<u8>>,
}

/// Stamp `shell_exe` into `out_path` with the product's container and
/// identity. Windows-only (the OS resource editor patches the identity);
/// callers on other hosts get a loud refusal instead of silently shipping
/// a generic-identity installer.
#[cfg(windows)]
pub fn stamp_shell(
    shell_exe: &Path,
    out_path: &Path,
    parts: &StampParts,
    identity: &StampIdentity,
) -> Result<(), String> {
    let exe = std::fs::read(shell_exe)
        .map_err(|e| format!("read shell binary {}: {e}", shell_exe.display()))?;
    // The shell binary is either a PLAIN generic build (local
    // `SHUN_SHELL_PRESET=generic cargo build` — no trailer: build scripts
    // run before the linker, so nothing can have appended one yet) or a
    // PUBLISHED generic asset (the release job bakes `Trailer::generic`
    // on). Both stamp; an already-stamped exe does not. A magic-bearing
    // exe whose trailer does not parse (unknown rev, bad flags,
    // non-abutting range) fails LOUDLY — stamping over it would bury
    // whatever went wrong.
    let trailer_at_end = exe.len() >= TRAILER_LEN
        && &exe[exe.len() - TRAILER_LEN..exe.len() - TRAILER_LEN + 8] == MAGIC;
    let input_body_len = if trailer_at_end {
        let trailer = parse_trailer(&exe)?;
        if trailer.len != 0 {
            return Err(format!(
                "shell binary {} already carries a container ({} bytes) — stamp the GENERIC shell",
                shell_exe.display(),
                trailer.len
            ));
        }
        exe.len() - TRAILER_LEN
    } else if exe.starts_with(b"MZ") {
        exe.len()
    } else {
        return Err(format!(
            "{} is neither a PE image nor a shun shell binary",
            shell_exe.display()
        ));
    };

    // 1. Identity resources FIRST, on a plain exe file: the OS resource
    //    editor rewrites the PE and must not meet our appended overlay.
    let tmp = out_path.with_extension("stamping.exe");
    std::fs::write(&tmp, &exe[..input_body_len])
        .map_err(|e| format!("write staging copy {}: {e}", tmp.display()))?;
    stamp_win::patch_identity(&tmp, identity)?;
    let exe = std::fs::read(&tmp).map_err(|e| format!("read patched exe: {e}"))?;
    let _ = std::fs::remove_file(&tmp);

    // 2. Container + trailer append — the overlay lands last, after the
    //    resource editor has finished rewriting the PE.
    //
    //    The container starts at the PATCHED file's true end: updating
    //    resources resizes sections (the .rsrc growth alone is tens of
    //    KB), so the pre-patch length is useless here — truncating to it
    //    would cut the last section's tail and park the container inside
    //    a live section. If the resource editor preserved the baked
    //    empty-container trailer, strip it; if it dropped it, the patched
    //    bytes are the body in full.
    let body_len = match parse_trailer(&exe) {
        Ok(trailer) if trailer.len == 0 => exe.len() - TRAILER_LEN,
        Ok(_) => {
            return Err(
                "the identity-patched exe unexpectedly carries a non-empty container".into(),
            );
        }
        Err(_) => exe.len(),
    };
    let mut out = exe;
    out.truncate(body_len);
    let offset = body_len as u64;
    let container = parts.pack_container()?;
    let trailer = build_trailer(offset, &container);
    out.extend_from_slice(&container);
    out.extend_from_slice(&trailer);

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    std::fs::write(out_path, &out)
        .map_err(|e| format!("write artifact {}: {e}", out_path.display()))
}

/// The non-Windows stub: the identity patch needs the OS resource
/// editor, so stamping is refused loudly instead of shipping a
/// generic-identity installer. The container format itself (trailer,
/// tar packing, verinfo) is host-independent and fully tested above.
#[cfg(not(windows))]
pub fn stamp_shell(
    _shell_exe: &Path,
    _out_path: &Path,
    _parts: &StampParts,
    _identity: &StampIdentity,
) -> Result<(), String> {
    Err(
        "stamping requires Windows: the version resource and icon are patched with the OS \
         resource editor (BeginUpdateResourceW). Run the stamp on a Windows host — the shun \
         release and product release CI both do."
            .into(),
    )
}

#[cfg(windows)]
#[path = "stamp_win.rs"]
pub mod stamp_win;

/// The VS_VERSIONINFO resource, built and parsed in pure bytes so the
/// layout is testable on every host — the Windows patcher only carries
/// the finished block to `UpdateResourceW`.
pub mod verinfo {
    use super::StampIdentity;
    use std::collections::BTreeMap;

    /// The classic English-US StringTable language/charset pair (code
    /// page 1200, Unicode): what tauri-build stamps and what Explorer
    /// expects to find.
    const LANG_ID: u16 = 0x0409;
    const CHARSET_ID: u16 = 0x04b0;

    fn push_wstr(out: &mut Vec<u8>, s: &str) {
        for unit in s.encode_utf16() {
            out.extend_from_slice(&unit.to_le_bytes());
        }
        out.extend_from_slice(&0u16.to_le_bytes());
    }

    fn pad32(out: &mut Vec<u8>) {
        while out.len() % 4 != 0 {
            out.push(0);
        }
    }

    fn header(total: u16, value_len: u16, value_type: u16, key: &str, out: &mut Vec<u8>) {
        out.extend_from_slice(&total.to_le_bytes());
        out.extend_from_slice(&value_len.to_le_bytes());
        out.extend_from_slice(&value_type.to_le_bytes());
        push_wstr(out, key);
        pad32(out);
    }

    /// The byte length of a block's key field as header() writes it:
    /// wide chars + NUL, padded so the field ENDS on a 32-bit boundary
    /// relative to the block start (the `6`-byte header precedes it, so
    /// the padding depends on the position — the same key after a
    /// different prefix pads differently).
    fn key_field_len(s: &str, prefix: usize) -> usize {
        let bytes = (s.encode_utf16().count() + 1) * 2;
        let aligned_end = (prefix + bytes).div_ceil(4) * 4;
        aligned_end - prefix
    }

    /// One `String` block: header (which carries the key) + the
    /// NUL-padded string value.
    fn string_block(key: &str, value: &str) -> Vec<u8> {
        let value_len = (value.encode_utf16().count() + 1) * 2;
        let mut body = Vec::new();
        push_wstr(&mut body, value);
        pad32(&mut body);
        let mut out = Vec::new();
        header(
            (6 + key_field_len(key, 6) + body.len()) as u16,
            value_len as u16,
            1,
            key,
            &mut out,
        );
        out.extend_from_slice(&body);
        out
    }

    /// "major.minor[.patch[.build]]" → packed (MS, LS) DWORD pair.
    /// Missing or non-numeric parts read as zero; the strings stay the
    /// source of truth in Explorer's Details view either way.
    fn packed_version(version: &str) -> (u32, u32) {
        let mut parts = [0u16; 4];
        for (i, part) in version.split('.').take(4).enumerate() {
            parts[i] = part.trim().parse().unwrap_or(0);
        }
        let ms = ((parts[0] as u32) << 16) | parts[1] as u32;
        let ls = ((parts[2] as u32) << 16) | parts[3] as u32;
        (ms, ls)
    }

    /// The fixed 52-byte VS_FIXEDFILEINFO + the string table — the whole
    /// VS_VERSIONINFO block `UpdateResourceW` accepts under RT_VERSION.
    pub fn build(ident: &StampIdentity) -> Vec<u8> {
        let (file_ms, file_ls) = packed_version(&ident.version);
        let fixed: [u32; 13] = [
            0xfeef04bd,  // dwSignature
            0x0001_0000, // dwStrucVersion
            file_ms,
            file_ls,
            file_ms,
            file_ls,
            0x0000_003f, // dwFileFlagsMask
            0,           // dwFileFlags
            0x0004_0004, // dwOS = VOS_NT_WINDOWS32
            1,           // dwFileType = VFT_APP
            0,           // dwFileSubtype
            0,           // dwFileDateMS
            0,           // dwFileDateLS
        ];

        let strings: [(&str, &str); 7] = [
            ("CompanyName", &ident.company),
            ("FileDescription", &ident.description),
            ("FileVersion", &ident.version),
            ("InternalName", &ident.internal_name),
            ("OriginalFilename", &format!("{}.exe", ident.internal_name)),
            ("ProductName", &ident.description),
            ("ProductVersion", &ident.version),
        ];
        let mut blocks = Vec::new();
        for (key, value) in strings {
            blocks.extend_from_slice(&string_block(key, value));
        }
        let lang_key = format!("{LANG_ID:04x}{CHARSET_ID:04x}");
        let table_len = 6 + key_field_len(&lang_key, 6) + blocks.len();
        let mut table = Vec::new();
        header(table_len as u16, 0, 1, &lang_key, &mut table);
        table.extend_from_slice(&blocks);

        let sfi_len = 6 + key_field_len("StringFileInfo", 6) + table.len();
        let mut sfi = Vec::new();
        header(sfi_len as u16, 0, 1, "StringFileInfo", &mut sfi);
        sfi.extend_from_slice(&table);

        // Var: Translation 0409/04b0, so Explorer's language list agrees
        // with the table key above.
        let var_value: [u8; 4] = [
            LANG_ID.to_le_bytes()[0],
            LANG_ID.to_le_bytes()[1],
            CHARSET_ID.to_le_bytes()[0],
            CHARSET_ID.to_le_bytes()[1],
        ];
        let var_body_len = 6 + key_field_len("Translation", 6) + var_value.len();
        let mut var_body = Vec::new();
        header(
            var_body_len as u16,
            var_value.len() as u16,
            1,
            "Translation",
            &mut var_body,
        );
        var_body.extend_from_slice(&var_value);
        let vfi_len = 6 + key_field_len("VarFileInfo", 6) + var_body.len();
        let mut vfi = Vec::new();
        header(vfi_len as u16, 0, 1, "VarFileInfo", &mut vfi);
        vfi.extend_from_slice(&var_body);

        let total =
            6 + key_field_len("VS_VERSION_INFO", 6) + fixed.len() * 4 + sfi.len() + vfi.len();
        let mut out = Vec::with_capacity(total);
        header(total as u16, 52, 0, "VS_VERSION_INFO", &mut out);
        for word in fixed {
            out.extend_from_slice(&word.to_le_bytes());
        }
        out.extend_from_slice(&sfi);
        out.extend_from_slice(&vfi);
        out
    }

    /// Independent parse-back: walk the block hierarchy and collect every
    /// string value by its key. The tests feed it `build`'s output — a
    /// builder that disagrees with the format fails here, not in front of
    /// Explorer.
    pub fn parse_strings(block: &[u8]) -> Result<BTreeMap<String, String>, String> {
        let mut out = BTreeMap::new();
        // Top-level children live after the header + its padded key field
        // + the 52-byte VS_FIXEDFILEINFO, and run to the block's declared
        // total length.
        let total = u16_at(block, 0)? as usize;
        let children_at = 6 + key_field_len("VS_VERSION_INFO", 6) + 52;
        parse_children(block, children_at, total, &mut out)?;
        Ok(out)
    }

    fn u16_at(b: &[u8], pos: usize) -> Result<u16, String> {
        b.get(pos..pos + 2)
            .map(|s| u16::from_le_bytes(s.try_into().unwrap()))
            .ok_or_else(|| "truncated u16".to_string())
    }

    fn read_wstr(b: &[u8], pos: &mut usize) -> Result<String, String> {
        let mut units = Vec::new();
        while *pos + 2 <= b.len() {
            let unit = u16_at(b, *pos)?;
            *pos += 2;
            if unit == 0 {
                return Ok(String::from_utf16_lossy(&units));
            }
            units.push(unit);
        }
        Err("unterminated wide string".into())
    }

    fn parse_children(
        b: &[u8],
        mut pos: usize,
        end: usize,
        out: &mut BTreeMap<String, String>,
    ) -> Result<(), String> {
        while pos + 6 <= end {
            let total = u16_at(b, pos)? as usize;
            let value_len = u16_at(b, pos + 2)? as usize;
            if total < 6 || pos + total > end {
                return Err(format!("block at {pos}: bad length {total} (end {end})"));
            }
            let mut cur = pos + 6;
            let key = read_wstr(b, &mut cur)?;
            pad(&mut cur);
            if value_len == 0 {
                // A container block (StringFileInfo / StringTable /
                // VarFileInfo): recurse into its children.
                parse_children(b, cur, pos + total, out)?;
            } else if key == "Translation" {
                // binary value — nothing to collect
            } else {
                let value = read_wstr(b, &mut cur)?;
                out.insert(key, value);
            }
            pos += total;
        }
        Ok(())
    }

    fn pad(pos: &mut usize) {
        *pos = pos.div_ceil(4) * 4;
    }
}

// ── shared resolution (the shell build's single source of transforms) ──

use crate::config::{BackgroundSpec, ShunConfig, ThemeConfig, WallpaperSourceSpec};

/// Resolve the delivery config for a manifest, applying the named
/// variant and embedding every theme image as a data URL — the exact
/// transform the shell's build.rs has always run before serializing the
/// runtime config. Returns the config and the manifest's directory (all
/// document paths resolve against it).
pub fn resolved_config(
    manifest: &Path,
    variant: Option<&str>,
) -> Result<(ShunConfig, PathBuf), String> {
    let mut config = ShunConfig::from_any(manifest)
        .map_err(|e| format!("manifest {}: {e}", manifest.display()))?;
    if let Some(variant) = variant {
        config
            .apply_variant(variant)
            .map_err(|e| format!("variant {variant}: {e}"))?;
    }
    let manifest_dir = manifest
        .parent()
        .map(|d| d.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    embed_theme_images(&mut config, &manifest_dir)?;
    Ok((config, manifest_dir))
}

/// Serialize the wizard pipeline resolved without a locale.
pub fn steps_json(config: &ShunConfig, manifest_dir: &Path) -> Result<String, String> {
    let steps = config
        .resolve_steps(manifest_dir, None)
        .map_err(|e| format!("wizard pipeline: {e}"))?;
    serde_json::to_vec_pretty(&steps)
        .map(|v| String::from_utf8_lossy(&v).into_owned())
        .map_err(|e| format!("steps serialize: {e}"))
}

/// Serialize the license documents resolved per shell locale — the map
/// the wizard and the egui fallback read documents from.
pub fn license_docs_json(config: &ShunConfig, manifest_dir: &Path) -> Result<String, String> {
    let mut license_docs: BTreeMap<String, Vec<crate::config::ResolvedLicenseDoc>> =
        BTreeMap::new();
    for locale in SHELL_LOCALES {
        let locale_steps = config
            .resolve_steps(manifest_dir, Some(locale))
            .map_err(|e| format!("wizard pipeline ({locale}): {e}"))?;
        if let Some(license) = locale_steps
            .iter()
            .find(|step| step.kind == crate::config::StepKind::License)
        {
            license_docs.insert(locale.to_string(), license.licenses.clone());
        }
    }
    serde_json::to_vec_pretty(&license_docs)
        .map(|v| String::from_utf8_lossy(&v).into_owned())
        .map_err(|e| format!("license docs serialize: {e}"))
}

/// Pack the payload directory the (variant-applied) config declares.
pub fn payload_archive(config: &ShunConfig, manifest_dir: &Path) -> Result<Vec<u8>, String> {
    let payload_relative = config
        .payload
        .clone()
        .ok_or_else(|| "the manifest declares no payload directory".to_string())?;
    let payload_dir = manifest_dir.join(payload_relative);
    crate::payload::pack_directory(&payload_dir)
        .map_err(|e| format!("payload {}: {e}", payload_dir.display()))
}

/// The product logo bytes + format kind (empty bytes and "none" when the
/// manifest declares no logo). A logo DECLARED but unreadable is an
/// error — a silently missing title-bar logo is a broken theme, and the
/// wallpaper lane already fails loudly for the same reason.
pub fn logo_parts(config: &ShunConfig, manifest_dir: &Path) -> Result<(Vec<u8>, String), String> {
    let Some(logo) = config.product.logo.as_ref() else {
        return Ok((Vec::new(), "none".into()));
    };
    let bytes = std::fs::read(manifest_dir.join(logo))
        .map_err(|e| format!("logo {}: {e}", manifest_dir.join(logo).display()))?;
    if bytes.is_empty() {
        return Ok((Vec::new(), "none".into()));
    }
    let kind = Path::new(logo)
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_else(|| "none".into())
        .to_lowercase();
    Ok((bytes, kind))
}

/// Resolve every part a stamped shell embeds.
pub fn resolve_parts(
    manifest: &Path,
    variant: Option<&str>,
    flavor: Option<String>,
) -> Result<StampParts, String> {
    let (config, manifest_dir) = resolved_config(manifest, variant)?;
    let steps_json = steps_json(&config, &manifest_dir)?;
    let license_docs_json = license_docs_json(&config, &manifest_dir)?;
    let payload_archive = payload_archive(&config, &manifest_dir)?;
    let (logo_bytes, logo_kind) = logo_parts(&config, &manifest_dir)?;
    let log_name: String = config
        .product
        .name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    Ok(StampParts {
        config_json: serde_json::to_vec_pretty(&config)
            .map(|v| String::from_utf8_lossy(&v).into_owned())
            .map_err(|e| format!("config serialize: {e}"))?,
        steps_json,
        license_docs_json,
        logo_bytes,
        logo_kind,
        payload_archive,
        meta: StampMeta {
            shun_version: env!("CARGO_PKG_VERSION").to_string(),
            product_name: config.product.name.clone(),
            product_version: config.product.version.clone(),
            variant: variant.map(str::to_string),
            flavor: flavor.unwrap_or_default(),
            log_name,
        },
    })
}

/// Re-encode the product logo as a multi-size PNG-framed ICO (the shell
/// build and the stamp CLI share this). Letterboxed onto a square canvas
/// per size so taskbar slots never upscale.
#[cfg(feature = "cli")]
pub fn logo_ico_bytes(logo: &[u8]) -> Result<Vec<u8>, String> {
    use image::ExtendedColorType;
    use image::codecs::ico::{IcoEncoder, IcoFrame};
    use image::imageops::FilterType;

    let logo = image::load_from_memory(logo)
        .map_err(|e| format!("the product logo does not decode as an image: {e}"))?;
    let mut frames = Vec::new();
    for size in [16u32, 24, 32, 48, 64, 128, 256] {
        let square = square_rgba(&logo, size, FilterType::Lanczos3);
        frames.push(
            IcoFrame::as_png(square.as_raw(), size, size, ExtendedColorType::Rgba8)
                .map_err(|e| format!("icon frame {size}px: {e}"))?,
        );
    }
    let mut out = Vec::new();
    IcoEncoder::new(&mut out)
        .encode_images(&frames)
        .map_err(|e| format!("the logo ICO encodes: {e}"))?;
    Ok(out)
}

#[cfg(feature = "cli")]
fn square_rgba(
    logo: &image::DynamicImage,
    size: u32,
    filter: image::imageops::FilterType,
) -> image::RgbaImage {
    let scaled = if logo.width() >= logo.height() {
        logo.resize_exact(size, (size * logo.height() / logo.width()).max(1), filter)
    } else {
        logo.resize_exact((size * logo.width() / logo.height()).max(1), size, filter)
    };
    let mut canvas = image::RgbaImage::new(size, size);
    let x = (size - scaled.width()) / 2;
    let y = (size - scaled.height()) / 2;
    image::imageops::overlay(&mut canvas, &scaled, x.into(), y.into());
    canvas
}

/// Wallpaper/background images become data URLs so the runtime config
/// carries its pixels — a single-file installer serves no asset files.
/// Missing files are a build error: a declared wallpaper that silently
/// vanishes is a broken theme. (Moved verbatim from the shell's
/// build.rs, which now calls this.)
pub fn embed_theme_images(config: &mut ShunConfig, manifest_dir: &Path) -> Result<(), String> {
    use base64::Engine as _;
    let Some(theme) = config.shell.as_mut().and_then(|shell| shell.theme.as_mut()) else {
        return Ok(());
    };
    if let Some(wallpaper) = theme.wallpaper.as_mut() {
        for source in wallpaper.sources.iter_mut() {
            let (kind, path_ref) = match source {
                WallpaperSourceSpec::Video { video } => ("video", video),
                WallpaperSourceSpec::Image { image } => ("image", image),
                WallpaperSourceSpec::Pipeline { .. } => continue,
            };
            if path_ref.starts_with("https://") || path_ref.starts_with("http://") {
                continue;
            }
            let path = manifest_dir.join(path_ref.clone());
            let bytes = std::fs::read(&path)
                .map_err(|e| format!("wallpaper {kind} {}: {e}", path.display()))?;
            let mime = match path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str()
            {
                "mp4" | "m4v" => "video/mp4",
                "webm" => "video/webm",
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "webp" => "image/webp",
                "gif" => "image/gif",
                other => return Err(format!("wallpaper {kind} {other}: unsupported format")),
            };
            let url = format!(
                "data:{mime};base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            );
            println!("cargo:rerun-if-changed={}", path.display());
            *path_ref = url;
        }
    }
    let ThemeConfig {
        background,
        rail_background,
        pane_background,
        ..
    } = theme;
    for spec in [background, rail_background, pane_background]
        .into_iter()
        .flatten()
    {
        if let BackgroundSpec::Image { image } = spec {
            let path = manifest_dir.join(image.clone());
            let bytes = std::fs::read(&path)
                .map_err(|e| format!("theme wallpaper {}: {e}", path.display()))?;
            let mime = match path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str()
            {
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "webp" => "image/webp",
                "gif" => "image/gif",
                "bmp" => "image/bmp",
                other => return Err(format!("theme wallpaper {other}: unsupported format")),
            };
            let url = format!(
                "data:{mime};base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            );
            println!("cargo:rerun-if-changed={}", path.display());
            *spec = BackgroundSpec::DataUrl(url);
        }
    }
    Ok(())
}

// ── tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_trailer_roundtrips_through_parse() {
        let mut exe = b"MZ fake shell".to_vec();
        let t = Trailer::generic();
        exe.extend_from_slice(&t);
        let parsed = parse_trailer(&exe).expect("generic trailer parses");
        assert_eq!(parsed.offset, 0);
        assert_eq!(parsed.len, 0);
        verify_container(&parsed, &[]).expect("empty container verifies");
    }

    #[test]
    fn stamped_image_roundtrips() {
        // exe body + container + trailer must parse back and slice clean.
        let mut exe = b"MZ fake shell bytes".to_vec();
        let entries: Vec<(&str, &[u8])> = vec![
            (entry::CONFIG, br#"{ "product": { "name": "Demo" } }"#),
            (entry::PAYLOAD, &[7u8; 1500]), // forces 512-byte padding
        ];
        let container = pack_container(&entries).expect("pack");
        let offset = exe.len() as u64;
        let trailer = build_trailer(offset, &container);
        exe.extend_from_slice(&container);
        exe.extend_from_slice(&trailer);

        let parsed = parse_trailer(&exe).expect("trailer parses");
        let container = &exe[parsed.offset as usize..(parsed.offset + parsed.len) as usize];
        verify_container(&parsed, container).expect("sha verifies");
        let index = ContainerIndex::parse(container).expect("index parses");
        assert_eq!(
            index.data(container, entry::CONFIG).unwrap(),
            &br#"{ "product": { "name": "Demo" } }"#[..]
        );
        assert_eq!(index.data(container, entry::PAYLOAD).unwrap(), &[7u8; 1500]);
        assert!(index.data(container, entry::LOGO).is_none());
    }

    #[test]
    fn parse_rejects_non_shell() {
        let exe = b"MZ nothing here".to_vec();
        assert!(parse_trailer(&exe).is_err());
    }

    #[test]
    fn parse_rejects_unknown_rev() {
        let mut t = Trailer::generic();
        t[8..12].copy_from_slice(&99u32.to_le_bytes());
        let mut exe = b"MZ".to_vec();
        exe.extend_from_slice(&t);
        let err = parse_trailer(&exe).unwrap_err();
        assert!(err.contains("rev 99"), "{err}");
    }

    #[test]
    fn parse_rejects_container_not_abutting_trailer() {
        let container = pack_container(&[(entry::CONFIG, b"x")]).unwrap();
        let mut exe = b"MZ body".to_vec();
        let offset = exe.len() as u64;
        let mut trailer = build_trailer(offset, &container);
        exe.extend_from_slice(&container);
        // Corrupt: claim a longer container than what sits before the trailer.
        trailer[24..32].copy_from_slice(&((container.len() as u64) + 5).to_le_bytes());
        exe.extend_from_slice(&trailer);
        assert!(parse_trailer(&exe).is_err());
    }

    #[test]
    fn meta_json_roundtrips() {
        let meta = StampMeta {
            shun_version: "0.6.33".into(),
            product_name: "WoWSP".into(),
            product_version: "0.6.9".into(),
            variant: Some("webview2".into()),
            flavor: "webview2".into(),
            log_name: "WoWSP".into(),
        };
        let json = serde_json::to_vec(&meta).unwrap();
        let back: StampMeta = serde_json::from_slice(&json).unwrap();
        assert_eq!(back.product_name, "WoWSP");
        assert_eq!(back.variant.as_deref(), Some("webview2"));
    }

    #[test]
    fn version_info_builds_and_parses_back() {
        let ident = StampIdentity {
            company: "smoke".into(),
            description: "SmokeTest Installer".into(),
            internal_name: "smoketest-1.2.3".into(),
            version: "1.2.3".into(),
            ico: None,
        };
        let block = verinfo::build(&ident);
        let strings = verinfo::parse_strings(&block).expect("version block parses");
        assert_eq!(
            strings.get("ProductName").map(String::as_str),
            Some("SmokeTest Installer")
        );
        assert_eq!(
            strings.get("FileVersion").map(String::as_str),
            Some("1.2.3")
        );
        assert_eq!(
            strings.get("OriginalFilename").map(String::as_str),
            Some("smoketest-1.2.3.exe")
        );
        assert_eq!(
            strings.get("CompanyName").map(String::as_str),
            Some("smoke")
        );
        assert_eq!(
            strings.get("InternalName").map(String::as_str),
            Some("smoketest-1.2.3")
        );
    }
}
