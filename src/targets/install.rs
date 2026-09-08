//! Install target: NSIS-like registration (ARP, uninstaller, shortcuts,
//! deep links) and a portable mode that writes no registry at all.
//!
//! The Windows backend follows the wowsp NSIS template as its executable
//! spec: HKCU ARP entry, a self-copying `uninstall.exe`, and a start-menu
//! shortcut. Portable mode writes a `.shun-portable` marker and skips all
//! registration — the same convention wowsp apps detect for data placement.

use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::config::InstallConfig;
use crate::error::ShunError;
use crate::flow::{Flow, FlowEvent};
use crate::payload::{MANIFEST_PATH, PayloadEntry, PayloadSource};

/// Marker file enabling portable (便捷) mode for a delivered copy.
pub const PORTABLE_MARKER: &str = ".shun-portable";

/// Name of the uninstaller binary copied into the install directory.
#[cfg(windows)]
pub const UNINSTALLER_NAME: &str = "uninstall.exe";
/// Name of the uninstaller binary copied into the install directory.
#[cfg(not(windows))]
pub const UNINSTALLER_NAME: &str = "uninstall";

/// A context-menu verb the registration backends expose on the app's
/// launchers (Explorer verbs on Windows, Desktop Actions on Linux).
#[derive(Debug, Clone, PartialEq)]
pub struct VerbSpec {
    /// Stable verb id — the registry segment / desktop-action id.
    pub key: String,

    /// Display string shown in the menu.
    pub display: String,

    /// What the verb invokes.
    pub target: VerbTarget,
}

/// What a context-menu verb invokes.
#[derive(Debug, Clone, PartialEq)]
pub enum VerbTarget {
    /// Open the install directory in the file manager.
    DataFolder,
    /// Run the copied uninstaller.
    Uninstall,
    /// Launch the app entry point with extra arguments.
    App { arguments: String },
}

/// Where the install lands: per-user (the default — HKCU / `~/.local` /
/// `~/Applications`, no elevation) or machine-wide (Windows: HKLM,
/// all-users Start Menu, public desktop; the shell self-elevates).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InstallScope {
    /// Per-user surfaces only (the default).
    #[default]
    User,
    /// Machine-wide surfaces (Windows-only today).
    Machine,
}

/// The wizard's answers to the `ask` policies, consumed by
/// [`InstallContext::apply_config`].
#[derive(Debug, Clone, Copy, Default)]
pub struct WizardAnswers {
    /// The desktop-shortcut checkbox (`ask` policy; default checked).
    pub desktop_shortcut: bool,
    /// The machine-scope choice (`ask` policy; default per-user).
    pub machine: bool,
}

impl WizardAnswers {
    /// The default-checked answers (headless runs): desktop shortcut on,
    /// per-user scope.
    pub fn defaults() -> Self {
        Self {
            desktop_shortcut: true,
            machine: false,
        }
    }
}

impl From<&crate::config::VerbConfig> for VerbSpec {
    fn from(config: &crate::config::VerbConfig) -> Self {
        use crate::config::VerbConfig;
        match config {
            VerbConfig::DataFolder { key, display } => Self {
                key: key.clone(),
                display: display.clone(),
                target: VerbTarget::DataFolder,
            },
            VerbConfig::Uninstall { key, display } => Self {
                key: key.clone(),
                display: display.clone(),
                target: VerbTarget::Uninstall,
            },
            VerbConfig::App {
                key,
                display,
                arguments,
            } => Self {
                key: key.clone(),
                display: display.clone(),
                target: VerbTarget::App {
                    arguments: arguments.clone(),
                },
            },
        }
    }
}

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

    /// Where the install lands (per-user default; machine-wide writes
    /// HKLM and all-users surfaces and needs elevation).
    pub scope: InstallScope,

    /// Create a desktop shortcut beside the start-menu one (local mode).
    pub desktop_shortcut: bool,

    /// Context-menu verbs to register (local mode).
    pub verbs: Vec<VerbSpec>,

    /// URL schemes the app owns (`myapp://…`), registered as protocol
    /// handlers (local mode).
    pub deep_links: Vec<String>,

    /// Grouping identity stamped on the shortcuts (Windows
    /// `System.AppUserModel.ID`); the app should pass the same value to
    /// `SetCurrentProcessExplicitAppUserModelID`.
    pub aumid: Option<String>,

    /// Payload-relative icon file the Linux launcher references.
    pub icon: Option<PathBuf>,

    /// ARP `EstimatedSize` in KiB; computed by the flow from the manifest.
    pub estimated_size_kb: u32,
}

impl InstallContext {
    /// A context with the identity fields set and every registration
    /// knob at its no-op default (no desktop shortcut, no verbs, no
    /// icon; the flow fills `estimated_size_kb`). Chain
    /// [`Self::apply_config`] to resolve the config-declared knobs.
    pub fn new(product: String, version: String, install_dir: PathBuf, portable: bool) -> Self {
        Self {
            product,
            version,
            publisher: None,
            install_dir,
            main_exe: None,
            portable,
            scope: InstallScope::User,
            desktop_shortcut: false,
            verbs: Vec::new(),
            deep_links: Vec::new(),
            aumid: None,
            icon: None,
            estimated_size_kb: 0,
        }
    }

    /// Applies the install-target configuration knobs onto a context:
    /// the desktop-shortcut and install-scope policies resolved against
    /// the wizard answers (`ask` consults them; headless runs pass
    /// [`WizardAnswers::defaults`]), the context-menu verbs, the
    /// deep-link schemes, the AUMID, and the launcher icon.
    pub fn apply_config(&mut self, install: &InstallConfig, answers: WizardAnswers) {
        use crate::config::{DesktopShortcutPolicy, ScopePolicy};
        self.desktop_shortcut = match install.desktop_shortcut {
            DesktopShortcutPolicy::Always => true,
            DesktopShortcutPolicy::Never => false,
            DesktopShortcutPolicy::Ask => answers.desktop_shortcut,
        };
        self.scope = match install.scope {
            ScopePolicy::User => InstallScope::User,
            ScopePolicy::Machine => InstallScope::Machine,
            ScopePolicy::Ask => {
                if answers.machine {
                    InstallScope::Machine
                } else {
                    InstallScope::User
                }
            }
        };
        self.verbs = install.verbs.iter().map(VerbSpec::from).collect();
        self.deep_links = install
            .deep_links
            .iter()
            .map(|scheme| url_scheme(scheme))
            .filter(|s| !s.is_empty())
            .collect();
        self.aumid = Some(
            install
                .aumid
                .clone()
                .unwrap_or_else(|| default_aumid(self.publisher.as_deref(), &self.product)),
        );
        self.icon = install.icon.clone();
    }
}

/// Normalizes a configured deep-link scheme to a registry-safe URL
/// scheme name: lowercase, `[a-z0-9+.-]` only, trailing `://` stripped
/// (empty schemes are dropped).
pub fn url_scheme(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches(':').trim_end_matches('/');
    trimmed
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '+' | '.' | '-'))
        .collect()
}

/// The default shortcut grouping identity: `{publisher}.{product}` with
/// each segment reduced to `[A-Za-z0-9-]` runs joined by dots (AUMIDs are
/// `Company.Product` style identifiers, not free text).
pub fn default_aumid(publisher: Option<&str>, product: &str) -> String {
    let segment = |raw: &str| {
        let cleaned: String = raw
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c
                } else {
                    '.'
                }
            })
            .collect();
        let collapsed: Vec<&str> = cleaned.split('.').filter(|s| !s.is_empty()).collect();
        collapsed.join("-")
    };
    match publisher {
        Some(publisher) if !segment(publisher).is_empty() => {
            format!("{}.{}", segment(publisher), segment(product))
        }
        _ => segment(product),
    }
}

/// Filesystem/registry-safe stem for a product name: the characters
/// Windows forbids in file names (`<>:"/\|?*` plus controls) become `_`,
/// and trailing dots/spaces (also illegal) are trimmed.
pub fn shortcut_stem(product: &str) -> String {
    let mut stem: String = product
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                '_'
            } else {
                c
            }
        })
        .collect();
    while stem.ends_with('.') || stem.ends_with(' ') {
        stem.pop();
    }
    if stem.is_empty() {
        stem.push('_');
    }
    stem
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
/// The product name is stem-sanitized: a `\` would nest a subkey. The
/// same path hangs off HKLM for machine-wide installs.
pub fn arp_key_path(product: &str) -> String {
    format!(
        r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{}",
        shortcut_stem(product)
    )
}

/// Windows registration backend (ARP, uninstaller self-copy, start-menu
/// and desktop shortcuts, Explorer verbs, deep links) — per-user by
/// default; `InstallScope::Machine` writes the same surfaces machine-
/// wide (HKLM, all-users Start Menu, public desktop).
#[derive(Debug, Clone, Copy, Default)]
pub struct WindowsRegistration;

#[cfg(windows)]
impl Registration for WindowsRegistration {
    fn register(&self, ctx: &InstallContext) -> Result<(), ShunError> {
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

        let root = registry_root(ctx.scope);
        let (key, _) = root.create_subkey(arp_key_path(&ctx.product))?;
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

        // Shortcuts: always the start-menu one, plus the desktop one when
        // the context resolved `desktop-shortcut`. Both carry the AUMID
        // (taskbar grouping / pinning identity). Machine scope writes the
        // all-users Start Menu and the public desktop.
        if let Some(main_exe) = &ctx.main_exe {
            let target = ctx.install_dir.join(main_exe);
            let stem = shortcut_stem(&ctx.product);

            let programs = start_menu_dir(ctx.scope)?;
            std::fs::create_dir_all(&programs)?;
            write_shortcut(ctx, &target, &programs.join(format!("{stem}.lnk")))?;

            if ctx.desktop_shortcut {
                // Best-effort: AV/EDR policies commonly deny `.lnk`
                // creation on the desktop specifically (fake-shortcut /
                // ransomware protection) — the desktop shortcut is a
                // convenience and never fails the install.
                if let Some(link) = desktop_dir(ctx.scope)
                    .ok()
                    .map(|d| d.join(format!("{stem}.lnk")))
                {
                    if let Err(e) = write_shortcut(ctx, &target, &link) {
                        eprintln!("shun: desktop shortcut skipped: {e}");
                    }
                }
            }

            register_verbs(ctx, main_exe)?;
            register_deep_links(ctx, main_exe)?;
        }

        Ok(())
    }

    fn unregister(&self, ctx: &InstallContext) -> Result<(), ShunError> {
        use winreg::enums::KEY_READ;

        let root = registry_root(ctx.scope);
        let arp = arp_key_path(&ctx.product);
        if root.open_subkey_with_flags(&arp, KEY_READ).is_ok() {
            root.delete_subkey(&arp)?;
        }

        if ctx.main_exe.is_some() {
            let stem = shortcut_stem(&ctx.product);
            if let Ok(programs) = start_menu_dir(ctx.scope) {
                let _ = std::fs::remove_file(programs.join(format!("{stem}.lnk")));
            }
            // The desktop shortcut is attempted unconditionally: the
            // config may have changed between install and uninstall.
            if let Ok(desktop) = desktop_dir(ctx.scope) {
                let _ = std::fs::remove_file(desktop.join(format!("{stem}.lnk")));
            }
            if let Some(main_exe) = &ctx.main_exe {
                unregister_verbs(ctx, main_exe);
            }
        }
        unregister_deep_links(ctx);

        // The uninstaller binary deletes itself (it is usually the process
        // being run) — see `schedule_self_delete`.
        Ok(())
    }
}

/// The registry root the install scope writes: HKCU per user, HKLM
/// machine-wide (the caller must hold an elevated token for HKLM).
#[cfg(windows)]
fn registry_root(scope: InstallScope) -> winreg::RegKey {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    match scope {
        InstallScope::User => RegKey::predef(HKEY_CURRENT_USER),
        InstallScope::Machine => RegKey::predef(HKEY_LOCAL_MACHINE),
    }
}

/// The Start Menu `Programs` directory for the scope: the user's
/// `%APPDATA%` folder, or the all-users `%ProgramData%` one.
#[cfg(windows)]
fn start_menu_dir(scope: InstallScope) -> Result<PathBuf, ShunError> {
    let (variable, which) = match scope {
        InstallScope::User => ("APPDATA", "the user start menu"),
        InstallScope::Machine => ("ProgramData", "the all-users start menu"),
    };
    std::env::var(variable)
        .map(|root| Path::new(&root).join(r"Microsoft\Windows\Start Menu\Programs"))
        .map_err(|_| ShunError::Config(format!("%{variable}% is not set ({which})")))
}

/// Creates a `.lnk` via mslnk, then stamps the AUMID on top through the
/// Shell property store (enhancement-only: a stamping failure never
/// blocks the install).
#[cfg(windows)]
fn write_shortcut(ctx: &InstallContext, target: &Path, link: &Path) -> Result<(), ShunError> {
    mslnk::ShellLink::new(target)
        .map_err(|e| ShunError::Config(format!("shortcut creation failed: {e}")))?
        .create_lnk(link)
        .map_err(|e| ShunError::Config(format!("shortcut creation failed: {e}")))?;
    if let Some(aumid) = &ctx.aumid {
        if let Err(e) = crate::targets::aumid::stamp(link, aumid) {
            eprintln!("shun: AUMID stamping skipped for {}: {e}", link.display());
        }
    }
    Ok(())
}

/// The desktop directory for the scope via `SHGetKnownFolderPath` — not
/// `%USERPROFILE%\Desktop`, which is wrong whenever the desktop is
/// redirected (OneDrive, domain policies). Machine scope resolves the
/// public desktop instead.
#[cfg(windows)]
fn desktop_dir(scope: InstallScope) -> Result<PathBuf, ShunError> {
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{
        FOLDERID_Desktop, FOLDERID_PublicDesktop, SHGetKnownFolderPath,
    };

    unsafe {
        let folder = match scope {
            InstallScope::User => &FOLDERID_Desktop,
            InstallScope::Machine => &FOLDERID_PublicDesktop,
        };
        let mut pwstr = std::ptr::null_mut();
        if SHGetKnownFolderPath(folder, 0, std::ptr::null_mut(), &mut pwstr) < 0 || pwstr.is_null()
        {
            return Err(ShunError::Config(
                "desktop folder lookup failed (SHGetKnownFolderPath)".into(),
            ));
        }
        let mut len = 0usize;
        while *pwstr.add(len) != 0 {
            len += 1;
        }
        let path = String::from_utf16_lossy(std::slice::from_raw_parts(pwstr as *const u16, len));
        CoTaskMemFree(pwstr.cast());
        Ok(PathBuf::from(path))
    }
}

/// Explorer-verb registry location for the entry executable:
/// `HKCU\Software\Classes\Applications\<exe file name>` — the documented
/// Application Registration surface; its verbs appear on the executable
/// and on shortcuts to it.
#[cfg(windows)]
fn applications_key(main_exe: &Path) -> String {
    let exe = main_exe
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "app.exe".into());
    format!(r"Software\Classes\Applications\{exe}")
}

#[cfg(windows)]
fn verb_command(ctx: &InstallContext, main_exe: &Path, target: &VerbTarget) -> String {
    match target {
        VerbTarget::DataFolder => format!("explorer.exe \"{}\"", ctx.install_dir.display()),
        VerbTarget::Uninstall => format!(
            "\"{}\" /uninstall",
            ctx.install_dir.join(UNINSTALLER_NAME).display()
        ),
        VerbTarget::App { arguments } => {
            let exe = ctx.install_dir.join(main_exe).display().to_string();
            let arguments = arguments.trim();
            if arguments.is_empty() {
                format!("\"{exe}\"")
            } else {
                format!("\"{exe}\" {arguments}")
            }
        }
    }
}

#[cfg(windows)]
fn register_verbs(ctx: &InstallContext, main_exe: &Path) -> Result<(), ShunError> {
    if ctx.verbs.is_empty() {
        return Ok(());
    }
    let root = registry_root(ctx.scope);
    for verb in &ctx.verbs {
        let (menu, _) = root.create_subkey(format!(
            r"{}\shell\{}",
            applications_key(main_exe),
            verb.key
        ))?;
        menu.set_value("", &verb.display)?;
        let (command, _) = root.create_subkey(format!(
            r"{}\shell\{}\command",
            applications_key(main_exe),
            verb.key
        ))?;
        command.set_value("", &verb_command(ctx, main_exe, &verb.target))?;
    }
    Ok(())
}

/// Removes the verb keys this install created, then the `shell` and
/// `Applications\<exe>` containers — but only where they are empty, so a
/// verb someone else registered under the same exe survives.
#[cfg(windows)]
fn unregister_verbs(ctx: &InstallContext, main_exe: &Path) {
    let root = registry_root(ctx.scope);
    for verb in &ctx.verbs {
        let _ = root.delete_subkey_all(format!(
            r"{}\shell\{}",
            applications_key(main_exe),
            verb.key
        ));
    }
    let _ = root.delete_subkey(format!(r"{}\shell", applications_key(main_exe)));
    let _ = root.delete_subkey(applications_key(main_exe));
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

/// Registers the configured URL schemes as per-user protocol handlers
/// (`HKCU\Software\Classes\<scheme>` with the empty `URL Protocol`
/// value that marks a protocol class, plus the open command receiving
/// the URL as `%1`). Schemes are config-declared app-owned namespaces,
/// so unregistering deletes the whole key — the NSIS convention.
#[cfg(windows)]
fn register_deep_links(ctx: &InstallContext, main_exe: &Path) -> Result<(), ShunError> {
    if ctx.deep_links.is_empty() {
        return Ok(());
    }
    let root = registry_root(ctx.scope);
    let exe = ctx.install_dir.join(main_exe);
    for scheme in &ctx.deep_links {
        let (key, _) = root.create_subkey(format!(r"Software\Classes\{scheme}"))?;
        key.set_value("", &format!("URL:{} ({scheme})", ctx.product))?;
        key.set_value("URL Protocol", &"")?;
        let (open, _) =
            root.create_subkey(format!(r"Software\Classes\{scheme}\shell\open\command"))?;
        open.set_value("", &format!("\"{}\" \"%1\"", exe.display()))?;
    }
    Ok(())
}

#[cfg(windows)]
fn unregister_deep_links(ctx: &InstallContext) {
    if ctx.deep_links.is_empty() {
        return;
    }
    let root = registry_root(ctx.scope);
    for scheme in &ctx.deep_links {
        let _ = root.delete_subkey_all(format!(r"Software\Classes\{scheme}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stems_replace_filename_illegal_characters() {
        assert_eq!(shortcut_stem("Plain-Name"), "Plain-Name");
        assert_eq!(shortcut_stem("ShunDemo/Test *2"), "ShunDemo_Test _2");
        assert_eq!(shortcut_stem(r"Back\slash"), "Back_slash");
        // Trailing dots and spaces are also illegal in file names.
        assert_eq!(shortcut_stem("App. "), "App");
        assert_eq!(shortcut_stem("///"), "___");
    }

    #[test]
    fn arp_keys_use_the_sanitized_stem() {
        assert_eq!(
            arp_key_path(r"App\Nested"),
            r"Software\Microsoft\Windows\CurrentVersion\Uninstall\App_Nested"
        );
    }

    #[test]
    fn aumids_are_identifier_segments() {
        assert_eq!(
            default_aumid(Some("celestia-island"), "ShunDemo"),
            "celestia-island.ShunDemo"
        );
        assert_eq!(default_aumid(Some("A & B Co"), "My App"), "A-B-Co.My-App");
        assert_eq!(default_aumid(None, "Widget"), "Widget");
        assert_eq!(default_aumid(Some("///"), "Widget"), "Widget");
    }

    #[test]
    fn apply_config_resolves_the_registration_knobs() {
        use crate::config::{DesktopShortcutPolicy, InstallConfig, VerbConfig};

        let mut ctx = InstallContext::new(
            "Shun Demo".into(),
            "0.0.1".into(),
            PathBuf::from("."),
            false,
        );
        ctx.publisher = Some("celestia-island".into());

        let mut install = InstallConfig {
            desktop_shortcut: DesktopShortcutPolicy::Never,
            aumid: Some("explicit.aumid".into()),
            verbs: vec![VerbConfig::DataFolder {
                key: "open-data".into(),
                display: "Open data".into(),
            }],
            ..InstallConfig::default()
        };
        ctx.apply_config(
            &install,
            WizardAnswers {
                desktop_shortcut: true,
                machine: false,
            },
        );
        assert!(!ctx.desktop_shortcut, "never wins over the wizard answer");
        assert_eq!(ctx.aumid.as_deref(), Some("explicit.aumid"));
        assert_eq!(ctx.verbs.len(), 1);
        assert_eq!(ctx.verbs[0].target, VerbTarget::DataFolder);

        install.desktop_shortcut = DesktopShortcutPolicy::Always;
        ctx.apply_config(&install, WizardAnswers::defaults());
        assert!(ctx.desktop_shortcut, "always wins over the wizard answer");

        install.desktop_shortcut = DesktopShortcutPolicy::Ask;
        ctx.apply_config(
            &install,
            WizardAnswers {
                desktop_shortcut: false,
                machine: false,
            },
        );
        assert!(!ctx.desktop_shortcut, "ask follows the wizard answer");

        // Without an explicit AUMID the default is generated from the
        // identity.
        install.aumid = None;
        ctx.apply_config(
            &install,
            WizardAnswers {
                desktop_shortcut: true,
                machine: false,
            },
        );
        assert_eq!(ctx.aumid.as_deref(), Some("celestia-island.Shun-Demo"));

        // Deep-link schemes normalize to registry-safe form.
        install.deep_links = vec!["MyApp://".into(), " shundemo ".into(), "///".into()];
        ctx.apply_config(
            &install,
            WizardAnswers {
                desktop_shortcut: true,
                machine: false,
            },
        );
        assert_eq!(
            ctx.deep_links,
            vec!["myapp".to_string(), "shundemo".to_string()]
        );
    }

    #[test]
    fn url_schemes_normalize_to_lowercase_identifiers() {
        assert_eq!(url_scheme("MyApp://"), "myapp");
        assert_eq!(url_scheme(" Web+Extension.1 "), "web+extension.1");
        assert_eq!(url_scheme("has spaces"), "hasspaces");
        assert_eq!(url_scheme("///"), "");
    }
}
