//! The unattended-update contract the wowsp updater depends on: an
//! install over a directory a lock survivor still holds (the taskkilled
//! host's WebView2 children pin it) must complete via the degraded
//! in-place merge, never strand the install on the old payload. The
//! holder is an open file handle inside the install directory — the
//! same sharing-violation rename block a surviving process's working
//! directory produces, without any process-spawn timing. Windows-only —
//! the lock shape is a Windows rename restriction.

#[cfg(windows)]
mod common;

#[cfg(windows)]
use common::{ctx, demo_payload};
#[cfg(windows)]
use shun::flow::Flow;
#[cfg(windows)]
use shun::targets::install::{InstallFlow, WindowsRegistration};

#[cfg(windows)]
#[test]
fn install_flow_completes_over_a_held_directory() {
    let dest = tempfile::tempdir().unwrap();
    let install_dir = dest.path().join("ShunDemo");
    // Portable context: the test exercises the delivery swap, not ARP
    // registration or shortcuts.
    let context = ctx("ShunDemo-Test-SwapLock", install_dir.clone(), true);
    let payload = demo_payload();

    // The first delivery lands the old tree.
    let flow = InstallFlow {
        payload: &payload,
        registration: &WindowsRegistration,
        ctx: context.clone(),
    };
    flow.run(&mut |_| {}).unwrap();
    // A file the PREVIOUS payload carried but the new one drops: plant
    // it and list it in the on-disk manifest the way an older install
    // would have left it, so the stale-file sweep has something real to
    // remove after the second delivery.
    std::fs::write(
        install_dir.join("stale-old.txt"),
        "dropped by the new payload",
    )
    .unwrap();
    {
        let manifest_path = install_dir.join("shun-manifest.json");
        let mut manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
        manifest["entries"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "path": "stale-old.txt",
                "size": 27u64,
                "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
            }));
        std::fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    // The survivor: an open handle inside the install directory pins it
    // against renames for as long as the handle lives.
    let _survivor = std::fs::File::create(install_dir.join("survivor-hold.lock")).unwrap();

    // The second delivery must complete despite the held directory.
    let flow = InstallFlow {
        payload: &payload,
        registration: &WindowsRegistration,
        ctx: context.clone(),
    };
    let mut steps = Vec::new();
    flow.run(&mut |event| {
        if let shun::flow::FlowEvent::Progress { step, .. } = event {
            steps.push(step.clone());
        }
    })
    .unwrap();

    assert!(
        install_dir.join("README.txt").is_file(),
        "the payload lands in place"
    );
    assert!(
        install_dir.join("shun-manifest.json").is_file(),
        "the on-disk manifest lands with it"
    );
    assert!(
        install_dir.join("bin").join("shun-demo.exe").is_file(),
        "nested entries land too"
    );
    assert!(
        steps
            .iter()
            .any(|s| s.contains("Waiting for the previous install to unlock")),
        "the retry window announces itself: {steps:?}"
    );
    assert!(!dest.path().join("ShunDemo.shun-partial").exists());
    assert!(
        install_dir.join("survivor-hold.lock").exists(),
        "files the delivery does not carry are left alone"
    );
}
