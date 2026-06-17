# Contributing to TSON

Thanks for your interest in contributing! Here's how to get started.

## Setup

```bash
git clone https://github.com/siktec-lab/tson.git
cd tson
cargo build
cargo test
make help
```

## Running Tests

```bash
make test          # all tests (Rust + Python + Node)
make test-rust     # Rust only (48 tests)
make test-python   # Python (requires `pip install maturin`)
make test-node     # Node.js (requires `npm install @napi-rs/cli`)
```

## Code Style

- Rust: standard `rustfmt` (run `cargo fmt`)
- Python: standard `ruff` or `black` (run `ruff python/`)
- No warnings: `cargo check` should produce zero warnings

## Pull Request Process

1. Fork the repo and create your branch from `main`
2. Add tests for any new functionality
3. Ensure `make test` passes
4. Update docs if you add or change public APIs
5. Open a PR with a clear description

## Project Structure

```
src/
├── lib.rs          Library entry point
├── tson.rs         Public API hub
├── structure.rs    TsonType, TsonData, TsonDocument, TsonHeader
├── encode.rs       TSON → bytes
├── decode.rs       bytes → TSON
├── stream.rs       TsonStreamReader, TsonDocReader
├── compile.rs      JSON → TSON (feature gated)
├── decompile.rs    TSON → JSON (feature gated)
├── python.rs       Python bindings (PyO3)
├── nodejs.rs       Node.js bindings (napi-rs)
├── main.rs         CLI tool
└── bin/            Benchmark & generation tools
```
