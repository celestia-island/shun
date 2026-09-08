//! Embedded-Python runner (design probe for docs/en/design/scripting.md).
//!
//! Build with `--features python-probe`; needs a script path argument.
//! The interpreter comes from whatever `python3xx.dll` sits beside the
//! executable — put this binary next to an unpacked CPython embeddable
//! distribution and it runs entirely on the carried runtime (the
//! `python313._pth` file next to the DLL pins the stdlib zip, isolated
//! from any host installation).
//!
//! ```text
//! cargo build --release --features python-probe --example pyembed_runner
//! cp target/release/examples/pyembed_runner.exe <unpacked-embeddable>/
//! ./pyembed_runner.exe script.py
//! ```

#![cfg(feature = "python-probe")]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let script = args
        .first()
        .expect("usage: pyembed_runner <script.py> [module-sys-path...]");
    let source = std::fs::read(script).expect("script reads");
    // Trailing NUL for the C string; interior NULs would be a parse
    // error anyway.
    let code = std::ffi::CString::new(source).expect("script is valid C string");

    pyo3::Python::with_gil(|py| {
        use pyo3::types::PyDictMethods;

        let globals = pyo3::types::PyDict::new(py);
        globals
            .set_item(pyo3::intern!(py, "__name__"), "__shun_embedded__")
            .expect("globals name");
        if let Err(err) = py.run(&code, Some(&globals), Some(&globals)) {
            eprintln!("script failed: {err}");
            std::process::exit(1);
        }
        // The embedded interpreter is not finalized on drop; flush the
        // script's stdio explicitly or the last buffered lines are lost.
        let _ = py.run(
            c"import sys; sys.stdout.flush(); sys.stderr.flush()",
            None,
            None,
        );
    });
}
