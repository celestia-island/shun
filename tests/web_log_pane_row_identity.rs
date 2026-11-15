//! Source-pin for the web install log pane's row keys.
//!
//! `LogPane` renders newest-first, so keying a row by its position in the
//! rendered list re-keys every row on each push and makes Vue re-write the
//! whole body's text. Against the 500-line cap that goes quadratic on a
//! chatty install (echo lines arrive per file), which is exactly the
//! stutter this pin guards against. Rows are keyed by the identity the
//! producer assigns instead.
//!
//! `shell/web` carries no JS test harness (no vitest in its
//! `package.json`), so the shape is pinned from the Rust side rather than
//! inventing one for a single component.

const PANE: &str = include_str!("../shell/web/src/components/LogPane.tsx");
const APP: &str = include_str!("../shell/web/src/App.tsx");

#[test]
fn log_pane_rows_are_keyed_by_identity() {
    assert!(
        !PANE.contains("key={index}"),
        "LogPane rows must not be keyed by the rendered list index — \
         newest-first ordering shifts every row on each push"
    );
    assert!(
        PANE.contains("key={line.key}"),
        "LogPane rows must be keyed by the line's producer-assigned identity"
    );
}

#[test]
fn the_log_producer_assigns_a_monotonic_row_identity() {
    let push = APP
        .lines()
        .find(|line| line.contains("logLines.value.push({"))
        .expect("App.tsx pushes log lines through logLines.value.push({ .. })");
    assert!(
        push.contains("key:"),
        "the pushed log line must carry a `key` identity field: {push}"
    );
    assert!(
        APP.contains("let logSeq"),
        "App.tsx must keep a monotonic counter for the row identities"
    );
}
