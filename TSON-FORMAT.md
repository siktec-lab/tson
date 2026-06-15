# TSON Binary Format Specification

> Version 1.0 — stable

## Overview

TSON (Terse JSON) is a compact binary serialization format for structured data. It separates **structure** (field names, types) from **values**, storing the structure once in a definition block and referencing it from the data block. This yields dramatic compaction for repetitive JSON payloads.

The format is designed for microcontrollers and constrained environments: the definition block is small enough to keep in RAM, and the data block can be streamed entry-by-entry with `O(1)` additional memory per entry.

## Document Layout

```
┌──────────────────────────────────────────────────────┐
│  HEADER (9 bytes, fixed)                             │
│  [version:u8] [def_off:u32 LE] [data_off:u32 LE]     │
├──────────────────────────────────────────────────────┤
│  DEFINITION BLOCK                                    │
│  [def_count:u16 LE]                                  │
│  For each definition:                                │
│    [type:u8] [index:u16 LE]                          │
│    If Object: [field_count:u16 LE]                   │
│      [name_len:u8] [name:UTF-8] [field_type:u8] × N  │
│    If Array:  [elem_type:u8]                         │
│    (Primitives carry no extra data)                  │
├──────────────────────────────────────────────────────┤
│  DATA BLOCK                                          │
│  [entry_count:u32 LE]                                │
│  For each entry:                                     │
│    [def_index:u16 LE] [payload_len:u32 LE]           │
│    Payload (per type — see §Payload Formats)         │
└──────────────────────────────────────────────────────┘
```

All multi-byte integers are little-endian (`LE`).

---

## 1. Header

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0      | 1    | `version` | Format version. Currently `1`. |
| 1      | 4    | `def_off` | Byte offset to the definition block. Always ≥ 9. |
| 5      | 4    | `data_off`| Byte offset to the data block. Always > `def_off`. |

The header is exactly 9 bytes. The byte range `[def_off .. data_off)` is the definition block. Bytes from `data_off` to end-of-file are the data block.

---

## 2. Type Tags

Each type has a unique `u8` tag used in definitions and payloads.

| Tag  | Name   | Wire size | Description |
|------|--------|-----------|-------------|
| 0x00 | Null   | 0 bytes   | Null value |
| 0x01 | Bool   | 1 byte    | `0x00` = false, any non-zero = true |
| 0x02 | Int    | 4 bytes   | Signed 32-bit integer, little-endian |
| 0x03 | UInt   | 4 bytes   | Unsigned 32-bit integer, little-endian |
| 0x04 | Float  | 4 bytes   | IEEE 754 single-precision float, little-endian |
| 0x05 | String | variable  | Length-prefixed UTF-8 string (see below) |
| 0x10 | Array  | variable  | Ordered list of same-type elements |
| 0x11 | Object | variable  | Set of key-value pairs whose schema is in a definition |

Tags 0x00–0x05 are **primitive** types. Tags 0x10–0x11 are **compound** types.

---

## 3. Definition Block

### 3.1 Block Header

```
[def_count: u16 LE]
```

Number of definitions that follow. Maximum 65535.

### 3.2 Definition Entry

Every definition begins with:

```
[type: u8] [index: u16 LE]
```

`index` is the zero-based position of this definition in the definition table. Indices 0–5 are reserved for the six primitive types (they are always present in the table). User-defined compound types start at index 6.

### 3.3 Object Definition

Followed by:

```
[field_count: u16 LE]
```

Then for each field:

```
[name_len: u8] [name: UTF-8 bytes] [field_type: u8]
```

Fields are stored in **canonical order** (alphabetically by field name) so that the same logical schema produces identical byte representations for deduplication.

**Example**: Definition #6 for an object with fields `id: Int` and `name: String`:

```
type = 0x11     (Object)
index = 0x0006  (u16 LE)
field_count = 2 (u16 LE)

  Field 0:
    name_len = 2
    name     = "id"
    type     = 0x02 (Int)

  Field 1:
    name_len = 4
    name     = "name"
    type     = 0x05 (String)
```

### 3.4 Array Definition

Followed by:

```
[elem_type: u8]
```

**Example**: Definition #7 for an array of strings:

```
type = 0x10     (Array)
index = 0x0007  (u16 LE)
elem_type = 0x05 (String)
```

### 3.5 Primitive Definitions (Indices 0–5)

The first six definitions are always present:

| Index | Type   |
|-------|--------|
| 0     | Null   |
| 1     | Bool   |
| 2     | Int    |
| 3     | UInt   |
| 4     | Float  |
| 5     | String |

They have the standard `[type:u8][index:u16]` header and no extra data (no fields, no `elem_type`).

---

## 4. Data Block

### 4.1 Block Header

```
[entry_count: u32 LE]
```

Number of data entries that follow. Maximum 4,294,967,295.

### 4.2 Data Entry

Each entry has a 6-byte header:

```
[def_index: u16 LE] [payload_len: u32 LE]
```

`def_index` tells us which definition describes the entry's structure. `payload_len` is the byte length of the **payload** that follows — it enables forward-compatible skipping of unknown types.

The payload format depends on the definition's type tag:

### 4.3 Payload Format by Type

#### Null
No payload bytes. `payload_len` is 0.

#### Bool
```
[value: u8]
```
1 byte. `0x00` = false, any non-zero = true. `payload_len` is 1.

#### Int
```
[value: i32 LE]
```
4 bytes. Signed 32-bit little-endian. `payload_len` is 4.

#### UInt
```
[value: u32 LE]
```
4 bytes. Unsigned 32-bit little-endian. `payload_len` is 4.

#### Float
```
[value: f32 LE]
```
4 bytes. IEEE-754 binary32 little-endian. `payload_len` is 4.

> **Precision warning**: binary32 has ~7 decimal digits of precision. Round-tripping a JSON float through TSON may introduce small representation errors (e.g., `3.14` → `3.1400001`). Use exactly-representable values (powers of two) when exactness matters.

#### String
```
[str_len: u16 LE] [data: UTF-8 bytes]
```
Length prefix (2 bytes) followed by the raw UTF-8 string. Maximum string length is 65535 bytes. `payload_len` = 2 + `str_len`.

#### Object
```
[self_def_index: u16 LE] [field_value] [field_value] ...
```

`self_def_index` (2 bytes) repeats the definition index — it makes every compound value self-describing, enabling uniform encoding/decoding even for nested values.

Field values are concatenated in the same order as the definition's field list. Each field value is encoded per its type (see §5 for compound-valued fields).

**Example**: Object #6 (id: Int, name: String) with values `id=1, name="Alice"`:

```
self_def_index = 0x0006    (u16 LE)
-------------------------------
Field 0 (id, Int):
  [0x01 0x00 0x00 0x00]    (i32 LE = 1)
Field 1 (name, String):
  [0x05 0x00]               (str_len = 5, u16 LE)
  [41 6C 69 63 65]          ("Alice")
-------------------------------
payload_len = 2 + 4 + 7 = 13
```

#### Array
```
[self_def_index: u16 LE] [elem_def_index: u16 LE] [elem_count: u16 LE] [element] [element] ...
```

`self_def_index` (2 bytes) identifies which Array definition to use. `elem_def_index` (2 bytes) identifies the element type's definition. `elem_count` (2 bytes) is the number of elements.

Elements are concatenated, each encoded per the element type (see §5 for compound-valued elements).

**Example**: Array #7 of String, with elements `["read", "ski"]`:

```
self_def_index = 0x0007  (u16 LE)
elem_def_index = 0x0005  (u16 LE, String = 5)
elem_count     = 2       (u16 LE)
-------------------------------
Element 0 (String):
  [0x04 0x00]              (str_len = 4)
  [72 65 61 64]           ("read")
Element 1 (String):
  [0x03 0x00]              (str_len = 3)
  [73 6B 69]              ("ski")
-------------------------------
payload_len = 2 + 2 + 2 + 6 + 5 = 17
```

---

## 5. Nested Compound Values

When a field type or element type is itself a compound type (Object or Array), the nested value is encoded with its own `self_def_index` as the first 2 bytes:

```
[field_value begins with self_def_index: u16 LE]
```

This makes every compound value self-describing regardless of depth.

**Example**: Object #8 with field `address` of type Object (#6). The `address` field value begins:

```
[self_def_index = 0x0006] ... (rest of Object #6 payload)
```

The outer object (definition #8) has its field `address` defined as type `Object`. When the decoder encounters this field, it:

1. Sees "type is Object"
2. Enters the compound-value decoder
3. Reads `self_def_index` (0x0006)
4. Looks up definition #6 for field names and types
5. Decodes the remaining bytes as fields of definition #6

---

## 6. Reserved Indices

Definition indices 0–5 are permanently reserved for primitive types:

| Index | Type   |
|-------|--------|
| 0     | Null   |
| 1     | Bool   |
| 2     | Int    |
| 3     | UInt   |
| 4     | Float  |
| 5     | String |

User-defined compound types start at index 6. Implementations MUST emit the six primitive definitions first, followed by compound definitions in discovery order.

---

## 7. Design Constraints

### 7.1 Homogeneous Arrays
Array elements are encoded as a single element type. JSON allows heterogeneous arrays (`[1, "two", null]`); TSON does not. Compilation of mixed-type arrays should treat all elements as the most general type or emit an error.

### 7.2 Float Precision
TSON uses binary32 (f32) for floats, providing ~7 significant digits. Values like `3.14` or `1.5e10` will not round-trip exactly. Use integer types or strings for exact numeric precision.

### 7.3 String Length
Maximum string length is 65535 bytes (limited by `u16` length prefix).

### 7.4 Array and Object Count
Maximum element count per array is 65535 (limited by `u16` count prefix). Maximum field count per object is 65535.

---

## 8. Streaming Rationale

The definition block carries the full schema (field names, types, nesting relationships) and is typically small (tens to hundreds of bytes). Once parsed into memory, the data block can be consumed entry-by-entry:

1. Read 6-byte entry header: `[def_index:u16][payload_len:u32]`
2. Jump `def_index` → definition table → know the exact structure
3. Read exactly `payload_len` bytes of payload
4. Decode the payload using the definition as a schema
5. Yield the entry. Move to the next one.

No lookahead, no backtracking, no materialising the entire data block. The streaming reader (`TsonStreamReader`) uses exactly this algorithm.

---

## 9. Binary Grammar (BNF-like)

```bnf
document    := header def_block data_block

header      := version:u8 def_off:u32 data_off:u32

def_block   := def_count:u16 { definition }
definition  := type:u8 index:u16 type_specific
type_specific := object_def | array_def | ε
object_def  := field_count:u16 { name_len:u8 name:bytes field_type:u8 }
array_def   := elem_type:u8

data_block  := entry_count:u32 { entry }
entry       := def_index:u16 payload_len:u32 payload
payload     := null_payload | bool_payload | int_payload
            |  uint_payload | float_payload | string_payload
            |  object_payload | array_payload

null_payload   := ε
bool_payload   := value:u8
int_payload    := value:i32
uint_payload   := value:u32
float_payload  := value:f32
string_payload := str_len:u16 data:bytes

object_payload := self_def:u16 { field_value }
field_value    := primitive_value | object_payload | array_payload

array_payload  := self_def:u16 elem_def:u16 elem_count:u16 { element }
element        := primitive_value | object_payload | array_payload
```

---

## 10. Reference Implementation

The Rust crate `tson` is the reference implementation. Build:

```bash
cargo build --release
```

See [README.md](README.md) for API documentation and usage examples.
