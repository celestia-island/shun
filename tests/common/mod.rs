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
    let archive = pack_directory(dir.path()).unwrap();
    ArchivePayload::from_bytes(&archive).unwrap()
}

#[allow(dead_code)]
pub fn ctx(product: &str, install_dir: PathBuf, portable: bool) -> InstallContext {
    InstallContext {
        product: product.to_string(),
        version: "0.0.1".to_string(),
        publisher: Some("celestia-island".to_string()),
        install_dir,
        main_exe: Some(PathBuf::from("bin/shun-demo.exe")),
        portable,
        estimated_size_kb: 0,
    }
}
