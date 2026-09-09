//! Stamps `System.AppUserModel.ID` onto a `.lnk` through the Shell COM
//! property store — the grouping identity Windows uses for taskbar
//! grouping, jump lists, and *user-initiated* pinning (there is no
//! supported programmatic pinning; the AUMID is what makes the manual
//! one behave). `mslnk` writes raw bytes only, so the stamp rides on
//! top of its output: load the link through `IShellLink` +
//! `IPersistFile`, set the property, commit, save.

use std::path::Path;

use crate::error::ShunError;

fn err(step: &str, error: windows::core::Error) -> ShunError {
    ShunError::Config(format!("AUMID stamp {step}: {error}"))
}

/// Writes `aumid` into the shortcut's `System.AppUserModel.ID` property.
pub fn stamp(lnk: &Path, aumid: &str) -> Result<(), ShunError> {
    use std::mem::ManuallyDrop;

    use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
    use windows::Win32::System::Com::StructuredStorage::{
        PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0, PropVariantClear,
    };
    use windows::Win32::System::Com::{
        CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemAlloc,
        CoUninitialize, IPersistFile, STGM_READWRITE, STGM_SHARE_DENY_NONE,
    };
    use windows::Win32::System::Variant::VT_LPWSTR;
    use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    use windows::core::HSTRING;
    use windows::core::Interface;
    use windows::core::PWSTR;

    unsafe {
        // RPC_E_CHANGED_MODE (the thread already owns another apartment)
        // is fine: we proceed without owning COM on this thread.
        let owns_com = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();

        let result = (|| -> Result<(), ShunError> {
            let shell_link: IShellLinkW =
                CoCreateInstance(&ShellLink, None, CLSCTX_ALL).map_err(|e| err("cocreate", e))?;
            let persist: IPersistFile = shell_link.cast().map_err(|e| err("cast persist", e))?;
            let store: IPropertyStore = shell_link.cast().map_err(|e| err("cast store", e))?;

            let path = HSTRING::from(lnk.as_os_str());
            // Read-write: the shell property store inherits the load mode,
            // and a read-only load makes SetValue fail with
            // STG_E_ACCESSDENIED.
            persist
                .Load(&path, STGM_READWRITE | STGM_SHARE_DENY_NONE)
                .map_err(|e| err("load", e))?;

            // A VT_LPWSTR PROPVARIANT over a CoTaskMemAlloc'd wide copy of
            // the id; PropVariantClear gives the buffer back.
            let wide: Vec<u16> = aumid.encode_utf16().chain(Some(0)).collect();
            let buffer = CoTaskMemAlloc(wide.len() * 2);
            if buffer.is_null() {
                return Err(ShunError::Config(
                    "AUMID stamp: CoTaskMemAlloc failed".into(),
                ));
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr().cast(), buffer, wide.len() * 2);
            let mut value = PROPVARIANT {
                Anonymous: PROPVARIANT_0 {
                    Anonymous: ManuallyDrop::new(PROPVARIANT_0_0 {
                        vt: VT_LPWSTR,
                        wReserved1: 0,
                        wReserved2: 0,
                        wReserved3: 0,
                        Anonymous: PROPVARIANT_0_0_0 {
                            pwszVal: PWSTR(buffer.cast()),
                        },
                    }),
                },
            };

            store
                .SetValue(&PKEY_AppUserModel_ID, &value)
                .map_err(|e| err("setvalue", e))?;
            let cleared = PropVariantClear(&mut value);
            cleared.map_err(|e| err("propvariant clear", e))?;
            store.Commit().map_err(|e| err("commit", e))?;
            persist.Save(&path, true).map_err(|e| err("save", e))
        })();

        if owns_com {
            CoUninitialize();
        }
        result
    }
}
