use crate::error::TsonError;
use crate::encode;
use crate::decode;

#[allow(unused_imports)]
pub use crate::structure::{
    TsonType, TsonHeader, TsonData, TsonDefinition, TsonChunk, TsonDocument,
};

pub use crate::stream::TsonStreamReader;

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
