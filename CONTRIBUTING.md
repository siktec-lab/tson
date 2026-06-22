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

### Prerequisites

- **Rust** (stable, via [rustup](https://rustup.rs)). On Windows use the
  **MSVC** toolchain (`stable-x86_64-pc-windows-msvc`) — it is the only Windows
  target napi-rs supports, and the bindings will not build under the GNU
  toolchain.
- **Windows only**: the MSVC toolchain needs the **Visual Studio Build Tools**
  with the *Desktop development with C++* workload **and** the **Windows SDK**
  (the SDK is a separate component — without it `link.exe` cannot link). Install
  with:
  ```powershell
  winget install --id Microsoft.VisualStudio.2022.BuildTools --override `
    "--quiet --add Microsoft.VisualStudio.Workload.VCTools `
     --add Microsoft.VisualStudio.Component.Windows11SDK.22621 --includeRecommended"
  ```
- **Python bindings**: `pip install maturin pytest`.
- **Node.js bindings**: Node 18+; the napi-rs CLI is pulled in via
  `cd js && npm install`.

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

## Releasing & Publishing

Package names: **crates.io `tson`**, **PyPI `tson-bin`** (imports as `tson`),
**npm `@siktec-lab/tson`** (`tson` was taken on PyPI and npm).

Releases are automated by
[`.github/workflows/release.yml`](.github/workflows/release.yml) on any `v*` tag.
crates.io and PyPI publish via **Trusted Publishing (OIDC)** — no stored tokens.
npm is deferred (see below).

### One-time setup (Trusted Publishing — no secrets)

- **PyPI** can be configured *before* the project exists. At
  <https://pypi.org/manage/account/publishing/> add a **pending** GitHub
  publisher → PyPI project `tson-bin`, owner `siktec-lab`, repo `tson`,
  workflow `release.yml`. The first tagged release then publishes via OIDC.
- **crates.io** requires the crate to exist first. Do **one manual publish**:
  ```bash
  cargo publish   # uses your local `cargo login` token, once
  ```
  Then at <https://crates.io/crates/tson/settings> add the GitHub trusted
  publisher (repo `siktec-lab/tson`, workflow `release.yml`). Every later tag
  publishes via OIDC.

### Cutting a release

```bash
./scripts/bump-version.sh 0.2.0     # bumps Cargo.toml, pyproject.toml, js/package.json + npm/*
git add -A && git commit -m "Release v0.2.0"
git tag v0.2.0 && git push --follow-tags
```

The tag triggers the Release workflow:
- **crates.io** — OIDC auth via `rust-lang/crates-io-auth-action`, then
  `cargo publish` (the `exclude` list in `Cargo.toml` keeps the crate to the
  library + CLI + README).
- **PyPI** — `maturin` builds a wheel per OS/arch (matrix), then
  `pypa/gh-action-pypi-publish` uploads via OIDC.

### npm

npm publishes the main package `@siktec-lab/tson` plus 5 per-platform packages
`@siktec-lab/tson-<platform>` (selected at install time via
`optionalDependencies`). Like crates.io and PyPI it publishes **automatically on
`v*` tags** via **Trusted Publishing (OIDC)** — no `NPM_TOKEN`. (You can also
trigger it manually: `gh workflow run release.yml -f publish_npm=true`.) The npm
jobs are independent of the crate/PyPI jobs, so an npm failure never blocks them.

**One-time OIDC setup** — npm requires each package to *exist first*, so the very
first release was bootstrapped with a short-lived token. After that, set this
workflow as the **trusted publisher** for each of the 6 packages at
`https://www.npmjs.com/package/<pkg>/access` (Repository `siktec-lab/tson`,
workflow `release.yml`). The publish job has `id-token: write` and upgrades the
npm CLI to ≥ 11.5.1 (OIDC requirement); provenance is attached automatically.

### Nuances worth knowing

- **napi-rs is v3** and only supports the **MSVC** target on Windows — the GNU
  toolchain triggers a `libnode.dll` requirement and is unsupported. The
  bindings build cleanly under plain `cargo` on MSVC and Linux; the Node CI job
  drives the build through the napi CLI (`npm run build` in `js/`).
- **`cargo publish --dry-run`** is the fastest way to validate crate metadata.
- A manual `make build-node` / `cd js && npm run build` locally on Windows may
  need `--target x86_64-pc-windows-msvc` if the GNU target is also installed —
  the napi CLI can otherwise misdetect the host as GNU.
