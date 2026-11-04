//! The running-instance guard against REAL processes: a spawned ping
//! whose image the Restart Manager registers by its exact path must be
//! found by that path, must not be confused with a same-named process
//! running from a different directory (the discrimination the old
//! `taskkill /IM` guard never had), and must be gone after the
//! graceful-then-forced shutdown. Windows-only — the Restart Manager
//! is a Windows API.

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

/// Spawns `image` through `cmd /c` — the literal `cmd` program keeps
/// the spawn itself static while the copied image rides as an
/// argument. `cmd` waits for the image to exit.
#[cfg(windows)]
fn spawn_holder_via_cmd(image: &Path) -> std::process::Child {
    Command::new("cmd")
        .arg("/c")
        .arg(image.as_os_str())
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

#[cfg(windows)]
#[test]
fn a_same_named_process_at_another_path_is_not_matched() {
    use shun::targets::instance::{running_instances, stop_instances};

    // A copy of ping under a temp directory: same image bytes, a
    // DIFFERENT image path. The old name-based guard would have killed
    // it together with the original; the Restart Manager matches by
    // path and must keep the two apart.
    let dir = tempfile::tempdir().unwrap();
    let copy = dir.path().join("ping-copy.exe");
    std::fs::copy(system_ping(), &copy).expect("copy the ping image");
    let mut holder = spawn_holder_via_cmd(&copy);

    // The copy shows up under its own path — the pids it lists are the
    // cmd-wrapped holder's family, none of which may leak into a query
    // against the system path.
    let copy_instances = wait_for_instances(&copy, Duration::from_secs(5));
    assert!(
        !copy_instances.is_empty(),
        "the copied holder is detected under its own path"
    );
    let copy_pids: std::collections::HashSet<u32> = copy_instances.iter().map(|i| i.pid).collect();
    assert!(
        !running_instances(&system_ping())
            .expect("the sibling detection session opens")
            .iter()
            .any(|i| copy_pids.contains(&i.pid)),
        "the copy must not be matched against the system path"
    );

    // The shutdown takes the copy down and clears its path.
    let survivors = stop_instances(&copy).expect("the shutdown session opens");
    assert!(
        survivors.is_empty(),
        "the copied holder stops: {survivors:?}"
    );
    let _ = holder.wait();
    assert!(
        running_instances(&copy)
            .expect("the post-shutdown session opens")
            .is_empty(),
        "the copy's path is clear afterwards"
    );
}
