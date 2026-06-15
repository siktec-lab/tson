# TSON — Terse JSON Binary Format

A compact, schema-deduplicated binary format for JSON data, built for microcontrollers and constrained environments.

**Core idea**: in repetitive JSON (API payloads, telemetry, config), field names appear thousands of times. TSON stores them **once** in a definition block. The data stream is pure typed values, no key repetition.

```
JSON (604 bytes)              TSON binary (~220 bytes)
[{                           ┌── Header (9 B)
  "id": 1,                   │   version=1, def_off=9, data_off=…
  "name": "Alice",           ├── Definition block
  "age": 30,                 │   #0 Null  #1 Bool  #2 Int  #3 UInt
  "address": {               │   #4 Float  #5 String
    "street": "123…",        │   #6 Array<String>
    "city": "Anytown",       │   #7 Object fields:
    "state": "CA",           │      street:String city:String
    "zip": "12345"           │      state:String zip:String
  },                         │   #8 Object fields:
  "hobbies": ["reading",     │      id:Int name:String age:Int
    "hiking", "cooking"]     │      address:#7 hobbies:#6
  },                         ├── Data block
  …                          │   Entry: #8 → 1, 'Alice', 30,
]                            │     #7 → '123…', 'Anytown', …
                             │     #6 → 3, 'reading', 'hiking', …
                             │   Entry: #8 → 2, 'Bob', 25, …
                             │   Entry: #8 → 3, 'Charlie', 35, …
                             └── (end)
```

## Features

- **Zero-dependency core**: encode/decode/stream on `&[u8]` slices, only needs `alloc`.
- **Streaming reader**: loads the tiny definition block into memory, then yields data entries one-at-a-time — `O(1)` memory per entry.
- **Schema deduplication**: identical object shapes share one definition. Field names stored once.
- **`no_std` capable**: disable the `std` feature for embedded targets.
- **Optional JSON bridge**: `serde_json`-based compile/decompile behind the `json` feature.
- **Self-describing wire format**: every compound value carries its definition index, enabling forward compatibility and partial decoding.

## Quick Start

```rust
// Round-trip a JSON string through TSON binary
let json = r#"{"name":"Alice","age":30}"#;

// JSON → TSON document → binary
let doc = tson::compile_json(json).unwrap();
let bytes = tson::to_bytes(&doc).unwrap();

// Binary → TSON document → JSON
let restored = tson::from_bytes(&bytes).unwrap();
let value = tson::decompile_to_value(&restored).unwrap();

assert_eq!(value.to_string(), r#"{"age":30,"name":"Alice"}"#);
```

## Command-Line Tool

```bash
# Build
cargo build --release

# Compile JSON → TSON binary
./target/release/tson-cli data.json      # writes data.tson

# Decompile TSON → pretty JSON
./target/release/tson-cli data.tson      # prints JSON to stdout

# Stream-debug (inspect header, definitions, entry types)
./target/release/tson-cli -s data.tson
```

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `std`   | on      | Enables `std::io::Read` helpers and the `IoError` variant. Off → `no_std` + `alloc`. |
| `json`  | on      | Enables `serde_json`-based `compile_json` / `decompile_to_value`. Off → pure core. |

```bash
# All features (default)
cargo build

# Core only (no serde, no std)
cargo build --no-default-features

# Core + std (no JSON bridge)
cargo build --no-default-features --features std
```

## Architecture

```
┌──────────────────────────────────────────┐
│  Public API  (tson.rs)                   │
│  to_bytes / from_bytes / compile_json …  │
├──────────────────────────────────────────┤
│  Encode        Decode        Stream      │
│  (encode.rs)   (decode.rs)   (stream.rs) │
├──────────────────────────────────────────┤
│  Type System   (structure.rs, error.rs)  │
├──────────────────────────────────────────┤
│  JSON Bridge   (compile.rs, decompile.rs)│
└──────────────────────────────────────────┘
```

All core modules (`structure`, `encode`, `decode`, `stream`) operate on `&[u8]` slices with zero system dependencies beyond `alloc`. The JSON bridge (`compile`, `decompile`) is feature-gated behind `#[cfg(feature = "json")]`.

## Full Format Specification

See [TSON-FORMAT.md](TSON-FORMAT.md) for the complete binary wire protocol.

## License

MIT
