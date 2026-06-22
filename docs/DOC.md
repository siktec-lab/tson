# TSON - User Guide

A practical how-to for compiling, emitting, querying, and streaming TSON data.
See [README.md](../README.md) for the project overview and [TSON-FORMAT.md](TSON-FORMAT.md) for the binary specification.

---

## 1. Primary Use Case - Client/Server with JSON->TSON Bridge

A legacy client sends plain JSON. A TSON proxy compresses it to 40-70% smaller binary. The server receives TSON, extracts only the fields it needs, and acts - never touching JSON.

```
Client (JSON)  ->  Proxy (compile)  ->  Server (stream + extract fields)
  890 B              ~12 µs               374 B, no JSON parse
```

### Server Side - Extract & Act

```rust
use tson::{TsonStreamReader, TsonData};

fn handle_sensor_message(tson_bin: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let mut reader = TsonStreamReader::new(tson_bin)?;
    let defs = reader.definitions();

    // Read one entry, extract exactly the fields we need
    let chunk = reader.next().unwrap()?;

    // Direct field access - no JSON, no allocation, type-safe
    let status = chunk.data.field("status", defs)
        .and_then(|v| match v { TsonData::String(s) => Some(s.as_str()), _ => None })
        .unwrap_or("unknown");

    // Nested array access - find the temperature sensor
    let temp = chunk.data.field("sensors", defs)
        .and_then(|arr| arr.values().iter().find_map(|el| {
            el.field("type", defs).and_then(|t| {
                if matches!(t, TsonData::String(s) if s == "temp") {
                    match el.field("value", defs)? { TsonData::Float(f) => Some(*f), _ => None }
                } else { None }
            })
        }));

    // Business logic - JSON never entered the server
    if let Some(t) = temp { if t > 30.0 { alert!("high temp"); } }
    Ok(())
}
```

**Why this matters:**
- No `serde_json` parse - 2x faster, ~2.4 µs decode vs ~180 µs JSON parse
- No field-name allocation - names are in the definitions block, shared across all entries
- Type-safe extraction - `TsonData::Float` not `Value::as_f64().unwrap_or()`
- 72% of payload ignored - the server extracts 2 fields out of 10, streams past the rest

---

## 2. Quick Start - Compile & Decompile

The most common path: JSON string -> TSON binary -> JSON string.

```rust
use tson;

let json = r#"{"name":"Alice","age":30}"#;

// JSON -> TSON binary
let doc = tson::compile_json(json).unwrap();
let bytes = tson::to_bytes(&doc).unwrap();

// TSON binary -> JSON
let restored = tson::from_bytes(&bytes).unwrap();
let value = tson::decompile_to_value(&restored).unwrap();

assert_eq!(value.to_string(), r#"{"age":30,"name":"Alice"}"#);
```

**What's happening under the hood:**
- `compile_json` walks the JSON tree, discovers object shapes, builds definitions, and interns repeated strings.
- `to_bytes` encodes the document into the TSON binary format (13-byte header + defs + dict + data). Internally it appends every value directly into one shared output buffer (`encode_value_into`) rather than allocating a Vec per tree node.
- `from_bytes` decodes the binary back into a `TsonDocument` with all definitions and values.
- `decompile_to_value` reconstructs a `serde_json::Value` from the document.


## 2. Emit Mode - Direct TsonData -> Binary

When you already have structured data (not JSON text), use `emit()` to produce TSON binary directly. This bypasses `serde_json` entirely.

```rust
use tson::{TsonData, emit};

let reading = TsonData::Object(0, vec![
    TsonData::Float(22.5),                    // temperature
    TsonData::Int(61),                        // humidity
    TsonData::String("nominal".to_string()),  // status
]);

let bytes = emit(&reading).unwrap();

// Round-trip: emit -> decode -> decompile
let doc = tson::from_bytes(&bytes).unwrap();
let value = tson::decompile_to_value(&doc).unwrap();
// value = {"f0": 22.5, "f1": 61, "f2": "nominal"}
```

**Why synthetic field names?** `TsonData` carries values but not field names. The emitter discovers the object shape automatically and assigns names `"f0"`, `"f1"`, etc. For proper field names, compile from JSON instead.

**Performance:** `emit()` is a full compile path - it discovers definitions, builds the string dict, and encodes. For raw payload encoding only (when you already have definitions), use `emit_value()`:

```rust
let payload = tson::emit_value(&TsonData::Int(42)).unwrap();
// payload = [0x2A, 0x00, 0x00, 0x00]  (4 bytes, i32 LE)
```

To encode many values into a single buffer without a fresh allocation per
value, append directly with `encode::encode_value_into`:

```rust
let mut buf = Vec::new();
tson::encode::encode_value_into(&TsonData::Int(42), &mut buf).unwrap();
tson::encode::encode_value_into(&TsonData::Bool(true), &mut buf).unwrap();
// buf now holds both payloads back-to-back
```

`emit_value` ultimately encodes through the same `encode_value_into` path on a
fresh `Vec`.


## 3. Field Access - Extracting Values

Once you have a decoded `TsonDocument`, you can extract values without the full decompose-to-JSON overhead.

### Top-Level Access

```rust
let doc = tson::compile_json(r#"{"name":"Alice","age":30}"#).unwrap();

// Direct field lookup on the first data entry
let name = doc.get("name").unwrap();
assert!(matches!(name, TsonData::String(s) if s == "Alice"));

let age = doc.get("age").unwrap();
assert!(matches!(age, TsonData::Int(30)));

// Missing fields return None
assert!(doc.get("nonexistent").is_none());
```

### Nested Access

```rust
let json = r#"{"user":{"name":"Alice","meta":{"role":"admin"}}}"#;
let doc = tson::compile_json(json).unwrap();
let defs = &doc.definitions;

let user = doc.get("user").unwrap();
let name = user.field("name", defs).unwrap();
let meta = user.field("meta", defs).unwrap();
let role = meta.field("role", defs).unwrap();

assert!(matches!(role, TsonData::String(s) if s == "admin"));
```

### Array & Iterator Access

```rust
let doc = tson::compile_json(r#"["a", "b", "c"]"#).unwrap();
let entry = doc.first_entry().unwrap();

assert_eq!(entry.data.len(), 3);
assert!(!entry.data.is_empty());

for val in entry.data.values() {
    // val is &TsonData - no allocation
    println!("{:?}", val);
}
```

### Working with All Types

| TsonData variant | `values()` | `field(name, defs)` | `len()` |
|------------------|-----------|---------------------|---------|
| `Null`, `Bool`, `Int`, `UInt`, `Float` | empty slice | `None` | 0 |
| `String`, `StrRef` | empty slice | `None` | 0 |
| `Array` | element slice | `None` | element count |
| `Object` | field value slice | lookup by name | field count |


## 4. Server Response Path - `emit_with_context()`

When a server receives a TSON message and needs to emit a response, it can reuse the incoming definitions and dict. This avoids re-discovering schemas and re-building the dict.

```rust
use tson::{TsonData, emit_with_context};

// defs and dict come from a previously parsed incoming TsonDocument
let response = TsonData::Object(6, vec![
    TsonData::String("processed".to_string()),
    TsonData::Int(42),
]);
let bytes = emit_with_context(&response, &incoming_defs, &incoming_dict).unwrap();
// bytes is a complete, valid TSON document - using the same schemas as the request
```

**Key requirement**: field values must be in **definition field order** (alphabetical). The template used to define the response shape determines the field order and types.

## 5. O(1) Field Access - `doc.index()` + `doc.get_by_index()`

When extracting the same field from many documents, resolve the field name to an index once, then use O(1) index-based access.

```rust
let doc = tson::compile_json(r#"{"name":"Alice","age":30}"#).unwrap();

// Resolve once
let name_idx = doc.index("name").unwrap();
let age_idx = doc.index("age").unwrap();

// Use many times - no string comparison
for _ in 0..1000 {
    let name = doc.get_by_index(name_idx).unwrap();
    let age = doc.get_by_index(age_idx).unwrap();
    // process...
}
```

| Method | Returns | Cost |
|--------|---------|------|
| `doc.index("name")` | `Option<usize>` | O(fields) - one-time |
| `doc.get_by_index(idx)` | `Option<&TsonData>` | O(1) - array lookup |

## 6. Multi-Document Stream - `TsonDocReader`

For archives or raw TCP streams where many TSON documents are concatenated with a 4-byte length prefix, use `TsonDocReader`.

```rust
use tson::stream::TsonDocReader;
use std::io::Cursor;

let cursor = Cursor::new(archive_bytes);
for doc_result in TsonDocReader::new(cursor) {
    let doc = doc_result.unwrap();
    println!("Defs: {}, Entries: {}", doc.definitions.len(), doc.data.len());
}
```

**Format**: each document is prefixed by a 4-byte LE length `u32`, followed by the TSON binary blob. This is the same format used by the `RollingArchive` pattern in REAL-LIFE.md.

## 7. Streaming Reader

For large datasets, the streaming reader processes entries one-at-a-time with `O(1)` additional memory per entry.

```rust
use tson::TsonStreamReader;

let bytes = tson::to_bytes(&doc).unwrap();
let mut reader = TsonStreamReader::new(&bytes).unwrap();

println!("Header version: {}", reader.header().version);
println!("Definitions: {}", reader.definitions().len());
println!("Dict entries: {}", reader.dict().len());

for result in &mut reader {
    let chunk = result.unwrap();
    println!("Entry[def={}]: {:?}", chunk.definition_index, chunk.data.type_tag());
}
```

**How it works:**
1. Parse the 13-byte header -> know block offsets
2. Load the definition + dict blocks into memory (small)
3. For each data entry: read 6-byte header `[def_index:u16][payload_len:u32]`, decode the payload, yield it
4. No lookahead, no backtracking - consume once, done


## 5. Feature Flags

| Flag | Default | What it controls |
|------|---------|------------------|
| `std` | on | `std::io::Read` helpers, `IoError` variant. Off -> `no_std` + `alloc` |
| `json` | on | `compile_json` / `decompile_to_value` via `serde_json` |
| `dict` | on | String interning. Off -> all strings inline, lower compile memory |

```bash
# Kitchen-sink (default)
cargo build

# Core only - for embedded targets
cargo build --no-default-features

# Core + std, no JSON bridge, no dict
cargo build --no-default-features --features std
```

**When to disable `dict`**: if you're compiling on a microcontroller with very limited RAM and all your string values are unique. The dict block adds ~2KB of compile-time memory for the `HashMap` lookup.


## 6. CLI Usage

```bash
# Build
cargo build --release

# Compile JSON -> TSON binary
./target/release/tson-cli data.json          # writes data.tson

# Decompile TSON -> pretty JSON
./target/release/tson-cli data.tson          # prints to stdout

# Stream-debug - inspect header, definitions, dict, entries
./target/release/tson-cli -s data.tson

# Benchmark all example files
cargo run --release --bin tson-bench

# Detailed performance comparison
cargo run --release --bin comp-bench

# Statistically rigorous micro-benchmarks (Criterion)
cargo bench
```


## 7. Real-Life Pattern - Sensor Pipeline

A typical IoT pipeline: receive JSON over the network, compile to TSON, store as a rolling archive, replay later.

```rust
use tson::{TsonStreamReader, TsonData};

// Ingestion: JSON -> TSON
fn ingest(json_text: &str) -> Vec<u8> {
    let doc = tson::compile_json(json_text).unwrap();
    println!("Defs: {}, Dict: {}", doc.definitions.len(), doc.dict.len());
    tson::to_bytes(&doc).unwrap()
}

// Storage: append to rolling archive (length-prefixed)
fn append_to_archive(tson_bin: &[u8], file: &mut std::fs::File) {
    use std::io::Write;
    file.write_all(&(tson_bin.len() as u32).to_le_bytes()).unwrap();
    file.write_all(tson_bin).unwrap();
}

// Replay: stream through archive
fn replay_archive(raw: &[u8]) {
    let mut pos = 0usize;
    while pos + 4 <= raw.len() {
        let len = u32::from_le_bytes(raw[pos..pos+4].try_into().unwrap()) as usize;
        pos += 4;
        if pos + len > raw.len() { break; }

        let mut reader = TsonStreamReader::new(&raw[pos..pos+len]).unwrap();
        for result in &mut reader {
            let chunk = result.unwrap();
            // Process one entry - no allocation for field names
            if let Some(temp) = chunk.data.field("temp", reader.definitions()) {
                if matches!(temp, TsonData::Float(t) if *t > 30.0) {
                    println!("ALARM: high temperature");
                }
            }
        }
        pos += len;
    }
}
```

**Key benefits in this pipeline:**
- Field names stored once per archive (not per entry)
- Repeated strings (like "temperature", "humidity") stored once in dict
- Streaming replay reads one entry at a time - never OOMs
- Field access via `field()` avoids full JSON deserialization


## 8. Performance Summary

| Operation | users-t1.json (890 B) | telemetry.json (54.4 KB) |
|-----------|----------------------|--------------------------|
| JSON parse (baseline) | ~2.6 µs | ~348 µs |
| TSON compile | ~8.1 µs | ~428 µs |
| TSON encode | ~0.45 µs | ~11 µs |
| TSON decode | ~2.2 µs | ~74 µs |
| TSON decompile | ~2.0 µs | ~189 µs |
| TSON stream (full) | ~2.1 µs | ~72 µs |
| Full round-trip | ~12.0 µs | ~0.69 ms |

Release build (`opt-level=3`, `lto=true`, `codegen-units=1`), 2000 iterations
via `comp-bench`. TSON decode is competitive with JSON parse - the definitions
are cached in memory and O(1)-indexed. Encode writes directly into a shared
buffer (no per-node allocation), so it is by far the cheapest stage.

For statistically rigorous measurement (warmup, outlier detection), run the
Criterion harness instead:

```bash
cargo bench           # benches/core.rs: compile/encode/decode/decompile/round-trip
```


## 9. See Also

- [README.md](../README.md) - Project overview, feature comparison, security
- [TSON-FORMAT.md](TSON-FORMAT.md) - Binary wire specification
- [REAL-LIFE.md](REAL-LIFE.md) - Full walkthrough of a sensor pipeline
- [python.md](python.md) - Python (`tson-bin`) usage guide
- [js.md](js.md) - Node.js (`@siktec-lab/tson`) usage guide
