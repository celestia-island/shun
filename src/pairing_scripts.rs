//! The scripts lane of the pairing step: runs a product's `python3`
//! helper per phase and speaks JSON with the prefabricated pane.
//!
//! The contract (documented on [`PairingSource::Scripts`] in
//! `config.rs`):
//! - `python3 <script>` (falling back to `python` when the host only
//!   provides the unversioned name — Windows installers do);
//! - the pane's answers travel as ONE JSON object on **stdin**, never
//!   argv: process listings are world-readable on every platform and
//!   the answers can carry device identity;
//! - stdout must be exactly one JSON value — the phase's answer;
//! - a non-zero exit is an error whose (trimmed) stderr becomes the
//!   pane's error line, so a script can explain itself;
//! - every phase is bounded by a deadline — a hung script is killed and
//!   reported, never wedging the wizard.
//!
//! Gateway-lane users never touch this module; it needs no network
//! stack, so it compiles on every feature combination.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How often the deadline loop checks on the child.
const POLL: Duration = Duration::from_millis(50);

/// Runs one pairing phase script and returns its JSON answer.
///
/// `answers` is the pane's current answers (identity fields, gateway
/// choice, the displayed code for `await`) serialized onto the script's
/// stdin; the script's stdout must parse as one JSON value. The call
/// blocks for at most `timeout`, killing the script past it.
pub fn run_pairing_script(
    script: &Path,
    answers: &serde_json::Value,
    timeout: Duration,
) -> Result<serde_json::Value, String> {
    let interpreter = ["python3", "python"]
        .iter()
        .find(|probe| which(probe).is_some())
        .ok_or_else(|| {
            "no python3 interpreter on PATH — the scripts pairing lane \
             needs one"
                .to_string()
        })?;

    if !script.is_file() {
        return Err(format!("pairing script {script:?} not found"));
    }
    let mut child = Command::new(interpreter)
        .arg(script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn {script:?}: {e}"))?;

    // Feed the answers, then close stdin so the script can finish
    // reading. A broken pipe here (the script exited early) is fine —
    // its output and status decide the verdict, not our write.
    {
        use std::io::Write as _;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = serde_json::to_writer(&mut stdin, answers);
            let _ = stdin.write_all(b"\n");
            // Dropped here: closes the pipe so the script finishes reading.
        }
    }

    // Drain both pipes on readers so a chatty script cannot deadlock on
    // a full pipe buffer while we wait.
    let mut stdout = child.stdout.take().expect("piped above");
    let mut stderr = child.stderr.take().expect("piped above");
    let readers = std::thread::scope(|scope| {
        let out = scope.spawn(move || read_all(&mut stdout));
        let err = scope.spawn(move || read_all(&mut stderr));
        // Poll the deadline while the readers drain.
        let started = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => {
                    if started.elapsed() >= timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        break Err(format!(
                            "pairing script {script:?} timed out after \
                             {}s",
                            timeout.as_secs()
                        ));
                    }
                    std::thread::sleep(POLL);
                }
                Err(e) => break Err(format!("wait for {script:?}: {e}")),
            }
        };
        (status, out.join(), err.join())
    });

    let (status, out_bytes, err_bytes) = readers;
    let status = status?;
    let out_bytes = out_bytes.unwrap_or_default();
    let err_bytes = err_bytes.unwrap_or_default();
    let stdout = String::from_utf8_lossy(&out_bytes);
    let stderr = String::from_utf8_lossy(&err_bytes);

    if !status.success() {
        let message = stderr.trim();
        return Err(if message.is_empty() {
            format!("pairing script {script:?} exited with {status}")
        } else {
            message.to_string()
        });
    }
    serde_json::from_str(stdout.trim())
        .map_err(|e| format!("pairing script {script:?} printed no JSON: {e}"))
}

fn read_all(pipe: &mut impl Read) -> Vec<u8> {
    let mut bytes = Vec::new();
    let _ = pipe.read_to_end(&mut bytes);
    bytes
}

/// The thinnest possible `which`: zero subprocesses, PATH + executable
/// bit / extension check only.
fn which(program: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    #[cfg(windows)]
    let extensions: &[&str] = &[".exe", ".cmd", ".bat", ""];
    #[cfg(not(windows))]
    let extensions: &[&str] = &[""];
    for dir in std::env::split_paths(&path) {
        for ext in extensions {
            let candidate = dir.join(format!("{program}{ext}"));
            if is_executable(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

fn is_executable(path: &std::path::PathBuf) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scratch_script(name: &str, body: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("shun-pairing-scripts-lane");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
        }
        path
    }

    fn python(body: &str) -> String {
        format!("#!/usr/bin/env python3\n{body}\n")
    }

    #[test]
    fn a_happy_script_reads_answers_and_answers_json() {
        let script = scratch_script(
            "happy.py",
            &python(
                "import json, sys\n\
                 answers = json.load(sys.stdin)\n\
                 print(json.dumps({\"code\": \"7QK2M4XP\", \"echo\": answers[\"node_id\"]}))",
            ),
        );
        let answers = serde_json::json!({ "node_id": "node-1" });
        let answer = run_pairing_script(&script, &answers, Duration::from_secs(15)).unwrap();
        assert_eq!(answer["code"], "7QK2M4XP");
        assert_eq!(answer["echo"], "node-1", "stdin carried the answers");
        let _ = std::fs::remove_file(&script);
    }

    #[test]
    fn a_failing_script_surfaces_its_stderr() {
        let script = scratch_script(
            "failing.py",
            &python(
                "import sys\n\
                 print(\"the gateway is unreachable\", file=sys.stderr)\n\
                 sys.exit(2)",
            ),
        );
        let err = run_pairing_script(&script, &serde_json::json!({}), Duration::from_secs(15))
            .unwrap_err();
        assert!(err.contains("the gateway is unreachable"), "{err}");
        let _ = std::fs::remove_file(&script);
    }

    #[test]
    fn non_json_stdout_is_rejected() {
        let script = scratch_script("noisy.py", &python("print(\"hello, not json\")"));
        let err = run_pairing_script(&script, &serde_json::json!({}), Duration::from_secs(15))
            .unwrap_err();
        assert!(err.contains("no JSON"), "{err}");
        let _ = std::fs::remove_file(&script);
    }

    #[test]
    fn a_hung_script_is_killed_at_the_deadline() {
        let script = scratch_script(
            "hung.py",
            &python("import time\ntime.sleep(30)\nprint(\"{}\")"),
        );
        let started = Instant::now();
        let err = run_pairing_script(&script, &serde_json::json!({}), Duration::from_secs(1))
            .unwrap_err();
        assert!(err.contains("timed out"), "{err}");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the kill must not wait for the script: {:?}",
            started.elapsed()
        );
        let _ = std::fs::remove_file(&script);
    }

    #[test]
    fn a_missing_script_reports_the_spawn_failure() {
        let err = run_pairing_script(
            Path::new("/nonexistent/lane.py"),
            &serde_json::json!({}),
            Duration::from_secs(5),
        )
        .unwrap_err();
        assert!(err.contains("not found"), "{err}");
    }

    #[test]
    fn which_finds_the_interpreter_the_lane_will_use() {
        // The test host has python3 (the lane tests depend on it) — the
        // resolution contract, not the interpreter, is what is pinned.
        assert!(which("python3").is_some() || which("python").is_some());
    }
}
