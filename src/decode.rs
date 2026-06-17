use alloc::{string::String, vec::Vec};
use crate::error::TsonError;
use crate::structure::*;

const MAX_RECURSION_DEPTH: u8 = 128;
const MAX_DEFINITIONS: usize = 2048;
const MAX_FIELDS_PER_OBJECT: usize = 256;
const MAX_DATA_ENTRIES: usize = 1_048_576;
const MIN_DEF_BYTES: usize = 3;
const DATA_ENTRY_HEADER_BYTES: usize = 6;

const STRREF_SENTINEL: u8 = 0xFF;
const LONG_TAG: u8 = 0xFE;

/// Decode a hybrid string payload (inline or StrRef). Returns the decoded
/// `TsonData` and number of bytes consumed.
///
/// Wire format:
///  - Short  (0x00..=0x7F): 1-byte length + UTF-8
///  - Medium (0x80..=0xBF): 2-byte length + UTF-8
///  - Long   (0xFE):        4-byte length (u24 LE) + UTF-8
///  - StrRef (0xFF):        5-byte dict index (u32 LE)
fn decode_str_payload(bytes: &[u8]) -> Result<(TsonData, usize), TsonError> {
    if bytes.is_empty() {
        return Err(TsonError::ParseError("Expected string/StrRef byte".into()));
    }
    let first = bytes[0];
    if first <= 0x7F {
        // Short: first byte is the length
        let len = first as usize;
        if bytes.len() < 1 + len {
            return Err(TsonError::ParseError(format!(
                "Truncated short string: need {} bytes, got {}", 1 + len, bytes.len()
            )));
        }
        let s = String::from_utf8(bytes[1..1 + len].to_vec())
            .map_err(|e| TsonError::ParseError(format!("Invalid UTF-8 string: {}", e)))?;
        Ok((TsonData::String(s), 1 + len))
    } else if first < 0xFE {
        // Medium: first byte has hi6, next byte is lo8
        if bytes.len() < 2 {
            return Err(TsonError::ParseError("Truncated medium string length".into()));
        }
        let len = ((first as usize & 0x3F) << 8) | bytes[1] as usize;
        if bytes.len() < 2 + len {
            return Err(TsonError::ParseError(format!(
                "Truncated medium string: need {} bytes, got {}", 2 + len, bytes.len()
            )));
        }
        let s = String::from_utf8(bytes[2..2 + len].to_vec())
            .map_err(|e| TsonError::ParseError(format!("Invalid UTF-8 string: {}", e)))?;
        Ok((TsonData::String(s), 2 + len))
    } else if first == LONG_TAG {
        // Long: next 3 bytes are u24 LE
        if bytes.len() < 4 {
            return Err(TsonError::ParseError("Truncated long string length".into()));
        }
        let mut u24_buf = [0u8; 4];
        u24_buf[..3].copy_from_slice(&bytes[1..4]);
        let len = u32::from_le_bytes(u24_buf) as usize;
        if bytes.len() < 4 + len {
            return Err(TsonError::ParseError(format!(
                "Truncated long string: need {} bytes, got {}", 4 + len, bytes.len()
            )));
        }
        let s = String::from_utf8(bytes[4..4 + len].to_vec())
            .map_err(|e| TsonError::ParseError(format!("Invalid UTF-8 string: {}", e)))?;
        Ok((TsonData::String(s), 4 + len))
    } else {
        // first == STRREF_SENTINEL (0xFF)
        if bytes.len() < 5 {
            return Err(TsonError::ParseError("Truncated StrRef dict index".into()));
        }
        let idx = u32::from_le_bytes(bytes[1..5].try_into().unwrap());
        Ok((TsonData::StrRef(idx), 5))
    }
}

/// Decode a hybrid-length dict string. Returns (string, byte_offset_advanced).
fn decode_dict_str(bytes: &[u8]) -> Result<(String, usize), TsonError> {
    if bytes.is_empty() {
        return Err(TsonError::ParseError("Truncated dict string".into()));
    }
    let first = bytes[0];
    if first == STRREF_SENTINEL || (first >= 0xC0 && first < 0xFE) {
        return Err(TsonError::ParseError(format!(
            "Invalid dict string encoding byte: 0x{:02X}", first
        )));
    }
    let (len, head_len) = if first <= 0x7F {
        (first as usize, 1)
    } else if first < 0xFE {
        if bytes.len() < 2 {
            return Err(TsonError::ParseError("Truncated medium dict string".into()));
        }
        (((first as usize & 0x3F) << 8) | bytes[1] as usize, 2)
    } else // first == LONG_TAG
    {
        if bytes.len() < 4 {
            return Err(TsonError::ParseError("Truncated long dict string".into()));
        }
        let mut u24 = [0u8; 4];
        u24[..3].copy_from_slice(&bytes[1..4]);
        (u32::from_le_bytes(u24) as usize, 4)
    };
    if bytes.len() < head_len + len {
        return Err(TsonError::ParseError("Truncated dict string data".into()));
    }
    let s = String::from_utf8(bytes[head_len..head_len + len].to_vec())
        .map_err(|e| TsonError::ParseError(format!("Invalid UTF-8 in dict: {}", e)))?;
    Ok((s, head_len + len))
}

pub fn decode_document(bytes: &[u8]) -> Result<TsonDocument, TsonError> {
    let header = TsonHeader::from_bytes(bytes)?;
    header.validate()?;
    let def_off = header.blk_definition as usize;
    let dict_off = header.blk_dict as usize;
    let data_off = header.blk_data as usize;
    if data_off > bytes.len() {
        return Err(TsonError::ParseError(format!("Data block offset {} exceeds buffer length {}", data_off, bytes.len())));
    }
    let definitions = decode_definitions_slice(&bytes[def_off..dict_off])?;
    let dict = decode_dict_slice(&bytes[dict_off..data_off])?;
    let data = decode_data_entries(&bytes[data_off..], &definitions)?;
    Ok(TsonDocument { header, definitions, dict, data })
}

fn decode_definitions_slice(bytes: &[u8]) -> Result<Vec<TsonDefinition>, TsonError> {
    if bytes.len() < 2 {
        return Err(TsonError::ParseError(format!("Definition block too short: {} bytes", bytes.len())));
    }
    let count = u16::from_le_bytes(bytes[0..2].try_into().unwrap()) as usize;
    if count > MAX_DEFINITIONS {
        return Err(TsonError::ParseError(format!("Definition count {} exceeds max {}", count, MAX_DEFINITIONS)));
    }
    let min_required = 2 + count * MIN_DEF_BYTES;
    if bytes.len() < min_required {
        return Err(TsonError::ParseError(format!("Definition block has {} bytes but {} defs require {}", bytes.len(), count, min_required)));
    }
    let mut defs = Vec::with_capacity(count);
    let mut pos = 2usize;
    for _ in 0..count {
        if pos >= bytes.len() {
            return Err(TsonError::ParseError(format!("Truncated definition block at entry {}", defs.len())));
        }
        let def_type = TsonType::from_u8(bytes[pos])?; pos += 1;
        if pos + 2 > bytes.len() { return Err(TsonError::ParseError("Truncated definition index".into())); }
        let index = u16::from_le_bytes(bytes[pos..pos + 2].try_into().unwrap()); pos += 2;
        match def_type {
            TsonType::Object => {
                if pos + 2 > bytes.len() { return Err(TsonError::ParseError("Truncated object field count".into())); }
                let field_count = u16::from_le_bytes(bytes[pos..pos + 2].try_into().unwrap()) as usize; pos += 2;
                if field_count > MAX_FIELDS_PER_OBJECT {
                    return Err(TsonError::ParseError(format!("Object #{} has {} fields (max {})", index, field_count, MAX_FIELDS_PER_OBJECT)));
                }
                let mut fields = Vec::with_capacity(field_count);
                for fi in 0..field_count {
                    if pos >= bytes.len() { return Err(TsonError::ParseError(format!("Truncated field {} name length", fi))); }
                    let name_len = bytes[pos] as usize; pos += 1;
                    if pos + name_len + 1 > bytes.len() { return Err(TsonError::ParseError(format!("Truncated field {} name data", fi))); }
                    let name = String::from_utf8(bytes[pos..pos + name_len].to_vec())
                        .map_err(|e| TsonError::ParseError(format!("Invalid UTF-8 in field name: {}", e)))?;
                    pos += name_len;
                    let field_type = TsonType::from_u8(bytes[pos])?; pos += 1;
                    fields.push((name, field_type));
                }
                defs.push(TsonDefinition { def_type, index, name: None, fields: Some(fields), elem_type: None });
            }
            TsonType::Array => {
                if pos >= bytes.len() { return Err(TsonError::ParseError("Truncated array elem_type".into())); }
                let elem_type = TsonType::from_u8(bytes[pos])?; pos += 1;
                defs.push(TsonDefinition { def_type, index, name: None, fields: None, elem_type: Some(elem_type) });
            }
            _ => { defs.push(TsonDefinition { def_type, index, name: None, fields: None, elem_type: None }); }
        }
    }
    Ok(defs)
}

pub fn decode_definitions(bytes: &[u8]) -> Result<Vec<TsonDefinition>, TsonError> {
    decode_definitions_slice(bytes)
}

pub(crate) fn decode_dict_slice(bytes: &[u8]) -> Result<Vec<String>, TsonError> {
    if bytes.is_empty() { return Ok(Vec::new()); }
    if bytes.len() < 4 { return Err(TsonError::ParseError("Dict block too short for entry count".into())); }
    let count = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
    let mut dict = Vec::with_capacity(count);
    let mut pos = 4usize;
    for _ in 0..count {
        let (s, consumed) = decode_dict_str(&bytes[pos..])?;
        pos += consumed;
        dict.push(s);
    }
    Ok(dict)
}

pub fn decode_data_entries(bytes: &[u8], defs: &[TsonDefinition]) -> Result<Vec<TsonChunk>, TsonError> {
    if bytes.len() < 4 { return Err(TsonError::ParseError(format!("Data block too short: {} bytes", bytes.len()))); }
    let count = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
    if count > MAX_DATA_ENTRIES { return Err(TsonError::ParseError(format!("Data entry count {} exceeds max {}", count, MAX_DATA_ENTRIES))); }
    let mut chunks = Vec::with_capacity(count);
    let mut pos = 4usize;
    for i in 0..count {
        if pos + DATA_ENTRY_HEADER_BYTES > bytes.len() { return Err(TsonError::ParseError(format!("Truncated entry {} header", i))); }
        let def_index = u16::from_le_bytes(bytes[pos..pos + 2].try_into().unwrap());
        let payload_len = u32::from_le_bytes(bytes[pos + 2..pos + 6].try_into().unwrap()) as usize;
        pos += DATA_ENTRY_HEADER_BYTES;
        if pos + payload_len > bytes.len() { return Err(TsonError::ParseError(format!("Entry {} payload ({} bytes) exceeds buffer", i, payload_len))); }
        let payload = &bytes[pos..pos + payload_len];
        let (data, consumed) = decode_root_value(payload, def_index, defs, 0)?;
        if consumed != payload_len { return Err(TsonError::ParseError(format!("Mismatch entry {}: consumed {} of {}", i, consumed, payload_len))); }
        chunks.push(TsonChunk { definition_index: def_index, data });
        pos += payload_len;
    }
    Ok(chunks)
}

pub(crate) fn decode_root_value(bytes: &[u8], def_index: u16, all_defs: &[TsonDefinition], depth: u8) -> Result<(TsonData, usize), TsonError> {
    let def = resolve_def(def_index, all_defs)?;
    match def.def_type {
        TsonType::Object => decode_object_value(bytes, all_defs, depth + 1),
        TsonType::Array => decode_array_value(bytes, all_defs, depth + 1),
        _ => decode_primitive_value(bytes, def.def_type),
    }
}

fn decode_inline_value(bytes: &[u8], inline_type: TsonType, all_defs: &[TsonDefinition], depth: u8) -> Result<(TsonData, usize), TsonError> {
    match inline_type {
        TsonType::Object => decode_object_value(bytes, all_defs, depth + 1),
        TsonType::Array => decode_array_value(bytes, all_defs, depth + 1),
        _ => decode_primitive_value(bytes, inline_type),
    }
}

fn decode_object_value(bytes: &[u8], all_defs: &[TsonDefinition], depth: u8) -> Result<(TsonData, usize), TsonError> {
    if depth > MAX_RECURSION_DEPTH {
        return Err(TsonError::ParseError(format!("Max recursion depth {} exceeded at Object", MAX_RECURSION_DEPTH).into()));
    }
    if bytes.len() < 2 { return Err(TsonError::ParseError("Truncated object self_def_index".into())); }
    let self_def = u16::from_le_bytes(bytes[0..2].try_into().unwrap());
    let def = resolve_def(self_def, all_defs)?;
    let fields = def.fields.as_ref().ok_or_else(|| TsonError::ParseError(format!("Object #{} has no fields", self_def)))?;
    let mut values = Vec::with_capacity(fields.len());
    let mut pos = 2usize;
    for (_field_name, field_type) in fields {
        let (val, consumed) = decode_inline_value(&bytes[pos..], *field_type, all_defs, depth)?;
        pos += consumed;
        values.push(val);
    }
    Ok((TsonData::Object(self_def, values), pos))
}

fn decode_array_value(bytes: &[u8], all_defs: &[TsonDefinition], depth: u8) -> Result<(TsonData, usize), TsonError> {
    if depth > MAX_RECURSION_DEPTH { return Err(TsonError::ParseError("Max recursion depth".into())); }
    if bytes.len() < 2 { return Err(TsonError::ParseError("Truncated array self_def_index".into())); }
    let self_def = u16::from_le_bytes(bytes[0..2].try_into().unwrap());
    let def = resolve_def(self_def, all_defs)?;
    let elem_type = def.elem_type.ok_or_else(|| TsonError::ParseError(format!("Array #{} has no elem_type", self_def)))?;
    if bytes.len() < 4 { return Err(TsonError::ParseError("Truncated array elem_def_index".into())); }
    let elem_def_index = u16::from_le_bytes(bytes[2..4].try_into().unwrap());
    if bytes.len() < 6 { return Err(TsonError::ParseError("Truncated array count".into())); }
    let count = u16::from_le_bytes(bytes[4..6].try_into().unwrap()) as usize;
    let mut items = Vec::with_capacity(count);
    let mut pos = 6usize;
    for _ in 0..count {
        let (item, consumed) = decode_inline_value(&bytes[pos..], elem_type, all_defs, depth)?;
        pos += consumed;
        items.push(item);
    }
    Ok((TsonData::Array(self_def, elem_def_index, items), pos))
}

fn decode_primitive_value(bytes: &[u8], ty: TsonType) -> Result<(TsonData, usize), TsonError> {
    match ty {
        TsonType::Null => Ok((TsonData::Null, 0)),
        TsonType::Bool => {
            if bytes.is_empty() { return Err(TsonError::ParseError("Expected bool byte".into())); }
            Ok((TsonData::Bool(bytes[0] != 0), 1))
        }
        TsonType::Int => {
            if bytes.len() < 4 { return Err(TsonError::ParseError("Expected int32 (4 bytes)".into())); }
            Ok((TsonData::Int(i32::from_le_bytes(bytes[0..4].try_into().unwrap())), 4))
        }
        TsonType::UInt => {
            if bytes.len() < 4 { return Err(TsonError::ParseError("Expected uint32 (4 bytes)".into())); }
            Ok((TsonData::UInt(u32::from_le_bytes(bytes[0..4].try_into().unwrap())), 4))
        }
        TsonType::Float => {
            if bytes.len() < 4 { return Err(TsonError::ParseError("Expected float32 (4 bytes)".into())); }
            Ok((TsonData::Float(f32::from_le_bytes(bytes[0..4].try_into().unwrap())), 4))
        }
        TsonType::String => decode_str_payload(bytes),
        TsonType::Array | TsonType::Object => Err(TsonError::ParseError(
            "Unexpected compound type in primitive decode path".into(),
        )),
    }
}

/// O(1) definition lookup - definitions are stored in index order.
fn resolve_def<'a>(index: u16, all_defs: &'a [TsonDefinition]) -> Result<&'a TsonDefinition, TsonError> {
    all_defs.get(index as usize).ok_or_else(|| TsonError::ParseError(format!("Unknown definition index: {}", index)))
}
