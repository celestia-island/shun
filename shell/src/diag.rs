//! Panic-free stderr diagnostics for a GUI-subsystem binary.
//!
//! `eprintln!` panics when the process has no stderr handle — which is the
//! normal state for a `windows_subsystem = "windows"` binary launched by
//! double-click: `GetStdHandle` yields NULL, the write fails, and std's
//! `print_to` turns that into `panic!("failed printing to stderr")` (found by
//! verification round 2, 2026-10-06; the DWM border-colour diagnostic would
//! have hit it on every Windows 10 launch, since that attribute does not
//! exist there). Everything the shell wants to say while the window may not
//! exist yet goes through [`diag`], which writes best-effort and swallows
//! the error instead.

use std::io::Write;

/// Best-effort line to stderr: console launches still see it, console-less
/// launches lose it, and nobody panics.
pub fn diag(args: std::fmt::Arguments<'_>) {
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr, "{args}");
}

/// `diag(format_args!(...))` with the usual `format!` ergonomics.
#[macro_export]
macro_rules! diag {
    ($($arg:tt)*) => {
        $crate::diag::diag(format_args!($($arg)*))
    };
}
