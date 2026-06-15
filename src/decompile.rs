//! Decompile a `TsonDocument` back into a `serde_json::Value`.
//!
//! Walks the definitions and data entries, reconstructing the original JSON
//! tree. Definition indices are used to look up structure (field names, array
//! element types); data entries are converted to JSON values.

use crate::error::TsonError;
use crate::structure::*;
use serde_json::Value as JsonValue;
use serde_json::json;

/// Decompile a `TsonDocument` into a `serde_json::Value`.
///
/// For a single root entry, returns the corresponding JSON value directly.
/// For multiple root entries, returns a JSON array.
pub fn decompile_document(doc: &TsonDocument) -> Result<JsonValue, TsonError> {
    let defs = &doc.definitions;

    if doc.data.is_empty() {
        return Ok(JsonValue::Null);
    }

    if doc.data.len() == 1 {
        return chunk_to_json(&doc.data[0], defs);
    }

    let mut arr = Vec::with_capacity(doc.data.len());
    for chunk in &doc.data {
        arr.push(chunk_to_json(chunk, defs)?);
    }
    Ok(JsonValue::Array(arr))
}

/// Convert a single `TsonChunk` to a `serde_json::Value`.
fn chunk_to_json(chunk: &TsonChunk, defs: &[TsonDefinition]) -> Result<JsonValue, TsonError> {
    // Resolve the definition to get type context
    match &chunk.data {
        TsonData::Null => Ok(JsonValue::Null),
        TsonData::Bool(b) => Ok(JsonValue::Bool(*b)),
        TsonData::Int(i) => Ok(json!(*i)),
        TsonData::UInt(u) => Ok(json!(*u)),
        TsonData::Float(f) => {
            // Use f64::from to convert f32 -> f64 for serde_json
            Ok(serde_json::Value::Number(
                serde_json::Number::from_f64(*f as f64).ok_or_else(|| {
                    TsonError::ParseError(format!("Cannot represent float {} as JSON number", f))
                })?,
            ))
        }
        TsonData::String(s) => Ok(JsonValue::String(s.clone())),
        TsonData::Array(_self_def_idx, elem_def_idx, items) => {
            let mut arr = Vec::with_capacity(items.len());
            for item in items {
                arr.push(data_to_json(item, *elem_def_idx, defs)?);
            }
            Ok(JsonValue::Array(arr))
        }
        TsonData::Object(def_idx, fields) => {
            let def = resolve_def(*def_idx, defs)?;
            let field_specs = def.fields.as_ref().ok_or_else(|| {
                TsonError::ParseError(format!(
                    "Object #{}/data#{}: no field definitions",
                    def_idx, chunk.definition_index
                ))
            })?;

            if fields.len() != field_specs.len() {
                return Err(TsonError::ParseError(format!(
                    "Object #{}: expected {} fields, got {}",
                    def_idx,
                    field_specs.len(),
                    fields.len()
                )));
            }

            let mut map = serde_json::Map::with_capacity(fields.len());
            for (i, (fname, _ftype)) in field_specs.iter().enumerate() {
                // To convert a field value, we need its type from the
                // definition. But the value already encodes its own type in
                // the TsonData enum. We use data_to_json with the field type
                // for nested compound lookups.
                let val = data_to_json(&fields[i], *def_idx, defs)?;
                map.insert(fname.clone(), val);
            }
            Ok(JsonValue::Object(map))
        }
    }
}

/// Convert a `TsonData` value to `serde_json::Value`, using the definition
/// context for compound type resolution.
fn data_to_json(
    data: &TsonData,
    _context_def_idx: u16,
    all_defs: &[TsonDefinition],
) -> Result<JsonValue, TsonError> {
    match data {
        TsonData::Null => Ok(JsonValue::Null),
        TsonData::Bool(b) => Ok(JsonValue::Bool(*b)),
        TsonData::Int(i) => Ok(json!(*i)),
        TsonData::UInt(u) => Ok(json!(*u)),
        TsonData::Float(f) => {
            Ok(serde_json::Value::Number(
                serde_json::Number::from_f64(*f as f64).ok_or_else(|| {
                    TsonError::ParseError(format!("Cannot represent float {} as JSON number", f))
                })?,
            ))
        }
        TsonData::String(s) => Ok(JsonValue::String(s.clone())),

        TsonData::Array(_self_def_idx, elem_def_idx, items) => {
            let mut arr = Vec::with_capacity(items.len());
            for item in items {
                arr.push(data_to_json(item, *elem_def_idx, all_defs)?);
            }
            Ok(JsonValue::Array(arr))
        }

        TsonData::Object(def_idx, fields) => {
            let def = resolve_def(*def_idx, all_defs)?;
            let field_specs = def.fields.as_ref().ok_or_else(|| {
                TsonError::ParseError(format!(
                    "Object #{}: no field definitions",
                    def_idx
                ))
            })?;

            if fields.len() != field_specs.len() {
                return Err(TsonError::ParseError(format!(
                    "Object #{}: expected {} fields, got {}",
                    def_idx,
                    field_specs.len(),
                    fields.len()
                )));
            }

            let mut map = serde_json::Map::with_capacity(fields.len());
            for (i, (fname, _ftype)) in field_specs.iter().enumerate() {
                let val = data_to_json(&fields[i], *def_idx, all_defs)?;
                map.insert(fname.clone(), val);
            }
            Ok(JsonValue::Object(map))
        }
    }
}

// ─── Helpers ────────────────────────────────────────────────────────────────

fn resolve_def<'a>(index: u16, all_defs: &'a [TsonDefinition]) -> Result<&'a TsonDefinition, TsonError> {
    all_defs
        .iter()
        .find(|d| d.index == index)
        .ok_or_else(|| TsonError::ParseError(format!("Unknown definition index: {}", index)))
}
