use alloc::{string::String, vec::Vec};
use crate::error::TsonError;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TsonType {
    Null    = 0x00,
    Bool    = 0x01,
    Int     = 0x02,
    UInt    = 0x03,
    Float   = 0x04,
    String  = 0x05,
    Array   = 0x10,
    Object  = 0x11,
}

impl TsonType {
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
            _ => Err(TsonError::ParseError(format!("Unknown TSON type tag: 0x{:02X}", tag))),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TsonHeader {
    pub version: u8,
    pub blk_definition: u32,
    pub blk_dict: u32,
    pub blk_data: u32,
}

impl TsonHeader {
    pub const SIZE: usize = 13;
    pub const SUPPORTED_VERSION: u8 = 1;

    pub fn new(version: u8, blk_definition: u32, blk_dict: u32, blk_data: u32) -> Self {
        Self { version, blk_definition, blk_dict, blk_data }
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, TsonError> {
        if bytes.len() < Self::SIZE {
            return Err(TsonError::ParseError(format!("Header requires {} bytes, got {}", Self::SIZE, bytes.len())));
        }
        let version = bytes[0];
        let blk_definition = u32::from_le_bytes(bytes[1..5].try_into().unwrap());
        let blk_dict = u32::from_le_bytes(bytes[5..9].try_into().unwrap());
        let blk_data = u32::from_le_bytes(bytes[9..13].try_into().unwrap());
        Ok(Self { version, blk_definition, blk_dict, blk_data })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut bytes = [0u8; Self::SIZE];
        bytes[0] = self.version;
        bytes[1..5].copy_from_slice(&self.blk_definition.to_le_bytes());
        bytes[5..9].copy_from_slice(&self.blk_dict.to_le_bytes());
        bytes[9..13].copy_from_slice(&self.blk_data.to_le_bytes());
        bytes
    }

    pub fn validate(&self) -> Result<(), TsonError> {
        if self.version != Self::SUPPORTED_VERSION {
            return Err(TsonError::ParseError(format!("Unsupported TSON version: {}", self.version)));
        }
        if self.blk_definition < Self::SIZE as u32 {
            return Err(TsonError::ParseError(format!("Def block offset {} before header", self.blk_definition)));
        }
        if self.blk_dict < self.blk_definition {
            return Err(TsonError::ParseError(format!("Dict block offset {} before defs", self.blk_dict)));
        }
        if self.blk_data < self.blk_dict {
            return Err(TsonError::ParseError(format!("Data block offset {} before dict", self.blk_data)));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TsonData {
    Null,
    Bool(bool),
    Int(i32),
    UInt(u32),
    Float(f32),
    /// Inline UTF-8 string.
    String(String),
    /// Dereference into `doc.dict[index]` — created by the decoder when
    /// the sentinel `0xFFFF_FFFF` is detected in a String payload.
    StrRef(u32),
    Array(u16, u16, Vec<TsonData>),
    Object(u16, Vec<TsonData>),
}

impl TsonData {
    pub fn type_tag(&self) -> TsonType {
        match self {
            TsonData::Null => TsonType::Null,
            TsonData::Bool(_) => TsonType::Bool,
            TsonData::Int(_) => TsonType::Int,
            TsonData::UInt(_) => TsonType::UInt,
            TsonData::Float(_) => TsonType::Float,
            TsonData::String(_) | TsonData::StrRef(_) => TsonType::String,
            TsonData::Array(_, _, _) => TsonType::Array,
            TsonData::Object(_, _) => TsonType::Object,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TsonDefinition {
    pub def_type: TsonType,
    pub index: u16,
    pub name: Option<String>,
    pub fields: Option<Vec<(String, TsonType)>>,
    pub elem_type: Option<TsonType>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TsonChunk {
    pub definition_index: u16,
    pub data: TsonData,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TsonDocument {
    pub header: TsonHeader,
    pub definitions: Vec<TsonDefinition>,
    pub dict: Vec<String>,
    pub data: Vec<TsonChunk>,
}
