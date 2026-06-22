# TSON for Node.js

Node.js bindings for [TSON](../README.md) — a compact binary JSON format.
Built with [napi-rs](https://napi.rs); ships a prebuilt native addon per
platform (Linux, macOS, Windows) so there's no compile step on install.

## Install

```bash
npm install @siktec-lab/tson
```

CommonJS and ESM both work:

```js
const tson = require('@siktec-lab/tson')   // CommonJS
import * as tson from '@siktec-lab/tson'   // ESM
```

TypeScript types are bundled (`index.d.ts`) — no `@types` package needed.

## API

The API mirrors `JSON.parse`/`JSON.stringify`, but produces/consumes TSON
**binary** (`Buffer`) instead of text.

```ts
function dumps(jsonText: string): Buffer        // JSON string -> TSON binary
function loads(bytes: Buffer): any              // TSON binary -> JS value
function dump(jsonText: string, path: string): void   // compile + write .tson
function load(path: string): any                // read .tson -> JS value
function emit(val: any): Buffer                 // JS value -> TSON (no JSON string)
```

> `dumps`/`dump` take a **JSON string**, not a JS object. To encode a JS object
> directly, use `emit()`.

## Examples

### Round-trip a JSON string

```js
const tson = require('@siktec-lab/tson')

const blob = tson.dumps('{"name":"Alice","age":30}')   // Buffer
console.log(blob.length, 'bytes')

const obj = tson.loads(blob)                            // { name: 'Alice', age: 30 }
```

### Compress before storing / sending over the wire

```js
const payload = JSON.stringify(myData)   // your existing JSON
const blob = tson.dumps(payload)         // ~30–40% the size
ws.send(blob)                            // send the Buffer
```

### Emit directly from a JS object

```js
const reading = { temp: 22.5, humidity: 61, status: 'nominal' }
const blob = tson.emit(reading)          // no intermediate JSON string
const obj  = tson.loads(blob)
```

Supported value types for `emit()`: object, array, string, number, boolean, `null`.

### File I/O

```js
tson.dump('{"msg":"hello"}', 'message.tson')   // write
const obj = tson.load('message.tson')          // -> { msg: 'hello' }
```

### Error handling

Invalid input throws:

```js
try {
  tson.dumps('{not valid json}')
} catch (e) {
  console.error('bad input:', e.message)
}
```

## Notes

- **Numbers**: TSON stores 32-bit ints/floats internally, so very large integers
  or high-precision doubles may be narrowed — intended for typical
  structured/telemetry data, not arbitrary-precision values.
- **Key order** is not preserved (TSON sorts fields when building the schema),
  the same as a `JSON.parse` → `JSON.stringify` cycle.
- **Platforms**: prebuilt binaries are published as
  `@siktec-lab/tson-<platform>` packages and selected automatically via
  `optionalDependencies`. Supported out of the box: linux-x64-gnu,
  linux-arm64-gnu, darwin-x64, darwin-arm64, win32-x64-msvc.

See the [main README](../README.md) and the [binary format spec](TSON-FORMAT.md)
for how TSON achieves its size savings.
