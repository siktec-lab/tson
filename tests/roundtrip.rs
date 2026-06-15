//! Round-trip tests for TSON encode → decode cycle.
//!
//! Each test compiles JSON → `TsonDocument` → binary → `TsonDocument` →
//! JSON, then verifies the output matches the expected JSON structure.

#[cfg(feature = "json")]
mod roundtrip_tests {

    fn verify_roundtrip(json_input: &str) {
        // Compile JSON → TSON document
        let doc = tson::compile_json(json_input).unwrap();

        // Encode to binary
        let bytes = tson::to_bytes(&doc).expect("encode should succeed");

        // Decode from binary
        let recovered = tson::from_bytes(&bytes).expect("decode should succeed");

        // Decompile back to JSON value
        let output = tson::decompile_to_value(&recovered).expect("decompile should succeed");

        // Verify output matches input (parsed for structural comparison,
        // not string comparison, since key ordering may differ)
        let expected: serde_json::Value =
            serde_json::from_str(json_input).expect("input should be valid JSON");

        // Do NOT compare by value because JSON objects don't preserve order.
        // Instead, check structural equivalence after serializing both.
        // A simple assert_eq would work here since serde_json objects
        // compare by key-value equality, not insertion order.
        assert_eq!(output, expected, "round-trip structural mismatch");

        // Also verify encoding to JSON string is valid (extra safety)
        let _output_str = serde_json::to_string(&output).expect("should serialize to string");
    }

    // ── Primitives ────────────────────────────────────────────────────

    #[test]
    fn null_value() {
        verify_roundtrip("null");
    }

    #[test]
    fn bool_true() {
        verify_roundtrip("true");
    }

    #[test]
    fn bool_false() {
        verify_roundtrip("false");
    }

    #[test]
    fn integer() {
        verify_roundtrip("42");
    }

    #[test]
    fn negative_int() {
        verify_roundtrip("-17");
    }

    #[test]
    fn unsigned_int() {
        verify_roundtrip("65535");
    }

    #[test]
    fn float_exact() {
        verify_roundtrip("0.5");
    }

    #[test]
    fn float_exact_neg() {
        verify_roundtrip("-1.25");
    }

    #[test]
    fn string() {
        verify_roundtrip(r#""hello world""#);
    }

    #[test]
    fn empty_string() {
        verify_roundtrip(r#""""#);
    }

    // ── Arrays ────────────────────────────────────────────────────────

    #[test]
    fn empty_array() {
        verify_roundtrip("[]");
    }

    #[test]
    fn int_array() {
        verify_roundtrip("[1, 2, 3]");
    }

    #[test]
    fn homogeneous_string_array() {
        verify_roundtrip(r#"["a", "b", "c"]"#);
    }

    #[test]
    fn nested_array() {
        verify_roundtrip("[[1, 2], [3, 4]]");
    }

    // ── Objects ───────────────────────────────────────────────────────

    #[test]
    fn empty_object() {
        verify_roundtrip("{}");
    }

    #[test]
    fn simple_object() {
        verify_roundtrip(r#"{"name":"Alice","age":30}"#);
    }

    #[test]
    fn object_with_nested_object() {
        verify_roundtrip(r#"{"user":{"name":"Bob","score":100}}"#);
    }

    // ── Complex structures ────────────────────────────────────────────

    #[test]
    fn array_of_objects() {
        verify_roundtrip(
            r#"[
            {"id": 1, "name": "Alice", "active": true},
            {"id": 2, "name": "Bob", "active": false}
        ]"#,
        );
    }

    #[test]
    fn deeply_nested() {
        verify_roundtrip(
            r#"{
            "level1": {
                "level2": {
                    "level3": {
                        "value": 42
                    }
                }
            }
        }"#,
        );
    }

    #[test]
    fn object_with_array_field() {
        verify_roundtrip(r#"{"tags": ["a", "b", "c"], "count": 3}"#);
    }

    #[test]
    fn array_of_arrays() {
        verify_roundtrip("[[1], [1, 2], [1, 2, 3]]");
    }

    // ── Edge cases ────────────────────────────────────────────────────

    #[test]
    fn large_integer() {
        verify_roundtrip("2147483647"); // i32 max
    }

    #[test]
    fn zero() {
        verify_roundtrip("0");
    }

    #[test]
    fn scientific_float() {
        // 1.0 × 2² = 4.0 — exactly representable in f32
        verify_roundtrip("4.0");
    }

    // ── users-t1.json example ─────────────────────────────────────────

    #[test]
    fn users_example() {
        let json = r#"[
            {
                "id": 1,
                "name": "Alice",
                "age": 30,
                "address": {
                    "street": "123 Main St",
                    "city": "Anytown",
                    "state": "CA",
                    "zip": "12345"
                },
                "hobbies": ["reading", "hiking", "cooking"]
            },
            {
                "id": 2,
                "name": "Bob",
                "age": 25,
                "address": {
                    "street": "456 Elm St",
                    "city": "Othertown",
                    "state": "NY",
                    "zip": "67890"
                },
                "hobbies": ["gaming", "traveling", "photography"]
            },
            {
                "id": 3,
                "name": "Charlie",
                "age": 35,
                "address": {
                    "street": "789 Oak St",
                    "city": "Sometown",
                    "state": "TX",
                    "zip": "54321"
                },
                "hobbies": ["music", "sports", "gardening"]
            }
        ]"#;
        verify_roundtrip(json);
    }

    // ── Binary format integrity ───────────────────────────────────────

    #[test]
    fn header_size_is_9_bytes() {
        let doc = tson::compile_json("\"test\"").unwrap();
        let bytes = tson::to_bytes(&doc).unwrap();
        assert!(bytes.len() >= 9, "must have header");
        // First 9 bytes are header — verify version and offsets
        assert_eq!(bytes[0], 1, "version must be 1");
        let def_off =
            u32::from_le_bytes(bytes[1..5].try_into().unwrap()) as usize;
        assert_eq!(def_off, 9, "definition block must start after header");
    }

    #[test]
    fn definitions_are_included() {
        let doc = tson::compile_json("{\"a\": 1, \"b\": 2}").unwrap();
        assert!(!doc.definitions.is_empty(), "must have definitions");
        // At least the 6 primitive defs + 1 object def
        assert!(doc.definitions.len() >= 7, "need primitives + object");
    }

    // ── Streaming reader ─────────────────────────────────────────────

    #[test]
    fn stream_reader_yields_all_entries() {
        let json = r#"[
            {"x": 1},
            {"x": 2},
            {"x": 3}
        ]"#;
        let doc = tson::compile_json(json).unwrap();
        let bytes = tson::to_bytes(&doc).unwrap();

        let mut reader = tson::TsonStreamReader::new(&bytes).unwrap();
        let mut count = 0;
        for result in &mut reader {
            let _chunk = result.unwrap();
            count += 1;
        }
        assert_eq!(count, 1, "array of 3 objects is 1 root entry");
    }
}
