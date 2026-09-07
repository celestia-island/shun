use std::io::{Read, Write};
use std::path::PathBuf;

use shun::payload::pack_directory;
use shun::payload_online::OnlinePayload;

/// A tiny HTTP/1.1 server serving exactly one GET response — enough to
/// exercise the online payload source without leaving the crate.
fn serve_one(body: Vec<u8>) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            let n = stream.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            request.extend_from_slice(&buf[..n]);
            if request.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(header.as_bytes()).unwrap();
        stream.write_all(&body).unwrap();
    });
    format!("http://127.0.0.1:{port}/ShunDemo.shun")
}

#[test]
fn online_payload_streams_into_target() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let archive = pack_directory(&manifest_dir.join("examples/demo_payload")).unwrap();
    let url = serve_one(archive);

    let dest = tempfile::tempdir().unwrap();
    let mut phases = Vec::new();
    OnlinePayload::new(url)
        .extract(dest.path(), &mut |event| {
            if let shun::flow::FlowEvent::Progress { phase, .. } = event {
                phases.push(phase);
            }
        })
        .unwrap();

    for entry in ["README.txt", "bin/shun-demo.cmd", "data/sample.json"] {
        assert!(dest.path().join(entry).exists(), "missing {entry}");
    }
    // Multi-phase progress: network + local delivery both reported.
    assert!(phases.contains(&shun::flow::FlowPhase::Download));
    assert!(phases.contains(&shun::flow::FlowPhase::Extract));
}

#[test]
fn online_payload_rejects_wrong_content() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut archive = pack_directory(&manifest_dir.join("examples/demo_payload")).unwrap();
    archive.truncate(archive.len() / 2); // corrupt the stream
    let url = serve_one(archive);

    let dest = tempfile::tempdir().unwrap();
    let result = OnlinePayload::new(url).extract(dest.path(), &mut |_| {});
    // Truncated streams must surface an error, not a partial success.
    assert!(result.is_err());
}
