mod common;

use std::path::{Path, PathBuf};

use common::{ctx, demo_payload};
use shun::flow::Flow;
use shun::targets::install::{
    InstallFlow, PORTABLE_MARKER, WindowsRegistration, read_manifest, uninstall,
};

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

#[test]
fn manifest_carries_the_wizard_language_through_the_flow() {
    // The language picked on the wizard's first step lands in the
    // on-disk manifest, where later passes (repair/update) read it
    // back; the uninstall pass consumes the very same manifest.
    let dest = tempfile::tempdir().unwrap();
    let install_dir = dest.path().join("ShunDemo");
    let mut context = ctx("ShunDemo-Test-Language", install_dir.clone(), true);
    context.language = Some("zh-Hans".into());

    let payload = demo_payload();
    let flow = InstallFlow {
        payload: &payload,
        registration: &WindowsRegistration,
        ctx: context.clone(),
    };
    flow.run(&mut |_| {}).unwrap();

    let manifest = read_manifest(&install_dir).unwrap();
    assert_eq!(manifest.language.as_deref(), Some("zh-Hans"));
    assert!(
        manifest
            .entries
            .iter()
            .any(|entry| entry.path == Path::new("README.txt")),
        "the payload entries ride along in the same manifest"
    );

    uninstall(&context, &WindowsRegistration).unwrap();
    assert!(!install_dir.exists());
}
