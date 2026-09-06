//! Python bindings for `skema`: `check()` and `export()`, each operating on
//! an entire project (root file plus `import` graph, like `skema-cli`).
//!
//! ```python
//! import skema
//! result = skema.check("project.skema")
//! if not result["ok"]:
//!     for d in result["diagnostics"]:
//!         print(f"{d['path']}:{d['line']}:{d['column']}: {d['severity']}: {d['message']}")
//!
//! model = skema.export("project.skema")["model"]
//! for table in model["tables"]:
//!     print(table["name"], [f["name"] for f in table["fields"]])
//! ```
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

#[pyfunction]
fn export<'py>(py: Python<'py>, path: PathBuf) -> PyResult<Bound<'py, PyAny>> {
    json_to_py(py, &run_export(&path))
}

fn run_check(path: &Path) -> Value {
    match skema_loader::load(path) {
        Ok(project) => {
            let files = project.input_files();
            let (_model, diagnostics) = skema_hir::analyze(&files);
            let reports = reports_json(&project, &diagnostics);
            let ok = !has_error(&reports);
            serde_json::json!({ "ok": ok, "diagnostics": reports })
        }
        Err(e) => load_error_json(path, &e),
    }
}

fn run_export(path: &Path) -> Value {
    match skema_loader::load(path) {
        Ok(project) => {
            let files = project.input_files();
            let (model, diagnostics) = skema_hir::analyze(&files);
            let reports = reports_json(&project, &diagnostics);
            let ok = !has_error(&reports);
            let model = serde_json::to_value(&model).unwrap_or(Value::Null);
            serde_json::json!({ "ok": ok, "model": model, "diagnostics": reports })
        }
        Err(e) => {
            let mut value = load_error_json(path, &e);
            value["model"] = Value::Null;
            value
        }
    }
}

fn has_error(reports: &Value) -> bool {
    reports
        .as_array()
        .is_some_and(|items| items.iter().any(|r| r["severity"] == "error"))
}

fn reports_json(project: &skema_loader::Project, diagnostics: &[skema_hir::Diagnostic]) -> Value {
    let entries: Vec<Value> = skema_loader::report::build(project, diagnostics)
        .into_iter()
        .map(|e| {
            serde_json::json!({
                "path": e.path.display().to_string(),
                "line": e.line,
                "column": e.column,
                "severity": severity_word(e.severity),
                "message": e.message,
            })
        })
        .collect();
    Value::Array(entries)
}

fn severity_word(severity: skema_syntax::Severity) -> &'static str {
    match severity {
        skema_syntax::Severity::Error => "error",
        skema_syntax::Severity::Warning => "warning",
        skema_syntax::Severity::Info => "info",
        skema_syntax::Severity::Hint => "hint",
    }
}

fn load_error_json(path: &Path, e: &skema_loader::LoadError) -> Value {
    serde_json::json!({
        "ok": false,
        "diagnostics": [{
            "path": path.display().to_string(),
            "line": 1,
            "column": 1,
            "severity": "error",
            "message": e.to_string(),
        }],
    })
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
