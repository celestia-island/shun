//! The relocation lane end-to-end: an install that landed in a
//! `relocate`-flagged location (the game folder, WoWSP's case) moves
//! through `run_install` into a fresh directory, the silent-lane probe
//! flags the old location, and the sweep takes only shun's own files
//! back out of it — the foreign neighbor survives.

mod common;

#[cfg(windows)]
use common::{ctx, demo_payload};
#[cfg(windows)]
use shun::wizard::{InstallRequest, WizardCore, relocation_required, run_install};

/// A standalone manifest carrying the WoWSP-shaped relocate rule.
#[cfg(windows)]
fn config() -> shun::config::ShunConfig {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shun.toml");
    std::fs::write(
        &path,
        r#"
product = "ShunDemo-Test-Relocate"
version = "0.1.0"

[install]
main-exe = "bin/shun-demo.exe"

[[install.dir-rules]]
files = ["WorldOfWarships.exe"]
action = "relocate"
message = "the game folder"
"#,
    )
    .unwrap();
    shun::config::ShunConfig::from_path(&path).unwrap()
}

#[cfg(windows)]
fn request(dir: &std::path::Path, relocate_from: Option<&std::path::Path>) -> InstallRequest {
    InstallRequest {
        mode: "local".into(),
        dir: dir.to_string_lossy().into_owned(),
        language: None,
        machine: false,
        relocate_from: relocate_from.map(|old| old.to_string_lossy().into_owned()),
    }
}

/// Local-mode install performs real HKCU ARP registration — the
/// registration moves with the relocation and the teardown at the end
/// goes through the normal uninstall (Windows-only backend).
#[cfg(windows)]
#[test]
fn a_relocation_moves_the_install_and_sweeps_only_its_own_files() {
    use shun::targets::install::{WindowsRegistration, arp_key_path, uninstall};

    let root = tempfile::tempdir().unwrap();
    let old = root.path().join("WorldOfWarships");
    let fresh = root.path().join("NewHome");
    std::fs::create_dir_all(&old).unwrap();

    let core = WizardCore::new(config(), std::collections::BTreeMap::new());
    let payload = demo_payload();

    // The legacy install lands in the game folder…
    run_install(&core, &payload, &request(&old, None), &mut |_| {}).unwrap();
    assert!(old.join("bin").join("shun-demo.exe").is_file());
    // …and the host game moves in beside it (the pre-guard mistake).
    std::fs::write(old.join("WorldOfWarships.exe"), b"MZ").unwrap();

    // The silent lane's probe now flags the location: an update must
    // relocate, never swap the whole directory aside in place.
    let old_text = old.to_string_lossy().into_owned();
    assert_eq!(
        relocation_required(&core.config, &old_text, None),
        Some("the game folder".to_string())
    );

    // The relocation: fresh install into the new home, the old
    // directory handed over for the post-install sweep.
    run_install(&core, &payload, &request(&fresh, Some(&old)), &mut |_| {}).unwrap();

    // The new install is complete…
    assert!(fresh.join("bin").join("shun-demo.exe").is_file());
    assert!(fresh.join(shun::payload::MANIFEST_PATH).is_file());
    // …the old install's own files (manifest entries, the copied
    // uninstaller) are swept…
    assert!(!old.join("bin").join("shun-demo.exe").exists());
    assert!(!old.join(shun::payload::MANIFEST_PATH).exists());
    assert!(!old.join("uninstall.exe").exists());
    // …the game survives, and so does its folder.
    assert!(old.join("WorldOfWarships.exe").is_file());
    assert!(old.is_dir());

    // ARP now points at the new home: the successor owns the
    // registration, and the sweep never unregistered anything.
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let key = arp_key_path("ShunDemo-Test-Relocate");
    let arp = hkcu
        .open_subkey_with_flags(&key, winreg::enums::KEY_READ)
        .unwrap();
    let location: String = arp.get_value("InstallLocation").unwrap();
    assert!(
        location.to_lowercase().ends_with("newhome"),
        "InstallLocation follows the relocation: {location}"
    );

    // Test hygiene: tear the moved install down through the normal
    // uninstall — which also proves the successor sweeps its OWN
    // directory cleanly, foreign folder untouched beside it.
    let context = ctx("ShunDemo-Test-Relocate", fresh.clone(), false);
    uninstall(&context, &WindowsRegistration).unwrap();
    assert!(!fresh.exists());
    assert!(old.is_dir());
}
