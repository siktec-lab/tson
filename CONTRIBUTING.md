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

Verify every build configuration compiles (the core is `no_std`):

```bash
make features                        # no_std / std-only / all-features checks
# equivalently:
cargo check --no-default-features    # no_std (alloc only)
cargo check --no-default-features --features std   # std, no json/dict
cargo check --all-features           # incl. python + nodejs bindings
```

## Benchmarks

```bash
cargo bench                          # Criterion harness (benches/core.rs)
make bench                           # compression + comp-bench summary tables
```

Run `cargo bench` before and after performance-sensitive changes to catch
regressions — Criterion compares against the previous run automatically.

## Code Style

- Rust: standard `rustfmt` — run `cargo fmt` before committing. CI enforces
  this with `cargo fmt --check`, which fails on any unformatted code.
- Python: standard `ruff` or `black` (run `ruff python/`)
- No warnings: CI runs `cargo clippy -- -D warnings`, so any warning fails the
  build. Keep `cargo build` / `cargo clippy` output clean.

## Pull Request Process

1. Fork the repo and create your branch from `main`
2. Add tests for any new functionality
3. Run **`make pre-push`** — it runs every gate CI enforces (`fmt --check`,
   `clippy -D warnings`, the no_std/std/all-features checks, and the Rust
   tests). A green result here means the Rust and feature-gate CI jobs pass.
4. Run `cargo bench` to confirm no performance regression
5. Update docs if you add or change public APIs
6. Open a PR with a clear description

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
