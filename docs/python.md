# TSON for Python

Python bindings for [TSON](../README.md) — a compact binary JSON format.
Built with [PyO3](https://pyo3.rs) + [maturin](https://www.maturin.rs), shipped
as a native extension.

## Install

```bash
pip install tson-bin
```

> The PyPI **distribution** is named `tson-bin` (the name `tson` was taken), but
> the **import** is plain `tson`:
>
> ```python
> import tson
> ```

## API

The API mirrors the stdlib `json` module's `dumps`/`loads`/`dump`/`load`, but
produces/consumes TSON **binary** instead of text.

| Function | Signature | Description |
|----------|-----------|-------------|
| `dumps(json_text)` | `str -> bytes` | Compile a **JSON string** to TSON binary |
| `loads(blob)` | `bytes -> object` | Decode TSON binary to a Python object |
| `dump(json_text, path)` | `(str, str) -> None` | Compile a JSON string and write `.tson` to a file |
| `load(path)` | `str -> object` | Read a `.tson` file and decode it |
| `emit(obj)` | `object -> bytes` | Encode a Python object directly to TSON (no JSON string) |

> Note: `dumps`/`dump` take a **JSON string**, not a Python object. To go
> straight from a Python `dict`/`list`, use `emit()`.

## Examples

### Round-trip a JSON string

```python
import tson

blob = tson.dumps('{"name": "Alice", "age": 30}')   # -> bytes (TSON binary)
print(len(blob), "bytes")

obj = tson.loads(blob)                                # -> dict
assert obj == {"name": "Alice", "age": 30}
```

### Compress a JSON payload before storing/sending

```python
import json, tson

payload = json.dumps(my_data)         # your existing JSON text
blob = tson.dumps(payload)            # compact binary, typically 30–40% the size
socket.send(blob)
```

### Emit directly from a Python object

```python
import tson

reading = {"temp": 22.5, "humidity": 61, "status": "nominal"}
blob = tson.emit(reading)             # no intermediate JSON string
obj  = tson.loads(blob)
```

Supported value types for `emit()`: `dict`, `list`, `str`, `int`, `float`,
`bool`, `None`.

### File I/O

```python
import tson

tson.dump('{"msg": "hello"}', "message.tson")   # write
obj = tson.load("message.tson")                  # read back -> {"msg": "hello"}
```

### Error handling

Invalid input raises `ValueError`:

```python
import tson

try:
    tson.dumps("{not valid json}")
except ValueError as e:
    print("bad input:", e)
```

## Notes

- **Numbers**: JSON integers decode to Python `int`, floats to `float`. Internally
  TSON stores 32-bit ints/floats, so very large integers or high-precision
  doubles may be narrowed — use it for typical structured/telemetry data.
- **Round-trip semantics** match JSON: object key order is not preserved
  (TSON sorts fields when building the schema), exactly like a JSON
  parse→serialize cycle.
- The native wheel is prebuilt for common platforms; on others, `pip` builds it
  from source (needs a Rust toolchain).

See the [main README](../README.md) and the [binary format spec](TSON-FORMAT.md)
for how TSON achieves its size savings.
