//! Compile a JSON value into a `TsonDocument`.
//!
//! # Algorithm
//!
//! 1. Primitive type definitions (Null, Bool, Int, UInt, Float, String) are
//!    pre-allocated at indices 0–5.
//! 2. **Pass 1** — Walk the JSON tree to discover all unique object and array
//!    shapes, assigning definition indices starting from 6.
//! 3. **Pass 2** — Walk again, producing `TsonChunk` entries that reference
//!    the collected definitions.
//!
//! Object deduplication is key to TSON's compression: two objects share a
//! definition if they have the same field names in the same order with the
//! same value types.

use crate::error::TsonError;
use crate::structure::*;
use serde_json::Value as JsonValue;
use std::collections::HashMap;

// ─── Primitive definition indices ───────────────────────────────────────────

const PRIM_NULL: u16 = 0;
const PRIM_BOOL: u16 = 1;
const PRIM_INT: u16 = 2;
const PRIM_UINT: u16 = 3;
const PRIM_FLOAT: u16 = 4;
const PRIM_STRING: u16 = 5;

fn prim_def(tag: TsonType) -> u16 {
    match tag {
        TsonType::Null => PRIM_NULL,
        TsonType::Bool => PRIM_BOOL,
        TsonType::Int => PRIM_INT,
        TsonType::UInt => PRIM_UINT,
        TsonType::Float => PRIM_FLOAT,
        TsonType::String => PRIM_STRING,
        _ => panic!("prim_def called on non-primitive type {:?}", tag),
    }
}

/// All 6 primitive definitions — prepended to every compiled document.
fn primitive_defs() -> Vec<TsonDefinition> {
    vec![
        TsonDefinition { def_type: TsonType::Null,   index: PRIM_NULL,  name: None, fields: None, elem_type: None },
        TsonDefinition { def_type: TsonType::Bool,   index: PRIM_BOOL,  name: None, fields: None, elem_type: None },
        TsonDefinition { def_type: TsonType::Int,    index: PRIM_INT,   name: None, fields: None, elem_type: None },
        TsonDefinition { def_type: TsonType::UInt,   index: PRIM_UINT,  name: None, fields: None, elem_type: None },
        TsonDefinition { def_type: TsonType::Float,  index: PRIM_FLOAT, name: None, fields: None, elem_type: None },
        TsonDefinition { def_type: TsonType::String, index: PRIM_STRING,name: None, fields: None, elem_type: None },
    ]
}

// ─── Public API ─────────────────────────────────────────────────────────────

/// Compile a `serde_json::Value` tree into a `TsonDocument`.
pub fn compile_json(root: &JsonValue) -> Result<TsonDocument, TsonError> {
    let mut builder = CompileBuilder::new();
    builder.discover(root)?;                        // Pass 1
    let chunks = builder.emit(root)?;               // Pass 2
    builder.finish(chunks)
}

/// Compile a JSON UTF-8 string into a `TsonDocument`.
pub fn compile_json_str(json_text: &str) -> Result<TsonDocument, TsonError> {
    let value: JsonValue =
        serde_json::from_str(json_text).map_err(|e| TsonError::ParseError(e.to_string()))?;
    compile_json(&value)
}

// ─── Builder ────────────────────────────────────────────────────────────────

struct CompileBuilder {
    /// Collected definitions — primitives (0–5) + discovered compounds (6+).
    defs: Vec<TsonDefinition>,
    /// Map from object-shape signature → definition index.
    shape_index: HashMap<String, u16>,
    /// Map from array-shape signature (e.g. "array:2") → definition index.
    array_index: HashMap<String, u16>,
    /// Next available index for compound types.
    next_idx: u16,
}

/// Generate a canonical string that uniquely identifies an object's shape:
/// `field_name:type_tag,field_name:type_tag,...`
fn object_shape_key(fields: &[(String, u8)]) -> String {
    let mut key = String::new();
    for (i, (name, tag)) in fields.iter().enumerate() {
        if i > 0 { key.push(','); }
        key.push_str(name);
        key.push(':');
        key.push_str(&tag.to_string());
    }
    key
}

impl CompileBuilder {
    fn new() -> Self {
        let prims = primitive_defs();
        Self {
            next_idx: prims.len() as u16,  // starts at 6
            defs: prims,
            shape_index: HashMap::new(),
            array_index: HashMap::new(),
        }
    }

    // ── Pass 1: Discover definitions ───────────────────────────────────

    /// Walk the JSON value tree, discover object/array shapes, assign
    /// definition indices.  Returns the definition index for this value.
    fn discover(&mut self, value: &JsonValue) -> Result<u16, TsonError> {
        match value {
            JsonValue::Null                   => Ok(PRIM_NULL),
            JsonValue::Bool(_)                => Ok(PRIM_BOOL),
            JsonValue::Number(n) if n.is_f64() && !n.is_i64() => Ok(PRIM_FLOAT),
            JsonValue::Number(n) if n.is_i64()               => Ok(PRIM_INT),
            JsonValue::Number(_)                              => Ok(PRIM_UINT),
            JsonValue::String(_)              => Ok(PRIM_STRING),

            JsonValue::Array(items) => {
                let elem_tag = items.iter()
                    .find_map(|v| { let t = json_type_tag(v); if t == 0 { None } else { Some(t) } })
                    .unwrap_or(0u8);
                let sign = format!("arr:{}", elem_tag);
                let idx = if let Some(&idx) = self.array_index.get(&sign) {
                    idx
                } else {
                    let idx = self.alloc_def();
                    self.defs.push(TsonDefinition {
                        def_type: TsonType::Array,
                        index: idx,
                        name: None,
                        fields: None,
                        elem_type: Some(TsonType::from_u8(elem_tag)?),
                    });
                    self.array_index.insert(sign, idx);
                    idx
                };
                // Recursively discover nested elements — must run even if the
                // array definition already exists, because different arrays
                // of the same element type can contain different shapes.
                for item in items {
                    self.discover(item)?;
                }
                Ok(idx)
            }

            JsonValue::Object(map) => {
                // Recursively discover field values FIRST so nested definitions
                // exist before we build this object's definition.
                for v in map.values() {
                    self.discover(v)?;
                }

                let mut field_entries: Vec<(String, u8)> = map.iter()
                    .map(|(k, v)| (k.clone(), json_type_tag(v))).collect();
                field_entries.sort_by(|a, b| a.0.cmp(&b.0));
                let sign = object_shape_key(&field_entries);

                if let Some(&idx) = self.shape_index.get(&sign) {
                    return Ok(idx);
                }
                let idx = self.alloc_def();
                let fields: Vec<(String, TsonType)> = field_entries.iter()
                    .map(|(n, t)| Ok::<_, TsonError>((n.clone(), TsonType::from_u8(*t)?)))
                    .collect::<Result<_, _>>()?;
                self.defs.push(TsonDefinition {
                    def_type: TsonType::Object,
                    index: idx,
                    name: None,
                    fields: Some(fields),
                    elem_type: None,
                });
                self.shape_index.insert(sign, idx);
                Ok(idx)
            }
        }
    }

    // ── Pass 2: Emit data chunks ───────────────────────────────────────

    /// Emit root-level data entries.
    fn emit(&self, value: &JsonValue) -> Result<Vec<TsonChunk>, TsonError> {
        match value {
            JsonValue::Null   => Ok(vec![TsonChunk { definition_index: PRIM_NULL,  data: TsonData::Null }]),
            JsonValue::Bool(b)=> Ok(vec![TsonChunk { definition_index: PRIM_BOOL,  data: TsonData::Bool(*b) }]),
            JsonValue::Number(n) => emit_number_chunk(n),
            JsonValue::String(s)=> Ok(vec![TsonChunk { definition_index: PRIM_STRING, data: TsonData::String(s.clone()) }]),

            JsonValue::Array(items) => {
                let arr_def_idx = self.find_array_def(items)?;
                let mut elements = Vec::with_capacity(items.len());
                for item in items {
                    elements.push(self.emit_inline(item)?);
                }
                // Determine the element definition index from the first
                // non-null element (handles both primitives and compounds).
                let elem_def_idx = resolve_elem_def(&elements);
                Ok(vec![TsonChunk {
                    definition_index: arr_def_idx,
                    data: TsonData::Array(arr_def_idx, elem_def_idx, elements),
                }])
            }

            JsonValue::Object(map) => {
                let obj_def_idx = self.find_object_def(map)?;
                let fields = self.emit_object_fields(map)?;
                Ok(vec![TsonChunk {
                    definition_index: obj_def_idx,
                    data: TsonData::Object(obj_def_idx, fields),
                }])
            }
        }
    }

    // ── Inline emission ────────────────────────────────────────────────

    /// Emit a nested value (no wrapper TsonChunk) — used for array elements
    /// and object field values.
    fn emit_inline(&self, value: &JsonValue) -> Result<TsonData, TsonError> {
        match value {
            JsonValue::Null       => Ok(TsonData::Null),
            JsonValue::Bool(b)    => Ok(TsonData::Bool(*b)),
            JsonValue::Number(n)  => emit_inline_number(n),
            JsonValue::String(s)  => Ok(TsonData::String(s.clone())),

            JsonValue::Array(items) => {
                let elem_tag = array_elem_tag(items);
                let sign = format!("arr:{}", elem_tag);
                let arr_def_idx = *self.array_index.get(&sign).ok_or_else(|| {
                    TsonError::ParseError("Array definition not found (did you call discover first?)".into())
                })?;
                let mut elements = Vec::with_capacity(items.len());
                for item in items {
                    elements.push(self.emit_inline(item)?);
                }
                let elem_def_idx = resolve_elem_def(&elements);
                Ok(TsonData::Array(arr_def_idx, elem_def_idx, elements))
            }

            JsonValue::Object(map) => {
                let mut field_entries: Vec<(String, u8)> = map.iter()
                    .map(|(k, v)| (k.clone(), json_type_tag(v))).collect();
                field_entries.sort_by(|a, b| a.0.cmp(&b.0));
                let sign = object_shape_key(&field_entries);
                let obj_def_idx = *self.shape_index.get(&sign).ok_or_else(|| {
                    TsonError::ParseError("Object definition not found (did you call discover first?)".into())
                })?;
                let fields = self.emit_object_fields(map)?;
                Ok(TsonData::Object(obj_def_idx, fields))
            }
        }
    }

    /// Emit the field values of a JSON object, in definition-field order.
    fn emit_object_fields(&self, map: &serde_json::Map<String, JsonValue>) -> Result<Vec<TsonData>, TsonError> {
        let mut field_vals = Vec::with_capacity(map.len());
        let mut sorted_names: Vec<&String> = map.keys().collect();
        sorted_names.sort();
        for name in sorted_names {
            let val = map.get(name.as_str()).unwrap();
            field_vals.push(self.emit_inline(val)?);
        }
        Ok(field_vals)
    }

    // ── Helpers ────────────────────────────────────────────────────────

    fn alloc_def(&mut self) -> u16 {
        let idx = self.next_idx;
        self.next_idx += 1;
        idx
    }

    fn find_array_def(&self, items: &[JsonValue]) -> Result<u16, TsonError> {
        let elem_tag = array_elem_tag(items);
        let sign = format!("arr:{}", elem_tag);
        self.array_index.get(&sign).copied().ok_or_else(|| {
            TsonError::ParseError("Array definition not found (did you call discover first?)".into())
        })
    }

    fn find_object_def(&self, map: &serde_json::Map<String, JsonValue>) -> Result<u16, TsonError> {
        let mut field_entries: Vec<(String, u8)> = map.iter()
            .map(|(k, v)| (k.clone(), json_type_tag(v))).collect();
        field_entries.sort_by(|a, b| a.0.cmp(&b.0));
        let sign = object_shape_key(&field_entries);
        self.shape_index.get(&sign).copied().ok_or_else(|| {
            TsonError::ParseError("Object definition not found (did you call discover first?)".into())
        })
    }

    fn finish(self, chunks: Vec<TsonChunk>) -> Result<TsonDocument, TsonError> {
        Ok(TsonDocument {
            header: TsonHeader::new(1, TsonHeader::SIZE as u32, 0),
            definitions: self.defs,
            data: chunks,
        })
    }
}

// ─── Number helpers ─────────────────────────────────────────────────────────

fn emit_number_chunk(n: &serde_json::Number) -> Result<Vec<TsonChunk>, TsonError> {
    // Check integer BEFORE float — serde_json::Number::as_f64() succeeds for
    // ALL numbers (including integers), so the float arm would shadow integers.
    if let Some(i) = n.as_i64() {
        Ok(vec![TsonChunk { definition_index: PRIM_INT, data: TsonData::Int(i as i32) }])
    } else if let Some(u) = n.as_u64() {
        Ok(vec![TsonChunk { definition_index: PRIM_UINT, data: TsonData::UInt(u as u32) }])
    } else {
        let f = n.as_f64().unwrap_or(0.0);
        Ok(vec![TsonChunk { definition_index: PRIM_FLOAT, data: TsonData::Float(f as f32) }])
    }
}

fn emit_inline_number(n: &serde_json::Number) -> Result<TsonData, TsonError> {
    if let Some(i) = n.as_i64() {
        Ok(TsonData::Int(i as i32))
    } else if let Some(u) = n.as_u64() {
        Ok(TsonData::UInt(u as u32))
    } else {
        Ok(TsonData::Float(n.as_f64().unwrap_or(0.0) as f32))
    }
}

// ─── Tag helpers ────────────────────────────────────────────────────────────

/// Returns the element type tag (u8) of a JSON array, based on the first
/// non-null element.
fn array_elem_tag(items: &[JsonValue]) -> u8 {
    items.iter()
        .find_map(|v| { let t = json_type_tag(v); if t == 0 { None } else { Some(t) } })
        .unwrap_or(0u8)
}

/// Map a `serde_json::Value` to its TSON type tag (u8).
fn json_type_tag(value: &JsonValue) -> u8 {
    match value {
        JsonValue::Null              => TsonType::Null as u8,
        JsonValue::Bool(_)           => TsonType::Bool as u8,
        JsonValue::Number(n) if n.is_f64() && !n.is_i64() => TsonType::Float as u8,
        JsonValue::Number(n) if n.is_i64()               => TsonType::Int as u8,
        JsonValue::Number(_)                              => TsonType::UInt as u8,
        JsonValue::String(_)         => TsonType::String as u8,
        JsonValue::Array(_)          => TsonType::Array as u8,
        JsonValue::Object(_)         => TsonType::Object as u8,
    }
}

/// Determine the element-definition index from an already-emitted slice of
/// `TsonData` values.
///
/// Scans for the first non-null element and extracts its definition index
/// (compound types carry it; primitives use the fixed primitive indices).
fn resolve_elem_def(elements: &[TsonData]) -> u16 {
    elements
        .iter()
        .find_map(|e| match e {
            TsonData::Object(idx, _) => Some(*idx),
            TsonData::Array(_, idx, _) => Some(*idx),
            TsonData::Null => None,
            _ => Some(prim_def(e.type_tag())),
        })
        .unwrap_or(PRIM_NULL)
}
