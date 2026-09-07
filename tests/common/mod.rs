//! Shared fixtures for the install-flow integration tests.

use std::path::PathBuf;

use shun::payload::{ArchivePayload, pack_directory};
use shun::targets::install::InstallContext;

pub fn demo_payload() -> ArchivePayload {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let archive = pack_directory(&manifest_dir.join("examples/demo_payload")).unwrap();
    ArchivePayload::from_bytes(&archive).unwrap()
}

pub fn ctx(product: &str, install_dir: PathBuf, portable: bool) -> InstallContext {
    InstallContext {
        product: product.to_string(),
        version: "0.0.1".to_string(),
        publisher: Some("celestia-island".to_string()),
        install_dir,
        main_exe: Some(PathBuf::from("bin/shun-demo.cmd")),
        portable,
        estimated_size_kb: 0,
    }
}
