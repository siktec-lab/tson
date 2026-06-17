//! Python bindings for TSON (via PyO3 + maturin).

use pyo3::prelude::*;
use pyo3::{Bound, PyObject, PyResult};

use crate::error::TsonError;
use crate::structure::*;

fn to_py_err(e: TsonError) -> pyo3::PyErr {
    pyo3::exceptions::PyValueError::new_err(e.to_string())
}

fn io_to_py_err(e: std::io::Error) -> pyo3::PyErr {
    pyo3::exceptions::PyValueError::new_err(format!("IO error: {}", e))
}

/// Compile a JSON string to TSON binary bytes.
#[pyfunction]
#[cfg(feature = "json")]
fn dumps(json_text: &str) -> PyResult<Vec<u8>> {
    let doc = crate::compile::compile_json_str(json_text).map_err(to_py_err)?;
    crate::encode::encode_document(&doc).map_err(to_py_err)
}

/// Decompile TSON binary bytes to a Python object.
#[pyfunction]
#[cfg(feature = "json")]
fn loads(bytes: &[u8]) -> PyResult<PyObject> {
    let doc = crate::decode::decode_document(bytes).map_err(to_py_err)?;
    let val = crate::decompile::decompile_document(&doc).map_err(to_py_err)?;
    let py = unsafe { Python::assume_gil_acquired() };
    json_value_to_py(&val, py)
}

/// Compile JSON string and write TSON binary to a file.
#[pyfunction]
#[cfg(feature = "json")]
fn dump(json_text: &str, path: &str) -> PyResult<()> {
    let doc = crate::compile::compile_json_str(json_text).map_err(to_py_err)?;
    let bytes = crate::encode::encode_document(&doc).map_err(to_py_err)?;
    std::fs::write(path, &bytes).map_err(io_to_py_err)
}

/// Read a TSON file and decompile to a Python object.
#[pyfunction]
#[cfg(feature = "json")]
fn load(path: &str) -> PyResult<PyObject> {
    let bytes = std::fs::read(path).map_err(io_to_py_err)?;
    let doc = crate::decode::decode_document(&bytes).map_err(to_py_err)?;
    let val = crate::decompile::decompile_document(&doc).map_err(to_py_err)?;
    let py = unsafe { Python::assume_gil_acquired() };
    json_value_to_py(&val, py)
}

/// Emit a Python value as TSON binary (no JSON string).
#[pyfunction(name = "emit")]
#[cfg(feature = "json")]
fn emit_py(obj: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    let data = py_to_tson_data(obj)?;
    let chunks = vec![TsonChunk { definition_index: 0, data }];
    let doc = crate::compile::compile_from_data(&chunks).map_err(to_py_err)?;
    crate::encode::encode_document(&doc).map_err(to_py_err)
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Convert a `serde_json::Value` to a Python object by manual construction.
/// Note: uses deprecated `to_object`; PyO3 1.0 will need `IntoPyObject`.
#[allow(deprecated)]
fn json_value_to_py(val: &serde_json::Value, py: Python<'_>) -> PyResult<PyObject> {
    match val {
        serde_json::Value::Null => Ok(py.None()),
        serde_json::Value::Bool(b) => Ok(b.to_object(py)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() { Ok(i.to_object(py)) }
            else { Ok(n.as_f64().unwrap_or(0.0).to_object(py)) }
        }
        serde_json::Value::String(s) => Ok(s.to_object(py)),
        serde_json::Value::Array(arr) => {
            let list = pyo3::types::PyList::empty(py);
            for v in arr { list.append(json_value_to_py(v, py)?)?; }
            Ok(list.into())
        }
        serde_json::Value::Object(map) => {
            let d = pyo3::types::PyDict::new(py);
            for (k, v) in map {
                d.set_item(k.as_str(), json_value_to_py(v, py)?)?;
            }
            Ok(d.into())
        }
    }
}

/// Convert a Python object (dict/list/scalar) to TsonData.
fn py_to_tson_data(obj: &Bound<'_, PyAny>) -> PyResult<TsonData> {
    if let Ok(d) = obj.downcast::<pyo3::types::PyDict>() {
        let mut fields = Vec::new();
        for (k, v) in d.iter() {
            let key: String = k.extract()?;
            fields.push(TsonData::String(key));
            fields.push(py_to_tson_data(&v)?);
        }
        return Ok(TsonData::Object(0, fields));
    }
    if let Ok(l) = obj.downcast::<pyo3::types::PyList>() {
        let mut items = Vec::new();
        for item in l.iter() { items.push(py_to_tson_data(&item)?); }
        return Ok(TsonData::Array(0, 0, items));
    }
    if let Ok(s) = obj.extract::<String>() { return Ok(TsonData::String(s)); }
    if let Ok(i) = obj.extract::<i64>() { return Ok(TsonData::Int(i as i32)); }
    if let Ok(f) = obj.extract::<f64>() { return Ok(TsonData::Float(f as f32)); }
    if let Ok(b) = obj.extract::<bool>() { return Ok(TsonData::Bool(b)); }
    Err(pyo3::exceptions::PyTypeError::new_err(
        format!("Unsupported Python type: {:?}", obj.get_type().name()),
    ))
}

// ─── Module initialization ─────────────────────────────────────────────────

#[pymodule]
fn tson(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    #[cfg(feature = "json")]
    {
        m.add_function(wrap_pyfunction!(dumps, m)?)?;
        m.add_function(wrap_pyfunction!(loads, m)?)?;
        m.add_function(wrap_pyfunction!(dump, m)?)?;
        m.add_function(wrap_pyfunction!(load, m)?)?;
        m.add_function(wrap_pyfunction!(emit_py, m)?)?;
    }
    Ok(())
}
