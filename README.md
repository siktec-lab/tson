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

## Benchmark

The project includes a built-in benchmark tool (`tson-bench`) that scans `examples/` for `.json` files, compiles each to TSON, and reports compression ratios with optional p50/p99 timing.

```bash
# Compression summary
cargo run --bin tson-bench

# With p50/p99 compile latency (200 iterations)
cargo run --release --bin tson-bench -- --perf
```

### Results (release build)

```
╔══════════════════════╤══════════╤══════════╤══════════╤══════════╤═════════╗
║ File                 │ JSON (B) │ TSON (B) │   Ratio  │    Defs  │ Entries ║
╠══════════════════════╪══════════╪══════════╪══════════╪══════════╪═════════╣
║ iot-t2.json          │     1.3K │    623 B │    48.2% │       13 │       1 ║
║ users-t1.json        │    886 B │    381 B │    43.0% │       10 │       1 ║
╟──────────────────────┼──────────┼──────────┼──────────┼──────────┼─────────╢
║ TOTAL                │     2.1K │   1004 B │    46.1% │          │         ║
╚══════════════════════╧══════════╧══════════╧══════════╧══════════╧═════════╝
```

**Overall**: 46.1% of original size — **53.9% space savings**.

| File | avg | p50 | p99 |
|------|-----|-----|-----|
| `iot-t2.json` (1.3K) | 17.2µs | 16.5µs | 32.3µs |
| `users-t1.json` (886 B) | 12.9µs | 12.5µs | 23.7µs |

200 iterations each, release build. Compile latency stays under 35µs p99 for both files.

### Observations
- **57% compression** on `users-t1.json` — 3 identical user records; field names stored once instead of 9 times.
- **52% compression** on `iot-t2.json` — mixed nested objects with 6 unique shapes; definition block fits in 30 bytes.
- Compile latency is sub-20µs typical — fast enough for real-time encoding on microcontrollers.
- The benchmark auto-discovers all `.json` files in `examples/` — drop in new files to expand the comparison.

## Why TSON? Comparison with Other Formats

TSON occupies a unique position in the binary JSON landscape — it is neither a general-purpose serializer nor a schema-first code generator. It compiles JSON into a **self-describing, compressed binary** that is optimised for *decoding on constrained devices*.

### Size Comparison (our example files)

| File | JSON | TSON | Savings |
|------|------|------|---------|
| `telemetry.json` (500 sensor readings) | 54.4 KB | 16.2 KB | **70.2%** |
| `config.json` (200 routing rules) | 27.9 KB |  8.4 KB | **69.7%** |
| `iot-t2.json` |  1.3 KB | 771 B | 40.3% |
| `users-t1.json` | 886 B | 546 B | 38.4% |

For repetitive structured data, TSON achieves **60-70% compression** by deduplicating field names and storing them once in a definition block. The larger and more repetitive the input, the better the ratio.

### Format Comparison

| Feature | TSON | MessagePack | CBOR | serde\_json | Protobuf | FlatBuffers |
|---------|------|-------------|------|-------------|----------|-------------|
| **Self-describing** | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ |
| **Schema discovery** | ✅ auto | ❌ | ❌ | ❌ | ❌ hardcoded | ❌ |
| **String interning** | ✅ per-document | ❌ | ❌ | ❌ | ❌ | ❌ |
| **Field-name dedup** | ✅ auto | ❌ repeats keys | ❌ | ❌ | ❌ | ❌ |
| **Streaming decode** | ✅ O(1) mem | ❌ | ❌ | ❌ | ❌ | ✅ |
| **no\_std + alloc** | ✅ | ❌ std | ❌ std | ❌ std | ❌ | ❌ |
| **Zero-copy strings** | ✅ StrRef | ❌ | ❌ | ❌ | ❌ | ✅ |
| **Security caps** | ✅ built-in | ❌ | ❌ | ❌ | ❌ | ❌ |
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

**TSON trades compile time for decode efficiency.** The compiler does the heavy lifting — discovering schemas, interning strings, building definitions — so that the decoder on a microcontroller can process data without allocating field names and strings. For a server compiling millions of telemetry packets, the compile cost is amortized. For the microcontroller decoding thousands of entries, the memory savings and allocation-free path are transformative.

## Security

TSON prioritizes safe decoding of untrusted input. The reference implementation includes:

- **Bounds-checked reads**: every byte access is guarded, no panics on malformed input.
- **OOM caps**: entry count (1M max), definition count (2048 max), fields per object (256 max).
- **Recursion guard**: nesting depth limited to 128 — prevents stack overflow from circular definitions.
- **UTF-8 validation**: all string data is validated; invalid sequences are rejected.
- **Header validation**: offsets checked for consistency before use.

See the [Security Considerations](TSON-FORMAT.md#10-security-considerations) section in TSON-FORMAT.md for full details.

## Full Format Specification

See [TSON-FORMAT.md](TSON-FORMAT.md) for the complete binary wire protocol.

## License

MIT
