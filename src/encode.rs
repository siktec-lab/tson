use alloc::vec::Vec;
use crate::error::TsonError;
use crate::structure::*;

/// Hybrid string-length encoding:
///
/// | First byte range  | Overhead | Max length | Format                                  |
/// |-------------------|----------|------------|-----------------------------------------|
/// | `0x00..=0x7F`     | 1 B      | 127 B      | `[len: u8][UTF-8]`                     |
/// | `0x80..=0xBF`     | 2 B      | 16 383 B  | `[0x80|hi6][lo8][UTF-8]`               |
/// | `0xFE`            | 4 B      | 16 M B    | `[0xFE][u24 LE][UTF-8]`               |
/// | `0xFF`            | 5 B      | (StrRef)   | `[0xFF][dict_idx: u32 LE]`            |
///
/// The remaining bytes `0xC0..=0xFD` are reserved for future extensions.
///
/// # Design rationale
///
/// - **Small strings dominate** - names, IDs, and status codes are usually
///   under 128 bytes.  A 1-byte length head saves 3 bytes per short string
///   compared to a flat u32.
/// - **Self-describing** - the decoder only needs the first byte to decide
///   how many more to read.  No cross-entry state, no mode switches.
/// - **Streaming-safe** - every value carries its own encoding; no need to
///   have decoded a previous entry to know the length width of the current
///   one.
/// - **Sentinel for StrRef** - `0xFF` is reserved as the dict-index marker.
///   A real string of exactly 0xFF bytes cannot be stored inline - it must
///   be interned into the dict (the compiler always does this for large
///   strings anyway).

// String encoding helpers

const STRREF_SENTINEL: u8 = 0xFF;
const LONG_TAG: u8 = 0xFE;

/// Encode a string value's length prefix + UTF-8 bytes into `buf`.
/// Handles short (1B), medium (2B), and long (4B) lengths automatically.
fn encode_str_inline(buf: &mut Vec<u8>, s: &str) -> Result<(), TsonError> {
    let len = s.len();
    if len <= 0x7F {
        buf.push(len as u8);
    } else if len <= 16383 {
        buf.push(0x80 | ((len >> 8) as u8 & 0x3F));
        buf.push(len as u8);
    } else if len <= 0x00FF_FFFF {
        buf.push(LONG_TAG);
        buf.extend_from_slice(&(len as u32).to_le_bytes()[..3]);
    } else {
        return Err(TsonError::ParseError(
            "String exceeds maximum inline length (16 777 215 bytes)".into(),
        ));
    }
    buf.extend_from_slice(s.as_bytes());
    Ok(())
}

/// Encode a StrRef value: [0xFF] + dict index as u32 LE.
fn encode_str_ref(buf: &mut Vec<u8>, idx: u32) {
    buf.push(STRREF_SENTINEL);
    buf.extend_from_slice(&idx.to_le_bytes());
}

// Document

pub fn encode_document(doc: &TsonDocument) -> Result<Vec<u8>, TsonError> {
    let mut buf = Vec::new();
    buf.resize(TsonHeader::SIZE, 0u8);

    let def_off = TsonHeader::SIZE as u32;
    encode_def_block_into(&doc.definitions, &mut buf)?;

    let dict_off = buf.len() as u32;
    encode_dict_block_into(&doc.dict, &mut buf)?;

    let data_off = buf.len() as u32;
    encode_data_block_into(&doc.data, &mut buf)?;

    let header = TsonHeader::new(doc.header.version, def_off, dict_off, data_off).to_bytes();
    buf[..TsonHeader::SIZE].copy_from_slice(&header);
    Ok(buf)
}

fn encode_def_block_into(defs: &[TsonDefinition], buf: &mut Vec<u8>) -> Result<(), TsonError> {
    let count = defs.len();
    if count > u16::MAX as usize {
        return Err(TsonError::ParseError(format!("Too many definitions ({}), max {}", count, u16::MAX)));
    }
    buf.extend_from_slice(&(count as u16).to_le_bytes());
    for def in defs {
        buf.push(def.def_type as u8);
        buf.extend_from_slice(&def.index.to_le_bytes());
        match def.def_type {
            TsonType::Object => {
                let fields = def.fields.as_ref().ok_or_else(|| {
                    TsonError::ParseError(format!("Object #{} has no fields", def.index))
                })?;
                let fc = fields.len();
                buf.extend_from_slice(&(fc as u16).to_le_bytes());
                for (name, ft) in fields {
                    if name.len() > 255 {
                        return Err(TsonError::ParseError("Field name too long".into()));
                    }
                    buf.push(name.len() as u8);
                    buf.extend_from_slice(name.as_bytes());
                    buf.push(*ft as u8);
                }
            }
            TsonType::Array => {
                let et = def.elem_type.ok_or_else(|| {
                    TsonError::ParseError(format!("Array #{} has no elem_type", def.index))
                })?;
                buf.push(et as u8);
            }
            _ => {}
        }
    }
    Ok(())
}

fn encode_dict_block_into(dict: &[String], buf: &mut Vec<u8>) -> Result<(), TsonError> {
    let count = dict.len();
    buf.extend_from_slice(&(count as u32).to_le_bytes());
    for s in dict {
        encode_str_inline(buf, s)?;
    }
    Ok(())
}

fn encode_data_block_into(chunks: &[TsonChunk], buf: &mut Vec<u8>) -> Result<(), TsonError> {
    let count = chunks.len();
    buf.extend_from_slice(&(count as u32).to_le_bytes());
    for chunk in chunks {
        let payload = encode_value(&chunk.data)?;
        buf.extend_from_slice(&chunk.definition_index.to_le_bytes());
        buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        buf.extend_from_slice(&payload);
    }
    Ok(())
}

pub fn encode_value(value: &TsonData) -> Result<Vec<u8>, TsonError> {
    match value {
        TsonData::Null => Ok(Vec::new()),
        TsonData::Bool(v) => Ok(vec![*v as u8]),
        TsonData::Int(v) => Ok(v.to_le_bytes().to_vec()),
        TsonData::UInt(v) => Ok(v.to_le_bytes().to_vec()),
        TsonData::Float(v) => Ok(v.to_le_bytes().to_vec()),

        TsonData::String(s) => {
            let mut buf = Vec::with_capacity(4 + s.len());
            encode_str_inline(&mut buf, s)?;
            Ok(buf)
        }

        TsonData::StrRef(idx) => {
            let mut buf = Vec::with_capacity(5);
            encode_str_ref(&mut buf, *idx);
            Ok(buf)
        }

        TsonData::Array(self_def, elem_def, items) => {
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
