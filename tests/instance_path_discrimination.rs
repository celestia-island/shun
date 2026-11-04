//! The path-discrimination probe, in its own test binary: a 32-bit
//! ping under SysWOW64 carries the same image NAME as the System32 one
//! at a genuinely different image PATH — exactly the discrimination
//! the old name-based taskkill guard never had. The Restart Manager
//! matches by path and must keep the two apart.
//!
//! Separate binary on purpose: a shared harness died instantly on the
//! windows-latest runner (exit 1, no failure report), so the surviving
//! binary keeps reporting while the dying one identifies the killer.

#[cfg(windows)]
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::process::{Command, Stdio};
#[cfg(windows)]
use std::time::Duration;

/// The System32 ping.exe path — `ping` on PATH resolves to it.
#[cfg(windows)]
fn system_ping() -> PathBuf {
    std::env::var("WINDIR")
        .map(|windows_dir| Path::new(&windows_dir).join("System32").join("ping.exe"))
        .unwrap_or_else(|_| PathBuf::from(r"C:\Windows\System32\ping.exe"))
}

/// The 32-bit ping under SysWOW64 — absent only where WoW64 is (a
/// non-x64 host).
#[cfg(windows)]
fn wow64_ping() -> PathBuf {
    std::env::var("WINDIR")
        .map(|windows_dir| Path::new(&windows_dir).join("SysWOW64").join("ping.exe"))
        .unwrap_or_else(|_| PathBuf::from(r"C:\Windows\SysWOW64\ping.exe"))
}

/// Spawns `image` through `cmd /c` — the literal `cmd` program keeps
/// the spawn itself static while the image path rides as an argument.
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

/// Polls the detection until ANY instance shows up for `exe`; empty
/// when the deadline passes first.
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
fn a_same_named_process_at_another_path_is_not_matched() {
    use shun::targets::instance::{running_instances, stop_instances};

    let wow_ping = wow64_ping();
    if !wow_ping.is_file() {
        eprintln!("SysWOW64\\ping.exe absent on this host — probe skipped");
        return;
    }
    let mut holder = spawn_holder_via_cmd(&wow_ping);

    // The WOW64 holder shows up under its own path — its pids may
    // never leak into a query against the System32 path.
    let wow_instances = wait_for_instances(&wow_ping, Duration::from_secs(5));
    assert!(
        !wow_instances.is_empty(),
        "the WOW64 holder is detected under its own path: {wow_instances:?}"
    );
    let wow_pids: std::collections::HashSet<u32> = wow_instances.iter().map(|i| i.pid).collect();
    assert!(
        !running_instances(&system_ping())
            .expect("the sibling detection session opens")
            .iter()
            .any(|i| wow_pids.contains(&i.pid)),
        "the WOW64 holder must not be matched against the System32 path"
    );

    // The shutdown takes the WOW64 holder down and clears its path.
    let survivors = stop_instances(&wow_ping).expect("the shutdown session opens");
    assert!(
        survivors.is_empty(),
        "the WOW64 holder stops: {survivors:?}"
    );
    let _ = holder.wait();
    assert!(
        running_instances(&wow_ping)
            .expect("the post-shutdown session opens")
            .is_empty(),
        "the WOW64 holder's path is clear afterwards"
    );
}
