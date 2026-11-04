//! The running-instance guard against a REAL process: a spawned ping
//! whose image the Restart Manager registers by its exact path must be
//! found by that path and be gone after the graceful-then-forced
//! shutdown. Windows-only — the Restart Manager is a Windows API.
//!
//! The path-discrimination probe lives in its own test binary
//! (`instance_path_discrimination.rs`) — a shared harness died
//! instantly on the windows-latest runner (exit 1, no failure report),
//! and separate binaries isolate the blast radius.

#[cfg(windows)]
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::process::{Command, Stdio};
#[cfg(windows)]
use std::time::Duration;

/// The System32 ping.exe path — a real image at a stable path. `ping`
/// resolves through PATH to exactly this image.
#[cfg(windows)]
fn system_ping() -> PathBuf {
    std::env::var("WINDIR")
        .map(|windows_dir| Path::new(&windows_dir).join("System32").join("ping.exe"))
        .unwrap_or_else(|_| PathBuf::from(r"C:\Windows\System32\ping.exe"))
}

/// Spawns the System32 ping as a long-running loop (~90 s of self-exit
/// slack should an assertion fail before the shutdown runs).
#[cfg(windows)]
fn spawn_ping_holder() -> std::process::Child {
    Command::new("ping")
        .args(["-n", "90", "127.0.0.1"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn the holder process")
}

/// Polls the detection until ANY instance shows up for `exe` (the
/// spawn returns before the process is necessarily visible to a
/// Restart Manager query); empty when the deadline passes first.
#[cfg(windows)]
fn wait_for_instances(
    exe: &Path,
    deadline: Duration,
) -> Vec<shun::targets::instance::RunningInstance> {
    let started = std::time::Instant::now();
    loop {
        match shun::targets::instance::running_instances(exe) {
            Ok(instances) if !instances.is_empty() => return instances,
            _ => {}
        }
        if started.elapsed() >= deadline {
            return Vec::new();
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(windows)]
#[test]
fn detects_and_stops_a_running_instance_by_exact_path() {
    use shun::targets::instance::{running_instances, stop_instances};

    let ping = system_ping();
    let mut child = spawn_ping_holder();

    // The instance is found by the image's exact path, and the child's
    // pid is among the matches.
    let detected = wait_for_instances(&ping, Duration::from_secs(5));
    assert!(
        detected.iter().any(|i| i.pid == child.id()),
        "the spawned holder is detected by path: {detected:?}"
    );

    // The shutdown takes every instance down and verifies the ground
    // is clear afterwards.
    let survivors = stop_instances(&ping).expect("the shutdown session opens");
    assert!(
        survivors.is_empty(),
        "nothing survives the shutdown: {survivors:?}"
    );
    let _ = child.wait();

    let detected = running_instances(&ping).expect("the post-shutdown session opens");
    assert!(
        !detected.iter().any(|i| i.pid == child.id()),
        "the holder is gone: {detected:?}"
    );
}
