# TSON - Terse JSON Binary Format

A compact, schema-deduplicated binary format for JSON data, built for microcontrollers and constrained environments.

**Core idea**: in repetitive JSON (API payloads, telemetry, config), field names appear thousands of times. TSON stores them **once** in a definition block. Repeated strings are stored once in a dict block. The data stream is pure typed values, no key repetition, no duplicate strings.

```
JSON (890 bytes)               TSON binary (~374 bytes)
[{                              ┌── Header (13 B)
  "id": 1,                      │   version=1, def_off=13,
  "name": "Alice",              │   dict_off=110, data_off=122
  "age": 30,                    ├── Definition block (97 B)
  "address": {                  │   #0 Null  #1 Bool  #2 Int  #3 UInt
    "street": "123…",           │   #4 Float  #5 String
    "city": "Anytown",          │   #6 Array<String>
    "state": "CA",              │   #7 Object fields:
    "zip": "12345"              │      street:String city:String
  },                            │      state:String zip:String
  "hobbies": ["reading",        │   #8 Object fields:
    "hiking", "cooking"]        │      id:Int name:String age:Int
  },                            │      address:#7 hobbies:#6
  …                              │   #9 Array<Object>
]                               ├── Dict block (12 B, only
                                │   repeated strings)
                                ├── Data block (252 B)
                                │   Entry #9: 3 elements
                                │     [0]: #8 -> 1, "Alice", 30…
                                │     [1]: #8 -> 2, "Bob",   25…
                                │     [2]: #8 -> 3, "Charlie",35…
                                └── (end)
```

## Features

- **Zero-dependency core**: encode/decode/stream on `&[u8]` slices, only needs `alloc`.
- **Streaming reader**: loads the tiny definition + dict blocks into memory, then yields data entries one-at-a-time - `O(1)` memory per entry.
- **Schema deduplication**: identical object shapes share one definition. Field names stored once.
- **String interning** (`dict` feature): repeated strings stored once in a dict block. `StrRef` points to them instead of repeating inline. Only strings that appear ≥2 times are included - no waste.
- **Hybrid string encoding**: short strings (≤127 B) use 1-byte length, medium strings 2 bytes, long strings 4 bytes - saves space over flat u32.
- **`no_std` capable**: disable the `std` feature for embedded targets.
- **Optional JSON bridge**: `serde_json`-based compile/decompile behind the `json` feature.
- **Self-describing wire format**: every compound value carries its definition index, enabling forward compatibility and partial decoding.

## Quick Start

```rust
// Round-trip a JSON string through TSON binary
let json = r#"{"name":"Alice","age":30}"#;

// JSON -> TSON document -> binary
let doc = tson::compile_json(json).unwrap();
let bytes = tson::to_bytes(&doc).unwrap();

// Binary -> TSON document -> JSON
let restored = tson::from_bytes(&bytes).unwrap();
let value = tson::decompile_to_value(&restored).unwrap();

assert_eq!(value.to_string(), r#"{"age":30,"name":"Alice"}"#);
```

## Emit Mode (Bypass JSON)

Need TSON binary directly from structured data without parsing JSON? `tson::emit()` takes a `TsonData` tree and produces a complete TSON document.

```rust
use tson::{TsonData, emit};

// Build a sensor reading value tree directly
let reading = TsonData::Object(0, vec![
    TsonData::Float(22.5),                   // temperature
    TsonData::Int(61),                       // humidity
    TsonData::String("nominal".to_string()), // status
]);

// Emit as TSON binary - no JSON parse step
let bytes = emit(&reading).unwrap();

// Decode back
let doc = tson::from_bytes(&bytes).unwrap();
let value = tson::decompile_to_value(&doc).unwrap();
// value = {"f0": 22.5, "f1": 61, "f2": "nominal"}
```

Field names are synthetic (`"f0"`, `"f1"`, …) since `TsonData` values don't carry names. Definitions and the string dict are discovered automatically from the value tree.

## Server Response Path - `emit_with_context()`

Reuse an incoming document's definitions and dict to emit a response - no schema re-discovery, no dict rebuild.

```rust
use tson::{TsonData, emit_with_context};

let response = TsonData::Object(6, vec![
    TsonData::String("processed".to_string()),
    TsonData::Int(42),
]);
let bytes = emit_with_context(&response, &incoming_defs, &incoming_dict).unwrap();
```

Field values must be in **definition field order** (alphabetical).

## Direct Field Access - `doc.get()`, `doc.index()`, `doc.get_by_index()`

Extract values without decompiling to JSON. O(1) access when you pre-resolve field indices:

```rust
let doc = tson::compile_json(r#"{"name":"Alice","age":30}"#).unwrap();

// By name (linear scan)
let name = doc.get("name").unwrap();
let age = doc.get("age").unwrap();

// Or pre-resolve index for O(1) repeated access
let name_idx = doc.index("name").unwrap();
for _ in 0..1000 {
    let n = doc.get_by_index(name_idx).unwrap();
}
```

## Multi-Document Stream - `TsonDocReader`

Read length-prefixed TSON documents from any byte source (archives, TCP streams).

```rust
use tson::stream::TsonDocReader;
use std::io::Cursor;

for doc in TsonDocReader::new(cursor) {
    println!("Defs: {}", doc.unwrap().definitions.len());
}
```

Each document is prefixed by a 4-byte LE length `u32` followed by the TSON binary.

## Command-Line Tool

```bash
# Build
cargo build --release

# Compile JSON -> TSON binary
./target/release/tson-cli data.json         # writes data.tson

# Decompile TSON -> pretty JSON
./target/release/tson-cli data.tson         # prints JSON to stdout

# Stream-debug (inspect header, defs, dict, entries)
./target/release/tson-cli -s data.tson
```

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `std`   | on      | Enables `std::io::Read` helpers and the `IoError` variant. Off -> `no_std` + `alloc`. |
| `json`  | on      | Enables `serde_json`-based `compile_json` / `decompile_to_value`. Off -> pure core. |
| `dict`  | on      | Enables string interning (dict block). Strings appearing ≥2 times get `StrRef` instead of inline copies. When off, all strings are emitted inline - reduces compile memory at the cost of larger output. |

```bash
# All features (default)
cargo build

# Core only (no serde, no std, no dict)
cargo build --no-default-features

# Core + std (no JSON bridge, no dict)
cargo build --no-default-features --features std

# Without dict (all strings inline - less compile memory)
cargo build --no-default-features --features std,json
```

## Architecture

```
┌──────────────────────────────────────────────────────┐
│  Public API  (tson.rs)                               │
│  to_bytes / from_bytes / compile_json / stream …     │
├──────────────────────────────────────────────────────┤
│  Encode          Decode          Stream              │
│  (encode.rs)     (decode.rs)     (stream.rs)          │
│  13B header      13B header     TsonStreamReader      │
│  hybrid strings  sentinel+StrRef dict available        │
├──────────────────────────────────────────────────────┤
│  Type System     (structure.rs, error.rs)             │
│  TsonType, TsonData::StrRef, TsonDocument::dict      │
├──────────────────────────────────────────────────────┤
│  JSON Bridge     (compile.rs, decompile.rs)           │
│  lazy-promotion dict, inline↔StrRef resolution       │
└──────────────────────────────────────────────────────┘
```

All core modules (`structure`, `encode`, `decode`, `stream`) operate on `&[u8]` slices with zero system dependencies beyond `alloc`. The JSON bridge (`compile`, `decompile`) is feature-gated behind `#[cfg(feature = "json")]`.

## Benchmark

The project includes two benchmark tools.

### `tson-bench` - Compression Summary

Scans `examples/` for `.json` files, compiles each to TSON, reports compression ratios with dict size and leaf entry counts.

```bash
cargo run --release --bin tson-bench                 # compression table
cargo run --release --bin tson-bench -- --perf        # + p50/p99 timing
```

```
╔══════════════════════╤══════════╤══════════╤══════════╤══════════╤══════════╤═════════╗
║ File                 │ JSON (B) │ TSON (B) │   Ratio  │    Defs  │    Dict  │ Entries ║
╠══════════════════════╪══════════╪══════════╪══════════╪══════════╪══════════╪═════════╣
║ telemetry.json       │    54.4K │    16.2K │    29.8% │       11 │       63 │     500 ║
║ config.json          │    27.9K │     8.4K │    30.3% │       16 │       20 │       1 ║
║ 128KB.json           │   249.2K │   104.3K │    41.9% │        8 │      601 │     788 ║
║ users-t1.json        │    890 B │    374 B │    42.0% │       10 │        1 │       3 ║
╟──────────────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼─────────╢
║ TOTAL                │   331.0K │   129.2K │    39.0% │          │          │         ║
╚══════════════════════╧══════════╧══════════╧══════════╧══════════╧══════════╧═════════╝
```

### `comp-bench` - Detailed Performance Breakdown

Measures 7 independent workloads: JSON parse, compile, encode, decode, decompile, streaming read, and full round-trip.

```bash
cargo run --release --bin comp-bench                            # users-t1.json
cargo run --release --bin comp-bench -- examples/telemetry.json
```

```
╔══════════════════════╤══════════════╤══════════════════╗
║  Operation           │    avg / iter│   % of per-op     ║
╠══════════════════════╪══════════════╪══════════════════╣
║  serde_json parse    │     3167 ns  │  13%  (baseline)   ║
║  TSON compile        │    11494 ns  │  47%               ║
║  TSON encode         │     2624 ns  │  11%               ║
║  TSON decode         │     2388 ns  │  10%               ║
║  TSON decompile      │     2933 ns  │  12%               ║
║  TSON stream (full)  │     1969 ns  │   8%   (fastest!)  ║
╟──────────────────────┼──────────────┼──────────────────╢
║  Full round-trip     │    20179 ns  │  summed            ║
╚══════════════════════╧══════════════════════════════════╝
```

### Observations
- **Compile dominates** (~47% of per-op time) - schema discovery + string interning + definition building.
- **Decode is competitive** with JSON parse (2.4µs vs 3.2µs) - cached definitions and O(1) index lookups.
- **Streaming is the fastest operation** (1.9µs) - loads defs+dict once, then yields entries without allocation.
- **Dict is empty for unique-only documents** - lazy-promotion ensures no waste. Only strings appearing ≥2 times are included.
- **70%+ savings** on large repetitive telemetry (500 sensor readings with 6 repeated field names per reading).

## Why TSON? Comparison with Other Formats

TSON occupies a unique position in the binary JSON landscape - it is neither a general-purpose serializer nor a schema-first code generator. It compiles JSON into a **self-describing, compressed binary** that is optimized for *decoding on constrained devices*.

### Size Comparison

| File | JSON | TSON | Savings |
|------|------|------|---------|
| `telemetry.json` (500 sensor readings) | 54.4 KB | 16.2 KB | **70.2%** |
| `config.json` (200 routing rules) | 27.9 KB |  8.4 KB | **69.7%** |
| `128KB.json` (mixed documents) | 249.2 KB | 104.3 KB | **58.1%** |
| `iot-t2.json` |  1.3 KB | 0.6 KB | 49.1% |
| `users-t1.json` | 890 B | 374 B | 58.0% |

For repetitive structured data, TSON achieves **60-70% compression** by deduplicating field names and interned strings. The larger and more repetitive the input, the better the ratio.

### Format Comparison

| Feature | TSON | MessagePack | CBOR | serde\_json | Protobuf | FlatBuffers |
|---------|------|-------------|------|-------------|----------|-------------|
| **Self-describing** | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ |
| **Schema discovery** | ✅ auto | ❌ | ❌ | ❌ | ❌ hardcoded | ❌ |
| **String interning** | ✅ per-doc | ❌ | ❌ | ❌ | ❌ | ❌ |
| **Field-name dedup** | ✅ auto | ❌ repeats keys | ❌ | ❌ | ❌ | ❌ |
| **Streaming decode** | ✅ O(1) mem | ❌ | ❌ | ❌ | ❌ | ✅ |
| **no\_std + alloc** | ✅ | ❌ std | ❌ std | ❌ std | ❌ | ❌ |
| **Zero-copy strings** | ✅ StrRef | ❌ | ❌ | ❌ | ❌ | ✅ |
| **Security caps** | ✅ built-in | ❌ | ❌ | ❌ | ❌ | ❌ |
| **Hybrid str lengths** | ✅ 1/2/4 B | ❌ | ❌ | ❌ | ❌ | ❌ |
| **Human-readable** | ❌ binary | ❌ binary | ❌ binary | ✅ text | ❌ | ❌ |

### When to Use Each Format

| Scenario | Best Choice | Why |
|----------|-------------|-----|
| Browser ↔ server REST API | **JSON** | Native support everywhere |
| General-purpose binary packing | **MessagePack** | Good libraries, no schema needed |
| IoT with constrained nodes | **CBOR** | RFC standard, concise encoding |
| High-performance RPC | **Protobuf** | Schema-first, fast, compact |
| Microcontroller receiving structured telemetry | **TSON** | No schema file, streaming, zero-copy strings |
| Embedded device with limited RAM | **TSON** | `no_std` + alloc, O(1) per-entry memory |
| Config files needing human readability | **JSON** | Text is still the universal interface |

### Key Insight

**TSON trades compile time for decode efficiency.** The compiler does the heavy lifting - discovering schemas, interning strings, building definitions - so that the decoder on a microcontroller can process data without allocating field names and strings. For a server compiling millions of telemetry packets, the compile cost is amortized. For the microcontroller decoding thousands of entries, the memory savings and allocation-free path are transformative.

## Security

TSON prioritizes safe decoding of untrusted input. The reference implementation includes:

- **Bounds-checked reads**: every byte access is guarded, no panics on malformed input.
- **OOM caps**: entry count (1M max), definition count (2048 max), fields per object (256 max).
- **Recursion guard**: nesting depth limited to 128 - prevents stack overflow from circular definitions.
- **UTF-8 validation**: all string data is validated; invalid sequences are rejected.
- **Header validation**: offsets checked for consistency before use (def ≥ 13, dict ≥ def, data ≥ dict).

See the [Security Considerations](TSON-FORMAT.md#10-security-considerations) section in TSON-FORMAT.md for full details.

## Full Format Specification

See [TSON-FORMAT.md](TSON-FORMAT.md) for the complete binary wire protocol with byte-level examples and BNF grammar.

## License

MIT
