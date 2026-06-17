# TSON — Future Bindings & Transport-Layer Guidelines

This document outlines recommended APIs and ergonomics for building TSON
libraries in languages other than Rust (Python, Node.js), and for using
TSON as a wire format in message-passing systems.

---

## 1. Python Bindings

The Python library should expose a minimal, pythonic API that mirrors the
Rust core:

```python
import tson

# ── Compile: JSON string → bytes ───────────────────────
data = tson.dumps('{"temp": 22.5, "sensor": "outdoor"}')
# → b'\x01\x0d\x00\x00\x00...'

# ── Decompile: bytes → dict/list ───────────────────────
obj = tson.loads(data)
# → {'temp': 22.5, 'sensor': 'outdoor'}

# ── File-based ─────────────────────────────────────────
tson.dump({'msg': 'hello'}, 'data.tson')
msg = tson.load('data.tson')

# ── Streaming reader ───────────────────────────────────
with tson.open('sensor-stream.tson') as reader:
    print(f"Definitions: {len(reader.definitions)}")
    print(f"Dict entries: {len(reader.dict)}")
    for entry in reader:
        print(entry.data)  # TsonData value
```

### Implementation Strategy

- Use **PyO3** + **maturin** to wrap the Rust crate directly.
- Expose `TsonDocument`, `TsonStreamReader`, `TsonData` as Python classes.
- `TsonData` maps naturally to Python types:

  | TsonData variant | Python type |
  |------------------|-------------|
  | `Null` | `None` |
  | `Bool(b)` | `True` / `False` |
  | `Int(i)` | `int` |
  | `UInt(u)` | `int` |
  | `Float(f)` | `float` |
  | `String(s)` | `str` |
  | `StrRef(idx)` | Resolved to `str` via dict |
  | `Array(_, _, items)` | `list` |
  | `Object(_, fields)` | `dict` (field names from definition) |

- `TsonStreamReader` maps to an `Iterator` in Python (`__iter__` / `__next__`).
- The `dict` feature is always on for Python (no `cfg` gating — Python users
  always get the compression benefit).

### Example: `pyproject.toml`

```toml
[build-system]
requires = ["maturin>=1.0"]
build-backend = "maturin"

[project]
name = "tson"
dependencies = []
```

### Example: `src/lib.rs` (PyO3)

```rust
use pyo3::prelude::*;

#[pyclass]
struct TsonStreamReader { … }

#[pymethods]
impl TsonStreamReader {
    #[getter]
    fn definitions(&self) -> … { … }
    fn __iter__(slf: PyRef<Self>) -> … { … }
    fn __next__(slf: PyRefMut<Self>) -> … { … }
}

#[pymodule]
fn tson(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(dumps, m)?)?;
    m.add_function(wrap_pyfunction!(loads, m)?)?;
    m.add_class::<TsonStreamReader>()?;
    Ok(())
}
```

---

## 2. Node.js / TypeScript Bindings

The Node library should be built with **napi-rs**, wrapping the Rust core
directly.

```typescript
import { compile, decompile, StreamReader } from 'tson';

// ── Compile: JSON string → Buffer ─────────────────────
const buf: Buffer = compile(JSON.stringify({ temp: 22.5 }));
fs.writeFileSync('data.tson', buf);

// ── Decompile: Buffer → object ────────────────────────
const raw = fs.readFileSync('data.tson');
const obj: unknown = decompile(raw);
console.log(obj); // { temp: 22.5 }

// ── Streaming reader ──────────────────────────────────
const reader = new StreamReader(raw);
console.log(`Defs: ${reader.definitions.length}`);
console.log(`Dict: ${reader.dict.length}`);
for (const entry of reader) {
    console.log(entry.data.typeTag, entry.data);
}
```

### Implementation Strategy

- Use **napi-rs** (`@napi-rs/cli`) to generate bindings from Rust.
- Expose `StreamReader` as an async iterable (or sync iterable, since parsing
  is synchronous).
- `TsonData` variants map to native JS types:

  | TsonData | JavaScript |
  |----------|------------|
  | `Null` | `null` |
  | `Bool` | `boolean` |
  | `Int` / `UInt` | `number` |
  | `Float` | `number` |
  | `String` / `StrRef` | `string` |
  | `Array` | `Array` |
  | `Object` | `object` (field names resolved from defs) |

---

## 3. Transport-Layer Ergonomics

TSON is designed as a **message-level** format — each TSON binary blob is
self-contained and independent. This makes it suitable for:

- **MQTT** — payload on a topic
- **WebSocket** — each frame is a TSON-encoded message
- **HTTP** — `Content-Type: application/tson`
- **UDP** — each datagram is one message
- **Log files** — one TSON document per line
- **BLOB storage** — S3 object, Redis value, SQLite BLOB

### Desired API Properties

When integrating into a transport layer, the API should satisfy:

1. **Stateless** — `decode(bytes)` produces a complete document. No setup, no
   pre-registration of schemas, no handshake. Drop a TSON binary into any
   system that understands the format and it works.

2. **Streamable** — the `StreamReader` API can be used to process entries
   as they arrive from a TCP stream. No need to buffer the entire message
   before starting.

3. **Zero-copy where possible** — `StrRef` values reference the dict block
   without allocation. In a long-lived receiver, the dict can be held once
   and all subsequent entries reference it.

4. **Compact** — a small TSON binary (374 B for users-t1.json vs 890 B JSON)
   reduces framing overhead for protocols with small MTU.

5. **Type-safe when needed** — the definitions block serves as a runtime
   schema. A receiver can introspect definitions before processing entries
   and validate structure without an out-of-band `.proto` file.

### Example: MQTT Sensor Gateway

```python
import paho.mqtt.client as mqtt
import tson

def on_message(client, userdata, msg):
    # Each MQTT payload is a complete TSON document
    doc = tson.loads(msg.payload)
    readings = doc['readings']
    for r in readings:
        if r['temp'] > 30.0:
            alert(r)

client = mqtt.Client()
client.on_message = on_message
client.subscribe("sensors/+/data")
client.loop_forever()
```

### Example: WebSocket Client-Server

```javascript
// Server-side: compile JSON → TSON before sending
ws.on('message', (jsonStr) => {
    const tsonBuf = tson.compile(jsonStr);
    broadcast(tsonBuf); // 40-70% less bandwidth than JSON
});

// Client-side: decompile TSON → object
ws.onmessage = (event) => {
    const data = tson.decompile(Buffer.from(event.data));
    updateDashboard(data);
};
```

### Architecture Recommendation

For systems that use TSON as a transport format, the recommended pattern is:

```
┌─────────┐   JSON    ┌──────────┐   TSON    ┌─────────────┐
│  Client  │ ───────→ │  Gateway  │ ───────→ │  Controller  │
│  (browser│          │  (compile)│          │  (decompile) │
│   or app)│          └──────────┘          └─────────────┘
└─────────┘
```

The gateway compiles incoming JSON to TSON before forwarding to the
controller. The controller decodes TSON with zero allocation for strings
(interned) and streams entries one-at-a-time. JSON is the human-readable
boundary; TSON is the machine-efficient transport.

---

## 4. Future: Schema-Gated TSON

A planned extension: allow the producer to specify a definition index in a
header or negotiation step, then **omit** the definition block from the
wire entirely. The receiver pre-loads the definitions from a shared
schema file. This gives Protobuf-level compactness with JSON-level
ergonomics.

**API sketch**:

```python
# Producer
tson.dumps(data, schema_id=3)  # emits data-only, no def block

# Consumer
tson.loads(bytes, schema=preloaded_defs)  # uses preloaded defs
```

This is not yet implemented, but the wire format supports it — the
definition block is already byte-offset-addressed by the header, so
replacing it with external data is a matter of API design.
