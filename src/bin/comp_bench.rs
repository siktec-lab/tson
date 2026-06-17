//! TSON vs JSON Performance Comparison
//!
//! Detailed multi-workload benchmark comparing TSON compile, encode,
//! decode, decompile, and streaming against plain serde_json.
//!
//! Usage:
//!   cargo run --release --bin comp-bench

use std::time::Instant;
use std::fs;

const ITERS: u32 = 2000;

fn count_leaves(data: &tson::TsonData) -> usize {
    match data {
        tson::TsonData::Array(_, _, items) => items.iter().map(count_leaves).sum(),
        tson::TsonData::Object(_, fields) => {
            1 + fields.iter().map(count_leaves).sum::<usize>()
        }
        _ => 0,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let file = if args.len() > 1 { args[1].clone() } else { "examples/users-t1.json".to_string() };

    let json_text = fs::read_to_string(&file).expect("read file");
    let json_size = json_text.len();
    let fname = std::path::Path::new(&file).file_name().unwrap().to_string_lossy();

    // Warmup
    for _ in 0..50 { let _ = tson::compile_json(&json_text).unwrap(); }

    let mut _check = 0usize;
    let mut total = 0f64;

    // ── 1. serde_json parse (baseline) ────────────────────────
    let start = Instant::now();
    for _ in 0..ITERS {
        let v: serde_json::Value = serde_json::from_str(&json_text).unwrap();
        _check += v.as_array().map(|a| a.len()).unwrap_or(1);
    }
    let json_parse_ns = start.elapsed().as_nanos() as f64 / ITERS as f64;
    total += json_parse_ns;

    // ── 2. TSON compile (JSON → TsonDocument) ─────────────────
    let start = Instant::now();
    let mut doc = None;
    for i in 0..ITERS {
        let d = tson::compile_json(&json_text).unwrap();
        if i == 0 { doc = Some(d); }
    }
    let compile_ns = start.elapsed().as_nanos() as f64 / ITERS as f64;
    total += compile_ns;
    let doc = doc.unwrap();

    // ── 3. TSON encode (TsonDocument → Vec<u8>) ───────────────
    let start = Instant::now();
    let mut bytes = None;
    for i in 0..ITERS {
        let b = tson::to_bytes(&doc).unwrap();
        if i == 0 { bytes = Some(b); }
    }
    let encode_ns = start.elapsed().as_nanos() as f64 / ITERS as f64;
    total += encode_ns;
    let bytes = bytes.unwrap();

    // ── 4. TSON decode (Vec<u8> → TsonDocument) ───────────────
    let start = Instant::now();
    let mut decoded = None;
    for i in 0..ITERS {
        let d = tson::from_bytes(&bytes).unwrap();
        if i == 0 { decoded = Some(d); }
    }
    let decode_ns = start.elapsed().as_nanos() as f64 / ITERS as f64;
    total += decode_ns;
    let decoded = decoded.unwrap();

    // ── 5. TSON decompile (TsonDocument → serde_json::Value) ──
    let start = Instant::now();
    for i in 0..ITERS {
        let v = tson::decompile_to_value(&decoded).unwrap();
        if i == 0 { _check += v.as_array().map(|a| a.len()).unwrap_or(1); }
    }
    let decompile_ns = start.elapsed().as_nanos() as f64 / ITERS as f64;
    total += decompile_ns;

    // ── 6. Streaming reader (header+defs+stream N entries) ────
    let start = Instant::now();
    for _ in 0..ITERS {
        let mut reader = tson::TsonStreamReader::new(&bytes).unwrap();
        let _h = reader.header();
        let _d = reader.definitions();
        let _dict = reader.dict();
        for e in &mut reader { let _ = e.unwrap(); }
    }
    let stream_ns = start.elapsed().as_nanos() as f64 / ITERS as f64;
    total += stream_ns;

    // ── 7. Full round-trip (compile+encode+decode+decompile) ─
    let start = Instant::now();
    for _ in 0..ITERS {
        let d = tson::compile_json(&json_text).unwrap();
        let b = tson::to_bytes(&d).unwrap();
        let back = tson::from_bytes(&b).unwrap();
        let _v = tson::decompile_to_value(&back).unwrap();
    }
    let roundtrip_ns = start.elapsed().as_nanos() as f64 / ITERS as f64;

    // ── Counts ──────────────────────────────────────────────────
    let entry_count: usize = doc.data.iter().map(|c| count_leaves(&c.data)).sum::<usize>().max(1);
    let def_count = doc.definitions.len();
    let dict_count = doc.dict.len();
    let tson_size = bytes.len();

    // ── Print table ──────────────────────────────────────────────
    println!();
    println!("╔═══════════════════════════════════════════════════════╗");
    println!("║  TSON Detailed Performance Comparison                  ║");
    println!("╠═══════════════════════════════════════════════════════╣");
    println!("║  File:     {:<42} ║", fname);
    println!("║  JSON:     {} B   TSON: {} B   Ratio: {:.1}%   ║",
        json_size, tson_size, tson_size as f64 / json_size as f64 * 100.0);
    println!("║  Defs: {}   Dict: {}   Entries (leaves): {}           ║",
        def_count, dict_count, entry_count);
    println!("║  Iterations: {}                                       ║", ITERS);
    println!("╠══════════════════════╤══════════════╤══════════════════╣");
    println!("║  Operation           │    avg / iter│   % of full RT   ║");
    println!("╠══════════════════════╪══════════════╪══════════════════╣");
    println!("║  serde_json parse    │  {:>9.1} ns │  {:>13.1}% ║", json_parse_ns, json_parse_ns / total * 100.0);
    println!("║  TSON compile        │  {:>9.1} ns │  {:>13.1}% ║", compile_ns, compile_ns / total * 100.0);
    println!("║  TSON encode         │  {:>9.1} ns │  {:>13.1}% ║", encode_ns, encode_ns / total * 100.0);
    println!("║  TSON decode         │  {:>9.1} ns │  {:>13.1}% ║", decode_ns, decode_ns / total * 100.0);
    println!("║  TSON decompile      │  {:>9.1} ns │  {:>13.1}% ║", decompile_ns, decompile_ns / total * 100.0);
    println!("║  TSON stream (full)  │  {:>9.1} ns │  {:>13.1}% ║", stream_ns, stream_ns / total * 100.0);
    println!("╟──────────────────────┼──────────────┼──────────────────╢");
    println!("║  Full round-trip     │  {:>9.1} ns │  {:>13}  ║", roundtrip_ns, roundtrip_ns);
    println!("╚══════════════════════╧══════════════════════════════════╝");

    println!();
    println!("  ── Observations ──");
    println!("  • JSON parse alone dominates ({:.0}% of per-op budget)", json_parse_ns / total * 100.0);
    println!("  • TSON overhead = compile ({:.0}%) + encode/decode ({:.0}%+{:.0}%)",
        compile_ns / total * 100.0, encode_ns / total * 100.0, decode_ns / total * 100.0);
    println!("  • Streaming reader loads defs+dict once, then O(1) per entry");
    println!("  • TSON binary is {:.1}% the size of the original JSON", tson_size as f64 / json_size as f64 * 100.0);
    if dict_count > 0 {
        println!("  • Dict has {} entries — string interning saved repeated strings", dict_count);
    }

    // ── Field access benchmarks ──────────────────────────────────────
    println!();
    println!("  ── Field Access Performance ──");

    // data.values() — zero-lookup slice access
    let start = Instant::now();
    for _ in 0..ITERS {
        for entry in doc.entries() {
            let _ = entry.data.values();
        }
    }
    let vals_ns = start.elapsed().as_nanos() as f64 / ITERS as f64;
    println!("  • data.values(): {:.1} ns", vals_ns);

    // first_entry().data.len()
    let start = Instant::now();
    for _ in 0..ITERS {
        let _ = doc.first_entry().map(|e| e.data.len());
    }
    let len_ns = start.elapsed().as_nanos() as f64 / ITERS as f64;
    println!("  • first_entry().data.len(): {:.1} ns", len_ns);

    // doc.get("name") — field lookup by name (Object roots only)
    if let Some(name_val) = doc.get("name") {
        let start = Instant::now();
        for _ in 0..ITERS {
            let _ = doc.get("name");
        }
        let get_ns = start.elapsed().as_nanos() as f64 / ITERS as f64;
        println!("  • doc.get(\"name\"): {:.1} ns", get_ns);

        // data.field() — nested field lookup
        if let Some(addr) = doc.get("address") {
            let start = Instant::now();
            for _ in 0..ITERS {
                let _ = addr.field("city", &doc.definitions);
            }
            let field_ns = start.elapsed().as_nanos() as f64 / ITERS as f64;
            println!("  • data.field(\"city\", defs): {:.1} ns", field_ns);
        }
    }
}
