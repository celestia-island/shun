//! Windows identity patching for stamped shells: the version resource
//! and the icon go in through the OS's own PE resource editor
//! (`BeginUpdateResourceW` / `UpdateResourceW` / `EndUpdateResourceW`),
//! so a stamped exe presents the product in Explorer, the taskbar and
//! UAC without recompiling the shell.
//!
//! Every pre-existing entry of the touched resource types is enumerated
//! on the file first (as a datafile module, no loading) and deleted
//! explicitly — resources are keyed by (type, name, language), and the
//! compile-time stamps may carry a language id we do not control. The
//! replacement version block is built by [`super::verinfo::build`], whose
//! byte layout is parse-tested on every host.

use std::os::windows::ffi::OsStrExt as _;
use std::path::Path;

use windows_sys::Win32::Foundation::{BOOL, FreeLibrary, GetLastError, HMODULE};
use windows_sys::Win32::System::LibraryLoader::{
    BeginUpdateResourceW, ENUMRESLANGPROCW, ENUMRESNAMEPROCW, EndUpdateResourceW,
    EnumResourceLanguagesW, EnumResourceNamesW, LOAD_LIBRARY_AS_DATAFILE, LoadLibraryExW,
    UpdateResourceW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{RT_GROUP_ICON, RT_ICON, RT_VERSION};
use windows_sys::core::PCWSTR;

use super::StampIdentity;
use super::verinfo;

/// US-English — the language id the compile-time stamps and the built
/// version block share.
const LANG_US: u16 = 0x0409;

/// Resource-not-found errors from the enumeration APIs: a shell without
/// an icon or version resource yet is fine, not a failure. (1812
/// ERROR_RESOURCE_TYPE_NOT_FOUND, 1814 ERROR_RESOURCE_NAME_NOT_FOUND,
/// 1815 ERROR_RESOURCE_LANG_NOT_FOUND — matched numerically to avoid a
/// fourth import surface for three constants used once.)
const ERR_RES_NOT_FOUND: [u32; 3] = [1812, 1814, 1815];

/// A resource name: the integer id (MAKEINTRESOURCE) or a wide string.
/// Which one matters — `UpdateResourceW` must be handed the same shape
/// it is asked to delete.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ResName {
    Id(u16),
    Wide(Vec<u16>),
}

impl ResName {
    /// Classify an incoming name pointer the way IS_INTRESOURCE does.
    ///
    /// # Safety
    /// `p` must be a valid resource name pointer as delivered by the
    /// enumeration callbacks: an integer id below 0x10000, or a pointer
    /// to a NUL-terminated wide string.
    unsafe fn from_pc(p: PCWSTR) -> Self {
        let v = p as usize;
        if v < 0x1_0000 {
            Self::Id(v as u16)
        } else {
            unsafe {
                let mut len = 0usize;
                while *p.add(len) != 0 {
                    len += 1;
                }
                Self::Wide(
                    String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
                        .encode_utf16()
                        .collect(),
                )
            }
        }
    }

    fn as_pc(&self) -> PCWSTR {
        match self {
            Self::Id(v) => *v as usize as PCWSTR,
            Self::Wide(w) => w.as_ptr(),
        }
    }
}

/// Patch the exe at `path` in place: replace the version resource and
/// (when the identity carries one) the icon group with the product's.
pub(super) fn patch_identity(path: &Path, identity: &StampIdentity) -> Result<(), String> {
    unsafe {
        // 1. Enumerate the existing entries as a datafile module — no
        //    initialization, no relocation, pure file access.
        let module = load_datafile_module(path)?;
        let mut old_version: Vec<(ResName, u16)> = Vec::new();
        let mut old_groups: Vec<(ResName, u16)> = Vec::new();
        let mut old_icons: Vec<(ResName, u16)> = Vec::new();
        let _ = collect_res(module, RT_VERSION, &mut old_version);
        let _ = collect_res(module, RT_GROUP_ICON, &mut old_groups);
        let _ = collect_res(module, RT_ICON, &mut old_icons);
        FreeLibrary(module);

        // 2. Begin the update transaction on the plain exe (BEFORE any
        //    overlay is appended — the editor rewrites the PE).
        let wide = wide_path(path);
        let handle = BeginUpdateResourceW(wide.as_ptr(), 0);
        if handle.is_null() {
            return Err(format!(
                "BeginUpdateResourceW failed (GetLastError={})",
                GetLastError()
            ));
        }

        let outcome: Result<(), String> = (|| {
            // Delete every enumerated entry explicitly: replacements are
            // adds, and a same-name entry under a different language id
            // would survive and present a second, stale identity block.
            for (rtype, list) in [
                (RT_VERSION, &old_version),
                (RT_GROUP_ICON, &old_groups),
                (RT_ICON, &old_icons),
            ] {
                for (name, lang) in list {
                    if UpdateResourceW(handle, rtype, name.as_pc(), *lang, std::ptr::null(), 0) == 0
                    {
                        return Err(format!(
                            "UpdateResourceW delete failed (GetLastError={})",
                            GetLastError()
                        ));
                    }
                }
            }

            // Fresh version resource under name 1.
            let ver = verinfo::build(identity);
            if UpdateResourceW(
                handle,
                RT_VERSION,
                1usize as PCWSTR,
                LANG_US,
                ver.as_ptr().cast(),
                ver.len() as u32,
            ) == 0
            {
                return Err(format!(
                    "UpdateResourceW version failed (GetLastError={})",
                    GetLastError()
                ));
            }

            // Icon group: new RT_ICON entries at ids 1..=n, then the
            // group directory referencing them.
            if let Some(ico) = &identity.ico {
                let images = parse_ico(ico)?;
                for (idx, image) in images.iter().enumerate() {
                    let id = (idx + 1) as PCWSTR;
                    if UpdateResourceW(
                        handle,
                        RT_ICON,
                        id,
                        LANG_US,
                        image.data.as_ptr().cast(),
                        image.data.len() as u32,
                    ) == 0
                    {
                        return Err(format!(
                            "UpdateResourceW icon {} failed (GetLastError={})",
                            idx + 1,
                            GetLastError()
                        ));
                    }
                }
                let group = build_group(&images);
                if UpdateResourceW(
                    handle,
                    RT_GROUP_ICON,
                    1usize as PCWSTR,
                    LANG_US,
                    group.as_ptr().cast(),
                    group.len() as u32,
                ) == 0
                {
                    return Err(format!(
                        "UpdateResourceW icon group failed (GetLastError={})",
                        GetLastError()
                    ));
                }
            }
            Ok(())
        })();

        if let Err(e) = outcome {
            EndUpdateResourceW(handle, 1); // discard
            return Err(e);
        }
        if EndUpdateResourceW(handle, 0) == 0 {
            return Err(format!(
                "EndUpdateResourceW failed (GetLastError={})",
                GetLastError()
            ));
        }
    }
    Ok(())
}

unsafe fn load_datafile_module(path: &Path) -> Result<HMODULE, String> {
    unsafe {
        let wide = wide_path(path);
        let module = LoadLibraryExW(
            wide.as_ptr(),
            std::ptr::null_mut(),
            LOAD_LIBRARY_AS_DATAFILE,
        );
        if module.is_null() {
            return Err(format!(
                "LoadLibraryExW(datafile) failed (GetLastError={})",
                GetLastError()
            ));
        }
        Ok(module)
    }
}

/// Collect every (name, language) pair of one resource type.
unsafe fn collect_res(
    module: HMODULE,
    rtype: PCWSTR,
    out: &mut Vec<(ResName, u16)>,
) -> Result<(), String> {
    unsafe {
        let names: *mut Vec<ResName> = Box::into_raw(Box::new(Vec::new()));
        let names_cb: ENUMRESNAMEPROCW = Some(tramp_names);
        let found_names = EnumResourceNamesW(module, rtype, names_cb, names as isize) != 0;
        let names_list = Box::from_raw(names);
        if !found_names {
            let err = GetLastError();
            if !ERR_RES_NOT_FOUND.contains(&err) {
                return Err(format!("EnumResourceNamesW failed (GetLastError={err})"));
            }
            return Ok(()); // the type simply is not present
        }
        let langs_cb: ENUMRESLANGPROCW = Some(tramp_langs);
        for name in names_list.iter() {
            let langs: *mut Vec<u16> = Box::into_raw(Box::new(Vec::new()));
            let found_langs =
                EnumResourceLanguagesW(module, rtype, name.as_pc(), langs_cb, langs as isize) != 0;
            let langs_list = Box::from_raw(langs);
            if !found_langs {
                let err = GetLastError();
                if !ERR_RES_NOT_FOUND.contains(&err) {
                    return Err(format!(
                        "EnumResourceLanguagesW failed (GetLastError={err})"
                    ));
                }
                continue;
            }
            for lang in langs_list.iter() {
                out.push((name.clone(), *lang));
            }
        }
        Ok(())
    }
}

unsafe extern "system" fn tramp_names(
    _module: HMODULE,
    _rtype: PCWSTR,
    name: PCWSTR,
    lparam: isize,
) -> BOOL {
    unsafe {
        let list = &mut *(lparam as *mut Vec<ResName>);
        list.push(ResName::from_pc(name));
    }
    1
}

unsafe extern "system" fn tramp_langs(
    _module: HMODULE,
    _rtype: PCWSTR,
    _name: PCWSTR,
    lang: u16,
    lparam: isize,
) -> BOOL {
    unsafe {
        let list = &mut *(lparam as *mut Vec<u16>);
        list.push(lang);
    }
    1
}

fn wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

// ── icon plumbing ───────────────────────────────────────────────────────

/// One image inside a multi-size .ico file, with the directory fields
/// the group entry must mirror.
struct IconImage {
    width: u8,
    height: u8,
    colors: u8,
    planes: u16,
    bitcount: u16,
    data: Vec<u8>,
}

/// Split a multi-size .ico (what `stamp::logo_ico_bytes` encodes) into
/// its images.
fn parse_ico(bytes: &[u8]) -> Result<Vec<IconImage>, String> {
    if bytes.len() < 6 {
        return Err("icon file shorter than an ICONDIR".into());
    }
    let kind = u16_le(bytes, 2);
    if kind != 1 {
        return Err(format!("icon file is not an icon (type {kind})"));
    }
    let count = u16_le(bytes, 4) as usize;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let base = 6 + i * 16;
        if base + 16 > bytes.len() {
            return Err(format!("icon directory entry {i} truncated"));
        }
        let data_len = u32_le(bytes, base + 8) as usize;
        let offset = u32_le(bytes, base + 12) as usize;
        let end = offset + data_len;
        if end > bytes.len() {
            return Err(format!(
                "icon image {i} runs past the file ({end} > {})",
                bytes.len()
            ));
        }
        out.push(IconImage {
            width: bytes[base],
            height: bytes[base + 1],
            colors: bytes[base + 2],
            planes: u16_le(bytes, base + 4),
            bitcount: u16_le(bytes, base + 6),
            data: bytes[offset..end].to_vec(),
        });
    }
    Ok(out)
}

/// The GRPICONENTRY directory tying the new RT_ICON ids together.
fn build_group(images: &[IconImage]) -> Vec<u8> {
    let mut out = Vec::with_capacity(6 + images.len() * 14);
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved
    out.extend_from_slice(&1u16.to_le_bytes()); // type = icon
    out.extend_from_slice(&(images.len() as u16).to_le_bytes());
    for (idx, image) in images.iter().enumerate() {
        out.push(image.width);
        out.push(image.height);
        out.push(image.colors);
        out.push(0); // reserved
        out.extend_from_slice(&image.planes.to_le_bytes());
        out.extend_from_slice(&image.bitcount.to_le_bytes());
        out.extend_from_slice(&(image.data.len() as u32).to_le_bytes());
        out.extend_from_slice(&((idx + 1) as u16).to_le_bytes());
    }
    out
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

    /// A two-image ico, hand-assembled the way IcoEncoder lays it out.
    fn sample_ico() -> Vec<u8> {
        let image_a = [0xa5u8; 40];
        let image_b = [0x5au8; 300];
        let mut out = Vec::new();
        out.extend_from_slice(&0u16.to_le_bytes()); // reserved
        out.extend_from_slice(&1u16.to_le_bytes()); // type icon
        out.extend_from_slice(&2u16.to_le_bytes()); // count
        // entry A: 16x16, offset 6 + 2*16
        out.push(16);
        out.push(16);
        out.push(8);
        out.push(0);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(image_a.len() as u32).to_le_bytes());
        out.extend_from_slice(&38u32.to_le_bytes());
        // entry B: 0x00 width = 256
        out.push(0);
        out.push(0);
        out.push(0);
        out.push(0);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(image_b.len() as u32).to_le_bytes());
        out.extend_from_slice(&(38 + image_a.len() as u32).to_le_bytes());
        out.extend_from_slice(&image_a);
        out.extend_from_slice(&image_b);
        out
    }

    #[test]
    fn ico_parses_into_images_with_directory_fields() {
        let images = parse_ico(&sample_ico()).expect("sample ico parses");
        assert_eq!(images.len(), 2);
        assert_eq!(images[0].width, 16);
        assert_eq!(images[0].colors, 8);
        assert_eq!(images[0].data, vec![0xa5u8; 40]);
        assert_eq!(images[1].width, 0); // 256 encodes as zero
        assert_eq!(images[1].data, vec![0x5au8; 300]);
    }

    #[test]
    fn group_directory_mirrors_images() {
        let images = parse_ico(&sample_ico()).unwrap();
        let group = build_group(&images);
        assert_eq!(u16_le(&group, 2), 1);
        assert_eq!(u16_le(&group, 4), 2);
        // first entry: width 16, id 1, 14-byte stride after the 6-byte header
        assert_eq!(group[6], 16);
        let id = u16_le(&group, 6 + 12);
        assert_eq!(id, 1);
        let len = u32_le(&group, 6 + 8);
        assert_eq!(len, 40);
        // second entry: id 2
        let id_b = u16_le(&group, 6 + 14 + 12);
        assert_eq!(id_b, 2);
    }

    #[test]
    fn truncated_ico_is_an_error() {
        let mut bad = sample_ico();
        bad.truncate(30);
        assert!(parse_ico(&bad).is_err());
    }

    #[test]
    fn int_and_string_names_classify() {
        let as_int = 7usize as PCWSTR;
        assert_eq!(unsafe { ResName::from_pc(as_int) }, ResName::Id(7));
        let wide: Vec<u16> = "APP\0".encode_utf16().collect();
        let as_str = wide.as_ptr();
        assert_eq!(
            unsafe { ResName::from_pc(as_str) },
            ResName::Wide("APP".encode_utf16().collect())
        );
    }
}
