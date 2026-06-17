# TSON — User Guide

A practical how-to for compiling, emitting, querying, and streaming TSON data.
See [README.md](README.md) for the project overview and [TSON-FORMAT.md](TSON-FORMAT.md) for the binary specification.

---

## 1. Quick Start — Compile & Decompile

The most common path: JSON string → TSON binary → JSON string.

```rust
use tson;

let json = r#"{"name":"Alice","age":30}"#;

// JSON → TSON binary
let doc = tson::compile_json(json).unwrap();
let bytes = tson::to_bytes(&doc).unwrap();

// TSON binary → JSON
let restored = tson::from_bytes(&bytes).unwrap();
let value = tson::decompile_to_value(&restored).unwrap();

assert_eq!(value.to_string(), r#"{"age":30,"name":"Alice"}"#);
```

**What's happening under the hood:**
- `compile_json` walks the JSON tree, discovers object shapes, builds definitions, and interns repeated strings.
- `to_bytes` encodes the document into the TSON binary format (13-byte header + defs + dict + data).
- `from_bytes` decodes the binary back into a `TsonDocument` with all definitions and values.
- `decompile_to_value` reconstructs a `serde_json::Value` from the document.


## 2. Emit Mode — Direct TsonData → Binary

When you already have structured data (not JSON text), use `emit()` to produce TSON binary directly. This bypasses `serde_json` entirely.

```rust
use tson::{TsonData, emit};

let reading = TsonData::Object(0, vec![
    TsonData::Float(22.5),                    // temperature
    TsonData::Int(61),                        // humidity
    TsonData::String("nominal".to_string()),  // status
]);

let bytes = emit(&reading).unwrap();

// Round-trip: emit → decode → decompile
let doc = tson::from_bytes(&bytes).unwrap();
let value = tson::decompile_to_value(&doc).unwrap();
// value = {"f0": 22.5, "f1": 61, "f2": "nominal"}
```

**Why synthetic field names?** `TsonData` carries values but not field names. The emitter discovers the object shape automatically and assigns names `"f0"`, `"f1"`, etc. For proper field names, compile from JSON instead.

**Performance:** `emit()` is a full compile path — it discovers definitions, builds the string dict, and encodes. For raw payload encoding only (when you already have definitions), use `emit_value()`:

```rust
let payload = tson::emit_value(&TsonData::Int(42)).unwrap();
// payload = [0x2A, 0x00, 0x00, 0x00]  (4 bytes, i32 LE)
```


## 3. Field Access — Extracting Values

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
    // val is &TsonData — no allocation
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


## 4. Streaming Reader

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
1. Parse the 13-byte header → know block offsets
2. Load the definition + dict blocks into memory (small)
3. For each data entry: read 6-byte header `[def_index:u16][payload_len:u32]`, decode the payload, yield it
4. No lookahead, no backtracking — consume once, done


## 5. Feature Flags

| Flag | Default | What it controls |
|------|---------|------------------|
| `std` | on | `std::io::Read` helpers, `IoError` variant. Off → `no_std` + `alloc` |
| `json` | on | `compile_json` / `decompile_to_value` via `serde_json` |
| `dict` | on | String interning. Off → all strings inline, lower compile memory |

```bash
# Kitchen-sink (default)
cargo build

# Core only — for embedded targets
cargo build --no-default-features

# Core + std, no JSON bridge, no dict
cargo build --no-default-features --features std
```

**When to disable `dict`**: if you're compiling on a microcontroller with very limited RAM and all your string values are unique. The dict block adds ~2KB of compile-time memory for the `HashMap` lookup.


## 6. CLI Usage

```bash
# Build
cargo build --release

# Compile JSON → TSON binary
./target/release/tson-cli data.json          # writes data.tson

# Decompile TSON → pretty JSON
./target/release/tson-cli data.tson          # prints to stdout

# Stream-debug — inspect header, definitions, dict, entries
./target/release/tson-cli -s data.tson

# Benchmark all example files
cargo run --release --bin tson-bench

# Detailed performance comparison
cargo run --release --bin comp-bench
```


## 7. Real-Life Pattern — Sensor Pipeline

A typical IoT pipeline: receive JSON over the network, compile to TSON, store as a rolling archive, replay later.

```rust
use tson::{TsonStreamReader, TsonData};

// ── Ingestion: JSON → TSON ────────────────────────────────────
fn ingest(json_text: &str) -> Vec<u8> {
    let doc = tson::compile_json(json_text).unwrap();
    println!("Defs: {}, Dict: {}", doc.definitions.len(), doc.dict.len());
    tson::to_bytes(&doc).unwrap()
}

// ── Storage: append to rolling archive (length-prefixed) ──────
fn append_to_archive(tson_bin: &[u8], file: &mut std::fs::File) {
    use std::io::Write;
    file.write_all(&(tson_bin.len() as u32).to_le_bytes()).unwrap();
    file.write_all(tson_bin).unwrap();
}

// ── Replay: stream through archive ──────────────────────────────
fn replay_archive(raw: &[u8]) {
    let mut pos = 0usize;
    while pos + 4 <= raw.len() {
        let len = u32::from_le_bytes(raw[pos..pos+4].try_into().unwrap()) as usize;
        pos += 4;
        if pos + len > raw.len() { break; }

        let mut reader = TsonStreamReader::new(&raw[pos..pos+len]).unwrap();
        for result in &mut reader {
            let chunk = result.unwrap();
            // Process one entry — no allocation for field names
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
- Streaming replay reads one entry at a time — never OOMs
- Field access via `field()` avoids full JSON deserialization


## 8. Performance Summary

| Operation | users-t1.json (890 B) | telemetry.json (54.4 KB) |
|-----------|----------------------|--------------------------|
| JSON parse (baseline) | ~3.2 µs | ~180 µs |
| TSON compile | ~11.5 µs | ~850 µs |
| TSON encode | ~2.6 µs | ~120 µs |
| TSON decode | ~2.4 µs | ~95 µs |
| TSON decompile | ~2.9 µs | ~130 µs |
| TSON stream (full) | ~2.0 µs | ~80 µs |
| Full round-trip | ~20.2 µs | ~1.2 ms |

Release build, 2000 iterations. TSON decode is competitive with JSON parse — the definitions are cached in memory and O(1)-indexed.


## 9. See Also

- [README.md](README.md) — Project overview, feature comparison, security
- [TSON-FORMAT.md](TSON-FORMAT.md) — Binary wire specification
- [REAL-LIFE.md](REAL-LIFE.md) — Full walkthrough of a sensor pipeline
- [Guidelines-future.md](Guidelines-future.md) — Python/Node bindings, transport-layer ergonomics
