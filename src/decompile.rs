use crate::error::TsonError;
use crate::structure::*;
use serde_json::json;
use serde_json::Value as JsonValue;

pub fn decompile_document(doc: &TsonDocument) -> Result<JsonValue, TsonError> {
    let defs = &doc.definitions;
    let dict = &doc.dict;

    if doc.data.is_empty() {
        return Ok(JsonValue::Null);
    }
    if doc.data.len() == 1 {
        return chunk_to_json(&doc.data[0], defs, dict);
    }

    let mut arr = Vec::with_capacity(doc.data.len());
    for chunk in &doc.data {
        arr.push(chunk_to_json(chunk, defs, dict)?);
    }
    Ok(JsonValue::Array(arr))
}

fn chunk_to_json(
    chunk: &TsonChunk,
    defs: &[TsonDefinition],
    dict: &[String],
) -> Result<JsonValue, TsonError> {
    match &chunk.data {
        TsonData::Null => Ok(JsonValue::Null),
        TsonData::Bool(b) => Ok(JsonValue::Bool(*b)),
        TsonData::Int(i) => Ok(json!(*i)),
        TsonData::UInt(u) => Ok(json!(*u)),
        TsonData::Float(f) => Ok(JsonValue::Number(
            serde_json::Number::from_f64(*f as f64)
                .ok_or_else(|| TsonError::ParseError(format!("Cannot represent float {}", f)))?,
        )),
        TsonData::String(s) => Ok(JsonValue::String(s.clone())),
        TsonData::StrRef(idx) => {
            let s = dict.get(*idx as usize).ok_or_else(|| {
                TsonError::ParseError(format!(
                    "StrRef index {} out of bounds (dict len {})",
                    idx,
                    dict.len()
                ))
            })?;
            Ok(JsonValue::String(s.clone()))
        }
        TsonData::Array(_self_def_idx, elem_def_idx, items) => {
            let mut arr = Vec::with_capacity(items.len());
            for item in items {
                arr.push(data_to_json(item, *elem_def_idx, defs, dict)?);
            }
            Ok(JsonValue::Array(arr))
        }
        TsonData::Object(def_idx, fields) => {
            let def = resolve_def(*def_idx, defs)?;
            let field_specs = def.fields.as_ref().ok_or_else(|| {
                TsonError::ParseError(format!("Object #{}: no field definitions", def_idx))
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
                map.insert(
                    fname.clone(),
                    data_to_json(&fields[i], *def_idx, defs, dict)?,
                );
            }
            Ok(JsonValue::Object(map))
        }
    }
}

fn data_to_json(
    data: &TsonData,
    _context_def_idx: u16,
    all_defs: &[TsonDefinition],
    dict: &[String],
) -> Result<JsonValue, TsonError> {
    match data {
        TsonData::Null => Ok(JsonValue::Null),
        TsonData::Bool(b) => Ok(JsonValue::Bool(*b)),
        TsonData::Int(i) => Ok(json!(*i)),
        TsonData::UInt(u) => Ok(json!(*u)),
        TsonData::Float(f) => Ok(JsonValue::Number(
            serde_json::Number::from_f64(*f as f64)
                .ok_or_else(|| TsonError::ParseError(format!("Cannot represent float {}", f)))?,
        )),
        TsonData::String(s) => Ok(JsonValue::String(s.clone())),
        TsonData::StrRef(idx) => {
            let s = dict.get(*idx as usize).ok_or_else(|| {
                TsonError::ParseError(format!("StrRef index {} out of bounds", idx))
            })?;
            Ok(JsonValue::String(s.clone()))
        }
        TsonData::Array(_self_def_idx, elem_def_idx, items) => {
            let mut arr = Vec::with_capacity(items.len());
            for item in items {
                arr.push(data_to_json(item, *elem_def_idx, all_defs, dict)?);
            }
            Ok(JsonValue::Array(arr))
        }
        TsonData::Object(def_idx, fields) => {
            let def = resolve_def(*def_idx, all_defs)?;
            let field_specs = def.fields.as_ref().ok_or_else(|| {
                TsonError::ParseError(format!("Object #{}: no field definitions", def_idx))
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
                map.insert(
                    fname.clone(),
                    data_to_json(&fields[i], *def_idx, all_defs, dict)?,
                );
            }
            Ok(JsonValue::Object(map))
        }
    }
}

/// O(1) definition lookup: definitions are stored in index order (the
/// compiler allocates indices sequentially), so `index` is the slot.
/// Falls back to a linear scan only if the fast slot's `.index` doesn't
/// match, preserving correctness if the ordering invariant is ever violated.
fn resolve_def(index: u16, all_defs: &[TsonDefinition]) -> Result<&TsonDefinition, TsonError> {
    if let Some(def) = all_defs.get(index as usize) {
        if def.index == index {
            return Ok(def);
        }
    }
    all_defs
        .iter()
        .find(|d| d.index == index)
        .ok_or_else(|| TsonError::ParseError(format!("Unknown definition index: {}", index)))
}
