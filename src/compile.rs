use alloc::string::String;
use crate::error::TsonError;
use crate::structure::*;
use serde_json::Value as JsonValue;
use std::collections::HashMap;

const PRIM_NULL: u16 = 0;
const PRIM_BOOL: u16 = 1;
const PRIM_INT: u16 = 2;
const PRIM_UINT: u16 = 3;
const PRIM_FLOAT: u16 = 4;
const PRIM_STRING: u16 = 5;

// Pre-computed digit chars for shape key building (avoids to_string() allocation)
const TAG_CHARS: [u8; 256] = {
    let mut arr = [b'?'; 256];
    arr[0] = b'0'; arr[1] = b'1'; arr[2] = b'2'; arr[3] = b'3'; arr[4] = b'4'; arr[5] = b'5';
    arr[16] = b'6'; arr[17] = b'7'; arr[10] = b'?'; arr[11] = b'?'; arr[12] = b'?'; arr[13] = b'?'; arr[14] = b'?'; arr[15] = b'?';
    arr
};

fn prim_def(tag: TsonType) -> u16 {
    match tag {
        TsonType::Null => PRIM_NULL, TsonType::Bool => PRIM_BOOL,
        TsonType::Int => PRIM_INT, TsonType::UInt => PRIM_UINT,
        TsonType::Float => PRIM_FLOAT, TsonType::String => PRIM_STRING,
        _ => panic!("prim_def on non-primitive {:?}", tag),
    }
}

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

pub fn compile_json(root: &JsonValue) -> Result<TsonDocument, TsonError> {
    let mut builder = CompileBuilder::new();
    // Single pass: compile recursively, building defs and emitting in one traversal
    let chunks = builder.compile(root)?;
    builder.finish(chunks)
}

pub fn compile_json_str(json_text: &str) -> Result<TsonDocument, TsonError> {
    let value: JsonValue = serde_json::from_str(json_text).map_err(|e| TsonError::ParseError(e.to_string()))?;
    compile_json(&value)
}

struct CompileBuilder {
    defs: Vec<TsonDefinition>,
    shape_index: HashMap<String, u16>,
    array_index: HashMap<String, u16>,
    next_idx: u16,
    dict: Vec<String>,
    dict_map: HashMap<String, u32>,
}

/// Build shape key WITHOUT allocating via `to_string()`.
/// Uses a pre-computed character table for tag digits.
fn fast_shape_key(name: &str, tag: u8) -> String {
    let mut key = String::with_capacity(name.len() + 1);
    key.push_str(name);
    key.push(':');
    key.push(TAG_CHARS[tag as usize] as char);
    key
}

impl CompileBuilder {
    fn new() -> Self {
        let prims = primitive_defs();
        Self { next_idx: prims.len() as u16, defs: prims, shape_index: HashMap::new(), array_index: HashMap::new(), dict: Vec::new(), dict_map: HashMap::new() }
    }

    fn emit_string(&mut self, s: &str) -> TsonData {
        if let Some(&idx) = self.dict_map.get(s) {
            return TsonData::StrRef(idx);
        }
        let idx = self.dict.len() as u32;
        let owned: String = alloc::string::ToString::to_string(s);
        self.dict_map.insert(owned.clone(), idx);
        self.dict.push(owned);
        TsonData::String(s.into())
    }

    /// Single-pass compile: recursively walk the JSON tree, building
    /// definitions on first sight and emitting TsonChunks.
    fn compile(&mut self, value: &JsonValue) -> Result<Vec<TsonChunk>, TsonError> {
        match value {
            JsonValue::Null   => Ok(vec![TsonChunk { definition_index: PRIM_NULL,  data: TsonData::Null }]),
            JsonValue::Bool(b)=> Ok(vec![TsonChunk { definition_index: PRIM_BOOL,  data: TsonData::Bool(*b) }]),
            JsonValue::Number(n) => emit_number_chunk(n),
            JsonValue::String(s)=> Ok(vec![TsonChunk { definition_index: PRIM_STRING, data: self.emit_string(s) }]),

            JsonValue::Array(items) => {
                let mut elements = Vec::with_capacity(items.len());
                for item in items {
                    elements.push(self.compile_inline(item)?);
                }
                let elem_tag = array_elem_tag(items);
                let arr_sign = format!("arr:{}", elem_tag);
                let arr_def_idx = if let Some(&idx) = self.array_index.get(&arr_sign) { idx } else {
                    let idx = self.alloc_def();
                    self.defs.push(TsonDefinition { def_type: TsonType::Array, index: idx, name: None, fields: None, elem_type: Some(TsonType::from_u8(elem_tag)?) });
                    self.array_index.insert(arr_sign, idx); idx
                };
                let elem_def_idx = resolve_elem_def(&elements);
                Ok(vec![TsonChunk { definition_index: arr_def_idx, data: TsonData::Array(arr_def_idx, elem_def_idx, elements) }])
            }

            JsonValue::Object(map) => {
                // Recursively compile field values first
                let mut sorted: Vec<(&String, &JsonValue)> = map.iter().collect();
                sorted.sort_by(|a, b| a.0.cmp(b.0));
                let mut field_vals = Vec::with_capacity(map.len());
                for (_k, v) in &sorted { field_vals.push(self.compile_inline(v)?); }

                // Build shape key from sorted fields
                let mut shape_key = String::new();
                for (i, (name, v)) in sorted.iter().enumerate() {
                    if i > 0 { shape_key.push(','); }
                    shape_key.push_str(&fast_shape_key(name, json_type_tag(v)));
                }

                let obj_def_idx = if let Some(&idx) = self.shape_index.get(&shape_key) { idx } else {
                    let idx = self.alloc_def();
                    let fields: Vec<(String, TsonType)> = sorted.iter()
                        .map(|(n, v)| Ok::<_, TsonError>((alloc::string::ToString::to_string(*n), TsonType::from_u8(json_type_tag(v))?)))
                        .collect::<Result<_, _>>()?;
                    self.defs.push(TsonDefinition { def_type: TsonType::Object, index: idx, name: None, fields: Some(fields), elem_type: None });
                    self.shape_index.insert(shape_key, idx); idx
                };

                Ok(vec![TsonChunk { definition_index: obj_def_idx, data: TsonData::Object(obj_def_idx, field_vals) }])
            }
        }
    }

    fn compile_inline(&mut self, value: &JsonValue) -> Result<TsonData, TsonError> {
        match value {
            JsonValue::Null       => Ok(TsonData::Null),
            JsonValue::Bool(b)    => Ok(TsonData::Bool(*b)),
            JsonValue::Number(n)  => emit_inline_number(n),
            JsonValue::String(s)  => Ok(self.emit_string(s)),

            JsonValue::Array(items) => {
                let mut elements = Vec::with_capacity(items.len());
                for item in items { elements.push(self.compile_inline(item)?); }
                let elem_tag = array_elem_tag(items);
                let arr_sign = format!("arr:{}", elem_tag);
                let arr_def_idx = if let Some(&idx) = self.array_index.get(&arr_sign) { idx } else {
                    let idx = self.alloc_def();
                    self.defs.push(TsonDefinition { def_type: TsonType::Array, index: idx, name: None, fields: None, elem_type: Some(TsonType::from_u8(elem_tag)?) });
                    self.array_index.insert(arr_sign, idx); idx
                };
                let elem_def_idx = resolve_elem_def(&elements);
                Ok(TsonData::Array(arr_def_idx, elem_def_idx, elements))
            }

            JsonValue::Object(map) => {
                let mut sorted: Vec<(&String, &JsonValue)> = map.iter().collect();
                sorted.sort_by(|a, b| a.0.cmp(b.0));
                let mut field_vals = Vec::with_capacity(map.len());
                for (_k, v) in &sorted { field_vals.push(self.compile_inline(v)?); }
                let mut shape_key = String::new();
                for (i, (name, v)) in sorted.iter().enumerate() {
                    if i > 0 { shape_key.push(','); }
                    shape_key.push_str(&fast_shape_key(name, json_type_tag(v)));
                }
                let obj_def_idx = if let Some(&idx) = self.shape_index.get(&shape_key) { idx } else {
                    let idx = self.alloc_def();
                    let fields: Vec<(String, TsonType)> = sorted.iter()
                        .map(|(n, v)| Ok::<_, TsonError>((alloc::string::ToString::to_string(*n), TsonType::from_u8(json_type_tag(v))?)))
                        .collect::<Result<_, _>>()?;
                    self.defs.push(TsonDefinition { def_type: TsonType::Object, index: idx, name: None, fields: Some(fields), elem_type: None });
                    self.shape_index.insert(shape_key, idx); idx
                };
                Ok(TsonData::Object(obj_def_idx, field_vals))
            }
        }
    }

    fn alloc_def(&mut self) -> u16 { let idx = self.next_idx; self.next_idx += 1; idx }

    fn finish(self, chunks: Vec<TsonChunk>) -> Result<TsonDocument, TsonError> {
        Ok(TsonDocument { header: TsonHeader::new(1, TsonHeader::SIZE as u32, 0, 0), definitions: self.defs, dict: self.dict, data: chunks })
    }
}

fn emit_number_chunk(n: &serde_json::Number) -> Result<Vec<TsonChunk>, TsonError> {
    if let Some(i) = n.as_i64() { Ok(vec![TsonChunk { definition_index: PRIM_INT, data: TsonData::Int(i as i32) }]) }
    else if let Some(u) = n.as_u64() { Ok(vec![TsonChunk { definition_index: PRIM_UINT, data: TsonData::UInt(u as u32) }]) }
    else { Ok(vec![TsonChunk { definition_index: PRIM_FLOAT, data: TsonData::Float(n.as_f64().unwrap_or(0.0) as f32) }]) }
}

fn emit_inline_number(n: &serde_json::Number) -> Result<TsonData, TsonError> {
    if let Some(i) = n.as_i64() { Ok(TsonData::Int(i as i32)) }
    else if let Some(u) = n.as_u64() { Ok(TsonData::UInt(u as u32)) }
    else { Ok(TsonData::Float(n.as_f64().unwrap_or(0.0) as f32)) }
}

fn array_elem_tag(items: &[JsonValue]) -> u8 {
    items.iter().find_map(|v| { let t = json_type_tag(v); if t == 0 { None } else { Some(t) } }).unwrap_or(0u8)
}

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

fn resolve_elem_def(elements: &[TsonData]) -> u16 {
    elements.iter().find_map(|e| match e {
        TsonData::Object(idx, _) => Some(*idx),
        TsonData::Array(_, idx, _) => Some(*idx),
        TsonData::Null => None,
        _ => Some(prim_def(e.type_tag())),
    }).unwrap_or(PRIM_NULL)
}
