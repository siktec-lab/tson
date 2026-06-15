use alloc::vec::Vec;
use crate::error::TsonError;
use crate::structure::*;

// ─── Document ───────────────────────────────────────────────────────────────

/// Encode a complete `TsonDocument` into its binary representation.
///
/// The returned `Vec<u8>` is structured as:
/// `[9-byte header] [definition block] [data block]`
///
/// The header is patched with the correct byte offsets for both blocks
/// after they have been encoded.
pub fn encode_document(doc: &TsonDocument) -> Result<Vec<u8>, TsonError> {
    // ── Reserve 9 bytes for the header (filled in at the end) ─────
    let mut buf = Vec::new();
    buf.resize(TsonHeader::SIZE, 0u8);

    // ── Definition block ──────────────────────────────────────────
    let def_off = TsonHeader::SIZE as u32;
    encode_def_block_into(&doc.definitions, &mut buf)?;

    // ── Data block ────────────────────────────────────────────────
    let data_off = def_off + (buf.len() - TsonHeader::SIZE) as u32;
    encode_data_block_into(&doc.data, &mut buf)?;

    // ── Patch header ──────────────────────────────────────────────
    let header = TsonHeader::new(doc.header.version, def_off, data_off).to_bytes();
    buf[..TsonHeader::SIZE].copy_from_slice(&header);

    Ok(buf)
}

// ─── Definition Block ───────────────────────────────────────────────────────

/// Encode the definition block and append to `buf`.
///
/// Wire layout:
/// ```text
/// [def_count: u16 LE]
///   for each definition:
///     [type: u8] [index: u16 LE]
///     if Object: [field_count: u16 LE]
///       [name_len: u8] [name bytes] [field_type: u8]  × field_count
///     if Array:  [elem_type: u8]
///     (other types carry no extra data)
/// ```
fn encode_def_block_into(defs: &[TsonDefinition], buf: &mut Vec<u8>) -> Result<(), TsonError> {
    let count = defs.len();
    if count > u16::MAX as usize {
        return Err(TsonError::ParseError(format!(
            "Too many definitions ({}) — maximum is {}",
            count,
            u16::MAX
        )));
    }
    buf.extend_from_slice(&(count as u16).to_le_bytes());

    for def in defs {
        buf.push(def.def_type as u8);
        buf.extend_from_slice(&def.index.to_le_bytes());

        match def.def_type {
            TsonType::Object => {
                let fields = def.fields.as_ref().ok_or_else(|| {
                    TsonError::ParseError(format!(
                        "Object definition #{} has no fields",
                        def.index
                    ))
                })?;
                let fc = fields.len();
                if fc > u16::MAX as usize {
                    return Err(TsonError::ParseError(format!(
                        "Object #{} has {} fields — maximum is {}",
                        def.index,
                        fc,
                        u16::MAX
                    )));
                }
                buf.extend_from_slice(&(fc as u16).to_le_bytes());
                for (name, ft) in fields {
                    if name.len() > u8::MAX as usize {
                        return Err(TsonError::ParseError(format!(
                            "Field name \"{}\" too long ({} bytes, max {})",
                            name,
                            name.len(),
                            u8::MAX
                        )));
                    }
                    buf.push(name.len() as u8);
                    buf.extend_from_slice(name.as_bytes());
                    buf.push(*ft as u8);
                }
            }
            TsonType::Array => {
                let et = def.elem_type.ok_or_else(|| {
                    TsonError::ParseError(format!(
                        "Array definition #{} has no elem_type",
                        def.index
                    ))
                })?;
                buf.push(et as u8);
            }
            _ => { /* Primitives have no extra data */ }
        }
    }
    Ok(())
}

/// Convenience: return the definition block as a standalone `Vec<u8>`.
#[allow(dead_code)]
pub fn encode_def_block(defs: &[TsonDefinition]) -> Result<Vec<u8>, TsonError> {
    let mut buf = Vec::new();
    encode_def_block_into(defs, &mut buf)?;
    Ok(buf)
}

// ─── Data Block ─────────────────────────────────────────────────────────────

/// Encode the data block and append to `buf`.
///
/// Wire layout:
/// ```text
/// [entry_count: u32 LE]
///   for each entry:
///     [def_index: u16 LE] [payload_len: u32 LE] [payload...]
/// ```
fn encode_data_block_into(chunks: &[TsonChunk], buf: &mut Vec<u8>) -> Result<(), TsonError> {
    let count = chunks.len();
    if count > u32::MAX as usize {
        return Err(TsonError::ParseError(format!(
            "Too many data entries ({}) — maximum is {}",
            count,
            u32::MAX
        )));
    }
    buf.extend_from_slice(&(count as u32).to_le_bytes());

    for chunk in chunks {
        encode_chunk_into(chunk, buf)?;
    }
    Ok(())
}

/// Convenience: return the data block as a standalone `Vec<u8>`.
#[allow(dead_code)]
pub fn encode_data_block(chunks: &[TsonChunk]) -> Result<Vec<u8>, TsonError> {
    let mut buf = Vec::new();
    encode_data_block_into(chunks, &mut buf)?;
    Ok(buf)
}

// ─── Single Entry (Chunk) ───────────────────────────────────────────────────

/// Encode one data entry (chunk) and append to `buf`.
fn encode_chunk_into(chunk: &TsonChunk, buf: &mut Vec<u8>) -> Result<(), TsonError> {
    let payload = encode_value(&chunk.data)?;

    buf.extend_from_slice(&chunk.definition_index.to_le_bytes());
    buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    buf.extend_from_slice(&payload);

    Ok(())
}

/// Encode a single entry and return its bytes.
#[allow(dead_code)]
pub fn encode_chunk(chunk: &TsonChunk) -> Result<Vec<u8>, TsonError> {
    let mut buf = Vec::new();
    encode_chunk_into(chunk, &mut buf)?;
    Ok(buf)
}

// ─── Value Encoding ─────────────────────────────────────────────────────────

/// Encode a `TsonData` value into its wire representation.
///
/// Wire formats per type:
///
/// | Type   | Encoding                                |
/// |--------|-----------------------------------------|
/// | Null   | *(empty)*                               |
/// | Bool   | `[0/1: u8]`                             |
/// | Int    | `[i32 LE]`                              |
/// | UInt   | `[u32 LE]`                              |
/// | Float  | `[f32 LE]`                              |
/// | String | `[len: u16 LE]` `[UTF-8 bytes]`        |
/// | Array  | `[elem_def_index: u16]` `[count: u16]` elements... |
/// | Object | `[def_index: u16]` field values in order        |
pub fn encode_value(value: &TsonData) -> Result<Vec<u8>, TsonError> {
    match value {
        TsonData::Null => Ok(Vec::new()),

        TsonData::Bool(v) => Ok(vec![*v as u8]),

        TsonData::Int(v) => Ok(v.to_le_bytes().to_vec()),

        TsonData::UInt(v) => Ok(v.to_le_bytes().to_vec()),

        TsonData::Float(v) => Ok(v.to_le_bytes().to_vec()),

        TsonData::String(s) => {
            if s.len() > u16::MAX as usize {
                return Err(TsonError::ParseError(format!(
                    "String too long ({} bytes, max {})",
                    s.len(),
                    u16::MAX
                )));
            }
            let mut buf = Vec::with_capacity(2 + s.len());
            buf.extend_from_slice(&(s.len() as u16).to_le_bytes());
            buf.extend_from_slice(s.as_bytes());
            Ok(buf)
        }

        TsonData::Array(self_def, elem_def, items) => {
            if items.len() > u16::MAX as usize {
                return Err(TsonError::ParseError(format!(
                    "Array too long ({} elements, max {})",
                    items.len(),
                    u16::MAX
                )));
            }
            let mut buf = Vec::new();
            buf.extend_from_slice(&self_def.to_le_bytes());
            buf.extend_from_slice(&elem_def.to_le_bytes());
            buf.extend_from_slice(&(items.len() as u16).to_le_bytes());
            for item in items {
                let encoded = encode_value(item)?;
                buf.extend_from_slice(&encoded);
            }
            Ok(buf)
        }

        TsonData::Object(def_index, fields) => {
            let mut buf = Vec::new();
            buf.extend_from_slice(&def_index.to_le_bytes());
            for field in fields {
                let encoded = encode_value(field)?;
                buf.extend_from_slice(&encoded);
            }
            Ok(buf)
        }
    }
}
