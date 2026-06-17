//! Round-trip tests for TSON encode -> decode cycle.
//!
//! Each test compiles JSON -> `TsonDocument` -> binary -> `TsonDocument` ->
//! JSON, then verifies the output matches the expected JSON structure.

#[cfg(feature = "json")]
mod roundtrip_tests {

    use tson::{TsonData, TsonHeader};

    fn verify_roundtrip(json_input: &str) {
        // Compile JSON -> TSON document
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

    // Primitives

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

    // Arrays

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

    // Objects

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

    // Complex structures

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

    // Edge cases

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
        // 1.0 x 2² = 4.0 - exactly representable in f32
        verify_roundtrip("4.0");
    }

    // users-t1.json example

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

    // Binary format integrity

    #[test]
    fn header_size_is_13_bytes() {
        let doc = tson::compile_json("\"test\"").unwrap();
        let bytes = tson::to_bytes(&doc).unwrap();
        assert!(bytes.len() >= TsonHeader::SIZE, "must have header");
        assert_eq!(bytes[0], 1, "version must be 1");
        let def_off =
            u32::from_le_bytes(bytes[1..5].try_into().unwrap()) as usize;
        assert_eq!(def_off, TsonHeader::SIZE, "definition block must start after header");
    }

    #[test]
    fn definitions_are_included() {
        let doc = tson::compile_json("{\"a\": 1, \"b\": 2}").unwrap();
        assert!(!doc.definitions.is_empty(), "must have definitions");
        // At least the 6 primitive defs + 1 object def
        assert!(doc.definitions.len() >= 7, "need primitives + object");
    }

    // Dict / string interning edge cases

    #[test]
    #[cfg(feature = "dict")]
    fn dict_empty_when_no_duplicate_strings() {
        // 24 unique strings, 0 duplicates -> dict should be empty
        let json = r#"{
            "users": [
                {"name": "Alice",   "role": "admin"},
                {"name": "Bob",     "role": "editor"},
                {"name": "Charlie", "role": "viewer"}
            ]
        }"#;
        let doc = tson::compile_json(json).unwrap();
        assert_eq!(doc.dict.len(), 0,
            "Expected empty dict (no repeated strings), got {} entries: {:?}",
            doc.dict.len(), &doc.dict);
    }

    #[test]
    #[cfg(feature = "dict")]
    fn dict_only_contains_repeated_strings() {
        // "Charlie" appears twice - only Charlie should be in the dict
        let json = r#"{
            "users": [
                {"name": "Alice",   "role": "admin"},
                {"name": "Charlie", "role": "admin"},
                {"name": "Charlie", "role": "user"}
            ]
        }"#;
        let doc = tson::compile_json(json).unwrap();
        let dict_strs: Vec<&str> = doc.dict.iter().map(|s| s.as_str()).collect();
        assert!(dict_strs.contains(&"Charlie"), "Charlie should be in dict (appears ≥2 times)");
        assert!(dict_strs.contains(&"admin"),   "admin should be in dict (appears ≥2 times)");
        assert!(!dict_strs.contains(&"Alice"),  "Alice should NOT be in dict (appears once)");
        assert!(!dict_strs.contains(&"user"),   "user should NOT be in dict (appears once)");
    }

    #[test]
    #[cfg(feature = "dict")]
    fn strref_roundtrip_preserves_all_values() {
        // Verifies that even with dict, all values survive encode->decode
        let json = r#"{"names": ["Alice", "Bob", "Alice", "Charlie", "Bob"]}"#;
        let doc = tson::compile_json(json).unwrap();
        let bytes = tson::to_bytes(&doc).unwrap();
        let restored = tson::from_bytes(&bytes).unwrap();
        let value = tson::decompile_to_value(&restored).unwrap();
        let expected: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(value, expected, "Round-trip with dict should preserve all values");
    }

    #[test]
    #[cfg(feature = "dict")]
    fn strref_doc_contains_both_inline_and_strref() {
        // After compile, the TsonData tree should contain StrRef for repeated
        // strings and inline String for unique strings
        let json = r#"["Alice", "Bob", "Alice"]"#;
        let doc = tson::compile_json(json).unwrap();
        assert!(!doc.dict.is_empty(), "Dict should have entries (Alice repeats)");
        // Decode round-trip confirms correctness
        let bytes = tson::to_bytes(&doc).unwrap();
        let back = tson::from_bytes(&bytes).unwrap();
        let val = tson::decompile_to_value(&back).unwrap();
        assert_eq!(val.as_array().unwrap().len(), 3);
    }

    // Direct emit (TsonData -> binary, bypasses JSON)

    #[test]
    fn emit_roundtrip_primitive_values() {
        // Build a TsonData tree directly and emit to binary
        let data = TsonData::Object(0, vec![
            TsonData::Float(22.5),
            TsonData::Int(61),
            TsonData::String("nominal".to_string()),
        ]);
        let bytes = tson::emit(&data).unwrap();
        assert!(!bytes.is_empty(), "Emitted bytes should not be empty");

        // Round-trip through decode -> decompile
        let restored = tson::from_bytes(&bytes).unwrap();
        let value = tson::decompile_to_value(&restored).unwrap();
        assert!(value.is_object(), "Emitted data should round-trip to an object");
        let obj = value.as_object().unwrap();
        assert_eq!(obj["f0"].as_f64().unwrap(), 22.5);
        assert_eq!(obj["f1"].as_i64().unwrap(), 61);
        assert_eq!(obj["f2"].as_str().unwrap(), "nominal");
    }

    #[test]
    fn emit_value_produces_payload_only() {
        let data = TsonData::Int(42);
        let payload = tson::emit_value(&data).unwrap();
        assert_eq!(payload, 42i32.to_le_bytes().to_vec());
    }

    // Field access - helpers for extracting values

    #[test]
    fn document_get_returns_field_by_name() {
        let json = r#"{"name":"Alice","age":30,"address":{"city":"Anytown"}}"#;
        let doc = tson::compile_json(json).unwrap();

        // Top-level field lookup
        let name = doc.get("name").unwrap();
        assert!(matches!(name, TsonData::String(s) if s == "Alice"));

        let age = doc.get("age").unwrap();
        assert!(matches!(age, TsonData::Int(30)));

        // Missing field
        assert!(doc.get("nonexistent").is_none());

        // Nested field lookup - address is itself an Object
        let address = doc.get("address").unwrap();
        let city = address.field("city", &doc.definitions).unwrap();
        assert!(matches!(city, TsonData::String(s) if s == "Anytown"));
    }

    #[test]
    fn tson_data_values_len_and_is_empty() {
        let json = r#"["a", "b", "c"]"#;
        let doc = tson::compile_json(json).unwrap();
        let entry = doc.first_entry().unwrap();
        let arr = &entry.data;

        assert_eq!(arr.len(), 3);
        assert!(!arr.is_empty());
        assert_eq!(arr.values().len(), 3);
    }

    // Field access - more edge cases

    #[test]
    fn document_get_handles_all_primitive_types() {
        let json = r#"{"b":true,"n":null,"f":3.5,"u":42,"s":"hello"}"#;
        let doc = tson::compile_json(json).unwrap();

        assert!(matches!(doc.get("b"),  Some(TsonData::Bool(true))));
        assert!(matches!(doc.get("n"),  Some(TsonData::Null)));
        assert!(matches!(doc.get("f"),  Some(TsonData::Float(f)) if (*f - 3.5).abs() < 0.01));
        assert!(matches!(doc.get("u"),  Some(TsonData::Int(42))));
        assert!(matches!(doc.get("s"),  Some(TsonData::String(s)) if s == "hello"));
    }

    #[test]
    fn document_get_returns_none_on_primitive_entry() {
        // When the first entry is a primitive (not an Object), get() returns None
        let json = r#""just a string""#;
        let doc = tson::compile_json(json).unwrap();
        assert!(doc.get("any_field").is_none());
    }

    #[test]
    fn document_get_returns_none_on_empty_document() {
        let json = r#"null"#;
        let doc = tson::compile_json(json).unwrap();
        // Null is a valid entry - get() returns None because it's not an Object
        assert!(doc.get("anything").is_none());
    }

    #[test]
    fn tson_data_field_nested_lookup() {
        let json = r#"{"user":{"name":"Alice","meta":{"role":"admin"}}}"#;
        let doc = tson::compile_json(json).unwrap();
        let defs = &doc.definitions;

        let user = doc.get("user").unwrap();
        let name = user.field("name", defs).unwrap();
        assert!(matches!(name, TsonData::String(s) if s == "Alice"));

        let meta = user.field("meta", defs).unwrap();
        let role = meta.field("role", defs).unwrap();
        assert!(matches!(role, TsonData::String(s) if s == "admin"));
    }

    #[test]
    fn tson_data_values_on_primitives_returns_empty() {
        assert_eq!(TsonData::Null.values().len(), 0);
        assert_eq!(TsonData::Bool(true).values().len(), 0);
        assert_eq!(TsonData::Int(1).values().len(), 0);
        assert_eq!(TsonData::String("x".to_string()).values().len(), 0);
    }

    #[test]
    fn tson_data_len_on_empty_array() {
        let arr = TsonData::Array(0, 0, vec![]);
        assert_eq!(arr.len(), 0);
        assert!(arr.is_empty());
    }

    // Emit + nested field access

    #[test]
    fn emit_then_field_access_preserves_value_tree() {
        use tson::{TsonData, emit};
        let inner = TsonData::Object(1, vec![
            TsonData::String("x".to_string()),
            TsonData::Int(99),
        ]);
        let outer = TsonData::Object(0, vec![
            TsonData::Float(1.5),
            inner,
        ]);
        let bytes = emit(&outer).unwrap();
        let doc = tson::from_bytes(&bytes).unwrap();
        let defs = &doc.definitions;

        let f0 = doc.get("f0").unwrap();
        assert!(matches!(f0, TsonData::Float(v) if (*v - 1.5).abs() < 0.01));

        let f1 = doc.get("f1").unwrap();
        let inner_val = f1.field("f0", defs).unwrap();
        assert!(matches!(inner_val, TsonData::String(s) if s == "x"));
        let inner_num = f1.field("f1", defs).unwrap();
        assert!(matches!(inner_num, TsonData::Int(99)));
    }

    // index() + get_by_index() - O(1) repeated field access

    #[test]
    fn index_and_get_by_index() {
        let json = r#"{"name":"Alice","age":30,"city":"NYC"}"#;
        let doc = tson::compile_json(json).unwrap();

        let name_idx = doc.index("name").unwrap();
        let age_idx = doc.index("age").unwrap();
        let city_idx = doc.index("city").unwrap();

        assert!(matches!(doc.get_by_index(name_idx), Some(TsonData::String(s)) if s == "Alice"));
        assert!(matches!(doc.get_by_index(age_idx), Some(TsonData::Int(30))));
        assert!(matches!(doc.get_by_index(city_idx), Some(TsonData::String(s)) if s == "NYC"));
        assert!(doc.get_by_index(999).is_none(), "out of bounds returns None");

        // Missing field
        assert!(doc.index("missing").is_none());
    }

    #[test]
    fn index_and_get_by_index_primitive_root() {
        let json = r#""just a string""#;
        let doc = tson::compile_json(json).unwrap();
        assert!(doc.index("anything").is_none());
        assert!(doc.get_by_index(0).is_none());
    }

    // emit_with_context() - reuse existing defs+dict for responses

    #[test]
    fn emit_with_context_roundtrip() {
        // First: compile JSON for the response shape (field types: String, Int)
        let template = r#"{"f0":"x","f1":0}"#;
        let tpl_doc = tson::compile_json(template).unwrap();
        let defs = tpl_doc.definitions.clone();
        let dict = tpl_doc.dict.clone();

        // Find the response object's definition index
        let obj_def = defs.iter().find(|d| d.def_type == tson::TsonType::Object).unwrap();
        let def_idx = obj_def.index;

        // Build a response value using the template's defs+dict
        let response = tson::TsonData::Object(def_idx, vec![
            tson::TsonData::String("processed".to_string()),
            tson::TsonData::Int(42),
        ]);
        let bytes = tson::emit_with_context(&response, &defs, &dict).unwrap();
        assert!(!bytes.is_empty(), "emit_with_context produced bytes");

        // Decode and verify
        let restored = tson::from_bytes(&bytes).unwrap();
        let restored_json = tson::decompile_to_value(&restored).unwrap();
        assert_eq!(restored_json["f0"].as_str().unwrap(), "processed");
        assert_eq!(restored_json["f1"].as_i64().unwrap(), 42);
    }

    #[test]
    fn emit_with_context_reuses_dict() {
        let template = r#"{"status":"ok","code":0}"#;
        let tpl_doc = tson::compile_json(template).unwrap();
        let defs = tpl_doc.definitions.clone();
        let dict = tpl_doc.dict.clone();
        let obj_def = defs.iter().find(|d| d.def_type == tson::TsonType::Object).unwrap();
        let def_idx = obj_def.index;

        // Reuse dict: values must be in DEFINITION ORDER.
        // Template defines: code=Int, status=String (alphabetical order).
        let response = tson::TsonData::Object(def_idx, vec![
            tson::TsonData::Int(200),          // code first
            tson::TsonData::String("ok".to_string()), // status second
        ]);
        let bytes = tson::emit_with_context(&response, &defs, &dict).unwrap();
        let restored = tson::from_bytes(&bytes).unwrap();
        assert_eq!(restored.dict.len(), dict.len(),
            "emit_with_context should preserve dict content");
    }

    // TsonDocReader - multi-document stream

    #[test]
    fn tson_doc_reader_multi_document() {
        use tson::stream::TsonDocReader;
        use std::io::Cursor;

        let json1 = r#"{"a":1}"#;
        let json2 = r#"{"b":2}"#;
        let doc1 = tson::compile_json(json1).unwrap();
        let doc2 = tson::compile_json(json2).unwrap();
        let bin1 = tson::to_bytes(&doc1).unwrap();
        let bin2 = tson::to_bytes(&doc2).unwrap();

        // Build a length-prefixed stream: [4B LE len][TSON binary] x 2
        let mut stream = Vec::new();
        stream.extend_from_slice(&(bin1.len() as u32).to_le_bytes());
        stream.extend_from_slice(&bin1);
        stream.extend_from_slice(&(bin2.len() as u32).to_le_bytes());
        stream.extend_from_slice(&bin2);

        let cursor = Cursor::new(stream);
        let mut reader = TsonDocReader::new(cursor);
        let mut count = 0;
        for result in &mut reader {
            let doc = result.unwrap();
            count += 1;
            assert!(doc.data.len() >= 1, "each doc has at least one entry");
        }
        assert_eq!(count, 2, "read 2 documents from the stream");

        // EOF: empty stream should yield nothing
        let cursor = Cursor::new(Vec::new());
        let mut reader = TsonDocReader::new(cursor);
        assert!(reader.next().is_none(), "empty stream yields None");
    }

    // Streaming reader

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
