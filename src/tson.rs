//! # TSON — Terse JSON Binary Format
//!
//! A compact binary format for JSON data, designed for microcontrollers and
//! constrained environments.  Key properties:
//!
//! * **Schema-deduplicated**: object and array structures are described once
//!   in a definition block; data entries reference definitions by index.
//!   Field names are stored once.
//!
//! * **Streaming**: the `TsonStreamReader` yields data entries without
//!   materialising the entire document.
//!
//! * **Zero-dependency core**: binary encoding/decoding works on `&[u8]`
//!   slices and only requires `alloc`.  JSON interop is optional behind the
//!   `json` feature.
//!
//! # Feature Flags
//!
//! * `json` (default on) — enables `serde_json` based compile/decompile.
//! * `std` (default on) — enables `std::io::Read` helpers on the header.

use crate::error::TsonError;
use crate::encode;
use crate::decode;

// ─── Re-exports ─────────────────────────────────────────────────────────────

#[allow(unused_imports)]
pub use crate::structure::{
    TsonType, TsonHeader, TsonData, TsonDefinition, TsonChunk, TsonDocument,
};

pub use crate::stream::TsonStreamReader;

// ─── Convenience: raw-bytes round-trip ──────────────────────────────────────

/// Encode a `TsonDocument` to its binary representation.
///
/// Zero-dependency — works without the `json` feature.
pub fn to_bytes(doc: &TsonDocument) -> Result<Vec<u8>, TsonError> {
    encode::encode_document(doc)
}

/// Decode a `TsonDocument` from its binary representation.
///
/// Zero-dependency — works without the `json` feature.
#[allow(dead_code)]
pub fn from_bytes(bytes: &[u8]) -> Result<TsonDocument, TsonError> {
    decode::decode_document(bytes)
}

/// Decode definitions from a raw definition block slice.
///
/// Useful for inspecting the schema of a TSON document without decoding
/// all data entries.
#[allow(dead_code)]
pub fn decode_definitions(bytes: &[u8]) -> Result<Vec<TsonDefinition>, TsonError> {
    decode::decode_definitions(bytes)
}

// ─── JSON convenience (feature-gated) ──────────────────────────────────────

/// Compile a JSON string into a `TsonDocument`.
#[cfg(feature = "json")]
#[allow(dead_code)]
pub fn compile_json(json_text: &str) -> Result<TsonDocument, TsonError> {
    crate::compile::compile_json_str(json_text)
}

/// Compile a `serde_json::Value` into a `TsonDocument`.
#[cfg(feature = "json")]
#[allow(dead_code)]
pub fn compile_value(value: &serde_json::Value) -> Result<TsonDocument, TsonError> {
    crate::compile::compile_json(value)
}

/// Decompile a `TsonDocument` into a `serde_json::Value`.
#[cfg(feature = "json")]
#[allow(dead_code)]
pub fn decompile_to_value(doc: &TsonDocument) -> Result<serde_json::Value, TsonError> {
    crate::decompile::decompile_document(doc)
}

// ─── Old API (backward compat for main.rs) ──────────────────────────────────

/// Open a file, parse it as JSON, and return a compiled `TsonDocument`.
///
/// Only available with the `json` feature.
#[cfg(feature = "json")]
pub fn compile_json_file(file: std::fs::File) -> Result<TsonDocument, TsonError> {
    use std::io::Read;
    let mut reader = std::io::BufReader::new(file);
    let mut text = String::new();
    reader
        .read_to_string(&mut text)
        .map_err(|e| TsonError::IoError(e))?;
    crate::compile::compile_json_str(&text)
}

/// Open a file, parse it as TSON, and return a decompiled `serde_json::Value`.
///
/// Only available with the `json` feature.
#[cfg(feature = "json")]
pub fn decompile_tson_file(file: std::fs::File) -> Result<serde_json::Value, TsonError> {
    use std::io::Read;
    let mut reader = std::io::BufReader::new(file);
    let mut buf = Vec::new();
    reader
        .read_to_end(&mut buf)
        .map_err(|e| TsonError::IoError(e))?;
    let doc = decode::decode_document(&buf)?;
    crate::decompile::decompile_document(&doc)
}