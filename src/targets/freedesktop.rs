//! freedesktop.org registration for Linux — the `.desktop` entry is the
//! single primitive every desktop environment understands (launchers,
//! taskbars, docks), and everything is per-user under `~/.local/share`:
//! no elevation, the direct analog of shun's HKCU model on Windows.
//!
//! The writer here is plain data plumbing, so it compiles (and is
//! unit-tested) on every platform; the process-spawning half —
//! `update-desktop-database`, the exec-bit restore — only runs on Linux.
//!
//! Two honest platform limits are baked into the design: there is no
//! cross-desktop taskbar/dock pinning API (`StartupWMClass` is what makes
//! *user-initiated* pinning group under the right icon), and GNOME
//! Software / KDE Discover never list non-package apps — which is why the
//! entry always carries an `Uninstall` Desktop Action.

use std::path::{Path, PathBuf};

use crate::error::ShunError;
use crate::targets::install::{InstallContext, Registration, UNINSTALLER_NAME, VerbTarget};

/// Renders the `.desktop` document for an install context: launcher
/// entry (with `StartupWMClass` keyed to the entry executable), an
/// `Uninstall` desktop action, plus one action per context-menu verb.
/// Returns an empty string when the context declares no entry point —
/// there is nothing to launch.
pub fn render_desktop_entry(ctx: &InstallContext) -> String {
    let Some(main_exe) = &ctx.main_exe else {
        return String::new();
    };
    let exe = ctx.install_dir.join(main_exe);
    let wm_class = main_exe
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| main_exe.to_string_lossy().into_owned());

    // The Actions= list references the group ids below it; ids are known
    // before the groups render.
    let mut actions = Vec::new();
    for verb in &ctx.verbs {
        actions.push(action_id(&verb.key));
    }
    actions.push("Uninstall".to_string());

    let mut doc = String::from("[Desktop Entry]\n");
    doc.push_str("Type=Application\n");
    doc.push_str(&format!("Name={}\n", ctx.product));
    if let Some(publisher) = &ctx.publisher {
        doc.push_str(&format!("Comment={publisher}\n"));
    }
    doc.push_str(&format!("Exec=\"{}\"\n", exe.display()));
    if let Some(icon) = &ctx.icon {
        doc.push_str(&format!("Icon={}\n", ctx.install_dir.join(icon).display()));
    }
    doc.push_str("Terminal=false\n");
    doc.push_str("Categories=Utility;\n");
    doc.push_str(&format!("StartupWMClass={wm_class}\n"));
    doc.push_str(&format!("Actions={};\n", actions.join(";")));

    for (verb, id) in ctx.verbs.iter().zip(&actions) {
        doc.push_str(&format!("\n[Desktop Action {id}]\n"));
        doc.push_str(&format!("Name={}\n", verb.display));
        doc.push_str(&format!(
            "Exec={}\n",
            desktop_action_exec(ctx, main_exe, &verb.target)
        ));
    }
    doc.push_str(&format!(
        "\n[Desktop Action Uninstall]\nName=Uninstall {}\nExec=\"{}\" --uninstall\n",
        ctx.product,
        ctx.install_dir.join(UNINSTALLER_NAME).display()
    ));
    doc
}

/// The command line a desktop action runs: the same verb mapping the
/// Windows backend registers, in freedesktop quoting.
fn desktop_action_exec(ctx: &InstallContext, main_exe: &Path, target: &VerbTarget) -> String {
    match target {
        VerbTarget::DataFolder => format!("xdg-open \"{}\"", ctx.install_dir.display()),
        VerbTarget::Uninstall => format!(
            "\"{}\" --uninstall",
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

/// Desktop-action ids tolerate `[A-Za-z0-9-]`; anything else collapses to
/// separators, and the id is capitalized (the `Uninstall` convention).
fn action_id(key: &str) -> String {
    let mapped: String = key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let joined: Vec<&str> = mapped.split('-').filter(|s| !s.is_empty()).collect();
    if joined.is_empty() {
        return "Action".into();
    }
    let id = joined.join("-");
    let mut chars = id.chars();
    let head = chars.next().unwrap().to_ascii_uppercase();
    format!("{head}{}", chars.as_str())
}

/// `~/.local/share/applications`, honoring `$XDG_DATA_HOME`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn applications_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from(".local/share"))
        .join("applications")
}

/// Linux registration backend: per-user `.desktop` launcher + desktop
/// actions, exec-bit restore for the entry point (the payload archive
/// carries 0644 for everything), and a MIME-database refresh.
#[derive(Debug, Clone, Copy, Default)]
pub struct LinuxRegistration;

#[cfg(target_os = "linux")]
impl Registration for LinuxRegistration {
    fn register(&self, ctx: &InstallContext) -> Result<(), ShunError> {
        use std::os::unix::fs::PermissionsExt;

        // Machine-wide scope is a Windows surface; the freedesktop/macOS
        // backends stay per-user.
        if ctx.scope == crate::targets::install::InstallScope::Machine {
            return Err(ShunError::Unsupported(
                "machine install scope (windows only)",
            ));
        }

        let Some(main_exe) = &ctx.main_exe else {
            return Ok(());
        };

        // Self-copy the uninstaller beside the payload (same contract as
        // the Windows backend).
        let uninstaller = ctx.install_dir.join(UNINSTALLER_NAME);
        if let Ok(current) = std::env::current_exe() {
            if current != uninstaller {
                std::fs::create_dir_all(&ctx.install_dir)?;
                std::fs::copy(current, &uninstaller)?;
            }
        }

        // The archive stores 0644 for every entry; the entry point (and
        // the uninstaller) need their exec bits back.
        std::fs::set_permissions(
            ctx.install_dir.join(main_exe),
            std::fs::Permissions::from_mode(0o755),
        )?;
        std::fs::set_permissions(&uninstaller, std::fs::Permissions::from_mode(0o755))?;

        let dir = applications_dir();
        std::fs::create_dir_all(&dir)?;
        let stem = crate::targets::install::shortcut_stem(&ctx.product);
        let desktop_file = format!("{stem}.desktop");
        std::fs::write(dir.join(&desktop_file), render_desktop_entry(ctx))?;

        // MIME-database refresh, plus claiming the configured deep-link
        // schemes as this launcher's defaults (writes mimeapps.list) —
        // both best-effort; DEs re-scan lazily when the tools are absent.
        let _ = std::process::Command::new("update-desktop-database")
            .arg(&dir)
            .status();
        for scheme in &ctx.deep_links {
            let _ = std::process::Command::new("xdg-mime")
                .args([
                    "default",
                    &desktop_file,
                    &format!("x-scheme-handler/{scheme}"),
                ])
                .status();
        }
        Ok(())
    }

    fn unregister(&self, ctx: &InstallContext) -> Result<(), ShunError> {
        let stem = crate::targets::install::shortcut_stem(&ctx.product);
        let _ = std::fs::remove_file(applications_dir().join(format!("{stem}.desktop")));
        let _ = std::process::Command::new("update-desktop-database")
            .arg(applications_dir())
            .status();
        Ok(())
    }
}

#[cfg(not(target_os = "linux"))]
impl Registration for LinuxRegistration {
    fn register(&self, _ctx: &InstallContext) -> Result<(), ShunError> {
        Err(ShunError::Unsupported(
            "linux registration backend (compile on linux)",
        ))
    }

    fn unregister(&self, _ctx: &InstallContext) -> Result<(), ShunError> {
        Err(ShunError::Unsupported(
            "linux registration backend (compile on linux)",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::targets::install::VerbSpec;

    fn ctx() -> InstallContext {
        InstallContext {
            product: "ShunDemo Test".into(),
            version: "0.0.1".into(),
            publisher: Some("celestia-island".into()),
            install_dir: PathBuf::from("/opt/shundemo"),
            main_exe: Some(PathBuf::from("bin/shun-demo")),
            portable: false,
            portable_marker: crate::targets::install::PORTABLE_MARKER.to_string(),
            scope: crate::targets::install::InstallScope::User,
            desktop_shortcut: false,
            verbs: vec![
                VerbSpec {
                    key: "open-data".into(),
                    display: "Open data folder".into(),
                    target: VerbTarget::DataFolder,
                },
                VerbSpec {
                    key: "safe-mode".into(),
                    display: "Safe mode".into(),
                    target: VerbTarget::App {
                        arguments: "--safe".into(),
                    },
                },
            ],
            deep_links: vec!["shundemo".into()],
            aumid: Some("celestia-island.ShunDemoTest".into()),
            icon: Some(PathBuf::from("assets/icon.png")),
            estimated_size_kb: 1,
        }
    }

    #[test]
    fn desktop_entry_declares_launcher_identity_and_actions() {
        let doc = render_desktop_entry(&ctx());
        // Expected paths are built the same way the renderer builds
        // them, so the test holds on every platform's Path display.
        let root = Path::new("/opt/shundemo");
        let exe = root.join("bin/shun-demo");
        assert!(doc.contains("Type=Application"));
        assert!(doc.contains("Name=ShunDemo Test"));
        assert!(doc.contains(&format!("Exec=\"{}\"", exe.display())));
        assert!(doc.contains(&format!("Icon={}", root.join("assets/icon.png").display())));
        // StartupWMClass is the taskbar-grouping identity: the exe stem.
        assert!(doc.contains("StartupWMClass=shun-demo"));
        // The uninstall action is always present (software centers never
        // list non-package apps), verbs ride along as desktop actions.
        assert!(doc.contains("Actions=Open-data;Safe-mode;Uninstall;"));
        assert!(doc.contains("[Desktop Action Uninstall]"));
        assert!(doc.contains(&format!(
            "Exec=\"{}\" --uninstall",
            root.join(UNINSTALLER_NAME).display()
        )));
        assert!(doc.contains("[Desktop Action Open-data]"));
        assert!(doc.contains(&format!("Exec=xdg-open \"{}\"", root.display())));
        assert!(doc.contains("[Desktop Action Safe-mode]"));
        assert!(doc.contains(&format!("Exec=\"{}\" --safe", exe.display())));
    }

    #[test]
    fn desktop_entry_without_extras_stays_minimal() {
        let mut context = ctx();
        context.verbs.clear();
        context.icon = None;
        context.publisher = None;
        let doc = render_desktop_entry(&context);
        assert!(doc.contains("Actions=Uninstall;\n"));
        assert!(!doc.contains("Icon="));
        assert!(!doc.contains("Comment="));
    }

    #[test]
    fn no_entry_point_renders_nothing() {
        let mut context = ctx();
        context.main_exe = None;
        assert_eq!(render_desktop_entry(&context), "");
    }

    #[test]
    fn action_ids_are_sanitized_and_capitalized() {
        assert_eq!(action_id("open data/x"), "Open-data-x");
        assert_eq!(action_id("///"), "Action");
    }
}
