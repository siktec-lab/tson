# tson-bin — Terse JSON binary format for Python

[![PyPI](https://img.shields.io/pypi/v/tson-bin.svg?logo=pypi&logoColor=white)](https://pypi.org/project/tson-bin/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://github.com/siktec-lab/tson/blob/main/LICENSE)

**TSON** is a compact, schema-deduplicated **binary format for JSON** — field
names are stored once, repeated strings are interned, giving **60–70% size
reduction** on repetitive data (API payloads, telemetry, config). These are the
Python bindings (a native extension built in Rust via PyO3).

> The PyPI distribution is **`tson-bin`** but it imports as **`tson`**.

## Install

```bash
pip install tson-bin
```

```python
import tson
```

## Usage

```python
import tson

# Compile a JSON string to TSON binary, and back
blob = tson.dumps('{"name": "Alice", "age": 30}')   # -> bytes
obj  = tson.loads(blob)                               # -> {"name": "Alice", "age": 30}

# Encode a Python object directly (no JSON string in between)
blob = tson.emit({"temp": 22.5, "status": "nominal"})

# File I/O
tson.dump('{"msg": "hello"}', "message.tson")
obj = tson.load("message.tson")
```

### API

| Function | Signature | Description |
|----------|-----------|-------------|
| `dumps(json_text)` | `str -> bytes` | Compile a JSON string to TSON binary |
| `loads(blob)` | `bytes -> object` | Decode TSON binary to a Python object |
| `dump(json_text, path)` | `(str, str) -> None` | Compile a JSON string to a `.tson` file |
| `load(path)` | `str -> object` | Read and decode a `.tson` file |
| `emit(obj)` | `object -> bytes` | Encode a Python object directly to TSON |

Invalid input raises `ValueError`.

## Documentation

- [Python usage guide](https://github.com/siktec-lab/tson/blob/main/docs/python.md)
- [Project README](https://github.com/siktec-lab/tson#readme)
- [Binary format spec](https://github.com/siktec-lab/tson/blob/main/docs/TSON-FORMAT.md)

## License

[MIT](https://github.com/siktec-lab/tson/blob/main/LICENSE) © SIKTEC Lab
