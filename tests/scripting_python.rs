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
        Ok(())
    })
}
