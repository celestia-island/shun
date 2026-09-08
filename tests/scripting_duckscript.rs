//! Feasibility proof for the scripting design note
//! (docs/en/design/scripting.md): duckscript embeds as an ordinary
//! dependency — the SDK loads its command set into a runtime context,
//! scripts run with flow control and std fs, and shun can expose its own
//! built-ins by registering custom commands. Nothing here is wired into
//! the delivery runtime yet; this test keeps the option honest.

use duckscript::runner;
use duckscript::types::command::{Command, CommandInvocationContext, CommandResult};
use duckscript::types::runtime::Context;

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
