use crate::error::TsonError;
use alloc::{string::String, vec::Vec};

// ─── Type Tags ──────────────────────────────────────────────────────────────

/// Type tags for the TSON binary format (u8 on the wire).
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TsonType {
    Null   = 0x00,
    Bool   = 0x01,
    Int    = 0x02,
    UInt   = 0x03,
    Float  = 0x04,
    String = 0x05,
    Array  = 0x10,
    Object = 0x11,
}

impl TsonType {
    /// Create a `TsonType` from its raw u8 tag. Returns an error for unknown
    /// tags so the caller never gets a partially-validated value.
    pub fn from_u8(tag: u8) -> Result<Self, TsonError> {
        match tag {
            0x00 => Ok(TsonType::Null),
            0x01 => Ok(TsonType::Bool),
            0x02 => Ok(TsonType::Int),
            0x03 => Ok(TsonType::UInt),
            0x04 => Ok(TsonType::Float),
            0x05 => Ok(TsonType::String),
            0x10 => Ok(TsonType::Array),
            0x11 => Ok(TsonType::Object),
            _ => Err(TsonError::ParseError(format!(
                "Unknown TSON type tag: 0x{:02X}",
                tag
            ))),
        }
    }

    /// Returns `true` for types whose wire encoding has a fixed byte size.
    #[allow(dead_code)]
    pub fn is_fixed_size(self) -> bool {
        matches!(
            self,
            TsonType::Null | TsonType::Bool | TsonType::Int | TsonType::UInt | TsonType::Float
        )
    }

    /// For fixed-size types, returns the exact byte count on the wire.
    /// Returns `None` for variable-size types (String, Array, Object).
    #[allow(dead_code)]
    pub fn fixed_wire_size(self) -> Option<usize> {
        match self {
            TsonType::Null => Some(0),
            TsonType::Bool => Some(1),
            TsonType::Int => Some(4),
            TsonType::UInt => Some(4),
            TsonType::Float => Some(4),
            _ => None,
        }
    }
}

// ─── Header (9 bytes, fixed) ────────────────────────────────────────────────

/// TSON Header — always 9 bytes on the wire.
#[derive(Debug, Clone, PartialEq)]
pub struct TsonHeader {
    pub version: u8,
    pub blk_definition: u32,
    pub blk_data: u32,
}

impl TsonHeader {
    pub const SIZE: usize = 9;
    pub const SUPPORTED_VERSION: u8 = 1;

    pub fn new(version: u8, blk_definition: u32, blk_data: u32) -> Self {
        Self {
            version,
            blk_definition,
            blk_data,
        }
    }

    /// Parse from an exact 9-byte slice.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, TsonError> {
        if bytes.len() < Self::SIZE {
            return Err(TsonError::ParseError(format!(
                "Header requires {} bytes, got {}",
                Self::SIZE,
                bytes.len()
            )));
        }
        let version = bytes[0];
        let blk_definition = u32::from_le_bytes(bytes[1..5].try_into().unwrap());
        let blk_data = u32::from_le_bytes(bytes[5..9].try_into().unwrap());
        Ok(Self {
            version,
            blk_definition,
            blk_data,
        })
    }

    /// Encode to a fixed 9-byte array.
    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut bytes = [0u8; Self::SIZE];
        bytes[0] = self.version;
        bytes[1..5].copy_from_slice(&self.blk_definition.to_le_bytes());
        bytes[5..9].copy_from_slice(&self.blk_data.to_le_bytes());
        bytes
    }

    /// Validate header fields (version must be 1, offsets must make sense).
    pub fn validate(&self) -> Result<(), TsonError> {
        if self.version != Self::SUPPORTED_VERSION {
            return Err(TsonError::ParseError(format!(
                "Unsupported TSON version: {}, expected {}",
                self.version,
                Self::SUPPORTED_VERSION
            )));
        }
        if self.blk_definition < Self::SIZE as u32 {
            return Err(TsonError::ParseError(format!(
                "Definition block offset {} is before header end ({})",
                self.blk_definition,
                Self::SIZE
            )));
        }
        if self.blk_data < self.blk_definition {
            return Err(TsonError::ParseError(format!(
                "Data block offset ({}) must not be before definition offset ({})",
                self.blk_data, self.blk_definition
            )));
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub fn is_valid(&self) -> bool {
        self.validate().is_ok()
    }
}

// ─── Data Values ────────────────────────────────────────────────────────────

/// A decoded TSON value.
///
/// Compound types (Array, Object) carry the **definition index** in their
/// first element so that every value subtree is self-describing and can be
/// encoded/decoded uniformly without external context.
#[derive(Debug, Clone, PartialEq)]
pub enum TsonData {
    Null,
    Bool(bool),
    Int(i32),
    UInt(u32),
    Float(f32),
    String(String),
    /// `Array(self_definition_index, elem_definition_index, elements)`
    Array(u16, u16, Vec<TsonData>),
    /// `Object(definition_index, field_values_in_order)`
    Object(u16, Vec<TsonData>),
}

impl TsonData {
    /// The type tag corresponding to this value.
    pub fn type_tag(&self) -> TsonType {
        match self {
            TsonData::Null => TsonType::Null,
            TsonData::Bool(_) => TsonType::Bool,
            TsonData::Int(_) => TsonType::Int,
            TsonData::UInt(_) => TsonType::UInt,
            TsonData::Float(_) => TsonType::Float,
            TsonData::String(_) => TsonType::String,
            TsonData::Array(_, _, _) => TsonType::Array,
            TsonData::Object(_, _) => TsonType::Object,
        }
    }
}

// ─── Definition ─────────────────────────────────────────────────────────────

/// A type definition — describes the schema of one data type.
///
/// - For **Object**: `fields` contains `(field_name, field_type)` pairs.
/// - For **Array**: `elem_type` describes the element type.
/// - **Primitives** (Null, Bool, Int, UInt, Float, String): no extra fields.
#[derive(Debug, Clone, PartialEq)]
pub struct TsonDefinition {
    pub def_type: TsonType,
    /// Index of this definition in the definition block (must match its
    /// position).
    pub index: u16,
    /// Optional human-readable name (not encoded, useful for tooling).
    pub name: Option<String>,
    /// For Object: field definitions `(name, type_tag)` in order.
    pub fields: Option<Vec<(String, TsonType)>>,
    /// For Array: the element type tag.
    pub elem_type: Option<TsonType>,
}

// ─── Chunk (single data entry) ───────────────────────────────────────────────

/// A single data entry — a value (possibly compound) together with the
/// definition index that describes its structure.
#[derive(Debug, Clone, PartialEq)]
pub struct TsonChunk {
    pub definition_index: u16,
    pub data: TsonData,
}

// ─── Document ───────────────────────────────────────────────────────────────

/// A complete TSON document.
#[derive(Debug, Clone, PartialEq)]
pub struct TsonDocument {
    pub header: TsonHeader,
    pub definitions: Vec<TsonDefinition>,
    pub data: Vec<TsonChunk>,
}
