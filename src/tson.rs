use crate::error::TsonError;
use crate::encode;
use crate::decode;

#[allow(unused_imports)]
pub use crate::structure::{
    TsonType, TsonHeader, TsonData, TsonDefinition, TsonChunk, TsonDocument,
};

pub use crate::stream::TsonStreamReader;

// ─── Emit Mode — Direct TsonData → Binary (Bypasses JSON) ──────────────────

/// Emit a single `TsonData` value as a complete TSON document.
///
/// Discovers definitions and builds the string dict automatically from the
/// data tree.  Field names are synthetic (`"f0"`, `"f1"`, …) since
/// `TsonData` carries values but not field names.
///
/// This bypasses JSON entirely — useful for emitting TSON binary directly
/// from structured data (sensor readings, database rows, in-memory structs).
///
/// # Example
///
/// ```rust
/// use tson::{TsonData, emit};
/// let reading = TsonData::Object(0, vec![
///     TsonData::Float(22.5),
///     TsonData::Int(61),
///     TsonData::String("nominal".to_string()),
/// ]);
/// let bytes = emit(&reading).unwrap();
/// assert!(bytes.len() > 13, "should produce a complete TSON document (header + defs + data)");
/// ```
#[allow(dead_code)]
pub fn emit(data: &TsonData) -> Result<Vec<u8>, TsonError> {
    let chunk = TsonChunk { definition_index: 0, data: data.clone() };
    let doc = crate::compile::compile_from_data(&[chunk])?;
    encode::encode_document(&doc)
}

/// Emit just the value payload (no header, definitions, or dict).
///
/// Use this when you already have a `TsonDocument` with parsed definitions
/// and dict, and want to encode one additional entry's payload bytes to
/// append to an existing TSON stream.
#[allow(dead_code)]
pub fn emit_value(data: &TsonData) -> Result<Vec<u8>, TsonError> {
    encode::encode_value(data)
}

/// Emit a `TsonData` value as a complete TSON document, reusing pre-existing
/// definitions and dict from an incoming document.
///
/// This is the **server response path**: receive a TSON message, extract
/// fields, build a response value, and emit it back using the incoming
/// message's definitions. No schema re-discovery, no dict rebuild — just
/// encode using the already-parsed context.
///
/// # Example
///
/// ```rust
/// use tson::{TsonData, TsonDefinition, TsonType, emit_with_context};
/// // defs and dict come from a previously parsed TsonDocument
/// let dummy_defs = vec![
///     TsonDefinition { def_type: TsonType::Null, index: 0, name: None, fields: None, elem_type: None },
///     TsonDefinition { def_type: TsonType::Object, index: 7, name: None,
///         fields: Some(vec![("f0".into(), TsonType::String), ("f1".into(), TsonType::Float)]),
///         elem_type: None },
/// ];
/// let response = TsonData::Object(7, vec![
///     TsonData::String("alert".to_string()),
///     TsonData::Float(35.2),
/// ]);
/// let bytes = emit_with_context(&response, &dummy_defs, &[]).unwrap();
/// assert!(bytes.len() > 13, "produces a complete document");
/// ```
#[allow(dead_code)]
pub fn emit_with_context(
    data: &TsonData,
    defs: &[TsonDefinition],
    dict: &[String],
) -> Result<Vec<u8>, TsonError> {
    // Extract the definition index from the data value itself
    let def_index = def_index_for_value(data);
    let doc = TsonDocument {
        header: TsonHeader::new(1, TsonHeader::SIZE as u32, 0, 0),
        definitions: defs.to_vec(),
        dict: dict.to_vec(),
        data: vec![TsonChunk {
            definition_index: def_index,
            data: data.clone(),
        }],
    };
    encode::encode_document(&doc)
}

/// Extract the definition index from a TsonData value.
fn def_index_for_value(data: &TsonData) -> u16 {
    match data {
        TsonData::Null           => 0,
        TsonData::Bool(_)        => 1,
        TsonData::Int(_)         => 2,
        TsonData::UInt(_)        => 3,
        TsonData::Float(_)       => 4,
        TsonData::String(_) | TsonData::StrRef(_) => 5,
        TsonData::Array(def, _, _) => *def,
        TsonData::Object(def, _)   => *def,
    }
}

// ─── Raw-bytes round-trip ──────────────────────────────────────────────────

/// Encode a `TsonDocument` to its binary representation.
pub fn to_bytes(doc: &TsonDocument) -> Result<Vec<u8>, TsonError> {
    encode::encode_document(doc)
}

#[allow(dead_code)]
pub fn from_bytes(bytes: &[u8]) -> Result<TsonDocument, TsonError> {
    decode::decode_document(bytes)
}

#[allow(dead_code)]
pub fn decode_definitions(bytes: &[u8]) -> Result<Vec<TsonDefinition>, TsonError> {
    decode::decode_definitions(bytes)
}

// ─── JSON convenience (feature-gated) ──────────────────────────────────────

#[cfg(feature = "json")]
#[allow(dead_code)]
pub fn compile_json(json_text: &str) -> Result<TsonDocument, TsonError> {
    crate::compile::compile_json_str(json_text)
}

#[cfg(feature = "json")]
#[allow(dead_code)]
pub fn compile_value(value: &serde_json::Value) -> Result<TsonDocument, TsonError> {
    crate::compile::compile_json(value)
}

#[cfg(feature = "json")]
#[allow(dead_code)]
pub fn decompile_to_value(doc: &TsonDocument) -> Result<serde_json::Value, TsonError> {
    crate::decompile::decompile_document(doc)
}

#[cfg(feature = "json")]
pub fn compile_json_file(file: std::fs::File) -> Result<TsonDocument, TsonError> {
    use std::io::Read;
    let mut reader = std::io::BufReader::new(file);
    let mut text = String::new();
    reader.read_to_string(&mut text)
        .map_err(|e| TsonError::IoError(e))?;
    crate::compile::compile_json_str(&text)
}

#[cfg(feature = "json")]
pub fn decompile_tson_file(file: std::fs::File) -> Result<serde_json::Value, TsonError> {
    use std::io::Read;
    let mut reader = std::io::BufReader::new(file);
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf)
        .map_err(|e| TsonError::IoError(e))?;
    let doc = decode::decode_document(&buf)?;
    crate::decompile::decompile_document(&doc)
}
