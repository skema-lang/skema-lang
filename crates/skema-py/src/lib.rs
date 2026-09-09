//! Python bindings for `skema`: `check()` and `export()`, each operating on
//! an entire project (root file plus `import` graph, like `skema-cli`).
//!
//! ```python
//! import skema
//! result = skema.check("project.skema")
//! if not result["ok"]:
//!     for d in result["diagnostics"]:
//!         loc = d["location"]
//!         print(f"{loc['file']}:{loc['line']}:{loc['column']}: {d['severity']}: {d['message']}")
//!
//! model = skema.export("project.skema")["model"]
//! for table in model["tables"]:
//!     print(table["name"], [f["name"] for f in table["fields"]])
//! ```
//!
//! # Why this goes through `skema-api`
//!
//! Both functions are thin wrappers over `skema_loader::analyze`, so the
//! `dict` a Python caller reads has the same keys the browser editor and the
//! language server see. Serializing `skema_hir::model::Model` directly – as
//! this crate used to – produced a *third* shape, and one that was missing
//! something: its `file` field is a raw `FileId` index, a number with no way
//! back to a path. It is a display path here.
//!
//! Both functions return a single `dict` instead of raising an exception
//! when the model contains errors – the same stance as the rest of the
//! core: an analysis always produces a result, errors are data (`ok: False`
//! plus `diagnostics`), not control flow. A Python `Exception` is raised
//! only when the root file itself cannot be read.

use std::path::{Path, PathBuf};

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use serde_json::Value;

#[pyfunction]
fn check<'py>(py: Python<'py>, path: PathBuf) -> PyResult<Bound<'py, PyAny>> {
    json_to_py(py, &run_check(&path))
}

/// `with_locations` gates the per-entity `location` in the model, the same
/// way `skema export --locations` does. Off by default: on a large model it
/// roughly doubles the `dict`, and a caller reading table and field names
/// never touches it.
#[pyfunction]
#[pyo3(signature = (path, with_locations = false))]
fn export<'py>(
    py: Python<'py>,
    path: PathBuf,
    with_locations: bool,
) -> PyResult<Bound<'py, PyAny>> {
    json_to_py(py, &run_export(&path, with_locations))
}

fn run_check(path: &Path) -> Value {
    match skema_loader::load(path) {
        Ok(project) => {
            let analysis = skema_loader::analyze(&project, false);
            serde_json::json!({
                "ok": analysis.ok(),
                "diagnostics": to_value(&analysis.diagnostics),
            })
        }
        Err(e) => load_error_json(&e, false),
    }
}

fn run_export(path: &Path, with_locations: bool) -> Value {
    match skema_loader::load(path) {
        Ok(project) => {
            let analysis = skema_loader::analyze(&project, with_locations);
            serde_json::json!({
                "ok": analysis.ok(),
                "model": to_value(&analysis.model),
                "diagnostics": to_value(&analysis.diagnostics),
            })
        }
        Err(e) => load_error_json(&e, true),
    }
}

/// A failed load, in the same shape a successful one returns – one
/// `diagnostic` in the list, built by `skema-loader` so it is the same type
/// as every other message rather than a hand-written JSON object.
fn load_error_json(e: &skema_loader::LoadError, with_model: bool) -> Value {
    let diagnostic = skema_loader::load_error_diagnostic(e);
    let mut value = serde_json::json!({
        "ok": false,
        "diagnostics": to_value(&[diagnostic]),
    });
    if with_model {
        value["model"] = Value::Null;
    }
    value
}

/// Serialization cannot fail for these types – they are plain data with
/// derived `Serialize` impls and no map keys but strings – but the core's
/// stance is that a caller gets a result rather than a panic, so a failure
/// degrades to `null` instead of unwrapping.
fn to_value(value: &impl serde::Serialize) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

/// `serde_json::Value` → native Python object (`dict`/`list`/`str`/…), so
/// callers don't have to go through a JSON string themselves.
fn json_to_py<'py>(py: Python<'py>, value: &Value) -> PyResult<Bound<'py, PyAny>> {
    Ok(match value {
        Value::Null => py.None().into_bound(py),
        Value::Bool(b) => b.into_pyobject(py)?.to_owned().into_any(),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.into_pyobject(py)?.into_any()
            } else if let Some(u) = n.as_u64() {
                u.into_pyobject(py)?.into_any()
            } else {
                n.as_f64().unwrap_or_default().into_pyobject(py)?.into_any()
            }
        }
        Value::String(s) => s.into_pyobject(py)?.into_any(),
        Value::Array(items) => {
            let elements = items
                .iter()
                .map(|v| json_to_py(py, v))
                .collect::<PyResult<Vec<_>>>()?;
            PyList::new(py, elements)?.into_any()
        }
        Value::Object(map) => {
            let dict = PyDict::new(py);
            for (key, v) in map {
                dict.set_item(key, json_to_py(py, v)?)?;
            }
            dict.into_any()
        }
    })
}

#[pymodule]
fn skema(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(check, m)?)?;
    m.add_function(wrap_pyfunction!(export, m)?)?;
    Ok(())
}
