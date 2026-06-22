# TSON — Terse JSON Binary Format

[![CI](https://github.com/siktec-lab/tson/actions/workflows/ci.yml/badge.svg)](https://github.com/siktec-lab/tson/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/tson.svg?logo=rust)](https://crates.io/crates/tson)
[![docs.rs](https://img.shields.io/docsrs/tson?logo=docs.rs)](https://docs.rs/tson)
[![PyPI](https://img.shields.io/pypi/v/tson-bin.svg?logo=pypi&logoColor=white)](https://pypi.org/project/tson-bin/)
[![npm](https://img.shields.io/npm/v/@siktec-lab/tson.svg?logo=npm)](https://www.npmjs.com/package/@siktec-lab/tson)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

A compact, schema-deduplicated **binary format for JSON**, built for
microcontrollers and constrained environments — with first-class **Rust**,
**Python**, and **Node.js** bindings.

> **Core idea:** in repetitive JSON (API payloads, telemetry, config) field
> names repeat thousands of times. TSON stores each field name **once** in a
> definition block and each repeated string **once** in a dict block. The data
> stream is then pure typed values — no key repetition, no duplicate strings —
> giving **60–70% size reduction** on real-world data.

## Install

| Language | Package | Install |
|----------|---------|---------|
| **Rust** | [`tson`](https://crates.io/crates/tson) | `cargo add tson` |
| **Python** | [`tson-bin`](https://pypi.org/project/tson-bin/) (imports as `tson`) | `pip install tson-bin` |
| **Node.js** | [`@siktec-lab/tson`](https://www.npmjs.com/package/@siktec-lab/tson) | `npm install @siktec-lab/tson` |

## Documentation

| Doc | What's inside |
|-----|---------------|
| 📘 [Rust user guide](docs/DOC.md) | compile, emit, query, stream — the full Rust API with examples |
| 🐍 [Python usage](docs/python.md) | `dumps`/`loads`/`dump`/`load`/`emit`, files, round-trips |
| 🟢 [Node.js usage](docs/js.md) | same API for JS/TS, Buffer handling, types |
| 📐 [Binary format spec](docs/TSON-FORMAT.md) | byte-level wire protocol + BNF grammar |
| 🛠️ [Real-life walkthrough](docs/REAL-LIFE.md) | an end-to-end IoT sensor pipeline |
| 🤝 [Contributing & releasing](CONTRIBUTING.md) | dev setup, CI gates, publishing |

## Quick Start (Rust)

```rust
// JSON text -> TSON binary -> back to JSON
let json = r#"{"name":"Alice","age":30}"#;

let doc      = tson::compile_json(json).unwrap();   // discover schema + intern strings
let bytes    = tson::to_bytes(&doc).unwrap();        // encode to binary
let restored = tson::from_bytes(&bytes).unwrap();    // decode
let value    = tson::decompile_to_value(&restored).unwrap();

assert_eq!(value.to_string(), r#"{"age":30,"name":"Alice"}"#);
```

Python and Node mirror the familiar `dumps`/`loads` shape:

```python
import tson
blob = tson.dumps('{"name":"Alice","age":30}')   # -> bytes
obj  = tson.loads(blob)                            # -> dict
```

```js
const tson = require('@siktec-lab/tson')
const blob = tson.dumps('{"name":"Alice","age":30}')  // -> Buffer
const obj  = tson.loads(blob)                          // -> object
```

See the [Rust](docs/DOC.md), [Python](docs/python.md), and [Node.js](docs/js.md)
guides for the full API (emit mode, streaming, direct field access, etc.).

## Why TSON?

```
JSON (890 bytes)               TSON binary (~374 bytes, 42%)
[{                              ┌── Header (13 B)
  "id": 1,                      ├── Definition block — every field name once
  "name": "Alice",             │   #7 {street,city,state,zip}
  "address": { … },            │   #8 {id,name,age,address,hobbies}
  "hobbies": [ … ]             ├── Dict block — repeated strings once
}, …]                          └── Data block — pure typed values
```

- **Zero-dependency core** — encode/decode/stream on `&[u8]`, only needs `alloc`.
- **`no_std` capable** — runs on microcontrollers; `O(1)` memory per entry with
  the streaming reader.
- **Schema + string dedup** — identical object shapes share one definition;
  strings seen ≥2× are interned (`StrRef`).
- **Self-describing** — every value carries its definition index; no external
  schema file, supports partial/streaming decode.
- **Safe by default** — bounds-checked reads, OOM caps, recursion guard, UTF-8
  validation (see [Security](#security)).

### Size comparison

| File | JSON | TSON | Savings |
|------|------|------|---------|
| `telemetry.json` (500 sensor readings) | 54.4 KB | 16.2 KB | **70.2%** |
| `config.json` (200 routing rules) | 27.9 KB | 8.4 KB | **69.7%** |
| `128KB.json` (mixed documents) | 249.2 KB | 104.3 KB | **58.1%** |

### vs. other binary formats

| | TSON | MessagePack | CBOR | Protobuf | FlatBuffers |
|--|------|-------------|------|----------|-------------|
| Self-describing | ✅ | ✅ | ✅ | ❌ | ❌ |
| Auto schema discovery | ✅ | ❌ | ❌ | ❌ | ❌ |
| Field-name dedup | ✅ | ❌ | ❌ | ❌ | ❌ |
| String interning | ✅ | ❌ | ❌ | ❌ | ❌ |
| Streaming decode, O(1) mem | ✅ | ❌ | ❌ | ❌ | ✅ |
| `no_std` + alloc | ✅ | ❌ | ❌ | ❌ | ❌ |

**TSON trades compile time for decode efficiency** — the compiler discovers
schemas and interns strings so a constrained decoder can read values without
allocating field names. Ideal when a server compiles telemetry once and many
small devices decode it.

## Command-line tool

```bash
cargo build --release
./target/release/tson-cli data.json     # JSON  -> data.tson
./target/release/tson-cli data.tson     # TSON  -> pretty JSON on stdout
./target/release/tson-cli -s data.tson  # inspect header / defs / dict / entries
```

## Feature flags

| Feature | Default | Description |
|---------|---------|-------------|
| `std`   | on | `std::io::Read` helpers + `IoError`. Off → `no_std` + `alloc`. |
| `json`  | on | `serde_json`-based `compile_json` / `decompile_to_value`. |
| `dict`  | on | String interning (dict block). Off → all strings inline. |

```bash
cargo build                               # default: std + json + dict
cargo build --no-default-features         # no_std core (alloc only)
cargo build --no-default-features --features std,json
```

## Performance

Round-trip ≈ **12 µs** for a small doc, ~0.7 ms for 54 KB telemetry (release
build). Encode is the cheapest stage (~0.45 µs) — values are appended straight
into one shared buffer with no per-node allocation. Compile dominates (~46%).
Reproduce with `cargo bench` (Criterion) or `make bench` (human-readable
tables). Full breakdown in the [Rust guide](docs/DOC.md#8-performance-summary).

## Security

TSON is built to decode **untrusted** input safely: bounds-checked reads (no
panics on malformed data), OOM caps (entries ≤ 1M, defs ≤ 2048, fields ≤ 256),
a recursion-depth guard (≤ 128), UTF-8 validation, and header-offset
consistency checks. Details in
[the format spec](docs/TSON-FORMAT.md#10-security-considerations).

## Development

```bash
make help          # list all targets
make test          # Rust + Python + Node test suites
make bench         # compression + performance tables
make pre-push      # every CI gate locally (fmt, clippy, features, tests)
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for dev setup, the CI gates, and how
releases publish to crates.io / PyPI / npm.

## License

[MIT](LICENSE) © SIKTEC Lab

