//! Install target: NSIS-like registration (ARP, uninstaller, shortcuts,
//! deep links) and a portable mode that writes no registry at all.
//!
//! The Windows backend follows the wowsp NSIS template as its executable
//! spec: HKCU ARP entry, a self-copying `uninstall.exe`, and a start-menu
//! shortcut. Portable mode writes a `.shun-portable` marker and skips all
//! registration — the same convention wowsp apps detect for data placement.

use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::error::ShunError;
use crate::flow::{Flow, FlowEvent};
use crate::payload::{MANIFEST_PATH, PayloadEntry, PayloadSource};

/// Marker file enabling portable (便捷) mode for a delivered copy.
pub const PORTABLE_MARKER: &str = ".shun-portable";

/// Name of the uninstaller binary copied into the install directory.
pub const UNINSTALLER_NAME: &str = "uninstall.exe";

/// Where and how an install-mode delivery lands on this machine.
#[derive(Debug, Clone, PartialEq)]
pub struct InstallContext {
    /// Product name (shortcut titles, ARP display name, registry key).
    pub product: String,

    /// Product version (ARP display version).
    pub version: String,

    /// Publisher shown in ARP (e.g. `celestia-island`).
    pub publisher: Option<String>,

    /// Final directory of the installed payload.
    pub install_dir: PathBuf,

    /// Payload-relative path of the app entry point the shortcut targets.
    pub main_exe: Option<PathBuf>,

    /// Portable mode: skip registry, shortcuts, and uninstaller entirely;
    /// all data stays beside the executable.
    pub portable: bool,

    /// ARP `EstimatedSize` in KiB; computed by the flow from the manifest.
    pub estimated_size_kb: u32,
}

/// The install delivery flow: extract the payload with progress, persist
/// the on-disk manifest (consumed by uninstall), then either register
/// (local mode) or drop the portable marker (portable mode).
pub struct InstallFlow<'a> {
    /// Payload to deliver.
    pub payload: &'a dyn PayloadSource,

    /// Per-OS registration backend.
    pub registration: &'a dyn Registration,

    /// Where and how the payload lands.
    pub ctx: InstallContext,
}

impl Flow for InstallFlow<'_> {
    fn run(&self, on_event: &mut dyn FnMut(FlowEvent)) -> Result<(), ShunError> {
        on_event(FlowEvent::Started);
        self.payload.extract(&self.ctx.install_dir, on_event)?;

        let manifest = serde_json::to_vec_pretty(self.payload.manifest())
            .map_err(|e| ShunError::Config(format!("manifest serialize: {e}")))?;
        std::fs::write(self.ctx.install_dir.join(MANIFEST_PATH), manifest)?;

        let mut ctx = self.ctx.clone();
        let total_bytes = PayloadEntry::total_bytes(self.payload.manifest());
        ctx.estimated_size_kb = u32::try_from(total_bytes / 1024).unwrap_or(u32::MAX);

        if ctx.portable {
            std::fs::write(ctx.install_dir.join(PORTABLE_MARKER), b"")?;
        } else {
            self.registration.register(&ctx)?;
        }

        on_event(FlowEvent::Completed);
        Ok(())
    }
}

/// Removes an install delivered by [`InstallFlow`]: registration trace,
/// payload files (per the on-disk manifest), marker, and the directory
/// itself once empty. Safe to call for both modes.
pub fn uninstall(ctx: &InstallContext, registration: &dyn Registration) -> Result<(), ShunError> {
    if !ctx.portable {
        registration.unregister(ctx)?;
    }

    let manifest_path = ctx.install_dir.join(MANIFEST_PATH);
    if manifest_path.exists() {
        let entries: Vec<PayloadEntry> = serde_json::from_slice(&std::fs::read(&manifest_path)?)?;
        for entry in entries {
            let _ = std::fs::remove_file(ctx.install_dir.join(entry.path));
        }
        let _ = std::fs::remove_file(&manifest_path);
        remove_empty_dirs_below(&ctx.install_dir);
    }

    let _ = std::fs::remove_file(ctx.install_dir.join(PORTABLE_MARKER));

    let uninstaller = ctx.install_dir.join(UNINSTALLER_NAME);
    if uninstaller.exists() {
        schedule_self_delete(&uninstaller);
    }

    let _ = std::fs::remove_dir(&ctx.install_dir);
    Ok(())
}

/// Removes directories under `root` deepest-first (payload subdirectories
/// whose files the manifest pass just deleted), then `root` itself.
/// Failures are ignored — a non-empty directory simply stays.
fn remove_empty_dirs_below(root: &Path) {
    let mut dirs: Vec<PathBuf> = WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_dir() && e.path() != root)
        .map(|e| e.path().to_path_buf())
        .collect();
    dirs.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
    for dir in dirs {
        let _ = std::fs::remove_dir(dir);
    }
    let _ = std::fs::remove_dir(root);
}

/// Per-OS registration backend.
///
/// Windows implements ARP entries, uninstaller registration, and start
/// menu shortcuts. Unix backends follow platform conventions (desktop
/// entries, launcher registration) once the macOS and Linux outputs land.
pub trait Registration {
    /// Register the install: ARP entry, uninstaller, shortcuts.
    fn register(&self, ctx: &InstallContext) -> Result<(), ShunError>;

    /// Remove every trace [`Registration::register`] left behind.
    fn unregister(&self, ctx: &InstallContext) -> Result<(), ShunError>;
}

/// HKCU ARP key path for a product (per-user installs need no elevation).
pub fn arp_key_path(product: &str) -> String {
    format!(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{product}")
}

/// Windows registration backend (ARP, uninstaller self-copy, start-menu
/// shortcut).
#[derive(Debug, Clone, Copy, Default)]
pub struct WindowsRegistration;

#[cfg(windows)]
impl Registration for WindowsRegistration {
    fn register(&self, ctx: &InstallContext) -> Result<(), ShunError> {
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;

        // Uninstaller: copy this binary next to the payload (skipped when
        // we are already running from the install directory, i.e. when the
        // uninstaller re-registers itself).
        let uninstaller = ctx.install_dir.join(UNINSTALLER_NAME);
        match std::env::current_exe() {
            Ok(current) if current != uninstaller => {
                std::fs::create_dir_all(&ctx.install_dir)?;
                std::fs::copy(current, &uninstaller)?;
            }
            Ok(_) => {}
            Err(e) => return Err(ShunError::Io(e)),
        }

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hkcu.create_subkey(arp_key_path(&ctx.product))?;
        key.set_value("DisplayName", &ctx.product)?;
        key.set_value("DisplayVersion", &ctx.version)?;
        if let Some(publisher) = &ctx.publisher {
            key.set_value("Publisher", publisher)?;
        }
        key.set_value("InstallLocation", &ctx.install_dir.as_os_str())?;
        key.set_value("DisplayIcon", &uninstaller.as_os_str())?;
        key.set_value(
            "UninstallString",
            &format!("\"{}\" /uninstall", uninstaller.display()),
        )?;
        key.set_value("NoModify", &1u32)?;
        key.set_value("NoRepair", &1u32)?;
        key.set_value("EstimatedSize", &ctx.estimated_size_kb)?;

        // Start-menu shortcut, targeting the payload's entry point.
        if let Some(main_exe) = &ctx.main_exe {
            let programs = std::env::var("APPDATA")
                .map(|appdata| Path::new(&appdata).join(r"Microsoft\Windows\Start Menu\Programs"))
                .map_err(|_| ShunError::Config("%APPDATA% is not set".to_string()))?;
            std::fs::create_dir_all(&programs)?;
            let link = programs.join(format!("{}.lnk", ctx.product));
            mslnk::ShellLink::new(ctx.install_dir.join(main_exe))
                .map_err(|e| ShunError::Config(format!("shortcut creation failed: {e}")))?
                .create_lnk(&link)
                .map_err(|e| ShunError::Config(format!("shortcut creation failed: {e}")))?;
        }

        Ok(())
    }

    fn unregister(&self, ctx: &InstallContext) -> Result<(), ShunError> {
        use winreg::RegKey;
        use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let arp = arp_key_path(&ctx.product);
        if hkcu.open_subkey_with_flags(&arp, KEY_READ).is_ok() {
            hkcu.delete_subkey(&arp)?;
        }

        if ctx.main_exe.is_some() {
            if let Ok(appdata) = std::env::var("APPDATA") {
                let link = Path::new(&appdata)
                    .join(r"Microsoft\Windows\Start Menu\Programs")
                    .join(format!("{}.lnk", ctx.product));
                let _ = std::fs::remove_file(link);
            }
        }

        // The uninstaller binary deletes itself (it is usually the process
        // being run) — see `schedule_self_delete`.
        Ok(())
    }
}

#[cfg(not(windows))]
impl Registration for WindowsRegistration {
    fn register(&self, _ctx: &InstallContext) -> Result<(), ShunError> {
        Err(ShunError::Unsupported(
            "windows registration backend (compile on windows)",
        ))
    }

    fn unregister(&self, _ctx: &InstallContext) -> Result<(), ShunError> {
        Err(ShunError::Unsupported(
            "windows registration backend (compile on windows)",
        ))
    }
}

/// Deletes `exe` after a short delay when it is the currently running
/// process (a running binary cannot delete itself outright): a detached
/// `cmd` waits ~1s then removes the file. Non-running files delete
/// directly.
#[cfg(windows)]
fn schedule_self_delete(exe: &Path) {
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    use std::os::windows::process::CommandExt;

    if std::env::current_exe()
        .map(|current| current == exe)
        .unwrap_or(false)
    {
        let script = format!("ping -n 2 127.0.0.1 > nul & del /q \"{}\"", exe.display());
        let _ = std::process::Command::new("cmd")
            .arg("/C")
            .raw_arg(script)
            .creation_flags(DETACHED_PROCESS)
            .spawn();
    } else {
        let _ = std::fs::remove_file(exe);
    }
}

#[cfg(not(windows))]
fn schedule_self_delete(exe: &Path) {
    let _ = std::fs::remove_file(exe);
}
