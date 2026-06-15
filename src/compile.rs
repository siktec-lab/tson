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
    builder.discover(root)?;
    let chunks = builder.emit(root)?;
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
    /// String interning: dict index → String, and String → dict index.
    dict: Vec<String>,
    dict_map: HashMap<String, u32>,
}

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
        Self { next_idx: prims.len() as u16, defs: prims, shape_index: HashMap::new(), array_index: HashMap::new(), dict: Vec::new(), dict_map: HashMap::new() }
    }

    /// Single-pass string interning: returns `StrRef(idx)` on repeat, inline `String(s)` on first sight.
    fn emit_string(&mut self, s: &str) -> TsonData {
        if let Some(&idx) = self.dict_map.get(s) {
            return TsonData::StrRef(idx);
        }
        let idx = self.dict.len() as u32;
        let owned: String = alloc::string::ToString::to_string(s);
        self.dict.push(owned.clone());
        self.dict_map.insert(owned, idx);
        TsonData::String(s.into())
    }

    fn discover(&mut self, value: &JsonValue) -> Result<u16, TsonError> {
        match value {
            JsonValue::Null => Ok(PRIM_NULL), JsonValue::Bool(_) => Ok(PRIM_BOOL),
            JsonValue::Number(n) if n.is_f64() && !n.is_i64() => Ok(PRIM_FLOAT),
            JsonValue::Number(n) if n.is_i64() => Ok(PRIM_INT),
            JsonValue::Number(_) => Ok(PRIM_UINT),
            JsonValue::String(_) => Ok(PRIM_STRING),

            JsonValue::Array(items) => {
                let elem_tag = items.iter().find_map(|v| { let t = json_type_tag(v); if t == 0 { None } else { Some(t) } }).unwrap_or(0u8);
                let sign = format!("arr:{}", elem_tag);
                let idx = if let Some(&idx) = self.array_index.get(&sign) { idx } else {
                    let idx = self.alloc_def();
                    self.defs.push(TsonDefinition { def_type: TsonType::Array, index: idx, name: None, fields: None, elem_type: Some(TsonType::from_u8(elem_tag)?) });
                    self.array_index.insert(sign, idx); idx
                };
                for item in items { self.discover(item)?; }
                Ok(idx)
            }

            JsonValue::Object(map) => {
                for v in map.values() { self.discover(v)?; }
                let mut field_entries: Vec<(String, u8)> = map.iter().map(|(k, v)| (k.clone(), json_type_tag(v))).collect();
                field_entries.sort_by(|a, b| a.0.cmp(&b.0));
                let sign = object_shape_key(&field_entries);
                if let Some(&idx) = self.shape_index.get(&sign) { return Ok(idx); }
                let idx = self.alloc_def();
                let fields: Vec<(String, TsonType)> = field_entries.iter().map(|(n, t)| Ok::<_, TsonError>((n.clone(), TsonType::from_u8(*t)?))).collect::<Result<_, _>>()?;
                self.defs.push(TsonDefinition { def_type: TsonType::Object, index: idx, name: None, fields: Some(fields), elem_type: None });
                self.shape_index.insert(sign, idx);
                Ok(idx)
            }
        }
    }

    fn emit(&mut self, value: &JsonValue) -> Result<Vec<TsonChunk>, TsonError> {
        match value {
            JsonValue::Null   => Ok(vec![TsonChunk { definition_index: PRIM_NULL,  data: TsonData::Null }]),
            JsonValue::Bool(b)=> Ok(vec![TsonChunk { definition_index: PRIM_BOOL,  data: TsonData::Bool(*b) }]),
            JsonValue::Number(n) => emit_number_chunk(n),
            JsonValue::String(s)=> Ok(vec![TsonChunk { definition_index: PRIM_STRING, data: self.emit_string(s) }]),

            JsonValue::Array(items) => {
                let arr_def_idx = self.find_array_def(items)?;
                let mut elements = Vec::with_capacity(items.len());
                for item in items { elements.push(self.emit_inline(item)?); }
                let elem_def_idx = resolve_elem_def(&elements);
                Ok(vec![TsonChunk { definition_index: arr_def_idx, data: TsonData::Array(arr_def_idx, elem_def_idx, elements) }])
            }

            JsonValue::Object(map) => {
                let obj_def_idx = self.find_object_def(map)?;
                let fields = self.emit_object_fields(map)?;
                Ok(vec![TsonChunk { definition_index: obj_def_idx, data: TsonData::Object(obj_def_idx, fields) }])
            }
        }
    }

    fn emit_inline(&mut self, value: &JsonValue) -> Result<TsonData, TsonError> {
        match value {
            JsonValue::Null       => Ok(TsonData::Null),
            JsonValue::Bool(b)    => Ok(TsonData::Bool(*b)),
            JsonValue::Number(n)  => emit_inline_number(n),
            JsonValue::String(s)  => Ok(self.emit_string(s)),

            JsonValue::Array(items) => {
                let elem_tag = array_elem_tag(items);
                let sign = format!("arr:{}", elem_tag);
                let arr_def_idx = *self.array_index.get(&sign).ok_or_else(|| TsonError::ParseError("Array definition not found".into()))?;
                let mut elements = Vec::with_capacity(items.len());
                for item in items { elements.push(self.emit_inline(item)?); }
                let elem_def_idx = resolve_elem_def(&elements);
                Ok(TsonData::Array(arr_def_idx, elem_def_idx, elements))
            }

            JsonValue::Object(map) => {
                let mut field_entries: Vec<(String, u8)> = map.iter().map(|(k, v)| (k.clone(), json_type_tag(v))).collect();
                field_entries.sort_by(|a, b| a.0.cmp(&b.0));
                let sign = object_shape_key(&field_entries);
                let obj_def_idx = *self.shape_index.get(&sign).ok_or_else(|| TsonError::ParseError("Object definition not found".into()))?;
                let fields = self.emit_object_fields(map)?;
                Ok(TsonData::Object(obj_def_idx, fields))
            }
        }
    }

    fn emit_object_fields(&mut self, map: &serde_json::Map<String, JsonValue>) -> Result<Vec<TsonData>, TsonError> {
        let mut field_vals = Vec::with_capacity(map.len());
        let mut sorted_names: Vec<&String> = map.keys().collect();
        sorted_names.sort();
        for name in sorted_names {
            let val = map.get(name.as_str()).unwrap();
            field_vals.push(self.emit_inline(val)?);
        }
        Ok(field_vals)
    }

    fn alloc_def(&mut self) -> u16 { let idx = self.next_idx; self.next_idx += 1; idx }

    fn find_array_def(&self, items: &[JsonValue]) -> Result<u16, TsonError> {
        let elem_tag = array_elem_tag(items);
        let sign = format!("arr:{}", elem_tag);
        self.array_index.get(&sign).copied().ok_or_else(|| TsonError::ParseError("Array definition not found".into()))
    }

    fn find_object_def(&self, map: &serde_json::Map<String, JsonValue>) -> Result<u16, TsonError> {
        let mut field_entries: Vec<(String, u8)> = map.iter().map(|(k, v)| (k.clone(), json_type_tag(v))).collect();
        field_entries.sort_by(|a, b| a.0.cmp(&b.0));
        let sign = object_shape_key(&field_entries);
        self.shape_index.get(&sign).copied().ok_or_else(|| TsonError::ParseError("Object definition not found".into()))
    }

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
