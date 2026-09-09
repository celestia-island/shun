//! macOS registration — one primitive: the `.app` bundle. Registration
//! = ensure the bundle is complete (`Info.plist` synthesized when the
//! payload carried none), restore the exec bit, strip inherited
//! quarantine attributes, and let Launch Services discover the bundle
//! (`lsregister -f`); Spotlight and Launchpad follow. Unregistration =
//! `lsregister -u` — the files themselves are removed by the generic
//! uninstall pass.
//!
//! Payloads delivered as bare executables (no `.app` wrapper) have
//! nothing to register on macOS and register as a no-op — the portable
//! mode conventions apply.

use std::path::{Path, PathBuf};

use crate::error::ShunError;
use crate::targets::install::{InstallContext, Registration};
#[cfg(target_os = "macos")]
use crate::targets::plist::render_info_plist;

/// macOS registration backend (bundle completion + Launch Services).
#[derive(Debug, Clone, Copy, Default)]
pub struct MacOSRegistration;

/// Locates the `.app` bundle the entry executable lives in, if any —
/// the first ancestor directory (within the install) whose name ends
/// with `.app`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn app_bundle(ctx: &InstallContext) -> Option<PathBuf> {
    let main_exe = ctx.main_exe.as_ref()?;
    let mut ancestor = ctx.install_dir.join(main_exe);
    while ancestor.pop() {
        if ancestor == ctx.install_dir {
            return None;
        }
        if ancestor
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(".app"))
        {
            return Some(ancestor);
        }
    }
    None
}

/// The Launch Services registration tool (stable system path).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const LSREGISTER: &str = "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister";

#[cfg(target_os = "macos")]
impl Registration for MacOSRegistration {
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
        // The archive stores 0644 for every entry; the entry point needs
        // its exec bit back before anything can launch it.
        std::fs::set_permissions(
            ctx.install_dir.join(main_exe),
            std::fs::Permissions::from_mode(0o755),
        )?;

        let Some(bundle) = app_bundle(ctx) else {
            return Ok(()); // bare payload — nothing to register
        };

        // A payload assembled outside Xcode may carry no Info.plist; a
        // bundle without one is not launchable.
        let plist = bundle.join("Contents").join("Info.plist");
        if !plist.exists() {
            std::fs::create_dir_all(bundle.join("Contents"))?;
            let executable = main_exe
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            std::fs::write(
                &plist,
                render_info_plist(
                    &ctx.product,
                    &ctx.version,
                    ctx.publisher.as_deref(),
                    &executable,
                    &ctx.deep_links,
                ),
            )?;
        }

        // The installer was downloaded from the web, and file copies on
        // macOS preserve xattrs — the delivered bundle can inherit
        // `com.apple.quarantine`, which gates first launch on Gatekeeper
        // approval the user already gave by running the installer.
        // Best-effort: machines without /usr/bin/xattr skip silently.
        let _ = std::process::Command::new("/usr/bin/xattr")
            .args(["-rd", "com.apple.quarantine"])
            .arg(&ctx.install_dir)
            .status();

        let _ = std::process::Command::new(LSREGISTER)
            .arg("-f")
            .arg(&bundle)
            .status();
        Ok(())
    }

    fn unregister(&self, ctx: &InstallContext) -> Result<(), ShunError> {
        if let Some(bundle) = app_bundle(ctx) {
            let _ = std::process::Command::new(LSREGISTER)
                .arg("-u")
                .arg(&bundle)
                .status();
        }
        Ok(())
    }
}

#[cfg(not(target_os = "macos"))]
impl Registration for MacOSRegistration {
    fn register(&self, _ctx: &InstallContext) -> Result<(), ShunError> {
        Err(ShunError::Unsupported(
            "macos registration backend (compile on macos)",
        ))
    }

    fn unregister(&self, _ctx: &InstallContext) -> Result<(), ShunError> {
        Err(ShunError::Unsupported(
            "macos registration backend (compile on macos)",
        ))
    }
}

/// Path helper re-exported for the bundle-detection unit test below.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn bundle_for(install_dir: &Path, main_exe: &Path) -> Option<PathBuf> {
    app_bundle(&InstallContext {
        product: String::new(),
        version: String::new(),
        publisher: None,
        install_dir: install_dir.to_path_buf(),
        main_exe: Some(main_exe.to_path_buf()),
        portable: false,
        portable_marker: crate::targets::install::PORTABLE_MARKER.to_string(),
        scope: crate::targets::install::InstallScope::User,
        desktop_shortcut: false,
        verbs: Vec::new(),
        deep_links: Vec::new(),
        aumid: None,
        icon: None,
        estimated_size_kb: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_bundle_is_found_through_ancestors() {
        let install = Path::new("/Applications");
        assert_eq!(
            bundle_for(install, Path::new("ShunDemo.app/Contents/MacOS/shun-demo")),
            Some(install.join("ShunDemo.app"))
        );
        // Nested helper apps inside the payload resolve to the nearest
        // wrapper, not the outermost.
        assert_eq!(
            bundle_for(
                install,
                Path::new("ShunDemo.app/Contents/Frameworks/Helper.app/Contents/MacOS/helper")
            ),
            Some(install.join("ShunDemo.app/Contents/Frameworks/Helper.app"))
        );
        // Bare executables have no bundle.
        assert_eq!(bundle_for(install, Path::new("bin/app")), None);
    }
}
