#![cfg(feature = "python-probe")]
//! Embedding probe for the optional Python runtime (design note:
//! docs/en/design/scripting.md). Opt-in via `--features python-probe`:
//! PyO3 links the build host's CPython (`auto-initialize` embeds the
//! interpreter into the process) — exactly the mechanism a shun
//! installer would use, with the shipped runtime's `python3xx.dll` +
//! stdlib taking the place of the host installation. Nothing here is
//! wired into the delivery runtime; this probe keeps the option honest.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyModule;

/// A stand-in for the future shun built-in surface: a Rust function the
/// embedded interpreter can call (progress emission, verified
/// downloads, ...).
#[pyfunction]
fn shun_note(call: String) -> PyResult<String> {
    Ok(format!("shun: {call}"))
}

#[test]
fn cpython_embeds_via_pyo3() -> PyResult<()> {
    Python::with_gil(|py| {
        // 1. The interpreter evaluates real Python with the stdlib
        //    available (json here — the kind of battery duckscript
        //    lacks). `eval` takes expressions, so the import goes
        //    through __import__.
        let parsed: bool = py
            .eval(
                cr#"__import__('json').loads('{"delivered": true}')["delivered"]"#,
                None,
                None,
            )?
            .extract()?;

        // 2. Custom Rust commands register into an embedded module —
        //    the same shape as the duckscript custom commands.
        let module = PyModule::new(py, "shun")?;
        module.add_function(wrap_pyfunction!(shun_note, &module)?)?;
        let note: String = module
            .getattr("shun_note")?
            .call1(("delivered-by-shun",))?
            .extract()?;

        // 3. Python exceptions surface as Rust errors (failure paths
        //    map onto FlowEvent::Failed) — an expression that raises at
        //    runtime, since `eval` only takes expressions.
        let failure = py.eval(c"int('x')", None, None);
        assert!(failure.is_err());

        assert!(parsed);
        assert_eq!(note, "shun: delivered-by-shun");
        assert_eq!(
            failure.unwrap_err().value(py).get_type().to_string(),
            PyValueError::new_err(()).get_type(py).to_string()
        );

        // 4. The hermetic network + streaming-hash check: a loopback
        //    HTTP server (real sockets, real urllib) and hashlib over
        //    chunked input — the verify-phase building blocks. These run
        //    identically on a carried embeddable runtime (stdlib only).
        let ops_globals = pyo3::types::PyDict::new(py);
        py.run(
            cr#"
import hashlib, threading, urllib.request
from http.server import BaseHTTPRequestHandler, HTTPServer

class One(BaseHTTPRequestHandler):
    def do_GET(self):
        body = b"delivered-by-shun"
        self.send_response(200)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def log_message(self, *a):
        pass

server = HTTPServer(("127.0.0.1", 0), One)
port = server.server_address[1]
threading.Thread(target=server.serve_forever, daemon=True).start()
try:
    with urllib.request.urlopen(f"http://127.0.0.1:{port}/", timeout=5) as r:
        payload = r.read()
finally:
    server.shutdown()

sha = hashlib.sha256()
for chunk in (b"delivered ", b"by ", b"a shun ", b"flow"):
    sha.update(chunk)
__result__ = (payload == b"delivered-by-shun", sha.hexdigest()[:16])
"#,
            Some(&ops_globals),
            Some(&ops_globals),
        )?;
        use pyo3::types::PyDictMethods;
        let (fetched, digest): (bool, String) = ops_globals
            .get_item("__result__")
            .expect("result present")
            .expect("result not none")
            .extract()?;
        assert!(fetched);
        assert_eq!(digest, "f1bb8bf9d1f29d65");
        Ok(())
    })
}
