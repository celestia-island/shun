//! Install target: NSIS-like registration (ARP, uninstaller, shortcuts,
//! deep links) and a portable mode that writes no registry at all.

use std::path::PathBuf;

use crate::error::ShunError;

/// Where and how an install-mode delivery lands on this machine.
#[derive(Debug, Clone, PartialEq)]
pub struct InstallContext {
    /// Product name (shortcut titles, ARP display name).
    pub product: String,

    /// Final directory of the installed payload.
    pub install_dir: PathBuf,

    /// Portable mode: skip registry, shortcuts, and uninstaller entirely;
    /// all data stays beside the executable.
    pub portable: bool,
}

/// Per-OS registration backend.
///
/// Windows implements ARP entries, uninstaller registration, start
/// menu/desktop shortcuts, and deep links. Unix backends follow platform
/// conventions (desktop entries, launcher registration) once the macOS and
/// Linux outputs land.
pub trait Registration {
    /// Register the install: ARP entry, uninstaller, shortcuts.
    fn register(&self, ctx: &InstallContext) -> Result<(), ShunError>;

    /// Remove every trace [`Registration::register`] left behind.
    fn unregister(&self, ctx: &InstallContext) -> Result<(), ShunError>;
}

/// Windows registration backend (ARP, uninstaller, shortcuts, deep links).
///
/// The implementation lands with the wowsp phase-2 port — the NSIS template
/// in that repository is the executable spec; v0 keeps the trait surface
/// only.
#[derive(Debug, Clone, Copy, Default)]
pub struct WindowsRegistration;

impl Registration for WindowsRegistration {
    fn register(&self, _ctx: &InstallContext) -> Result<(), ShunError> {
        Err(ShunError::Unsupported(
            "windows registration backend (lands with the wowsp phase-2 port)",
        ))
    }

    fn unregister(&self, _ctx: &InstallContext) -> Result<(), ShunError> {
        Err(ShunError::Unsupported(
            "windows registration backend (lands with the wowsp phase-2 port)",
        ))
    }
}

/// Unix registration backend: desktop entries and launcher registration,
/// following per-platform conventions. Scaffolding for now.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnixRegistration;

impl Registration for UnixRegistration {
    fn register(&self, _ctx: &InstallContext) -> Result<(), ShunError> {
        Err(ShunError::Unsupported("unix registration backend"))
    }

    fn unregister(&self, _ctx: &InstallContext) -> Result<(), ShunError> {
        Err(ShunError::Unsupported("unix registration backend"))
    }
}
