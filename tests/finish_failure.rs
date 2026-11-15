//! The done-page confirmation's classified failures, against the REAL
//! launch path: a missing entry point and a non-executable entry point are
//! distinct kinds, and only the application-control codes carry a guide
//! topic. The Windows-only application-control code itself is pinned by
//! the unit tests in `shun::failure` (a Linux test host cannot mint a real
//! `os error 4551`), so this file covers the cross-platform arms of the
//! same contract.

mod common;

use std::collections::BTreeMap;
use std::path::Path;

use shun::config::{ShunConfig, TargetConfig};
use shun::failure::{FailureKind, FinishStage};
use shun::help::TOPIC_APP_CONTROL as HELP_TOPIC_APP_CONTROL;
use shun::wizard::{WizardCore, launch_installed_checked};

/// The demo manifest with its entry point retargeted at `main_exe`, so the
/// real launch path resolves a path under the test's own temp dir.
fn core_with_main_exe(main_exe: &str) -> WizardCore {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("demo-app")
        .join("Cargo.toml");
    let mut config = ShunConfig::from_cargo_manifest(&manifest).expect("demo manifest parses");
    let mut retargeted = false;
    for target in &mut config.targets {
        if let TargetConfig::Install(install) = target {
            install.main_exe = Some(std::path::PathBuf::from(main_exe));
            retargeted = true;
        }
    }
    assert!(retargeted, "the demo manifest declares an install target");
    WizardCore::new(config, BTreeMap::new())
}

#[test]
fn a_missing_entry_point_is_its_own_kind_without_a_guide() {
    let core = core_with_main_exe("bin/absent-app");
    let dir = tempfile::tempdir().unwrap();

    let failure = launch_installed_checked(&core, &dir.path().to_string_lossy())
        .expect_err("a missing main executable must fail");

    assert_eq!(failure.kind, FailureKind::MissingExecutable);
    assert_eq!(failure.stage, FinishStage::Launch);
    assert!(failure.os_error.is_none(), "no OS code for a path check");
    assert!(
        failure.help_topic.is_none(),
        "there is no guide for a missing file"
    );
    assert!(
        failure.message.contains("absent-app"),
        "{}",
        failure.message
    );
}

#[test]
fn a_present_but_unlaunchable_entry_point_is_plain_io_without_a_guide() {
    // A real file at the entry point: the OS refuses the process creation
    // (EACCES on unix, ERROR_BAD_EXE_FORMAT on Windows) and the failure is
    // an ordinary I/O error — NOT an application-control block, so no guide
    // may be offered.
    let core = core_with_main_exe("bin/not-an-executable");
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("bin")).unwrap();
    std::fs::write(
        dir.path().join("bin").join("not-an-executable"),
        b"not a program",
    )
    .unwrap();

    let failure = launch_installed_checked(&core, &dir.path().to_string_lossy())
        .expect_err("a non-executable file must not launch");

    assert_eq!(failure.stage, FinishStage::Launch);
    assert!(
        failure.os_error.is_some(),
        "the OS refused the spawn with a code: {}",
        failure.message
    );
    // The refusal's family is platform-shaped — unix answers EACCES
    // (a permissions refusal), Windows answers ERROR_BAD_EXE_FORMAT
    // (plain io) — but neither is ever an application-control block:
    // only the app-control OS codes may open the app-control guide.
    assert_ne!(failure.kind, FailureKind::AppControlBlocked);
    assert_ne!(failure.help_topic.as_deref(), Some(HELP_TOPIC_APP_CONTROL));
}
