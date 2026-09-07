mod common;

use std::path::PathBuf;

use common::{ctx, demo_payload};
use shun::flow::Flow;
use shun::targets::install::{InstallFlow, PORTABLE_MARKER, WindowsRegistration, uninstall};

#[test]
fn portable_install_and_uninstall_roundtrip() {
    let dest = tempfile::tempdir().unwrap();
    let install_dir = dest.path().join("ShunDemo");
    let context = ctx("ShunDemo-Test-Portable", install_dir.clone(), true);

    let payload = demo_payload();
    let flow = InstallFlow {
        payload: &payload,
        registration: &WindowsRegistration,
        ctx: context.clone(),
    };
    flow.run(&mut |_| {}).unwrap();

    assert!(install_dir.join("README.txt").exists());
    assert!(install_dir.join("data/sample.json").exists());
    assert!(install_dir.join(PORTABLE_MARKER).exists());
    assert!(!install_dir.join("uninstall.exe").exists());

    uninstall(&context, &WindowsRegistration).unwrap();

    assert!(!install_dir.join("README.txt").exists());
    assert!(!install_dir.join(PORTABLE_MARKER).exists());
    assert!(!install_dir.exists());
}

#[test]
fn uninstall_is_tolerant_of_missing_install() {
    // Uninstalling a directory that never received a payload must not
    // error: every removal is best-effort.
    let dest = tempfile::tempdir().unwrap();
    let install_dir = dest.path().join("NeverInstalled");
    let context = ctx("ShunDemo-Test-Empty", install_dir, true);
    uninstall(&context, &WindowsRegistration).unwrap();
    let _ = PathBuf::new(); // PathBuf in scope for the assertions above
}
