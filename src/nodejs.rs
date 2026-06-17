//! JavaScript/Node.js bindings for TSON (via napi-rs).
//!
//! Exposes compile, decompile, and emit functions as native Node.js addon.

use napi::bindgen_prelude::*;
use napi_derive::napi;

use crate::error::TsonError;
use crate::structure::*;

fn to_napi_err(e: TsonError) -> napi::Error {
    napi::Error::from_reason(e.to_string())
}

fn io_to_napi_err(e: std::io::Error) -> napi::Error {
    napi::Error::from_reason(format!("IO error: {}", e))
}

fn json_value_to_tson(val: &serde_json::Value) -> Result<TsonData> {
    match val {
        serde_json::Value::Null => Ok(TsonData::Null),
        serde_json::Value::Bool(b) => Ok(TsonData::Bool(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() { Ok(TsonData::Int(i as i32)) }
            else if let Some(u) = n.as_u64() { Ok(TsonData::UInt(u as u32)) }
            else { Ok(TsonData::Float(n.as_f64().unwrap_or(0.0) as f32)) }
        }
        serde_json::Value::String(s) => Ok(TsonData::String(s.clone())),
        serde_json::Value::Array(arr) => {
            let mut items = Vec::with_capacity(arr.len());
            for v in arr { items.push(json_value_to_tson(v)?); }
            Ok(TsonData::Array(0, 0, items))
        }
        serde_json::Value::Object(map) => {
            let mut fields = Vec::with_capacity(map.len() * 2);
            for (k, v) in map {
                fields.push(TsonData::String(k.clone()));
                fields.push(json_value_to_tson(v)?);
            }
            Ok(TsonData::Object(0, fields))
        }
    }
}

/// Compile a JSON string to a TSON binary `Buffer`.
#[napi]
#[cfg(feature = "json")]
pub fn dumps(json_text: String) -> Result<Buffer> {
    let doc = crate::compile::compile_json_str(&json_text)
        .map_err(to_napi_err)?;
    let bytes = crate::encode::encode_document(&doc)
        .map_err(to_napi_err)?;
    Ok(Buffer::from(bytes))
}

/// Decompile a TSON binary `Buffer` to a JavaScript value.
#[napi]
#[cfg(feature = "json")]
pub fn loads(bytes: Buffer) -> Result<serde_json::Value> {
    let doc = crate::decode::decode_document(&bytes)
        .map_err(to_napi_err)?;
    crate::decompile::decompile_document(&doc).map_err(to_napi_err)
}

/// Compile a JSON string and write the TSON binary to a file.
#[napi]
#[cfg(feature = "json")]
pub fn dump(json_text: String, path: String) -> Result<()> {
    let doc = crate::compile::compile_json_str(&json_text)
        .map_err(to_napi_err)?;
    let bytes = crate::encode::encode_document(&doc)
        .map_err(to_napi_err)?;
    std::fs::write(&path, &bytes).map_err(io_to_napi_err)?;
    Ok(())
}

/// Read a TSON file and decompile to a JavaScript value.
#[napi]
#[cfg(feature = "json")]
pub fn load(path: String) -> Result<serde_json::Value> {
    let bytes = std::fs::read(&path).map_err(io_to_napi_err)?;
    let doc = crate::decode::decode_document(&bytes)
        .map_err(to_napi_err)?;
    crate::decompile::decompile_document(&doc).map_err(to_napi_err)
}

/// Emit a JavaScript value directly as TSON binary, bypassing JSON text.
#[napi]
#[cfg(feature = "json")]
pub fn emit(val: serde_json::Value) -> Result<Buffer> {
    let data = json_value_to_tson(&val)?;
    let chunks = vec![TsonChunk { definition_index: 0, data }];
    let doc = crate::compile::compile_from_data(&chunks)
        .map_err(to_napi_err)?;
    let bytes = crate::encode::encode_document(&doc)
        .map_err(to_napi_err)?;
    Ok(Buffer::from(bytes))
}
