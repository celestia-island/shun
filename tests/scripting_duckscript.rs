//! Feasibility proof for the scripting design note
//! (docs/en/design/scripting.md): duckscript embeds as an ordinary
//! dependency — the SDK loads its command set into a runtime context,
//! scripts run with flow control and std fs, and shun can expose its own
//! built-ins by registering custom commands. Nothing here is wired into
//! the delivery runtime yet; this test keeps the option honest.

use duckscript::runner;
use duckscript::types::command::{Command, CommandInvocationContext, CommandResult};
use duckscript::types::runtime::Context;

use shun::targets::install::{InstallContext, LANGUAGE_ENV};

/// A stand-in for the future shun built-in surface: the progress
/// emitter, flow hooks, streaming helpers — all custom commands.
#[derive(Clone)]
struct ShunNote {
    calls: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

impl Command for ShunNote {
    fn name(&self) -> String {
        "shun_note".into()
    }

    fn help(&self) -> String {
        "record a note for the install flow".into()
    }

    fn clone_and_box(&self) -> Box<dyn Command> {
        Box::new(self.clone())
    }

    fn run(&self, context: CommandInvocationContext) -> CommandResult {
        self.calls
            .lock()
            .unwrap()
            .push(context.arguments.first().cloned().unwrap_or_default());
        CommandResult::Continue(Some("noted".into()))
    }
}

#[test]
fn duckscript_embeds_as_a_dependency() {
    let mut context = Context::new();
    duckscriptsdk::load(&mut context.commands).expect("sdk command set loads");

    let calls = ShunNote {
        calls: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
    };
    context
        .commands
        .set(Box::new(calls.clone()))
        .expect("custom command registers");

    // Flow control + std fs + the custom shun command, one script.
    // (duckscript treats a backslash as an escape in arguments, so the
    // path is passed in forward-slash form — std::fs accepts both.)
    let dir = tempfile::tempdir().unwrap();
    let marker = dir
        .path()
        .join("prepared.txt")
        .to_string_lossy()
        .replace('\\', "/");
    let script = format!(
        r#"
payload = set delivered-by-shun

writefile {} ${{payload}}
content = readfile {}
assert_eq ${{content}} ${{payload}}
shun_note ${{payload}}
"#,
        marker, marker
    );

    runner::run_script(&script, context, None).expect("script runs to completion");

    assert_eq!(
        std::fs::read_to_string(dir.path().join("prepared.txt"))
            .unwrap()
            .trim(),
        "delivered-by-shun"
    );
    assert_eq!(
        *calls.calls.lock().unwrap(),
        vec!["delivered-by-shun".to_string()]
    );
}

#[test]
fn script_env_reaches_a_duckscript_run() {
    // The delivery flow has no script runner wired in yet (the design
    // note records the decision) — this pins the contract its wiring
    // must keep: the pairs from `InstallContext::script_env` seed the
    // process environment, and the script reads the wizard language
    // back through the SDK's `get_env` (a std::env read).
    //
    // The seed/cleanup below mutates the process environment, which
    // other tests' `std::env::var` reads may race — hold this mutex for
    // the whole seed → run → cleanup span.
    static ENV_LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    let _guard = ENV_LOCK
        .get_or_init(std::sync::Mutex::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let mut ctx = InstallContext::new(
        "ShunDemo".into(),
        "0.0.1".into(),
        std::path::PathBuf::from("."),
        false,
    );
    ctx.language = Some("zh-Hans".into());

    for (key, value) in ctx.script_env() {
        // Safety: test-only process-global mutation, undone right after
        // the run under ENV_LOCK; the key is shun-owned and no other
        // test reads it.
        unsafe { std::env::set_var(&key, &value) };
    }
    let dir = tempfile::tempdir().unwrap();
    let marker = dir
        .path()
        .join("language.txt")
        .to_string_lossy()
        .replace('\\', "/");
    let script = format!(
        r#"
language = get_env {LANGUAGE_ENV}
writefile {marker} ${{language}}
"#
    );
    let mut context = Context::new();
    duckscriptsdk::load(&mut context.commands).expect("sdk command set loads");
    let result = runner::run_script(&script, context, None);
    // Safety: symmetric cleanup of the seeding above, under ENV_LOCK.
    unsafe { std::env::remove_var(LANGUAGE_ENV) };

    result.expect("script runs to completion");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("language.txt")).unwrap(),
        "zh-Hans"
    );
}
