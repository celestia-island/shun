//! Update-watch resolution against local mirrors: ordered probes, the
//! first reachable source wins, and every declared file resolves under
//! the winning base.

#![cfg(feature = "online")]

use std::io::{Read, Write};

use shun::flow::{FlowEvent, FlowLog, FlowPhase};
use shun::update::{fetch_text, resolve};

/// A tiny HTTP/1.1 server answering one GET with a fixed status + body —
/// the same minimal harness the online payload tests use.
fn serve_status(status: u16, body: &'static str) -> String {
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
        let reason = if status == 200 { "OK" } else { "Not Found" };
        let header = format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(header.as_bytes()).unwrap();
        stream.write_all(body.as_bytes()).unwrap();
    });
    format!("http://127.0.0.1:{port}")
}

/// A mirror that accepts connections and drops them without answering —
/// the unreachable source the probe must fall through.
fn serve_dead() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        drop(stream);
    });
    format!("http://127.0.0.1:{port}")
}

#[test]
fn resolve_falls_through_to_the_first_reachable_source() {
    let dead = serve_dead();
    let live = serve_status(200, "0.1.1\n");

    let mut events = Vec::new();
    let watch = resolve(
        &[dead.clone(), live.clone()],
        &["latest".to_string(), "app-setup.exe".to_string()],
        &mut |event| events.push(event),
    )
    .expect("the live mirror answers");

    assert_eq!(watch.source, live);
    assert_eq!(
        watch.files,
        [
            ("app-setup.exe".to_string(), format!("{live}/app-setup.exe")),
            ("latest".to_string(), format!("{live}/latest")),
        ]
        .into_iter()
        .collect()
    );

    // One probe attempt per source, then the winner announcement.
    let steps: Vec<String> = events
        .iter()
        .filter_map(|event| match event {
            FlowEvent::Progress {
                phase: FlowPhase::Download,
                step,
                ..
            } => Some(step.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        steps,
        vec![
            format!("Checking {}", dead.trim_start_matches("http://")),
            format!("Checking {}", live.trim_start_matches("http://")),
            format!("Using {live} …"),
        ]
    );

    // The skipped source surfaces as a warning record.
    assert!(events.iter().any(|event| matches!(event,
        FlowEvent::Log { record: FlowLog::Warning { code, .. } } if code == "update-source-skipped"
    )));
}

#[test]
fn resolve_skips_non_200_answers() {
    let missing = serve_status(404, "gone");
    let live = serve_status(200, "0.1.1\n");

    let watch = resolve(
        &[missing.clone(), live.clone()],
        &["latest".to_string()],
        &mut |_| {},
    )
    .expect("the second mirror answers");

    assert_eq!(watch.source, live);
    assert_eq!(watch.files["latest"], format!("{live}/latest"));
}

#[test]
fn resolve_normalizes_bases_and_skips_empty_entries() {
    let live = serve_status(200, "0.1.1\n");

    let watch = resolve(
        &["".to_string(), format!("{live}/")],
        &["latest".to_string(), "".to_string()],
        &mut |_| {},
    )
    .expect("the normalized base answers");

    assert_eq!(watch.source, live);
    assert_eq!(watch.files["latest"], format!("{live}/latest"));
}

#[test]
fn resolve_needs_both_sources_and_files() {
    let mut events = Vec::new();
    assert!(resolve(&[], &["latest".to_string()], &mut |e| events.push(e)).is_none());
    assert!(
        resolve(
            &["https://mirror.example.test".to_string()],
            &[],
            &mut |e| events.push(e)
        )
        .is_none()
    );
    // Nothing resolved, so nothing was reported.
    assert!(events.is_empty());
}

#[test]
fn fetch_text_trims_the_version_marker() {
    let live = serve_status(200, "0.1.1\n");
    assert_eq!(fetch_text(&format!("{live}/latest")).unwrap(), "0.1.1");
}

#[test]
fn fetch_text_rejects_non_200() {
    let missing = serve_status(404, "gone");
    assert!(fetch_text(&format!("{missing}/latest")).is_err());
}
