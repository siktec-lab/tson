# TSON Binary Format Specification

## Overview

TSON (Terse JSON) is a compact binary serialization format for structured data. It separates **structure** (field names, types) from **values**, storing the structure once in a definition block and referencing it from the data block. Repeated strings are stored once in a dict block. This yields dramatic compaction for repetitive JSON payloads.

The format is designed for microcontrollers and constrained environments: the definition and dict blocks are small enough to keep in RAM, and the data block can be streamed entry-by-entry with `O(1)` additional memory per entry.

## Document Layout

```
┌──────────────────────────────────────────────────────┐
│  HEADER (13 bytes, fixed)                              │
│  [version:u8][def_off:u32][dict_off:u32][data_off:u32]│
├──────────────────────────────────────────────────────┤
│  DEFINITION BLOCK                                     │
│  [def_count:u16 LE]                                   │
│    [type:u8][index:u16 LE]                           │
│    Object: [field_count:u16] fields...                │
│    Array:  [elem_type:u8]                             │
│    (Primitive: no extra data)                         │
├──────────────────────────────────────────────────────┤
│  DICT BLOCK (string interning)                        │
│  [entry_count:u32 LE]                                 │
│    [str_len:hybrid][UTF-8 bytes]  x N                 │
├──────────────────────────────────────────────────────┤
│  DATA BLOCK                                           │
│  [entry_count:u32 LE]                                 │
│    [def_index:u16 LE] [payload_len:u32 LE]           │
│    … payload per type …                              │
└──────────────────────────────────────────────────────┘
```

All multi-byte integers are little-endian (`LE`).

---

## 1. Header (13 bytes, fixed)

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0      | 1    | `version` | Format version. Always `1`. |
| 1      | 4    | `def_off` | Byte offset to definition block. Always ≥ 13. |
| 5      | 4    | `dict_off`| Byte offset to dict block. Must be ≥ `def_off`. |
| 9      | 4    | `data_off`| Byte offset to data block. Must be ≥ `dict_off`. |

The byte range `[def_off .. dict_off)` is the definition block. The range `[dict_off .. data_off)` is the dict block (zero-length if `dict_off == data_off`, meaning no strings were interning). Bytes from `data_off` onward are the data block.

---

## 2. Type Tags

| Tag  | Name   | Wire size | Description |
|------|--------|-----------|-------------|
| 0x00 | Null   | 0 B       | Null value |
| 0x01 | Bool   | 1 B       | `0x00` = false, non-zero = true |
| 0x02 | Int    | 4 B       | Signed i32, little-endian |
| 0x03 | UInt   | 4 B       | Unsigned u32, little-endian |
| 0x04 | Float  | 4 B       | IEEE 754 binary32, little-endian |
| 0x05 | String | variable  | Length-prefixed UTF-8 with hybrid encoding (see §4.3). This type tag is also used for StrRef payloads - the distinction is made per-value via a sentinel byte. |
| 0x10 | Array  | variable  | Ordered list of same-type elements |
| 0x11 | Object | variable  | Key-value pairs whose schema is in a definition |

Tags 0x00–0x05 are **primitive**. Tags 0x10–0x11 are **compound**. There is no separate StrRef type tag - string interning is handled within the String payload via the sentinel value `0xFF`.

---

## 3. Definition Block

### 3.1 Block Header

```
[def_count: u16 LE]
```

Number of definitions that follow. Maximum in the reference implementation: 2048.

### 3.2 Definition Entry

Every definition begins with:

```
[type: u8] [index: u16 LE]
```

`index` is the zero-based position of this definition in the definition table. Indices 0–5 are reserved for primitive types (always present). User-defined compound types start at index 6.

### 3.3 Object Definition

```
[field_count: u16 LE]
[name_len: u8][name: UTF-8][field_type: u8]  x field_count
```

Fields are stored in **canonical order** (alphabetically by field name) so that the same logical schema produces identical byte representations for deduplication.

**Example** - Definition #6 for `{id: Int, name: String}`:

```
type = 0x11     (Object)
index = 0x0006  (u16 LE)
field_count = 2
  Field 0: name_len=2, name="id",  type=0x02 (Int)
  Field 1: name_len=4, name="name", type=0x05 (String)
```

### 3.4 Array Definition

```
[elem_type: u8]
```

**Example** - Definition #7 for an array of strings:

```
type = 0x10     (Array)
index = 0x0007
elem_type = 0x05 (String)
```

### 3.5 Primitive Definitions (Indices 0–5)

Always present:

| Index | Type   |
|-------|--------|
| 0     | Null   |
| 1     | Bool   |
| 2     | Int    |
| 3     | UInt   |
| 4     | Float  |
| 5     | String |

---

## 4. Dict Block (String Interning)

### 4.1 Block Header

```
[entry_count: u32 LE]
```

Number of interned strings. Zero when no strings appeared ≥2 times in the document (lazy-promotion - see §4.2).

### 4.2 Lazy-Promotion Rule

Strings are added to the dict only when they appear for the **second time** during compilation. The first occurrence is always emitted as an inline string. This guarantees the dict never contains strings that appear only once - there is zero wasted space from unique strings.

### 4.3 Dict String Encoding

Dict strings use the same hybrid-length encoding as inline strings (see §5.6):

```
[first_byte: u8]  [length_extension: 0–3 bytes]  [UTF-8]
```

---

## 5. Data Block

### 5.1 Block Header

```
[entry_count: u32 LE]
```

Number of data entries. Maximum in the reference implementation: 1,048,576.

### 5.2 Data Entry

```
[def_index: u16 LE] [payload_len: u32 LE] [payload …]
```

`def_index` identifies which definition describes this entry's structure. `payload_len` enables forward-compatible skipping.

### 5.3 Null

Zero bytes. `payload_len = 0`.

### 5.4 Bool

```
[value: u8]
```
`0x00` = false, non-zero = true. `payload_len = 1`.

### 5.5 Int / UInt / Float

```
[value: i32 LE]     (Int,  4 B)
[value: u32 LE]     (UInt, 4 B)
[value: f32 LE]     (Float, 4 B)
```

> **Precision**: binary32 has ~7 decimal digits. Values like `3.14` will not round-trip exactly through TSON. Use integers or strings for exact numeric precision.

### 5.6 String (hybrid-length + StrRef sentinel)

String payloads use a **self-describing length prefix**:

| First byte | Overhead | Max inline length | Format |
|------------|----------|-------------------|--------|
| `0x00–0x7F` | 1 B | 127 B | `[len: u8][UTF-8]` |
| `0x80–0xBF` | 2 B | 16 383 B | `[0x80\|hi6][lo8][UTF-8]` |
| `0xFE` | 4 B | 16 777 215 B | `[0xFE][u24 LE][UTF-8]` |
| `0xFF` | 5 B | (StrRef) | `[0xFF][dict_idx: u32 LE]` |

Bytes `0xC0–0xFD` are reserved for future extensions. The sentinel `0xFF` converts the payload from inline string data to a dict index - the decoder reads the next 4 bytes as a `u32 LE` dict index and resolves it against the dict block.

**Rationale**:
- Small strings (≤127 B) cost 1 byte overhead - common for field values like "CA", "Anytown", names.
- Medium strings (≤16 KB) cost 2 bytes.
- Large strings (≤16 MB) cost 4 bytes - handles base64 blobs and large text.
- StrRef overhead is 5 bytes (1 sentinel + 4 index) - always wins over inline for strings ≥5 bytes on the second+ occurrence.

**Examples**:
```
"hi"       -> [02][68 69]                          (inline, 4 B total)
"CA"       -> [02][43 41]                          (inline, 4 B total)
0xFF + 42  -> [FF][2A 00 00 00]                   (StrRef, dict[42])
"temperature" (11 B), interned: [FF][idx]         (StrRef, 5 B on wire)
"temperature" (11 B), not interned: [0B][74 65…]  (inline, 12 B total)
```

### 5.7 Object

```
[self_def_index: u16 LE] [field_value] [field_value] …
```

Field values are concatenated in definition field order. Each field value is encoded per its declared type.

**Example** - Object #6 (id: Int, name: String) with `id=1, name="Alice"`:

```
[self_def = 0x0006]          (2 B)
Field 0 (id: Int):
  [01 00 00 00]              (i32 LE = 1, 4 B)
Field 1 (name: String):
  [05]                       (len = 5, short encoding, 1 B)
  [41 6C 69 63 65]           ("Alice", 5 B)
──────────────────────────────
payload_len = 2 + 4 + 6 = 12
```

### 5.8 Array

```
[self_def_index: u16 LE] [elem_def_index: u16 LE] [elem_count: u16 LE] [element] [element] …
```

**Example** - Array #7 (String elements) with `["read", "ski"]`:

```
[self_def = 0x0007]          (2 B)
[elem_def = 0x0005]          (2 B, String = 5)
[elem_count = 2]             (2 B)
Element 0 (String):
  [04][72 65 61 64]          ("read", 5 B total)
Element 1 (String):
  [03][73 6B 69]             ("ski", 4 B total)
──────────────────────────────
payload_len = 2 + 2 + 2 + 5 + 4 = 15
```

---

## 6. Nested Compound Values

When a field or element type is itself a compound type (Object or Array), the nested value is encoded with its own `self_def_index` as the first 2 bytes. This makes every compound value self-describing regardless of depth.

---

## 7. Reserved Indices

Definition indices 0–5 are permanently reserved:

| Index | Type   |
|-------|--------|
| 0     | Null   |
| 1     | Bool   |
| 2     | Int    |
| 3     | UInt   |
| 4     | Float  |
| 5     | String |

Compound definitions start at index 6.

---

## 8. Design Constraints

### 8.1 Homogeneous Arrays
TSON arrays are typed by element. JSON allows mixed types (`[1, "two", null]`). Mixed-type arrays are not supported - the compiler determines a single element type from the first non-null entry.

### 8.2 Float Precision
Binary32 provides ~7 significant digits. Values like `3.14` or `1.5e10` will not round-trip exactly.

### 8.3 String Length
Maximum inline string length is 16,777,215 bytes (24-bit, ~16 MB). The sentinel `0xFF` is never a valid inline string length.

### 8.4 Count Limits
Maximum element count per array: 65,535 (`u16`). Maximum field count per object: 65,535 (`u16`).

---

## 9. Streaming Rationale

The definition and dict blocks carry the complete schema and string table - typically tens to hundreds of bytes. Once parsed into memory, the data block can be consumed entry-by-entry:

1. Read 6-byte entry header: `[def_index:u16][payload_len:u32]`
2. Jump `def_index` -> definition table -> know the exact structure
3. Read exactly `payload_len` bytes of payload
4. Decode the payload using the definition as a schema
5. Yield the entry. Move to the next one.

No lookahead, no backtracking, no materialising the entire data block.

---

## 10. Binary Grammar (BNF)

```bnf
document    := header def_block dict_block data_block

header      := version:u8 def_off:u32 dict_off:u32 data_off:u32

def_block   := def_count:u16 { definition }
definition  := type:u8 index:u16 type_specific
type_specific := object_def | array_def | ε
object_def  := field_count:u16 { name_len:u8 name:bytes field_type:u8 }
array_def   := elem_type:u8

dict_block  := dict_count:u32 { string_encoding }
string_encoding := short | medium | long
short       := len:u8 (0x00..0x7F) data:bytes
medium      := hi6(0x80|hi6):u8 lo8:u8 data:bytes
long        := tag:0xFE len:u24 data:bytes

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
string_payload := short | medium | long | strref
strref         := sentinel:0xFF dict_idx:u32

object_payload := self_def:u16 { field_value }
field_value    := primitive_value | object_payload | array_payload

array_payload  := self_def:u16 elem_def:u16 elem_count:u16 { element }
element        := primitive_value | object_payload | array_payload
```

---

## 11. Security Considerations

### 11.1 Malformed Headers

Decoders MUST validate all header fields before use:

| Field | Check | Rationale |
|-------|-------|-----------|
| `version` | Must equal `1` | Rejects unknown formats |
| `def_off` | Must be ≥ 13. Must be ≤ buffer length | Prevents OOB reads |
| `dict_off` | Must be ≥ `def_off` | Dict follows defs |
| `data_off` | Must be ≥ `dict_off` | Data follows dict |

The reference implementation (`tson` crate) enforces all four checks.

### 11.2 Memory Exhaustion (OOM)

| Field | Max wire value | Reference cap |
|-------|---------------|---------------|
| `entry_count` (u32) | 4,294,967,295 | 1,048,576 |
| `def_count` (u16) | 65,535 | 2,048 |
| `field_count` (u16) | 65,535 | 256 |

Counts are validated against remaining bytes before pre-allocating.

### 11.3 Recursive/Circular Definitions

A recursion depth counter is incremented for each nested Object/Array. Depth > 128 aborts with a `ParseError`, preventing stack overflow.

### 11.4 Integer Overflow

All `pos + len` arithmetic is guarded by bounds checks against the buffer length.

### 11.5 UTF-8 Validation

All strings (field names, values, dict entries) are validated as UTF-8. Invalid sequences produce `ParseError`.

### 11.6 Definition Index Validation

Every `def_index` lookup verifies the index exists. Unknown indices return `ParseError`. Type mismatches are defended against by the `self_def_index` embedded in compound payloads.

---

## 12. Reference Implementation

The Rust crate `tson` is the reference implementation.

```bash
cargo build --release
cargo test
cargo run --release --bin tson-bench
```

See [README.md](../README.md) for API documentation and usage examples, and
[DOC.md](DOC.md) for the full Rust user guide.
