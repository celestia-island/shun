//! `install.post-install-launch` — the declarative handoff from the
//! installer to a payload-carried companion program (a gateway pairer,
//! an activation helper): where it lives, the fixed argv it receives, who
//! decides whether it runs, and that nothing else can run it.
//!
//! The argv shape is proven with a REAL process: the companion here is
//! `/bin/sh` used as a *target* executable that writes `$0` and each
//! positional parameter to a file. Nothing is ever handed to a shell —
//! each argument must land as exactly one `$N`, so an argument carrying
//! spaces, quotes or `$(…)` proves it was passed as data rather than
//! re-parsed as a command line.

use std::path::{Path, PathBuf};

use shun::config::{
    InstallConfig, PostInstallLaunchConfig, PostInstallLaunchPolicy, ShunConfig, TargetConfig,
};
use shun::targets::install::{InstallContext, launch_post_install};
use shun::wizard::{WizardCore, finish_launch_targets};

/// A minimal config with the install target carrying `launch`.
fn config_with(launch: Option<PostInstallLaunchConfig>) -> ShunConfig {
    ShunConfig {
        product: shun::config::ProductIdentity {
            name: "ShunDemo-Test-Handoff".into(),
            version: "0.0.1".into(),
            publisher: Some("celestia-island".into()),
            logo: None,
        },
        payload: None,
        webview2: None,
        script: None,
        targets: vec![TargetConfig::Install(InstallConfig {
            post_install_launch: launch,
            ..InstallConfig::default()
        })],
        shell: None,
        source: None,
        variants: None,
        update: None,
        attachments: Vec::new(),
        license_sysl: None,
        msix: None,
        license: None,
        license_locales: Default::default(),
        licenses: Vec::new(),
        custom_steps: Vec::new(),
        steps: None,
        signing: None,
    }
}

fn declared(exe: &str, args: &[&str], policy: PostInstallLaunchPolicy) -> PostInstallLaunchConfig {
    PostInstallLaunchConfig {
        exe: PathBuf::from(exe),
        args: args.iter().map(|arg| arg.to_string()).collect(),
        policy,
    }
}

/// The fixed argv shape the chest/evernight payload uses.
fn pairer_args() -> Vec<&'static str> {
    vec!["pair", "--gateway", "https://gateway.example.world"]
}

/// The app launch answer × companion policy matrix: the two are
/// INDEPENDENT. `always` starts the companion even when the app box was
/// cleared, `never` stays off even when it was ticked, `ask` follows its
/// own answer, and the app starts only when its box was ticked.
#[test]
fn the_app_answer_and_the_companion_policy_are_independent() {
    // No declaration: the app launch keeps its answer, the companion
    // stays off whatever the answer.
    for launch in [false, true] {
        for companion in [false, true] {
            let targets = finish_launch_targets(&config_with(None), launch, companion).unwrap();
            assert_eq!(targets.app, launch);
            assert!(!targets.companion, "no declaration launches nothing");
        }
    }

    // (policy, app box, companion answer) → (app launch, companion launch)
    let cases = [
        (PostInstallLaunchPolicy::Always, false, false, false, true),
        (PostInstallLaunchPolicy::Always, false, true, false, true),
        (PostInstallLaunchPolicy::Always, true, false, true, true),
        (PostInstallLaunchPolicy::Always, true, true, true, true),
        (PostInstallLaunchPolicy::Never, false, true, false, false),
        (PostInstallLaunchPolicy::Never, true, true, true, false),
        (PostInstallLaunchPolicy::Ask, false, false, false, false),
        (PostInstallLaunchPolicy::Ask, false, true, false, true),
        (PostInstallLaunchPolicy::Ask, true, false, true, false),
        (PostInstallLaunchPolicy::Ask, true, true, true, true),
    ];
    for (policy, app_box, answer, app, companion) in cases {
        let config = config_with(Some(declared("bin/evernight.exe", &pairer_args(), policy)));
        let targets = finish_launch_targets(&config, app_box, answer).unwrap();
        assert_eq!(
            (targets.app, targets.companion),
            (app, companion),
            "policy {policy:?}, app box {app_box}, companion answer {answer}"
        );
    }
}

/// The regression this matrix exists for: a user who CLEARS the app box
/// must still get a `policy = "always"` companion (the bootstrap is not
/// the app), and a `never` companion must not ride a ticked box.
#[test]
fn an_always_companion_survives_a_cleared_app_box() {
    let always = config_with(Some(declared(
        "bin/evernight.exe",
        &pairer_args(),
        PostInstallLaunchPolicy::Always,
    )));
    let targets = finish_launch_targets(&always, false, false).unwrap();
    assert!(!targets.app, "the cleared box keeps the app closed");
    assert!(targets.companion, "the companion is always-pinned");

    let never = config_with(Some(declared(
        "bin/evernight.exe",
        &pairer_args(),
        PostInstallLaunchPolicy::Never,
    )));
    let targets = finish_launch_targets(&never, true, true).unwrap();
    assert!(targets.app);
    assert!(!targets.companion, "never wins over every answer");
}

/// An escaping declaration fails the CONFIG LOAD — `resolve_steps`, the
/// pipeline every face resolves before it can install anything — not
/// only the launch step on the done page. The declaration is read back
/// through the same accessor the delivery lanes use, so the load check
/// and the launch check cannot drift apart.
#[test]
fn an_escaping_companion_fails_the_config_load() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("shun.toml");
    std::fs::write(
        &manifest,
        "product = \"Escaping\"\n\n[install]\npost-install-launch = { exe = \"../../pairer\" }\n",
    )
    .unwrap();
    let config = ShunConfig::from_path(&manifest).unwrap();
    let err = config
        .resolve_steps(dir.path(), None)
        .expect_err("the load-time validation refuses the declaration");
    assert!(err.to_string().contains("escapes"), "{err}");

    // A well-formed declaration loads exactly as before.
    let clean = dir.path().join("clean.toml");
    std::fs::write(
        &clean,
        "product = \"Clean\"\n\n[install]\npost-install-launch = { exe = \"bin/pairer.exe\", args = [\"pair\"] }\n",
    )
    .unwrap();
    let config = ShunConfig::from_path(&clean).unwrap();
    config
        .resolve_steps(dir.path(), None)
        .expect("a payload-relative declaration loads");
}

/// The companion runs with EXACTLY the declared argv — no shell in
/// between, no word splitting, no command substitution — and its stdout
/// is the proof. One argument with spaces, quotes, `$HOME` and `$(…)`
/// arrives as one `$4`.
#[cfg(unix)]
#[test]
fn the_companion_receives_the_fixed_argv_without_a_shell() {
    let dir = tempfile::tempdir().unwrap();
    let install_dir = dir.path().join("app");
    let bin = install_dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let record = dir.path().join("argv.txt");
    // `/bin/sh` as the companion: it prints `$0` then one line per
    // positional parameter, so the file is the argv the OS delivered.
    std::fs::write(&record, b"").unwrap();
    let script = format!(
        "#!/bin/sh\n{{ echo \"$0\"; for a in \"$@\"; do echo \"$a\"; done; }} > '{}'\n",
        record.display()
    );
    let caret = bin.join("pairer");
    std::fs::write(&caret, script.as_bytes()).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&caret, std::fs::Permissions::from_mode(0o755)).unwrap();

    let hostile = "two words; echo pwned $(id) \"quoted\" $HOME";
    let declaration = declared(
        "bin/pairer",
        &[
            "pair",
            "--gateway",
            "https://gateway.example.world",
            hostile,
        ],
        PostInstallLaunchPolicy::Always,
    );
    let ctx = InstallContext::new(
        "ShunDemo-Test-Handoff".into(),
        "0.0.1".into(),
        install_dir.clone(),
        false,
    );
    launch_post_install(&ctx, &declaration).expect("the companion spawns");

    let mut observed = wait_for_lines(&record, 5);
    let program = observed.remove(0);
    assert_eq!(
        Path::new(&program),
        caret,
        "the companion runs from the install directory"
    );
    assert_eq!(
        observed,
        vec![
            "pair".to_string(),
            "--gateway".to_string(),
            "https://gateway.example.world".to_string(),
            // Verbatim: one argument, not four, and nothing expanded.
            hostile.to_string(),
        ],
        "the argv arrives whole — no shell, no splitting, no expansion"
    );
}

/// Waits for the companion's detached write, then returns its lines.
#[cfg(unix)]
fn wait_for_lines(path: &Path, expected: usize) -> Vec<String> {
    for _ in 0..200 {
        let Ok(text) = std::fs::read_to_string(path) else {
            std::thread::sleep(std::time::Duration::from_millis(25));
            continue;
        };
        let lines: Vec<String> = text.lines().map(str::to_string).collect();
        if lines.len() >= expected {
            return lines;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    let text = std::fs::read_to_string(path).unwrap_or_default();
    panic!("the companion never wrote {expected} lines (got {text:?})");
}

/// A declaration whose executable is missing fails, and the message
/// names the payload-relative form — never the hidden `.shun-partial`
/// staging path the install directory spells.
#[test]
fn a_missing_companion_fails_by_its_payload_name() {
    let dir = tempfile::tempdir().unwrap();
    let install_dir = dir.path().join("app");
    std::fs::create_dir_all(&install_dir).unwrap();
    let ctx = InstallContext::new(
        "ShunDemo-Test-Handoff".into(),
        "0.0.1".into(),
        install_dir.clone(),
        false,
    );
    let declaration = declared(
        "bin/evernight.exe",
        &pairer_args(),
        PostInstallLaunchPolicy::Ask,
    );
    let err =
        launch_post_install(&ctx, &declaration).expect_err("the declared executable is not there");
    let message = err.to_string();
    assert!(
        message.contains("bin/evernight.exe"),
        "the failure names the payload path: {message}"
    );
    assert!(
        !message.contains("shun-partial"),
        "the failure never prints the staging spelling: {message}"
    );
}

/// The executable is joined strictly under the install directory — a
/// traversal or absolute declaration is refused before the spawn, so a
/// poisoned config can never run a system binary through the handoff.
#[test]
fn an_escaping_companion_is_refused_before_any_spawn() {
    let dir = tempfile::tempdir().unwrap();
    let install_dir = dir.path().join("app");
    std::fs::create_dir_all(&install_dir).unwrap();
    let ctx = InstallContext::new(
        "ShunDemo-Test-Handoff".into(),
        "0.0.1".into(),
        install_dir.clone(),
        false,
    );
    for refused in [
        "../outside/pairer",
        "/bin/sh",
        "bin/../../pairer",
        r"C:\Windows\System32\cmd.exe",
    ] {
        let declaration = declared(refused, &[], PostInstallLaunchPolicy::Always);
        let err = launch_post_install(&ctx, &declaration)
            .err()
            .unwrap_or_else(|| panic!("{refused} must be refused"));
        assert!(
            err.to_string().contains("post-install-launch"),
            "the refusal names the declaration: {err}"
        );
    }
}

/// The declarations the app manifest itself uses: a `pairer` under
/// `bin/` with the gateway argv — the shape chest ships.
#[test]
fn the_pairer_shape_resolves_through_a_hand_built_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("Cargo.toml");
    std::fs::write(
        &manifest,
        r#"
[package]
name = "shun-chest"
version = "0.0.1"

[package.metadata.shun.install.post-install-launch]
exe = "bin/evernight.exe"
args = ["pair", "--gateway", "https://gateway.example.world"]
policy = "ask"
"#,
    )
    .unwrap();
    let config = ShunConfig::from_cargo_manifest(&manifest).unwrap();
    let declared = config
        .post_install_launch()
        .unwrap()
        .expect("the declaration resolves");
    assert_eq!(declared.exe, PathBuf::from("bin/evernight.exe"));
    assert_eq!(
        declared.args,
        vec!["pair", "--gateway", "https://gateway.example.world"]
    );
    // Load-time validation passes for the shipping shape...
    let dir = tempfile::tempdir().unwrap();
    config
        .resolve_steps(dir.path(), None)
        .expect("the pairer declaration loads");
    // ...and the done page launches both off the same answer.
    let targets = finish_launch_targets(&config, true, true).unwrap();
    assert!(targets.app && targets.companion, "both ride the answer");
    let core = WizardCore::new(config, Default::default());
    // The done page resolves the same companion off the core's config.
    assert!(core.config.post_install_launch().unwrap().is_some());
}

/// The missing companion classifies like the missing app executable:
/// `FinishStage::Launch` + `FailureKind::MissingExecutable` + the
/// missing-files help topic. The error is observed through the REAL
/// classification entry point — `apply_finish_checked`, the done-page
/// chain both faces call — rather than a hand-built `ShunError`, so a
/// regression in the wizard's error shape turns this red.
#[test]
fn a_missing_companion_classifies_as_missing_executable() {
    use shun::failure::{FailureKind, FinishFailure, FinishStage};
    use shun::help::TOPIC_MISSING_FILES;

    let dir = tempfile::tempdir().unwrap();
    let install_dir = dir.path().join("app");
    std::fs::create_dir_all(&install_dir).unwrap();
    // The app entry point EXISTS (the app launch succeeds and stays out
    // of the way); the companion was never delivered.
    std::fs::create_dir_all(install_dir.join("bin")).unwrap();
    std::fs::write(install_dir.join("bin").join("shun-demo.exe"), b"MZ").unwrap();

    let config = config_with(Some(declared(
        "bin/evernight.exe",
        &pairer_args(),
        PostInstallLaunchPolicy::Always,
    )));
    let core = WizardCore::new(config, Default::default());
    let failure: FinishFailure = shun::wizard::apply_finish_checked(
        &core,
        &install_dir.to_string_lossy(),
        Some(false),
        Some(false),
        false,
    )
    .expect_err("the declared companion was never delivered");

    assert_eq!(failure.stage, FinishStage::Launch);
    assert_eq!(failure.kind, FailureKind::MissingExecutable);
    assert_eq!(failure.help_topic.as_deref(), Some(TOPIC_MISSING_FILES));
    assert!(failure.has_help(), "the user gets the missing-files guide");
    assert!(
        failure.message.contains("evernight.exe"),
        "the message names the undelivered file: {}",
        failure.message
    );

    // The app launch's own missing entry lands on the same kind/topic.
    let app = FinishFailure::from_error(
        FinishStage::Launch,
        &shun::ShunError::MissingEntry(install_dir.join("bin/shun-demo.exe")),
    );
    assert_eq!(app.kind, failure.kind);
    assert_eq!(app.help_topic, failure.help_topic);
}
