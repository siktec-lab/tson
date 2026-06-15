use alloc::{string::String, vec::Vec};
use crate::error::TsonError;
use crate::structure::*;

// ─── Security limits ────────────────────────────────────────────────────────

/// Maximum nesting depth for compound values (Object, Array). Prevents stack
/// overflow from malicious circular or deeply-nested definitions.
const MAX_RECURSION_DEPTH: u8 = 128;

/// Maximum number of definitions in a single document. Prevents OOM from
/// an attacker-declared `def_count = u16::MAX` (65535).
const MAX_DEFINITIONS: usize = 2048;

/// Maximum number of field entries per object definition. Real schemas never
/// need hundreds of fields.
const MAX_FIELDS_PER_OBJECT: usize = 256;

/// Maximum number of data entries in a single document. Prevents OOM from
/// an attacker-declared `entry_count = u32::MAX` (4 billion).
const MAX_DATA_ENTRIES: usize = 1_048_576; // 2²⁰

/// Minimum byte size for one definition entry (type u8 + index u16).
const MIN_DEF_BYTES: usize = 3;

/// Minimum byte size for one data entry header (def_index u16 + payload_len u32).
const DATA_ENTRY_HEADER_BYTES: usize = 6;

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

    let definitions = decode_definitions_slice(&bytes[def_off..data_off])?;
    let data = decode_data_entries(&bytes[data_off..], &definitions)?;

    Ok(TsonDocument { header, definitions, data })
}

// ─── Definitions ────────────────────────────────────────────────────────────

/// Decode the definition block.
///
/// This is the internal implementation used by `decode_document`.
/// The public wrapper `decode_definitions` ensures bounds are checked
/// at the caller site.
fn decode_definitions_slice(bytes: &[u8]) -> Result<Vec<TsonDefinition>, TsonError> {
    if bytes.len() < 2 {
        return Err(TsonError::ParseError(format!(
            "Definition block too short: {} bytes, need at least 2 for count",
            bytes.len()
        )));
    }
    let count = u16::from_le_bytes(bytes[0..2].try_into().unwrap()) as usize;

    // SECURITY: cap against OOM — attacker can set count = 65535
    if count > MAX_DEFINITIONS {
        return Err(TsonError::ParseError(format!(
            "Definition count {} exceeds maximum {}",
            count, MAX_DEFINITIONS
        )));
    }

    // SECURITY: validate count against remaining bytes before pre-allocating
    let min_required = 2 + count * MIN_DEF_BYTES;
    if bytes.len() < min_required {
        return Err(TsonError::ParseError(format!(
            "Definition block has {} bytes but {} definitions require at least {}",
            bytes.len(), count, min_required
        )));
    }

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
                let field_count =
                    u16::from_le_bytes(bytes[pos..pos + 2].try_into().unwrap()) as usize;
                pos += 2;

                // SECURITY: cap fields per object
                if field_count > MAX_FIELDS_PER_OBJECT {
                    return Err(TsonError::ParseError(format!(
                        "Object #{} has {} fields — maximum is {}",
                        index, field_count, MAX_FIELDS_PER_OBJECT
                    )));
                }

                let mut fields = Vec::with_capacity(field_count);
                for fi in 0..field_count {
                    if pos >= bytes.len() {
                        return Err(TsonError::ParseError(format!(
                            "Truncated field {} name length",
                            fi
                        )));
                    }
                    let name_len = bytes[pos] as usize;
                    pos += 1;
                    if pos + name_len + 1 > bytes.len() {
                        return Err(TsonError::ParseError(format!(
                            "Truncated field {} name data",
                            fi
                        )));
                    }
                    let name = String::from_utf8(bytes[pos..pos + name_len].to_vec()).map_err(
                        |e| TsonError::ParseError(format!("Invalid UTF-8 in field name: {}", e)),
                    )?;
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

/// Public convenience — decodes raw definition bytes (e.g. from a TSON stream
/// prefix). Caller must ensure `bytes` is a valid definition block slice.
///
/// This is a thin wrapper over the internal decode function.
pub fn decode_definitions(bytes: &[u8]) -> Result<Vec<TsonDefinition>, TsonError> {
    decode_definitions_slice(bytes)
}

// ─── Data Entries ───────────────────────────────────────────────────────────

/// Decode all data entries from the data block.
pub fn decode_data_entries(
    bytes: &[u8],
    defs: &[TsonDefinition],
) -> Result<Vec<TsonChunk>, TsonError> {
    if bytes.len() < 4 {
        return Err(TsonError::ParseError(format!(
            "Data block too short: {} bytes, need at least 4 for count",
            bytes.len()
        )));
    }
    let count = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;

    // SECURITY: cap against OOM
    if count > MAX_DATA_ENTRIES {
        return Err(TsonError::ParseError(format!(
            "Data entry count {} exceeds maximum {}",
            count, MAX_DATA_ENTRIES
        )));
    }

    let mut chunks = Vec::with_capacity(count);
    let mut pos = 4usize;

    for i in 0..count {
        if pos + DATA_ENTRY_HEADER_BYTES > bytes.len() {
            return Err(TsonError::ParseError(format!(
                "Truncated entry {} header",
                i
            )));
        }

        let def_index =
            u16::from_le_bytes(bytes[pos..pos + 2].try_into().unwrap());
        let payload_len =
            u32::from_le_bytes(bytes[pos + 2..pos + 6].try_into().unwrap()) as usize;
        pos += DATA_ENTRY_HEADER_BYTES;

        if pos + payload_len > bytes.len() {
            return Err(TsonError::ParseError(format!(
                "Entry {} payload ({} bytes) exceeds buffer ({})",
                i,
                payload_len,
                bytes.len() - pos
            )));
        }

        let payload = &bytes[pos..pos + payload_len];
        let (data, consumed) = decode_root_value(payload, def_index, defs, 0)?;

        if consumed != payload_len {
            return Err(TsonError::ParseError(format!(
                "Entry {} payload size mismatch: consumed {} of {} bytes",
                i, consumed, payload_len
            )));
        }

        chunks.push(TsonChunk {
            definition_index: def_index,
            data,
        });
        pos += payload_len;
    }
    Ok(chunks)
}

// ─── Root Value Decoding ────────────────────────────────────────────────────

/// Decode a root data entry value with an initial recursion depth of 0.
pub(crate) fn decode_root_value(
    bytes: &[u8],
    def_index: u16,
    all_defs: &[TsonDefinition],
    depth: u8,
) -> Result<(TsonData, usize), TsonError> {
    let def = resolve_def(def_index, all_defs)?;
    match def.def_type {
        TsonType::Object => decode_object_value(bytes, all_defs, depth + 1),
        TsonType::Array => decode_array_value(bytes, all_defs, depth + 1),
        _ => decode_primitive_value(bytes, def.def_type),
    }
}

// ─── Inline Value Decoding ──────────────────────────────────────────────────

/// Decode a value from a nested context (inside an object or array).
fn decode_inline_value(
    bytes: &[u8],
    inline_type: TsonType,
    all_defs: &[TsonDefinition],
    depth: u8,
) -> Result<(TsonData, usize), TsonError> {
    match inline_type {
        TsonType::Object => decode_object_value(bytes, all_defs, depth + 1),
        TsonType::Array => decode_array_value(bytes, all_defs, depth + 1),
        _ => decode_primitive_value(bytes, inline_type),
    }
}

// ─── Object ─────────────────────────────────────────────────────────────────

/// Decode an Object value.
///
/// SECURITY: this is a recursive function. The `depth` parameter tracks
/// nesting and rejects values beyond `MAX_RECURSION_DEPTH` to prevent
/// stack overflow from circular or deeply-nested definitions.
fn decode_object_value(
    bytes: &[u8],
    all_defs: &[TsonDefinition],
    depth: u8,
) -> Result<(TsonData, usize), TsonError> {
    // SECURITY: recursion guard
    if depth > MAX_RECURSION_DEPTH {
        return Err(TsonError::ParseError(
            format!(
                "Max recursion depth {} exceeded at Object",
                MAX_RECURSION_DEPTH
            )
            .into(),
        ));
    }

    if bytes.len() < 2 {
        return Err(TsonError::ParseError(
            "Truncated object self_def_index".into(),
        ));
    }
    let self_def = u16::from_le_bytes(bytes[0..2].try_into().unwrap());
    let def = resolve_def(self_def, all_defs)?;
    let fields = def.fields.as_ref().ok_or_else(|| {
        TsonError::ParseError(format!("Object #{} has no fields", self_def))
    })?;

    let mut values = Vec::with_capacity(fields.len());
    let mut pos = 2usize;

    for (_field_name, field_type) in fields {
        let (val, consumed) =
            decode_inline_value(&bytes[pos..], *field_type, all_defs, depth)?;
        pos += consumed;
        values.push(val);
    }

    Ok((TsonData::Object(self_def, values), pos))
}

// ─── Array ──────────────────────────────────────────────────────────────────

/// Decode an Array value.
///
/// SECURITY: this is a recursive function (for nested compounds). The `depth`
/// parameter tracks nesting and rejects values beyond `MAX_RECURSION_DEPTH`.
fn decode_array_value(
    bytes: &[u8],
    all_defs: &[TsonDefinition],
    depth: u8,
) -> Result<(TsonData, usize), TsonError> {
    // SECURITY: recursion guard
    if depth > MAX_RECURSION_DEPTH {
        return Err(TsonError::ParseError(
            format!(
                "Max recursion depth {} exceeded at Array",
                MAX_RECURSION_DEPTH
            )
            .into(),
        ));
    }

    if bytes.len() < 2 {
        return Err(TsonError::ParseError(
            "Truncated array self_def_index".into(),
        ));
    }
    let self_def = u16::from_le_bytes(bytes[0..2].try_into().unwrap());
    let def = resolve_def(self_def, all_defs)?;
    let elem_type = def.elem_type.ok_or_else(|| {
        TsonError::ParseError(format!("Array #{} has no elem_type", self_def))
    })?;

    if bytes.len() < 4 {
        return Err(TsonError::ParseError(
            "Truncated array elem_def_index".into(),
        ));
    }
    let elem_def_index = u16::from_le_bytes(bytes[2..4].try_into().unwrap());

    if bytes.len() < 6 {
        return Err(TsonError::ParseError("Truncated array count".into()));
    }
    let count = u16::from_le_bytes(bytes[4..6].try_into().unwrap()) as usize;

    let mut items = Vec::with_capacity(count);
    let mut pos = 6usize;

    for _ in 0..count {
        let (item, consumed) =
            decode_inline_value(&bytes[pos..], elem_type, all_defs, depth)?;
        pos += consumed;
        items.push(item);
    }

    Ok((TsonData::Array(self_def, elem_def_index, items), pos))
}

// ─── Primitives ─────────────────────────────────────────────────────────────

/// Decode a fixed-size or length-prefixed primitive value. (Not recursive.)
fn decode_primitive_value(
    bytes: &[u8],
    ty: TsonType,
) -> Result<(TsonData, usize), TsonError> {
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
                return Err(TsonError::ParseError(
                    "Expected int32 (4 bytes)".into(),
                ));
            }
            let v = i32::from_le_bytes(bytes[0..4].try_into().unwrap());
            Ok((TsonData::Int(v), 4))
        }

        TsonType::UInt => {
            if bytes.len() < 4 {
                return Err(TsonError::ParseError(
                    "Expected uint32 (4 bytes)".into(),
                ));
            }
            let v = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
            Ok((TsonData::UInt(v), 4))
        }

        TsonType::Float => {
            if bytes.len() < 4 {
                return Err(TsonError::ParseError(
                    "Expected float32 (4 bytes)".into(),
                ));
            }
            let v = f32::from_le_bytes(bytes[0..4].try_into().unwrap());
            Ok((TsonData::Float(v), 4))
        }

        TsonType::String => {
            if bytes.len() < 2 {
                return Err(TsonError::ParseError(
                    "Truncated string length".into(),
                ));
            }
            let len = u16::from_le_bytes(bytes[0..2].try_into().unwrap()) as usize;
            if bytes.len() < 2 + len {
                return Err(TsonError::ParseError(format!(
                    "Truncated string: header says {} bytes but only {} remain",
                    len,
                    bytes.len() - 2
                )));
            }
            let s = String::from_utf8(bytes[2..2 + len].to_vec()).map_err(|e| {
                TsonError::ParseError(format!("Invalid UTF-8 string: {}", e))
            })?;
            Ok((TsonData::String(s), 2 + len))
        }

        TsonType::Array | TsonType::Object => Err(TsonError::ParseError(
            "Unexpected compound type in primitive decode path".into(),
        )),
    }
}

// ─── Helpers ────────────────────────────────────────────────────────────────

fn resolve_def<'a>(
    index: u16,
    all_defs: &'a [TsonDefinition],
) -> Result<&'a TsonDefinition, TsonError> {
    all_defs
        .iter()
        .find(|d| d.index == index)
        .ok_or_else(|| TsonError::ParseError(format!("Unknown definition index: {}", index)))
}
