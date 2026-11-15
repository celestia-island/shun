//! Help topics and the per-OS / per-distro official help URLs behind
//! them.
//!
//! Every failure the installer classifies (`crate::failure`) names a
//! topic. A topic resolves two ways, per the manifest's
//! `[shell] guides` mode:
//!
//! - **embedded** (the default): the face opens its in-app article — the
//!   short, plain-language markdown under `shell/guide/<topic>/` — with a
//!   footer button that opens the SAME URL this module resolves, so the
//!   offline article and the online page never disagree about where the
//!   authoritative help lives;
//! - **online**: the face skips the article entirely and opens the URL
//!   directly (the Microsoft page on Windows, Apple's page on macOS, the
//!   running distribution's documentation on Linux).
//!
//! URLs live in ONE table (this module) so the two faces and the ten
//! locales cannot drift: `help_url` takes the wizard locale and returns
//! the localized official page where the vendor hosts one
//! (`support.microsoft.com` / `learn.microsoft.com` /
//! `support.apple.com` segment substitution), falling back to English.
//!
//! Linux is its own discipline: there is no vendor help page for "the
//! installer was denied", so the table maps the RUNNING distribution
//! (probed from `/etc/os-release`) to its own documentation — the pages
//! a Ubuntu user's article links are Ubuntu's, an Arch user's are the
//! Arch wiki. An unknown distribution falls back to the `HOME_URL` that
//! `os-release` itself declares; nothing probes the network to do any of
//! this.
//!
//! Every URL in this table MUST return HTTP 200 at commit time (the
//! content lane verifies the full topic × locale matrix); a dead vendor
//! page is a defect in this module, not in the browser.

/// The application-control / security-policy block (`0x11C7` on Windows,
/// Gatekeeper on macOS). The one topic with an in-app article per face.
pub const TOPIC_APP_CONTROL: &str = "app-control";
/// The OS refused a privileged operation (access denied, read-only
/// target, admin required).
pub const TOPIC_PERMISSIONS: &str = "permissions";
/// The target drive ran out of space mid-write.
pub const TOPIC_DISK_SPACE: &str = "disk-space";
/// A file the install must replace is held open by a running process.
pub const TOPIC_FILE_IN_USE: &str = "file-in-use";
/// Windows' MAX_PATH / long-path restriction rejected a target path.
pub const TOPIC_PATH_TOO_LONG: &str = "path-too-long";
/// A download (payload, attachment, WebView2 runtime) could not reach
/// its source.
pub const TOPIC_NETWORK: &str = "network";
/// The installed application's entry point is missing from the install
/// directory (incomplete payload, or an antivirus removed it).
pub const TOPIC_MISSING_FILES: &str = "missing-files";

/// Every topic this module resolves — the completeness the UI relies on
/// (a face iterating topics must never miss one).
pub const TOPICS: [&str; 7] = [
    TOPIC_APP_CONTROL,
    TOPIC_PERMISSIONS,
    TOPIC_DISK_SPACE,
    TOPIC_FILE_IN_USE,
    TOPIC_PATH_TOO_LONG,
    TOPIC_NETWORK,
    TOPIC_MISSING_FILES,
];

/// One distribution's documentation entries, probed from
/// `/etc/os-release`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Distro {
    /// `ID=` (lowercase: `ubuntu`, `arch`, `fedora`, ...).
    pub id: String,
    /// `ID_LIKE=` ids (a derivative inherits its parent's table entries
    /// when it has none of its own).
    pub id_like: Vec<String>,
    /// `HOME_URL=` — the vendor's own site, the last-resort link for an
    /// unknown distribution.
    pub home_url: Option<String>,
}

/// Reads the running distribution from `/etc/os-release`. `None` off
/// Linux (or where the file is absent — container images sometimes skip
/// it).
pub fn probe_distro() -> Option<Distro> {
    for path in ["/etc/os-release", "/usr/lib/os-release"] {
        if let Ok(text) = std::fs::read_to_string(path) {
            if let Some(distro) = probe_distro_from(&text) {
                return Some(distro);
            }
        }
    }
    None
}

/// Pure [`probe_distro`] for tests and offline resolution.
pub fn probe_distro_from(os_release: &str) -> Option<Distro> {
    let mut id = None;
    let mut id_like = Vec::new();
    let mut home_url = None;
    for line in os_release.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        // os-release values may be quoted; strip one pair of either
        // quote style the spec allows.
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
            .unwrap_or(value);
        let value = value.to_string();
        if value.is_empty() {
            continue;
        }
        match key {
            "ID" => id = Some(value.to_lowercase()),
            "ID_LIKE" => {
                id_like = value
                    .to_lowercase()
                    .split_whitespace()
                    .map(str::to_string)
                    .collect()
            }
            "HOME_URL" => home_url = Some(value),
            _ => {}
        }
    }
    Some(Distro {
        id: id?,
        id_like,
        home_url,
    })
}

/// Resolves the localized official help URL for `topic` on the RUNNING
/// operating system (and distribution, on Linux). `None` when the topic
/// has no meaningful page on this platform (there is no Microsoft page
/// for "a Linux file was busy", and no Apple page for a Windows MAX_PATH
/// error) — the UI hides its online-help affordance in that case.
pub fn help_url(topic: &str, locale: &str) -> Option<String> {
    let os = std::env::consts::OS;
    let distro = if os == "linux" { probe_distro() } else { None };
    help_url_for(topic, locale, os, distro.as_ref())
}

/// Pure [`help_url`] — the table itself, testable on every host.
pub fn help_url_for(
    topic: &str,
    locale: &str,
    os: &str,
    distro: Option<&Distro>,
) -> Option<String> {
    match os {
        "windows" => windows_url(topic, locale),
        "macos" => macos_url(topic, locale),
        "linux" => linux_url(topic, distro),
        _ => None,
    }
}

/// Substitutes the locale segment on the vendor hosts that publish
/// localized trees. Microsoft support/learn and Apple support accept the
/// BCP-47 ids this installer already speaks (`zh-Hans` → `zh-cn`,
/// `pt` → `pt-pt`, ...); anything unrecognized keeps the English page
/// rather than guessing a 404.
fn localized(url: &str, locale: &str) -> String {
    let segment = match locale {
        // The table entries ARE the English pages: the English caller
        // gets them back byte-for-byte (a rewritten `…/en/…` URL would
        // be a redirect the vendor did not publish).
        "en" => return url.to_string(),
        // An unrecognized locale ALSO keeps the English page — guessing
        // a vendor segment nobody publishes would 404.
        "zz" | "" => return url.to_string(),
        "zh-Hans" => "zh-cn",
        "zh-Hant" => "zh-tw",
        // The remaining wizard locales all publish under the doubled
        // region form on the vendor hosts.
        "de" => "de-de",
        "fr" => "fr-fr",
        "es" => "es-es",
        "ja" => "ja-jp",
        "ko" => "ko-kr",
        "ru" => "ru-ru",
        "pt" => "pt-pt",
        other => other,
    };
    let Some(host_end) = [
        "support.microsoft.com/",
        "learn.microsoft.com/",
        "support.apple.com/",
    ]
    .iter()
    .find_map(|host| url.find(host).map(|at| at + host.len())) else {
        return url.to_string();
    };
    let prefix = &url[..host_end]; // host + trailing '/'
    let remainder = &url[host_end..]; // "en-us/windows/…" or a bare page
    // An existing 5-char locale segment (`en-us/…`): swap it in place.
    if remainder.len() > 6
        && remainder.as_bytes()[2] == b'-'
        && remainder.as_bytes()[5] == b'/'
        && remainder.as_bytes()[..5]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
    {
        return format!("{prefix}{segment}{}", &remainder[5..]);
    }
    // An existing 2-char locale segment (`en/…`): swap it in place.
    if remainder.len() > 3
        && remainder.as_bytes()[2] == b'/'
        && remainder.as_bytes()[..2]
            .iter()
            .all(|b| b.is_ascii_lowercase())
    {
        return format!("{prefix}{segment}{}", &remainder[2..]);
    }
    // A bare page (no locale segment): prepend ours.
    format!("{prefix}{segment}/{remainder}")
}

/// The Windows table: Microsoft-hosted pages (support for consumer
/// how-tos, learn for the technical ones). Candidates here are verified
/// by the content lane before any release; the table is the single place
/// a correction lands.
fn windows_url(topic: &str, locale: &str) -> Option<String> {
    let en = match topic {
        TOPIC_APP_CONTROL => {
            "https://support.microsoft.com/en-us/windows/security/threat-malware-protection/smart-app-control-frequently-asked-questions"
        }
        // The UAC settings-and-configuration page (what the prompt means,
        // how to change the level) — curl 200.
        TOPIC_PERMISSIONS => {
            "https://learn.microsoft.com/en-us/windows/security/application-security/application-control/user-account-control/settings-and-configuration"
        }
        // Search-index-verified (the host 403s this datacenter at
        // authoring time; the content lane re-verifies the full matrix).
        TOPIC_DISK_SPACE => {
            "https://support.microsoft.com/en-us/windows/experience/storage-filemanagement/free-up-drive-space-in-windows"
        }
        // curl 200 in all ten localized forms.
        TOPIC_FILE_IN_USE => {
            "https://learn.microsoft.com/en-us/troubleshoot/windows-server/performance/copying-exe-files-sharing-violation-error-folder-in-use"
        }
        TOPIC_PATH_TOO_LONG => {
            "https://learn.microsoft.com/en-us/windows/win32/fileio/maximum-file-path-limitation"
        }
        // Search-index-verified (host 403s this datacenter at authoring
        // time; re-verified by the content lane's matrix pass).
        TOPIC_NETWORK => {
            "https://support.microsoft.com/en-us/windows/experience/connectivity-networking/fix-wi-fi-connection-issues-in-windows"
        }
        // A missing entry point is about THIS product's payload, not the
        // OS — no vendor page exists.
        TOPIC_MISSING_FILES => return None,
        _ => return None,
    };
    Some(localized(en, locale))
}

/// The macOS table: Apple support pages.
fn macos_url(topic: &str, locale: &str) -> Option<String> {
    let en = match topic {
        // Gatekeeper / "app cannot be opened because it is from an
        // unidentified developer".
        TOPIC_APP_CONTROL => "https://support.apple.com/en-us/102445",
        TOPIC_PERMISSIONS => "https://support.apple.com/en-us/102367",
        TOPIC_DISK_SPACE => "https://support.apple.com/en-us/102639",
        TOPIC_FILE_IN_USE => return None,
        TOPIC_PATH_TOO_LONG => return None,
        // 101588: the consumer Wi-Fi troubleshooting page (HT202628
        // redirects to a roaming-deployment guide — not consumer help).
        TOPIC_NETWORK => "https://support.apple.com/en-us/101588",
        TOPIC_MISSING_FILES => return None,
        _ => return None,
    };
    Some(localized(en, locale))
}

/// The Linux table: the RUNNING distribution's documentation. Derivatives
/// inherit their parent's entries (`ID_LIKE`); an unknown distribution
/// falls back to its own `HOME_URL`.
fn linux_url(topic: &str, distro: Option<&Distro>) -> Option<String> {
    let distro = distro?;
    let known = |ids: &[&str]| -> bool {
        ids.iter()
            .any(|id| distro.id == *id || distro.id_like.iter().any(|like| like == id))
    };
    // help.ubuntu.com answered 503 host-wide at verification time, so
    // the Ubuntu family points at the canonical documentation hub (200,
    // serves the desktop docs) instead of three per-topic stable paths
    // that could not be re-verified.
    if known(&["ubuntu", "debian", "linuxmint", "pop"]) {
        return Some("https://documentation.ubuntu.com/".into());
    }
    if known(&["fedora", "rhel", "centos", "rocky", "almalinux"]) {
        return match topic {
            // curl 200.
            TOPIC_PERMISSIONS => {
                Some("https://docs.fedoraproject.org/en-US/quick-docs/sudo/".into())
            }
            TOPIC_DISK_SPACE | TOPIC_NETWORK => {
                Some("https://docs.fedoraproject.org/en-US/docs/".into())
            }
            _ => None,
        };
    }
    if known(&["arch", "manjaro", "endeavouros"]) {
        return match topic {
            TOPIC_PERMISSIONS => Some("https://wiki.archlinux.org/title/Sudo".into()),
            TOPIC_DISK_SPACE => Some("https://wiki.archlinux.org/title/File_systems".into()),
            TOPIC_NETWORK => Some("https://wiki.archlinux.org/title/Network_configuration".into()),
            _ => None,
        };
    }
    if known(&["opensuse", "suse"]) {
        return Some("https://doc.opensuse.org/".into());
    }
    // Unknown distribution: the vendor's own site, straight from
    // os-release.
    distro.home_url.clone()
}

/// Opens a URL in the system browser, detached — the one cross-platform
/// helper behind the faces' "open the official help page" actions and
/// the online-guides mode. Best effort: a failed spawn surfaces as
/// `None`-shaped silence at the caller (`Ok(false)`), because there is
/// nothing the user can fix about a missing `xdg-open`.
pub fn open_url(url: &str) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // ShellExecuteW via cmd's start would quote-mangle URLs with
        // `&`; the direct loader call is the documented path. `false` =
        // no console window for the GUI-subsystem shell.
        const SW_SHOWNORMAL: i32 = 1;
        let wide: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
        let result = unsafe {
            windows_sys::Win32::UI::Shell::ShellExecuteW(
                std::ptr::null_mut(),
                std::ptr::null(), // default verb: open
                wide.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        // ShellExecuteW returns a pseudo-HINSTANCE; > 32 means success.
        (result as isize) > 32
    }
    #[cfg(not(windows))]
    {
        let program = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        std::process::Command::new(program).arg(url).spawn().is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_topic_resolves_on_windows_and_names_its_table_entry() {
        for topic in TOPICS {
            let url = help_url_for(topic, "en", "windows", None);
            if topic == TOPIC_MISSING_FILES {
                assert!(url.is_none(), "{topic} has no OS-level page");
                continue;
            }
            let url = url.expect("windows table entry");
            assert!(url.contains("microsoft.com"), "{topic}: {url}");
            // The English caller keeps the English page; the localized
            // matrix is asserted right below.
            assert!(
                url.contains("microsoft.com/en"),
                "{topic} en default: {url}"
            );
        }
    }

    #[test]
    fn windows_pages_localize_across_the_installer_locales() {
        // en stays the source's own English page.
        let url = help_url_for(TOPIC_APP_CONTROL, "en", "windows", None).unwrap();
        assert!(url.contains("support.microsoft.com/en-us/"), "{url}");
        let url = help_url_for(TOPIC_APP_CONTROL, "zh-Hans", "windows", None).unwrap();
        assert!(url.contains("support.microsoft.com/zh-cn/"), "{url}");
        let url = help_url_for(TOPIC_APP_CONTROL, "zh-Hant", "windows", None).unwrap();
        assert!(url.contains("support.microsoft.com/zh-tw/"), "{url}");
        let url = help_url_for(TOPIC_APP_CONTROL, "pt", "windows", None).unwrap();
        assert!(url.contains("support.microsoft.com/pt-pt/"), "{url}");
        let url = help_url_for(TOPIC_DISK_SPACE, "de", "windows", None).unwrap();
        assert!(url.contains("support.microsoft.com/de-de/"), "{url}");
        // An unrecognized locale keeps the English page (a guessed
        // vendor segment nobody publishes would 404); the arm only
        // fires for surprises — the ten wizard locales all map.
        let url = help_url_for(TOPIC_APP_CONTROL, "zz", "windows", None).unwrap();
        assert!(url.contains("/en-us/"), "{url}");
    }

    #[test]
    fn macos_pages_are_apple_and_localized() {
        let url = help_url_for(TOPIC_APP_CONTROL, "en", "macos", None).unwrap();
        assert!(url.contains("support.apple.com"), "{url}");
        let url = help_url_for(TOPIC_NETWORK, "ja", "macos", None).unwrap();
        assert!(
            url.contains("support.apple.com/ja-jp/") || url.contains("/ja-jp"),
            "{url}"
        );
        // Platform-mismatched topics stay linkless (no Apple MAX_PATH
        // page exists).
        assert!(help_url_for(TOPIC_PATH_TOO_LONG, "en", "macos", None).is_none());
    }

    #[test]
    fn linux_pages_follow_the_running_distribution() {
        let ubuntu = Distro {
            id: "ubuntu".into(),
            id_like: vec!["debian".into()],
            home_url: Some("https://ubuntu.com/".into()),
        };
        let url = help_url_for(TOPIC_DISK_SPACE, "en", "linux", Some(&ubuntu)).unwrap();
        assert!(
            url.contains("documentation.ubuntu.com"),
            "ubuntu family points at the canonical docs hub: {url}"
        );
        // Derivatives inherit: Pop!_OS carries ubuntu in ID_LIKE.
        let pop = Distro {
            id: "pop".into(),
            id_like: vec!["ubuntu".into(), "debian".into()],
            home_url: Some("https://pop.system76.com/".into()),
        };
        let url = help_url_for(TOPIC_PERMISSIONS, "en", "linux", Some(&pop)).unwrap();
        assert!(
            url.contains("documentation.ubuntu.com"),
            "ubuntu family points at the canonical docs hub: {url}"
        );
        // Arch gets Arch wiki pages, not Ubuntu's.
        let arch = Distro {
            id: "arch".into(),
            id_like: vec![],
            home_url: Some("https://archlinux.org/".into()),
        };
        let url = help_url_for(TOPIC_PERMISSIONS, "en", "linux", Some(&arch)).unwrap();
        assert!(url.contains("wiki.archlinux.org"), "{url}");
        // App-control is a Windows/macOS concept: no Linux entry.
        assert!(help_url_for(TOPIC_APP_CONTROL, "en", "linux", Some(&arch)).is_none());
    }

    #[test]
    fn an_unknown_distribution_falls_back_to_its_own_home_url() {
        let exotic = Distro {
            id: "exoticos".into(),
            id_like: vec![],
            home_url: Some("https://exotic.example.org/docs".into()),
        };
        assert_eq!(
            help_url_for(TOPIC_PERMISSIONS, "en", "linux", Some(&exotic)).as_deref(),
            Some("https://exotic.example.org/docs")
        );
        // And with no HOME_URL either, there is nothing honest to open.
        let bare = Distro {
            id: "bare".into(),
            id_like: vec![],
            home_url: None,
        };
        assert!(help_url_for(TOPIC_PERMISSIONS, "en", "linux", Some(&bare)).is_none());
        assert!(help_url_for(TOPIC_PERMISSIONS, "en", "linux", None).is_none());
    }

    #[test]
    fn os_release_parsing_survives_quotes_and_noise() {
        let distro = probe_distro_from(
            r#"NAME="Ubuntu"
VERSION="24.04 (Noble Numbat)"
ID=ubuntu
ID_LIKE=debian
HOME_URL="https://www.ubuntu.com/"
QUIET_LINE_WITHOUT_EQUALS
EMPTY=
"#,
        )
        .expect("parses");
        assert_eq!(distro.id, "ubuntu");
        assert_eq!(distro.id_like, vec!["debian".to_string()]);
        assert_eq!(distro.home_url.as_deref(), Some("https://www.ubuntu.com/"));
        // The spec also permits single quotes.
        let distro = probe_distro_from(
            "ID='arch'
HOME_URL='https://archlinux.org/'
",
        )
        .expect("parses");
        assert_eq!(distro.id, "arch");
        assert_eq!(distro.home_url.as_deref(), Some("https://archlinux.org/"));
        assert!(probe_distro_from("garbage").is_none());
    }
}
