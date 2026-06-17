//! Python bindings for TSON (via PyO3 + maturin).
//!
//! Exposes compile, decompile, emit, and round-trip functions as a
//! python-native `tson` module.

use pyo3::prelude::*;
use pyo3::{Bound, PyResult};

use crate::error::TsonError;
use crate::structure::*;

// ─── dumps: str → bytes ───────────────────────────────────────────────────

/// Compile a JSON string to TSON binary bytes.
///
/// Python: `tson.dumps('{"a":1}') → b'...'`
#[pyfunction]
#[cfg(feature = "json")]
fn dumps(json_text: &str) -> PyResult<Vec<u8>> {
    let doc = crate::compile::compile_json_str(json_text)
        .map_err(to_py_err)?;
    crate::encode::encode_document(&doc).map_err(to_py_err)
}

// ─── loads: bytes → Python object ─────────────────────────────────────────

/// Decompile TSON binary bytes to a Python object (dict/list/scalar).
///
/// Python: `tson.loads(b'...') → {'a': 1}`
#[pyfunction]
#[cfg(feature = "json")]
fn loads(bytes: &[u8]) -> PyResult<PyObject> {
    let doc = crate::decode::decode_document(bytes).map_err(to_py_err)?;
    let value = crate::decompile::decompile_document(&doc).map_err(to_py_err)?;
    let py = unsafe { Python::assume_gil_acquired() };
    serde_json_to_py(&value, py)
}

// ─── dump: str + path → None ──────────────────────────────────────────────

/// Compile a JSON string and write TSON binary to a file.
///
/// Python: `tson.dump('{"a":1}', 'data.tson')`
#[pyfunction]
#[cfg(feature = "json")]
fn dump(json_text: &str, path: &str) -> PyResult<()> {
    let doc = crate::compile::compile_json_str(json_text)
        .map_err(to_py_err)?;
    let bytes = crate::encode::encode_document(&doc)
        .map_err(to_py_err)?;
    std::fs::write(path, &bytes).map_err(to_py_err)
}

// ─── load: path → Python object ───────────────────────────────────────────

/// Read a TSON file and decompile to a Python object.
///
/// Python: `tson.load('data.tson') → {'a': 1}`
#[pyfunction]
#[cfg(feature = "json")]
fn load(path: &str) -> PyResult<PyObject> {
    let bytes = std::fs::read(path).map_err(to_py_err)?;
    let doc = crate::decode::decode_document(&bytes).map_err(to_py_err)?;
    let value = crate::decompile::decompile_document(&doc).map_err(to_py_err)?;
    let py = unsafe { Python::assume_gil_acquired() };
    serde_json_to_py(&value, py)
}

// ─── emit: dict → bytes (bypasses JSON) ───────────────────────────────────

/// Emit a Python dict/list as TSON binary directly (no JSON string).
///
/// Python: `tson.emit({'temp': 22.5}) → b'...'`
#[pyfunction]
#[cfg(feature = "json")]
fn emit_obj(obj: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    let data = py_to_tson_data(obj)?;
    let chunks = vec![TsonChunk { definition_index: 0, data }];
    let doc = crate::compile::compile_from_data(&chunks).map_err(to_py_err)?;
    crate::encode::encode_document(&doc).map_err(to_py_err)
}

// ─── Helpers ──────────────────────────────────────────────────────────────

fn to_py_err(e: TsonError) -> pyo3::PyErr {
    pyo3::exceptions::PyValueError::new_err(e.to_string())
}

/// Convert a `serde_json::Value` to a Python object.
fn serde_json_to_py(value: &serde_json::Value, py: Python<'_>) -> PyResult<PyObject> {
    match value {
        serde_json::Value::Null => Ok(py.None()),
        serde_json::Value::Bool(b) => Ok(b.into_py(py)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() { Ok(i.into_py(py)) }
            else { Ok(n.as_f64().unwrap_or(0.0).into_py(py)) }
        }
        serde_json::Value::String(s) => Ok(s.clone().into_py(py)),
        serde_json::Value::Array(arr) => {
            let mut py_arr = Vec::with_capacity(arr.len());
            for v in arr { py_arr.push(serde_json_to_py(v, py)?); }
            Ok(py_arr.into_py(py))
        }
        serde_json::Value::Object(map) => {
            let dict = pyo3::types::PyDict::new_bound(py);
            for (k, v) in map {
                dict.set_item(k.as_str(), serde_json_to_py(v, py)?)?;
            }
            Ok(dict.into())
        }
    }
}

/// Convert a Python object (dict/list/scalar) to `TsonData`.
fn py_to_tson_data(obj: &Bound<'_, PyAny>) -> PyResult<TsonData> {
    // Try dict first
    if let Ok(d) = obj.downcast::<pyo3::types::PyDict>() {
        let mut fields = Vec::new();
        for (k, v) in d.iter() {
            let key: String = k.extract()?;
            // Flatten key-value into field values (order not guaranteed)
            let val = py_to_tson_data(&v)?;
            fields.push(TsonData::String(key));
            fields.push(val);
        }
        return Ok(TsonData::Object(0, fields));
    }

    // Try list
    if let Ok(l) = obj.downcast::<pyo3::types::PyList>() {
        let mut items = Vec::new();
        for item in l.iter() {
            items.push(py_to_tson_data(&item)?);
        }
        return Ok(TsonData::Array(0, 0, items));
    }

    // Try string
    if let Ok(s) = obj.extract::<String>() {
        return Ok(TsonData::String(s));
    }

    // Try i64 (Python int)
    if let Ok(i) = obj.extract::<i64>() {
        return Ok(TsonData::Int(i as i32));
    }

    // Try f64 (Python float)
    if let Ok(f) = obj.extract::<f64>() {
        return Ok(TsonData::Float(f as f32));
    }

    // Try bool
    if let Ok(b) = obj.extract::<bool>() {
        return Ok(TsonData::Bool(b));
    }

    Err(pyo3::exceptions::PyTypeError::new_err(
        format!("Unsupported Python type: {:?}", obj.get_type().name()),
    ))
}

// ─── Module initialisation ────────────────────────────────────────────────

#[pymodule]
fn tson(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    #[cfg(feature = "json")]
    {
        m.add_function(wrap_pyfunction!(loads, m)?)?;
        m.add_function(wrap_pyfunction!(load, m)?)?;
        m.add_function(wrap_pyfunction!(dumps, m)?)?;
        m.add_function(wrap_pyfunction!(dump, m)?)?;
        m.add_function(wrap_pyfunction!(emit_obj, m)?)?;
    }
    Ok(())
}
