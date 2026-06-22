use crate::error::TsonError;
use crate::structure::*;
use alloc::string::String;
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
    arr[0] = b'0';
    arr[1] = b'1';
    arr[2] = b'2';
    arr[3] = b'3';
    arr[4] = b'4';
    arr[5] = b'5';
    arr[16] = b'6';
    arr[17] = b'7';
    arr[10] = b'?';
    arr[11] = b'?';
    arr[12] = b'?';
    arr[13] = b'?';
    arr[14] = b'?';
    arr[15] = b'?';
    arr
};

fn prim_def(tag: TsonType) -> u16 {
    match tag {
        TsonType::Null => PRIM_NULL,
        TsonType::Bool => PRIM_BOOL,
        TsonType::Int => PRIM_INT,
        TsonType::UInt => PRIM_UINT,
        TsonType::Float => PRIM_FLOAT,
        TsonType::String => PRIM_STRING,
        _ => panic!("prim_def on non-primitive {:?}", tag),
    }
}

fn primitive_defs() -> Vec<TsonDefinition> {
    vec![
        TsonDefinition {
            def_type: TsonType::Null,
            index: PRIM_NULL,
            name: None,
            fields: None,
            elem_type: None,
        },
        TsonDefinition {
            def_type: TsonType::Bool,
            index: PRIM_BOOL,
            name: None,
            fields: None,
            elem_type: None,
        },
        TsonDefinition {
            def_type: TsonType::Int,
            index: PRIM_INT,
            name: None,
            fields: None,
            elem_type: None,
        },
        TsonDefinition {
            def_type: TsonType::UInt,
            index: PRIM_UINT,
            name: None,
            fields: None,
            elem_type: None,
        },
        TsonDefinition {
            def_type: TsonType::Float,
            index: PRIM_FLOAT,
            name: None,
            fields: None,
            elem_type: None,
        },
        TsonDefinition {
            def_type: TsonType::String,
            index: PRIM_STRING,
            name: None,
            fields: None,
            elem_type: None,
        },
    ]
}

pub fn compile_json(root: &JsonValue) -> Result<TsonDocument, TsonError> {
    let mut builder = CompileBuilder::new();
    // Single pass: compile recursively, building defs and emitting in one traversal
    let chunks = builder.compile(root)?;
    builder.finish(chunks)
}

pub fn compile_json_str(json_text: &str) -> Result<TsonDocument, TsonError> {
    let value: JsonValue =
        serde_json::from_str(json_text).map_err(|e| TsonError::ParseError(e.to_string()))?;
    compile_json(&value)
}

struct CompileBuilder {
    defs: Vec<TsonDefinition>,
    shape_index: HashMap<String, u16>,
    /// Array shapes keyed by element type tag (a single byte) — no per-array
    /// string key allocation.
    array_index: HashMap<u8, u16>,
    /// Reusable scratch buffer for building object shape keys, cleared and
    /// refilled per object instead of allocating a fresh String each time.
    shape_key: String,
    next_idx: u16,
    /// Strings that have appeared ≥2 times - the output dict.
    dict: Vec<String>,
    /// String -> dict index (only contains strings in `dict`).
    #[cfg(feature = "dict")]
    dict_map: HashMap<String, u32>,
    /// Strings seen exactly once so far (not yet in `dict`).
    #[cfg(feature = "dict")]
    seen_once: HashMap<String, ()>,
}

/// Append one `name:tag` field segment to `key` in place, with no per-field
/// String allocation. Uses a pre-computed character table for tag digits.
fn append_shape_field(key: &mut String, name: &str, tag: u8) {
    key.push_str(name);
    key.push(':');
    key.push(TAG_CHARS[tag as usize] as char);
}

impl CompileBuilder {
    fn new() -> Self {
        let prims = primitive_defs();
        Self {
            next_idx: prims.len() as u16,
            defs: prims,
            shape_index: HashMap::new(),
            array_index: HashMap::new(),
            shape_key: String::new(),
            dict: Vec::new(),
            #[cfg(feature = "dict")]
            dict_map: HashMap::new(),
            #[cfg(feature = "dict")]
            seen_once: HashMap::new(),
        }
    }

    /// Single-pass string interning with **lazy promotion**.
    ///
    /// - First occurrence: emit inline `String`, record in `seen_once`.
    /// - Second occurrence: promote from `seen_once` -> `dict`, emit `StrRef`.
    /// - Third+ occurrences: found in `dict`, emit `StrRef`.
    ///
    /// This ensures the output dict only contains strings that actually
    /// appeared ≥2 times - no unused entries.
    ///
    /// When the `dict` feature is disabled, all strings are always emitted
    /// inline and the output dict is always empty.
    fn emit_string(&mut self, s: &str) -> TsonData {
        #[cfg(feature = "dict")]
        {
            // Fast path: already in the dict (≥2 occurrences so far)
            if let Some(&idx) = self.dict_map.get(s) {
                return TsonData::StrRef(idx);
            }
            // Second occurrence: promote from seen_once to dict
            if self.seen_once.contains_key(s) {
                let idx = self.dict.len() as u32;
                let owned: String = alloc::string::ToString::to_string(s);
                self.dict_map.insert(owned.clone(), idx);
                self.dict.push(owned);
                return TsonData::StrRef(idx);
            }
            // First occurrence: remember and emit inline
            self.seen_once
                .insert(alloc::string::ToString::to_string(s), ());
        }
        TsonData::String(s.into())
    }

    /// Single-pass compile: recursively walk the JSON tree, building
    /// definitions on first sight and emitting TsonChunks.
    fn compile(&mut self, value: &JsonValue) -> Result<Vec<TsonChunk>, TsonError> {
        match value {
            JsonValue::Null => Ok(vec![TsonChunk {
                definition_index: PRIM_NULL,
                data: TsonData::Null,
            }]),
            JsonValue::Bool(b) => Ok(vec![TsonChunk {
                definition_index: PRIM_BOOL,
                data: TsonData::Bool(*b),
            }]),
            JsonValue::Number(n) => emit_number_chunk(n),
            JsonValue::String(s) => Ok(vec![TsonChunk {
                definition_index: PRIM_STRING,
                data: self.emit_string(s),
            }]),

            JsonValue::Array(items) => {
                let mut elements = Vec::with_capacity(items.len());
                for item in items {
                    elements.push(self.compile_inline(item)?);
                }
                let elem_tag = array_elem_tag(items);
                let arr_def_idx = self.intern_array_shape(elem_tag)?;
                let elem_def_idx = resolve_elem_def(&elements);
                Ok(vec![TsonChunk {
                    definition_index: arr_def_idx,
                    data: TsonData::Array(arr_def_idx, elem_def_idx, elements),
                }])
            }

            JsonValue::Object(map) => {
                // Recursively compile field values first
                let mut sorted: Vec<(&String, &JsonValue)> = map.iter().collect();
                sorted.sort_by(|a, b| a.0.cmp(b.0));
                let mut field_vals = Vec::with_capacity(map.len());
                for (_k, v) in &sorted {
                    field_vals.push(self.compile_inline(v)?);
                }

                let obj_def_idx = self.intern_object_shape(&sorted)?;
                Ok(vec![TsonChunk {
                    definition_index: obj_def_idx,
                    data: TsonData::Object(obj_def_idx, field_vals),
                }])
            }
        }
    }

    fn compile_inline(&mut self, value: &JsonValue) -> Result<TsonData, TsonError> {
        match value {
            JsonValue::Null => Ok(TsonData::Null),
            JsonValue::Bool(b) => Ok(TsonData::Bool(*b)),
            JsonValue::Number(n) => emit_inline_number(n),
            JsonValue::String(s) => Ok(self.emit_string(s)),

            JsonValue::Array(items) => {
                let mut elements = Vec::with_capacity(items.len());
                for item in items {
                    elements.push(self.compile_inline(item)?);
                }
                let elem_tag = array_elem_tag(items);
                let arr_def_idx = self.intern_array_shape(elem_tag)?;
                let elem_def_idx = resolve_elem_def(&elements);
                Ok(TsonData::Array(arr_def_idx, elem_def_idx, elements))
            }

            JsonValue::Object(map) => {
                let mut sorted: Vec<(&String, &JsonValue)> = map.iter().collect();
                sorted.sort_by(|a, b| a.0.cmp(b.0));
                let mut field_vals = Vec::with_capacity(map.len());
                for (_k, v) in &sorted {
                    field_vals.push(self.compile_inline(v)?);
                }
                let obj_def_idx = self.intern_object_shape(&sorted)?;
                Ok(TsonData::Object(obj_def_idx, field_vals))
            }
        }
    }

    fn alloc_def(&mut self) -> u16 {
        let idx = self.next_idx;
        self.next_idx += 1;
        idx
    }

    /// Intern an array shape by element tag, returning its definition index.
    /// Keyed by the single-byte `elem_tag`, so no per-array key allocation.
    fn intern_array_shape(&mut self, elem_tag: u8) -> Result<u16, TsonError> {
        if let Some(&idx) = self.array_index.get(&elem_tag) {
            return Ok(idx);
        }
        let idx = self.alloc_def();
        self.defs.push(TsonDefinition {
            def_type: TsonType::Array,
            index: idx,
            name: None,
            fields: None,
            elem_type: Some(TsonType::from_u8(elem_tag)?),
        });
        self.array_index.insert(elem_tag, idx);
        Ok(idx)
    }

    /// Intern an object shape (sorted (name, value) pairs), returning its
    /// definition index. Builds the shape key into the reused `self.shape_key`
    /// scratch buffer and only allocates an owned key + field Vec on a miss.
    fn intern_object_shape(&mut self, sorted: &[(&String, &JsonValue)]) -> Result<u16, TsonError> {
        // Build key into the reusable scratch buffer (swapped out to satisfy
        // the borrow checker, then restored).
        let mut key = core::mem::take(&mut self.shape_key);
        key.clear();
        for (i, (name, v)) in sorted.iter().enumerate() {
            if i > 0 {
                key.push(',');
            }
            append_shape_field(&mut key, name, json_type_tag(v));
        }

        let result = if let Some(&idx) = self.shape_index.get(&key) {
            Ok(idx)
        } else {
            let idx = self.alloc_def();
            let fields: Result<Vec<(String, TsonType)>, TsonError> = sorted
                .iter()
                .map(|(n, v)| {
                    Ok((
                        alloc::string::ToString::to_string(*n),
                        TsonType::from_u8(json_type_tag(v))?,
                    ))
                })
                .collect();
            match fields {
                Ok(fields) => {
                    self.defs.push(TsonDefinition {
                        def_type: TsonType::Object,
                        index: idx,
                        name: None,
                        fields: Some(fields),
                        elem_type: None,
                    });
                    self.shape_index
                        .insert(alloc::string::ToString::to_string(&key), idx);
                    Ok(idx)
                }
                Err(e) => Err(e),
            }
        };

        // Restore the scratch buffer (retains its capacity for reuse).
        self.shape_key = key;
        result
    }

    fn finish(self, chunks: Vec<TsonChunk>) -> Result<TsonDocument, TsonError> {
        // dict already only contains strings that appeared ≥2 times.
        Ok(TsonDocument {
            header: TsonHeader::new(1, TsonHeader::SIZE as u32, 0, 0),
            definitions: self.defs,
            dict: self.dict,
            data: chunks,
        })
    }
}

fn emit_number_chunk(n: &serde_json::Number) -> Result<Vec<TsonChunk>, TsonError> {
    if let Some(i) = n.as_i64() {
        Ok(vec![TsonChunk {
            definition_index: PRIM_INT,
            data: TsonData::Int(i as i32),
        }])
    } else if let Some(u) = n.as_u64() {
        Ok(vec![TsonChunk {
            definition_index: PRIM_UINT,
            data: TsonData::UInt(u as u32),
        }])
    } else {
        Ok(vec![TsonChunk {
            definition_index: PRIM_FLOAT,
            data: TsonData::Float(n.as_f64().unwrap_or(0.0) as f32),
        }])
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

fn array_elem_tag(items: &[JsonValue]) -> u8 {
    items
        .iter()
        .find_map(|v| {
            let t = json_type_tag(v);
            if t == 0 {
                None
            } else {
                Some(t)
            }
        })
        .unwrap_or(0u8)
}

fn json_type_tag(value: &JsonValue) -> u8 {
    match value {
        JsonValue::Null => TsonType::Null as u8,
        JsonValue::Bool(_) => TsonType::Bool as u8,
        JsonValue::Number(n) if n.is_f64() && !n.is_i64() => TsonType::Float as u8,
        JsonValue::Number(n) if n.is_i64() => TsonType::Int as u8,
        JsonValue::Number(_) => TsonType::UInt as u8,
        JsonValue::String(_) => TsonType::String as u8,
        JsonValue::Array(_) => TsonType::Array as u8,
        JsonValue::Object(_) => TsonType::Object as u8,
    }
}

// Direct Data Compilation (Bypasses JSON)

/// Compile a `TsonChunk` slice directly into a `TsonDocument`, bypassing JSON.
///
/// Walks the TsonData tree to discover object/array shapes and builds the
/// string dict.  Field names are synthetic ("f0", "f1", …) since `TsonData`
/// carries values but not field names.
///
/// This is the backend behind `tson::emit()` - useful when you have
/// structured data from a sensor, database, or API and want TSON binary
/// without going through `serde_json`.
pub fn compile_from_data(chunks: &[TsonChunk]) -> Result<TsonDocument, TsonError> {
    let mut builder = DataCompiler::new();
    for chunk in chunks {
        builder.walk(&chunk.data);
    }
    builder.finish(chunks)
}

struct DataCompiler {
    defs: Vec<TsonDefinition>,
    shape_index: HashMap<String, u16>,
    array_index: HashMap<String, u16>,
    next_idx: u16,
    dict: Vec<String>,
    dict_map: HashMap<String, u32>,
    /// Maps user-provided definition indices -> real definition indices.
    /// The user writes `Object(0, fields)` but the real index might be 6.
    index_map: HashMap<u16, u16>,
}

impl DataCompiler {
    fn new() -> Self {
        let prims = primitive_defs();
        Self {
            next_idx: prims.len() as u16,
            defs: prims,
            shape_index: HashMap::new(),
            array_index: HashMap::new(),
            dict: Vec::new(),
            dict_map: HashMap::new(),
            index_map: HashMap::new(),
        }
    }

    fn walk(&mut self, value: &TsonData) {
        match value {
            TsonData::String(s) => {
                self.dict_map
                    .entry(alloc::string::ToString::to_string(s))
                    .or_insert_with(|| {
                        let idx = self.dict.len() as u32;
                        self.dict.push(alloc::string::ToString::to_string(s));
                        idx
                    });
            }
            TsonData::Object(def_idx, fields) => {
                let mut type_tags = String::new();
                for (i, field) in fields.iter().enumerate() {
                    if i > 0 {
                        type_tags.push(',');
                    }
                    type_tags.push(TAG_CHARS[field.type_tag() as u8 as usize] as char);
                    self.walk(field);
                }
                let sign = format!("obj:{}:{}", def_idx, type_tags);
                let new_idx = if let Some(&idx) = self.shape_index.get(&sign) {
                    idx
                } else {
                    let idx = self.alloc_def();
                    let mut field_defs = Vec::with_capacity(fields.len());
                    for (fi, field) in fields.iter().enumerate() {
                        let name = alloc::string::ToString::to_string(&format!("f{}", fi));
                        field_defs.push((name, field.type_tag()));
                    }
                    self.defs.push(TsonDefinition {
                        def_type: TsonType::Object,
                        index: idx,
                        name: None,
                        fields: Some(field_defs),
                        elem_type: None,
                    });
                    self.shape_index.insert(sign, idx);
                    idx
                };
                // Record mapping from user's index to real index
                self.index_map.insert(*def_idx, new_idx);
            }
            TsonData::Array(self_def, _elem_def, items) => {
                let elem_tag = items
                    .first()
                    .map(|i| i.type_tag())
                    .unwrap_or(TsonType::Null);
                let sign = format!("arr:{}:{}", self_def, elem_tag as u8);
                let new_idx = if let Some(&idx) = self.array_index.get(&sign) {
                    idx
                } else {
                    let idx = self.alloc_def();
                    self.defs.push(TsonDefinition {
                        def_type: TsonType::Array,
                        index: idx,
                        name: None,
                        fields: None,
                        elem_type: Some(elem_tag),
                    });
                    self.array_index.insert(sign, idx);
                    idx
                };
                self.index_map.insert(*self_def, new_idx);
                for item in items {
                    self.walk(item);
                }
            }
            _ => {}
        }
    }

    fn alloc_def(&mut self) -> u16 {
        let idx = self.next_idx;
        self.next_idx += 1;
        idx
    }

    /// Recursively rewrite definition indices in a TsonData tree using
    /// the index_map (old user-provided index -> real definition index).
    fn rewrite_indices(&self, data: &TsonData) -> TsonData {
        match data {
            TsonData::Object(old_def, fields) => {
                let new_def = self.index_map.get(old_def).copied().unwrap_or(*old_def);
                let new_fields: Vec<TsonData> =
                    fields.iter().map(|f| self.rewrite_indices(f)).collect();
                TsonData::Object(new_def, new_fields)
            }
            TsonData::Array(old_self, old_elem, items) => {
                let new_self = self.index_map.get(old_self).copied().unwrap_or(*old_self);
                let new_elem = self.index_map.get(old_elem).copied().unwrap_or(*old_elem);
                let new_items: Vec<TsonData> =
                    items.iter().map(|i| self.rewrite_indices(i)).collect();
                TsonData::Array(new_self, new_elem, new_items)
            }
            _ => data.clone(),
        }
    }

    fn finish(self, chunks: &[TsonChunk]) -> Result<TsonDocument, TsonError> {
        let mapped: Vec<TsonChunk> = chunks
            .iter()
            .map(|c| TsonChunk {
                definition_index: self
                    .index_map
                    .get(&c.definition_index)
                    .copied()
                    .unwrap_or(c.definition_index),
                data: self.rewrite_indices(&c.data),
            })
            .collect();
        Ok(TsonDocument {
            header: TsonHeader::new(1, TsonHeader::SIZE as u32, 0, 0),
            definitions: self.defs,
            dict: self.dict,
            data: mapped,
        })
    }
}

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
