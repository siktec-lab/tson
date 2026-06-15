use std::time::Instant;

const USERS_JSON: &str = r#"[
    {"id":1,"name":"Alice","age":30,"address":{"street":"123 Main St","city":"Anytown","state":"CA","zip":"12345"},"hobbies":["reading","hiking","cooking"]},
    {"id":2,"name":"Bob","age":25,"address":{"street":"456 Elm St","city":"Othertown","state":"NY","zip":"67890"},"hobbies":["gaming","traveling","photography"]},
    {"id":3,"name":"Charlie","age":35,"address":{"street":"789 Oak St","city":"Sometown","state":"TX","zip":"54321"},"hobbies":["music","sports","gardening"]}
]"#;

const ITERS: u32 = 5000;

fn main() {
    // Warmup
    for _ in 0..100 {
        let _ = tson::compile_json(USERS_JSON).unwrap();
    }

    // 1. serde_json parse only (baseline)
    let start = Instant::now();
    let mut _total = 0usize;
    for i in 0..ITERS {
        let v: serde_json::Value = serde_json::from_str(USERS_JSON).unwrap();
        if i == 0 { _total = v.as_array().map(|a| a.len()).unwrap_or(0); }
    }
    let json_ns = start.elapsed().as_nanos() as f64 / ITERS as f64;

    // 2. TSON compile + encode
    let start = Instant::now();
    for i in 0..ITERS {
        let doc = tson::compile_json(USERS_JSON).unwrap();
        let bytes = tson::to_bytes(&doc).unwrap();
        if i == 0 { _total = bytes.len(); }
    }
    let tson_ns = start.elapsed().as_nanos() as f64 / ITERS as f64;

    // 3. TSON full round-trip
    let start = Instant::now();
    for i in 0..ITERS {
        let doc = tson::compile_json(USERS_JSON).unwrap();
        let bytes = tson::to_bytes(&doc).unwrap();
        let back = tson::from_bytes(&bytes).unwrap();
        let val = tson::decompile_to_value(&back).unwrap();
        if i == 0 { _total = val.as_array().map(|a| a.len()).unwrap_or(0); }
    }
    let rt_ns = start.elapsed().as_nanos() as f64 / ITERS as f64;

    // Print results
    println!("╔═══════════════════════════════════════════════╗");
    println!("║  TSON Performance Comparison (release build)   ║");
    println!("║  Input: {} B JSON, {} iters each       ║", USERS_JSON.len(), ITERS);
    println!("╠═══════════════════════════════════════════════╣");
    println!("║  Operation                     │   avg / iter ║");
    println!("╠═══════════════════════════════════════════════╣");
    println!("║  serde_json parse (baseline)   │  {:>9.1} ns ║", json_ns);
    println!("║  TSON compile + encode         │  {:>9.1} ns ║", tson_ns);
    println!("║  TSON full round-trip          │  {:>9.1} ns ║", rt_ns);
    println!("╠═══════════════════════════════════════════════╣");
    println!("║  TSON overhead (schema+dict):  │  {:>9.1} ns ║", tson_ns - json_ns);
    println!("╚═══════════════════════════════════════════════╝");

    let doc = tson::compile_json(USERS_JSON).unwrap();
    let bytes = tson::to_bytes(&doc).unwrap();
    println!();
    println!("  JSON input: {}", USERS_JSON.len());
    println!("  TSON binary: {} B ({:.1}%)", bytes.len(), bytes.len() as f64 / USERS_JSON.len() as f64 * 100.0);
    println!("  Definitions: {}, Dict entries: {}, Data entries: {}",
        doc.definitions.len(), doc.dict.len(), doc.data.len());
}
