//! `install.launch-after-install` — the policy resolution against the
//! wizard answers, and the `launch` helper's contract.
//!
//! The spawn path itself is never exercised (a test must not start a real
//! application process): the helper's arms are pinned through the
//! disabled no-op and the missing-entry error instead.

mod common;

use shun::ShunError;
use shun::config::{InstallConfig, ShortcutPolicy};
use shun::targets::install::{InstallContext, WizardAnswers, launch};

/// `WizardAnswers` with the launch answer under test; the other policies
/// keep their default-checked values.
fn answers(launch: bool) -> WizardAnswers {
    WizardAnswers {
        launch_after_install: launch,
        ..WizardAnswers::defaults()
    }
}

/// An install config carrying only the launch policy under test.
fn config(policy: ShortcutPolicy) -> InstallConfig {
    InstallConfig {
        launch_after_install: policy,
        ..InstallConfig::default()
    }
}

#[test]
fn wizard_defaults_check_the_launch_box() {
    assert!(
        WizardAnswers::defaults().launch_after_install,
        "the done-page default is checked (the NSIS convention)"
    );
}

#[test]
fn a_hand_built_context_launches_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let context = common::ctx("ShunDemo-Test-Launch", dir.path().join("app"), false);
    assert!(
        !context.launch_after_install,
        "the no-op baseline launches nothing"
    );
}

#[test]
fn ask_consults_the_wizard_answer() {
    let dir = tempfile::tempdir().unwrap();
    let mut context = common::ctx("ShunDemo-Test-Launch", dir.path().join("app"), false);

    context.apply_config(&config(ShortcutPolicy::Ask), answers(true));
    assert!(context.launch_after_install, "ask follows a checked box");
    context.apply_config(&config(ShortcutPolicy::Ask), answers(false));
    assert!(
        !context.launch_after_install,
        "ask follows an unchecked box"
    );
}

#[test]
fn always_and_never_pin_the_policy() {
    let dir = tempfile::tempdir().unwrap();
    let mut context = common::ctx("ShunDemo-Test-Launch", dir.path().join("app"), false);

    context.apply_config(&config(ShortcutPolicy::Always), answers(false));
    assert!(
        context.launch_after_install,
        "always wins over the wizard answer"
    );
    context.apply_config(&config(ShortcutPolicy::Never), answers(true));
    assert!(
        !context.launch_after_install,
        "never wins over the wizard answer"
    );
}

#[test]
fn launch_is_a_no_op_without_the_flag_or_an_entry_point() {
    let dir = tempfile::tempdir().unwrap();
    let install_dir = dir.path().join("app");

    // The entry point is declared but absent on disk (the common fixture
    // stages no files): the resolved-false flag short-circuits before the
    // entry-point check.
    let context = common::ctx("ShunDemo-Test-Launch", install_dir.clone(), false);
    assert!(launch(&context).is_ok());

    // Enabled, but no entry point declared at all: still a no-op.
    let mut bare = InstallContext::new(
        "ShunDemo-Test-Launch".into(),
        "0.0.1".into(),
        install_dir,
        false,
    );
    bare.launch_after_install = true;
    assert!(launch(&bare).is_ok());
}

#[test]
fn launch_reports_the_missing_entry_point() {
    let dir = tempfile::tempdir().unwrap();
    let install_dir = dir.path().join("app");
    let mut context = common::ctx("ShunDemo-Test-Launch", install_dir.clone(), false);
    context.launch_after_install = true;

    match launch(&context) {
        Err(ShunError::MissingEntry(path)) => {
            assert_eq!(path, install_dir.join("bin/shun-demo.exe"));
        }
        other => panic!("expected the missing entry point, got {other:?}"),
    }
}
