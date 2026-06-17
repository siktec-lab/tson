## TSON Build System
##
## Usage:
##   make help           Show this help
##   make check          Cargo check (all features)
##   make build          Release build
##   make test           Run all tests (Rust + Python + Node)
##   make test-rust      Cargo test
##   make test-python    Build Python wheel + pytest
##   make test-node      Build Node addon + node tests
##   make bench          Run benchmarks
##   make bench-size     Compression benchmark
##   make bench-perf     Performance benchmark
##   make clean          Cargo clean

.DEFAULT_GOAL := help

# Rust targets

check:  ## Cargo check (all)
	cargo check

build:  ## Release build
	cargo build --release

test-rust:  ## Run Rust tests
	cargo test

bench-size:  ## Compression benchmark
	cargo run --release --bin tson-bench

bench-perf:  ## Performance benchmark
	cargo run --release --bin comp-bench -- examples/telemetry.json

bench: bench-size bench-perf  ## All benchmarks

clean:  ## Cargo clean
	cargo clean

# Python targets

python-build:  ## Build Python wheel
	@echo "==> Building Python wheel..."
	@pip install maturin >/dev/null 2>&1 || true
	@cargo check --features python || { echo "FAIL: cargo check"; exit 1; }
	@maturin develop 2>/dev/null && echo "   ok" || { echo "FAIL: maturin develop"; echo "   Install: pip install maturin"; exit 1; }

test-python: python-build  ## Build + run Python tests
	@echo "==> Running Python tests..."
	@pytest python/tests/ -v 2>/dev/null || { echo "FAIL: pytest"; echo "   Install: pip install pytest"; }

# Node.js targets

node-build:  ## Build Node.js addon
	@echo "==> Building Node.js addon..."
	@cargo check --features nodejs,json || { echo "FAIL: cargo check"; exit 1; }
	@cd js && npx napi build --platform --release 2>/dev/null && echo "   ok" || { echo "FAIL: napi build"; echo "   Install: npm install @napi-rs/cli"; exit 1; }

test-node: node-build  ## Build + run Node tests
	@echo "==> Running Node.js tests..."
	@node js/test.js

# Combined targets

test: test-rust test-python test-node  ## Run all tests

all: build python-build node-build  ## Build everything

# Help

help:  ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		sort | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2}'
