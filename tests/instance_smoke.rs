//! A Restart Manager smoke test that only queries — no process spawn
//! and no shutdown. If even this dies on a runner, the Restart Manager
//! session API itself is unwelcome there; if it survives, the killer
//! was the live probe's spawn-and-terminate pattern (runner
//! anti-malware), not the session API.

#[cfg(windows)]
#[test]
fn a_query_over_an_idle_path_survives_everywhere() {
    use shun::targets::instance::running_instances;

    // An existing file nothing runs from: the registration succeeds
    // and the match list is empty.
    let idle = std::env::temp_dir().join("shun-rm-smoke-idle.exe");
    std::fs::write(&idle, b"smoke").expect("the idle smoke file is created");
    let instances = running_instances(&idle).expect("the smoke query session opens");
    assert!(
        instances.is_empty(),
        "nothing runs from an idle path: {instances:?}"
    );
    std::fs::remove_file(&idle).ok();
}
