//! Pluggable delivery targets.
//!
//! Every target consumes the same flow model ([`crate::flow`]) and payload
//! surface ([`crate::payload`]) but lands the payload differently:
//! [`install`] performs direct registration (Windows: ARP + shortcuts +
//! Explorer verbs; Linux: `.desktop` via [`freedesktop`]; macOS: `.app`
//! bundles via [`macos`]) or skips it for portable mode, [`flash`] writes
//! images to block devices.

pub mod flash;
pub mod freedesktop;
pub mod install;
pub mod macos;
pub mod plist;

/// Machine-scope elevation helpers (self-relaunch via runas).
pub mod elevate;

#[cfg(windows)]
pub mod aumid;

#[cfg(windows)]
pub mod shortcuts;

/// Non-Windows stand-in for [`shortcuts`]. The whole `.lnk` surface —
/// Start-menu entries, desktop links, AUMID stamps, shell notifications —
/// exists only on Windows (`mslnk`/`windows-sys` are
/// `cfg(windows)`-scoped dependencies), so on any other host a shortcut
/// choice simply applies nothing. Keep the signature in lockstep with the
/// real module: the wizard (`crate::wizard::apply_finish`) and the shells
/// call it unconditionally.
#[cfg(not(windows))]
pub mod shortcuts {
    /// Applies a set of shortcut choices — a no-op away from Windows.
    pub fn apply_shortcut_choices(
        _aumid: &str,
        _product: &str,
        _main_exe: &str,
        _desktop: Option<bool>,
        _menu: Option<bool>,
        _dir: &str,
    ) -> Result<(), String> {
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::apply_shortcut_choices;

        #[test]
        fn shortcut_choices_apply_nothing_away_from_windows() {
            // The install dir is not even checked here — there is nothing
            // to create, so there is nothing to fail on either.
            assert!(
                apply_shortcut_choices("", "Demo", "demo.exe", Some(true), Some(false), "").is_ok()
            );
        }
    }
}
