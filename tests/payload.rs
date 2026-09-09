use std::path::PathBuf;

use shun::flow::FlowEvent;
use shun::payload::{ArchivePayload, PayloadSource, pack_directory};

fn payload_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/demo_payload")
}

#[test]
fn pack_extract_roundtrip_verifies_and_reports_progress() {
    let archive = pack_directory(&payload_dir()).unwrap();
    let payload = ArchivePayload::from_bytes(&archive).unwrap();

    let fixture_count = walkdir::WalkDir::new(payload_dir())
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .count();
    assert_eq!(payload.manifest().len(), fixture_count);

    let dest = tempfile::tempdir().unwrap();
    let mut events: Vec<FlowEvent> = Vec::new();
    payload
        .extract(dest.path(), &mut |e| events.push(e))
        .unwrap();

    // Every fixture file landed with identical bytes.
    for entry in payload.manifest() {
        let original = std::fs::read(payload_dir().join(&entry.path)).unwrap();
        let landed = std::fs::read(dest.path().join(&entry.path)).unwrap();
        assert_eq!(original, landed);
    }

    // Progress streamed and finished at 100% — the very first event may
    // now be the first file's log record, which is equally fine.
    assert!(matches!(
        events.first(),
        Some(FlowEvent::Progress { .. } | FlowEvent::Log { .. })
    ));
    let last = events.last().unwrap();
    assert!(matches!(
        last,
        FlowEvent::Progress {
            percent: Some(100),
            ..
        }
    ));

    // Every staged file logged a terminal record — one write per entry,
    // and each record precedes its progress tick.
    let writes: Vec<&shun::flow::FlowLog> = events
        .iter()
        .filter_map(|e| match e {
            FlowEvent::Log { record } => Some(record),
            _ => None,
        })
        .collect();
    assert_eq!(
        writes.len(),
        fixture_count,
        "one terminal line per payload file"
    );
    assert!(
        writes
            .iter()
            .all(|record| matches!(record, shun::flow::FlowLog::FileWrite { .. }))
    );
    // Each log record is immediately followed by its progress tick.
    let first_log = events
        .iter()
        .position(|e| matches!(e, FlowEvent::Log { .. }))
        .unwrap();
    assert!(matches!(events[first_log + 1], FlowEvent::Progress { .. }));
}

#[test]
fn garbage_bytes_are_rejected() {
    assert!(ArchivePayload::from_bytes(b"definitely not a shun payload").is_err());
}

#[test]
fn manifest_paths_are_relative() {
    let payload = ArchivePayload::from_bytes(&pack_directory(&payload_dir()).unwrap()).unwrap();
    for entry in payload.manifest() {
        assert!(entry.path.is_relative());
        assert!(entry.size > 0);
        assert_eq!(entry.sha256.len(), 64);
    }
}

/// Counts extraction events whose step label starts with `marker`.
fn count_steps(payload: &ArchivePayload, dest: &std::path::Path, marker: &str) -> usize {
    let mut count = 0;
    payload
        .extract(dest, &mut |event| {
            if let FlowEvent::Progress { step, .. } = event {
                if step.starts_with(marker) {
                    count += 1;
                }
            }
        })
        .unwrap();
    count
}

#[test]
fn extraction_reuses_identical_files_and_rewrites_drifted_ones() {
    // Build a tiny synthetic payload: two files, so drift can hit one.
    let source = tempfile::tempdir().unwrap();
    std::fs::write(source.path().join("a.txt"), b"alpha").unwrap();
    std::fs::create_dir(source.path().join("sub")).unwrap();
    std::fs::write(source.path().join("sub").join("b.txt"), b"beta").unwrap();
    let archive = pack_directory(source.path()).unwrap();
    let payload = ArchivePayload::from_bytes(&archive).unwrap();

    let dest = tempfile::tempdir().unwrap();
    let extracted = count_steps(&payload, dest.path(), "Extracting");
    assert_eq!(extracted, 2);

    // Second pass over identical bytes: every entry reused, none written.
    let reused = count_steps(&payload, dest.path(), "Reusing");
    assert_eq!(reused, 2);

    // Drift one file: it is rewritten, the untouched one still reused.
    std::fs::write(dest.path().join("a.txt"), b"drifted").unwrap();
    let reused = count_steps(&payload, dest.path(), "Reusing");
    assert_eq!(reused, 1);
    assert_eq!(std::fs::read(dest.path().join("a.txt")).unwrap(), b"alpha");
}

#[test]
fn extract_prefix_stages_only_the_subtree() {
    let source = tempfile::tempdir().unwrap();
    std::fs::write(source.path().join("top.txt"), b"top").unwrap();
    std::fs::create_dir_all(source.path().join("WebView2Runtime").join("x64")).unwrap();
    std::fs::write(
        source
            .path()
            .join("WebView2Runtime")
            .join("x64")
            .join("engine.bin"),
        b"engine",
    )
    .unwrap();
    let archive = pack_directory(source.path()).unwrap();
    let payload = ArchivePayload::from_bytes(&archive).unwrap();

    let dest = tempfile::tempdir().unwrap();
    let written = payload
        .extract_prefix(
            dest.path(),
            std::path::Path::new("WebView2Runtime"),
            &mut |_| {},
        )
        .unwrap();
    assert_eq!(written, b"engine".len() as u64);
    assert!(
        dest.path()
            .join("WebView2Runtime")
            .join("x64")
            .join("engine.bin")
            .is_file()
    );
    assert!(!dest.path().join("top.txt").exists());

    // Restaging the same subtree reuses everything: zero bytes written.
    let written = payload
        .extract_prefix(
            dest.path(),
            std::path::Path::new("WebView2Runtime"),
            &mut |_| {},
        )
        .unwrap();
    assert_eq!(written, 0);

    // Unknown prefixes are an error, not an empty stage.
    assert!(
        payload
            .extract_prefix(dest.path(), std::path::Path::new("nope"), &mut |_| {})
            .is_err()
    );
}
