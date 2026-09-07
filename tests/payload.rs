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

    // Progress streamed and finished at 100%.
    assert!(matches!(events.first(), Some(FlowEvent::Progress { .. })));
    let last = events.last().unwrap();
    assert!(matches!(
        last,
        FlowEvent::Progress {
            percent: Some(100),
            ..
        }
    ));
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
