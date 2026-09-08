//! Shared fixtures for the install-flow integration tests.

use std::path::PathBuf;

use shun::payload::{ArchivePayload, pack_directory};
use shun::targets::install::InstallContext;

/// A synthetic demo payload. The real one (examples/demo_payload) has its
/// application binary staged by `just demo-payload` — a build product —
/// so the integration tests pack their own fixture instead of depending
/// on staging. The entry executable only needs to exist for shortcut
/// creation; it is not executed.
#[allow(dead_code)]
pub fn demo_payload() -> ArchivePayload {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("bin")).unwrap();
    std::fs::create_dir_all(dir.path().join("data")).unwrap();
    std::fs::write(dir.path().join("README.txt"), "demo payload fixture").unwrap();
    std::fs::write(dir.path().join("data").join("sample.json"), "{}\n").unwrap();
    std::fs::write(dir.path().join("bin").join("shun-demo.exe"), [0x4d, 0x5a]).unwrap();
    // A second entry point whose Explorer-verb key is not shared with the
    // other tests (the Applications key is keyed by exe file name).
    std::fs::write(
        dir.path().join("bin").join("absent-probe.exe"),
        [0x4d, 0x5a],
    )
    .unwrap();
    let archive = pack_directory(dir.path()).unwrap();
    ArchivePayload::from_bytes(&archive).unwrap()
}

#[allow(dead_code)]
pub fn ctx(product: &str, install_dir: PathBuf, portable: bool) -> InstallContext {
    let mut ctx = InstallContext::new(product.to_string(), "0.0.1".into(), install_dir, portable);
    ctx.publisher = Some("celestia-island".into());
    ctx.main_exe = Some(PathBuf::from("bin/shun-demo.exe"));
    ctx
}
