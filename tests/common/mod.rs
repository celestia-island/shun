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

/// Serializes the registration tests ACROSS test-binaries: two suites
/// running concurrently (a second checkout, a reviewer's harness) share
/// the same HKCU ARP keys and Start Menu directory for identically named
/// products, and one suite's uninstall sweeps the other's state mid-test.
/// std has no file locks, so a lock directory with PID stamping and stale
/// reclamation (older than 10 minutes) does the job.
#[allow(dead_code)]
pub struct RegistrationLock {
    path: std::path::PathBuf,
}

#[allow(dead_code)]
impl RegistrationLock {
    pub fn acquire() -> Self {
        let dir = std::env::temp_dir().join("shun-registration-tests.lock");
        let stamp = dir.join("owner.txt");
        loop {
            match std::fs::create_dir(&dir) {
                Ok(()) => {
                    let _ = std::fs::write(&stamp, format!("pid={}", std::process::id()));
                    return Self { path: dir };
                }
                Err(_) => {
                    // Reclaim a stale lock (older than 10 minutes).
                    if let Ok(meta) = std::fs::metadata(&dir) {
                        if let Ok(age) =
                            meta.created().unwrap_or(meta.modified().unwrap()).elapsed()
                        {
                            if age > std::time::Duration::from_secs(600) {
                                let _ = std::fs::remove_dir_all(&dir);
                                continue;
                            }
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_millis(300));
                }
            }
        }
    }
}

impl Drop for RegistrationLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
