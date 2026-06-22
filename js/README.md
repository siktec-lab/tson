# @siktec-lab/tson — Terse JSON binary format for Node.js

[![npm](https://img.shields.io/npm/v/@siktec-lab/tson.svg?logo=npm)](https://www.npmjs.com/package/@siktec-lab/tson)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://github.com/siktec-lab/tson/blob/main/LICENSE)

**TSON** is a compact, schema-deduplicated **binary format for JSON** — field
names are stored once, repeated strings are interned, giving **60–70% size
reduction** on repetitive data (API payloads, telemetry, config). These are the
Node.js bindings: a prebuilt native addon (built in Rust via napi-rs), so
there's no compile step on install. TypeScript types are bundled.

## Install

```bash
npm install @siktec-lab/tson
```

## Usage

```js
const tson = require('@siktec-lab/tson')   // or: import * as tson from '@siktec-lab/tson'

// Compile a JSON string to TSON binary, and back
const blob = tson.dumps('{"name":"Alice","age":30}')   // -> Buffer
const obj  = tson.loads(blob)                            // -> { name: 'Alice', age: 30 }

// Encode a JS object directly (no JSON string in between)
const b = tson.emit({ temp: 22.5, status: 'nominal' })

// File I/O
tson.dump('{"msg":"hello"}', 'message.tson')
const loaded = tson.load('message.tson')
```

### API

```ts
function dumps(jsonText: string): Buffer        // JSON string -> TSON binary
function loads(bytes: Buffer): any              // TSON binary -> JS value
function dump(jsonText: string, path: string): void
function load(path: string): any
function emit(val: any): Buffer                 // JS value -> TSON binary
```

Invalid input throws.

## Platforms

Prebuilt binaries are published per platform and selected automatically via
`optionalDependencies`: linux-x64-gnu, linux-arm64-gnu, darwin-x64,
darwin-arm64, win32-x64-msvc.

## Documentation

- [Node.js usage guide](https://github.com/siktec-lab/tson/blob/main/docs/js.md)
- [Project README](https://github.com/siktec-lab/tson#readme)
- [Binary format spec](https://github.com/siktec-lab/tson/blob/main/docs/TSON-FORMAT.md)

## License

[MIT](https://github.com/siktec-lab/tson/blob/main/LICENSE) © SIKTEC Lab
