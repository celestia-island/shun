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

/// The launch answer × policy matrix: nothing launches without a
/// declaration, `never` stays off, `always` ignores the answer, and
/// `ask` follows it.
#[test]
fn the_done_page_answer_and_policy_decide_both_launches() {
    // No declaration: the app launch keeps its premise, the companion
    // stays off whatever the answer.
    for answer in [false, true] {
        let targets = finish_launch_targets(&config_with(None), answer).unwrap();
        assert_eq!(targets.app, answer);
        assert!(!targets.companion, "no declaration launches nothing");
    }

    let cases = [
        (PostInstallLaunchPolicy::Never, false, false),
        (PostInstallLaunchPolicy::Never, true, false),
        (PostInstallLaunchPolicy::Always, false, true),
        (PostInstallLaunchPolicy::Always, true, true),
        (PostInstallLaunchPolicy::Ask, false, false),
        (PostInstallLaunchPolicy::Ask, true, true),
    ];
    for (policy, answer, expected) in cases {
        let config = config_with(Some(declared("bin/evernight.exe", &pairer_args(), policy)));
        let targets = finish_launch_targets(&config, answer).unwrap();
        assert_eq!(
            targets.companion, expected,
            "policy {policy:?} with the answer {answer} launches {expected}"
        );
        assert_eq!(targets.app, answer, "the app launch keeps its own policy");
    }

    // A declaration that escapes the payload fails the resolution before
    // any process starts, whatever the answer. The absolute path is
    // spelled host-independently (a temp-dir join), so the case is the
    // same on the Windows runner.
    let absolute = std::env::temp_dir().join("pairer");
    let poisoned = config_with(Some(declared(
        &absolute.to_string_lossy(),
        &[],
        PostInstallLaunchPolicy::Always,
    )));
    let err = finish_launch_targets(&poisoned, true)
        .expect_err("an absolute executable is a configuration error");
    assert!(err.contains("relative to the payload"), "{err}");
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
    let targets = finish_launch_targets(&config, true).unwrap();
    assert!(targets.app && targets.companion, "both ride the answer");
    let core = WizardCore::new(config, Default::default());
    // The done page resolves the same companion off the core's config.
    assert!(core.config.post_install_launch().unwrap().is_some());
}
