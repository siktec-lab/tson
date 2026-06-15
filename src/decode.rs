use alloc::{string::String, vec::Vec};
use crate::error::TsonError;
use crate::structure::*;

// ─── Document ───────────────────────────────────────────────────────────────

/// Decode a complete TSON document from a byte slice.
pub fn decode_document(bytes: &[u8]) -> Result<TsonDocument, TsonError> {
    let header = TsonHeader::from_bytes(bytes)?;
    header.validate()?;

    let def_off = header.blk_definition as usize;
    let data_off = header.blk_data as usize;

    if def_off > bytes.len() {
        return Err(TsonError::ParseError(format!(
            "Definition block offset {} exceeds buffer length {}",
            def_off, bytes.len()
        )));
    }
    if data_off > bytes.len() {
        return Err(TsonError::ParseError(format!(
            "Data block offset {} exceeds buffer length {}",
            data_off, bytes.len()
        )));
    }

    let definitions = decode_definitions(&bytes[def_off..data_off])?;
    let data = decode_data_entries(&bytes[data_off..], &definitions)?;

    Ok(TsonDocument { header, definitions, data })
}

// ─── Definitions ────────────────────────────────────────────────────────────

/// Decode the definition block.
pub fn decode_definitions(bytes: &[u8]) -> Result<Vec<TsonDefinition>, TsonError> {
    if bytes.len() < 2 {
        return Err(TsonError::ParseError(format!(
            "Definition block too short: {} bytes, need at least 2 for count",
            bytes.len()
        )));
    }
    let count = u16::from_le_bytes(bytes[0..2].try_into().unwrap()) as usize;
    let mut defs = Vec::with_capacity(count);
    let mut pos = 2usize;

    for _ in 0..count {
        if pos >= bytes.len() {
            return Err(TsonError::ParseError(format!(
                "Truncated definition block at entry {}",
                defs.len()
            )));
        }

        let def_type = TsonType::from_u8(bytes[pos])?;
        pos += 1;

        if pos + 2 > bytes.len() {
            return Err(TsonError::ParseError("Truncated definition index".into()));
        }
        let index = u16::from_le_bytes(bytes[pos..pos + 2].try_into().unwrap());
        pos += 2;

        match def_type {
            TsonType::Object => {
                if pos + 2 > bytes.len() {
                    return Err(TsonError::ParseError("Truncated object field count".into()));
                }
                let field_count = u16::from_le_bytes(bytes[pos..pos + 2].try_into().unwrap()) as usize;
                pos += 2;

                let mut fields = Vec::with_capacity(field_count);
                for fi in 0..field_count {
                    if pos >= bytes.len() {
                        return Err(TsonError::ParseError(format!("Truncated field {} name length", fi)));
                    }
                    let name_len = bytes[pos] as usize;
                    pos += 1;
                    if pos + name_len + 1 > bytes.len() {
                        return Err(TsonError::ParseError(format!("Truncated field {} name data", fi)));
                    }
                    let name = String::from_utf8(bytes[pos..pos + name_len].to_vec())
                        .map_err(|e| TsonError::ParseError(format!("Invalid UTF-8 in field name: {}", e)))?;
                    pos += name_len;
                    let field_type = TsonType::from_u8(bytes[pos])?;
                    pos += 1;
                    fields.push((name, field_type));
                }

                defs.push(TsonDefinition {
                    def_type,
                    index,
                    name: None,
                    fields: Some(fields),
                    elem_type: None,
                });
            }

            TsonType::Array => {
                if pos >= bytes.len() {
                    return Err(TsonError::ParseError("Truncated array elem_type".into()));
                }
                let elem_type = TsonType::from_u8(bytes[pos])?;
                pos += 1;
                defs.push(TsonDefinition {
                    def_type,
                    index,
                    name: None,
                    fields: None,
                    elem_type: Some(elem_type),
                });
            }

            _ => {
                defs.push(TsonDefinition {
                    def_type,
                    index,
                    name: None,
                    fields: None,
                    elem_type: None,
                });
            }
        }
    }
    Ok(defs)
}

// ─── Data Entries ───────────────────────────────────────────────────────────

/// Decode all data entries from the data block.
pub fn decode_data_entries(bytes: &[u8], defs: &[TsonDefinition]) -> Result<Vec<TsonChunk>, TsonError> {
    if bytes.len() < 4 {
        return Err(TsonError::ParseError(format!(
            "Data block too short: {} bytes, need at least 4 for count",
            bytes.len()
        )));
    }
    let count = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
    let mut chunks = Vec::with_capacity(count);
    let mut pos = 4usize;

    for i in 0..count {
        if pos + 6 > bytes.len() {
            return Err(TsonError::ParseError(format!("Truncated entry {} header", i)));
        }

        let def_index = u16::from_le_bytes(bytes[pos..pos + 2].try_into().unwrap());
        let payload_len = u32::from_le_bytes(bytes[pos + 2..pos + 6].try_into().unwrap()) as usize;
        pos += 6;

        if pos + payload_len > bytes.len() {
            return Err(TsonError::ParseError(format!(
                "Entry {} payload ({} bytes) exceeds buffer ({})",
                i, payload_len, bytes.len() - pos
            )));
        }

        let payload = &bytes[pos..pos + payload_len];
        let (data, consumed) = decode_root_value(payload, def_index, defs)?;

        if consumed != payload_len {
            return Err(TsonError::ParseError(format!(
                "Entry {} payload size mismatch: consumed {} of {} bytes",
                i, consumed, payload_len
            )));
        }

        chunks.push(TsonChunk { definition_index: def_index, data });
        pos += payload_len;
    }
    Ok(chunks)
}

// ─── Root Value Decoding ────────────────────────────────────────────────────

/// Decode a root data entry value.  For compound types (Object, Array) the
/// payload includes a `self_def_index` that identifies the definition — we
/// just pass through to the compound decoder which reads it.
pub(crate) fn decode_root_value(
    bytes: &[u8],
    def_index: u16,
    all_defs: &[TsonDefinition],
) -> Result<(TsonData, usize), TsonError> {
    let def = resolve_def(def_index, all_defs)?;
    match def.def_type {
        TsonType::Object => decode_object_value(bytes, all_defs),
        TsonType::Array => decode_array_value(bytes, all_defs),
        _ => decode_primitive_value(bytes, def.def_type),
    }
}

// ─── Inline Value Decoding ──────────────────────────────────────────────────

/// Decode a value from a nested context (inside an object or array).
///
/// Compound values (Object, Array) carry their own `self_def_index` in the
/// byte stream, so we pass through to the compound decoders directly.
/// Primitives are decoded inline from the type tag.
fn decode_inline_value(
    bytes: &[u8],
    inline_type: TsonType,
    all_defs: &[TsonDefinition],
) -> Result<(TsonData, usize), TsonError> {
    match inline_type {
        TsonType::Object => decode_object_value(bytes, all_defs),
        TsonType::Array => decode_array_value(bytes, all_defs),
        _ => decode_primitive_value(bytes, inline_type),
    }
}

// ─── Object ─────────────────────────────────────────────────────────────────

/// Decode an Object value.  Wire format:
/// ```text
/// [self_def_index: u16 LE] [field value] [field value] ...
/// ```
/// The `self_def_index` tells us which definition describes the field names
/// and types.
fn decode_object_value(
    bytes: &[u8],
    all_defs: &[TsonDefinition],
) -> Result<(TsonData, usize), TsonError> {
    if bytes.len() < 2 {
        return Err(TsonError::ParseError("Truncated object self_def_index".into()));
    }
    let self_def = u16::from_le_bytes(bytes[0..2].try_into().unwrap());
    let def = resolve_def(self_def, all_defs)?;
    let fields = def.fields.as_ref().ok_or_else(|| {
        TsonError::ParseError(format!("Object #{} has no fields", self_def))
    })?;

    let mut values = Vec::with_capacity(fields.len());
    let mut pos = 2usize;

    for (_field_name, field_type) in fields {
        let (val, consumed) = decode_inline_value(&bytes[pos..], *field_type, all_defs)?;
        pos += consumed;
        values.push(val);
    }

    Ok((TsonData::Object(self_def, values), pos))
}

// ─── Array ──────────────────────────────────────────────────────────────────

/// Decode an Array value.  Wire format:
/// ```text
/// [self_def_index: u16 LE] [elem_def_index: u16 LE] [count: u16 LE] [element] [element] ...
/// ```
/// The `self_def_index` tells us which Array definition to resolve for the
/// element type.
fn decode_array_value(
    bytes: &[u8],
    all_defs: &[TsonDefinition],
) -> Result<(TsonData, usize), TsonError> {
    if bytes.len() < 2 {
        return Err(TsonError::ParseError("Truncated array self_def_index".into()));
    }
    let self_def = u16::from_le_bytes(bytes[0..2].try_into().unwrap());
    let def = resolve_def(self_def, all_defs)?;
    let elem_type = def.elem_type.ok_or_else(|| {
        TsonError::ParseError(format!("Array #{} has no elem_type", self_def))
    })?;

    if bytes.len() < 4 {
        return Err(TsonError::ParseError("Truncated array elem_def_index".into()));
    }
    let elem_def_index = u16::from_le_bytes(bytes[2..4].try_into().unwrap());

    if bytes.len() < 6 {
        return Err(TsonError::ParseError("Truncated array count".into()));
    }
    let count = u16::from_le_bytes(bytes[4..6].try_into().unwrap()) as usize;

    let mut items = Vec::with_capacity(count);
    let mut pos = 6usize;

    for _ in 0..count {
        let (item, consumed) = decode_inline_value(&bytes[pos..], elem_type, all_defs)?;
        pos += consumed;
        items.push(item);
    }

    Ok((TsonData::Array(self_def, elem_def_index, items), pos))
}

// ─── Primitives ─────────────────────────────────────────────────────────────

/// Decode a fixed-size or length-prefixed primitive value.
fn decode_primitive_value(bytes: &[u8], ty: TsonType) -> Result<(TsonData, usize), TsonError> {
    match ty {
        TsonType::Null => Ok((TsonData::Null, 0)),

        TsonType::Bool => {
            if bytes.is_empty() {
                return Err(TsonError::ParseError("Expected bool byte".into()));
            }
            Ok((TsonData::Bool(bytes[0] != 0), 1))
        }

        TsonType::Int => {
            if bytes.len() < 4 {
                return Err(TsonError::ParseError("Expected int32 (4 bytes)".into()));
            }
            let v = i32::from_le_bytes(bytes[0..4].try_into().unwrap());
            Ok((TsonData::Int(v), 4))
        }

        TsonType::UInt => {
            if bytes.len() < 4 {
                return Err(TsonError::ParseError("Expected uint32 (4 bytes)".into()));
            }
            let v = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
            Ok((TsonData::UInt(v), 4))
        }

        TsonType::Float => {
            if bytes.len() < 4 {
                return Err(TsonError::ParseError("Expected float32 (4 bytes)".into()));
            }
            let v = f32::from_le_bytes(bytes[0..4].try_into().unwrap());
            Ok((TsonData::Float(v), 4))
        }

        TsonType::String => {
            if bytes.len() < 2 {
                return Err(TsonError::ParseError("Truncated string length".into()));
            }
            let len = u16::from_le_bytes(bytes[0..2].try_into().unwrap()) as usize;
            if bytes.len() < 2 + len {
                return Err(TsonError::ParseError(format!(
                    "Truncated string: header says {} bytes but only {} remain",
                    len,
                    bytes.len() - 2
                )));
            }
            let s = String::from_utf8(bytes[2..2 + len].to_vec())
                .map_err(|e| TsonError::ParseError(format!("Invalid UTF-8 string: {}", e)))?;
            Ok((TsonData::String(s), 2 + len))
        }

        TsonType::Array | TsonType::Object => Err(TsonError::ParseError(
            "Unexpected compound type in primitive decode path".into(),
        )),
    }
}

// ─── Helpers ────────────────────────────────────────────────────────────────

fn resolve_def<'a>(index: u16, all_defs: &'a [TsonDefinition]) -> Result<&'a TsonDefinition, TsonError> {
    all_defs
        .iter()
        .find(|d| d.index == index)
        .ok_or_else(|| TsonError::ParseError(format!("Unknown definition index: {}", index)))
}
