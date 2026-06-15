//! TSON Benchmark Tool
//!
//! Scans `examples/` for `.json` files, compiles each to TSON, reports
//! compression ratios, and optionally measures p50/p99 compile times.
//!
//! Usage:
//!   cargo run --release --bin tson-bench           # basic summary
//!   cargo run --release --bin tson-bench -- --perf  # with p50/p99 timing

use std::fs;
use std::time::Instant;

const EXAMPLES_DIR: &str = "examples";
const PERF_ITERATIONS: u32 = 200;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let perf_mode = args.iter().any(|a| a == "--perf");

    // ── Collect .json files ────────────────────────────────────────
    let mut entries: Vec<String> = Vec::new();
    if let Ok(dir) = fs::read_dir(EXAMPLES_DIR) {
        for entry in dir.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "json") {
                entries.push(path.to_string_lossy().to_string());
            }
        }
    }
    entries.sort();

    if entries.is_empty() {
        eprintln!("No .json files found in {}/", EXAMPLES_DIR);
        std::process::exit(1);
    }

    // ── Header ─────────────────────────────────────────────────────
    println!("\n╔══════════════════════╤══════════╤══════════╤══════════╤══════════╤═════════╗");
    println!("║ File                 │ JSON (B) │ TSON (B) │   Ratio  │    Defs  │ Entries ║");
    println!("╠══════════════════════╪══════════╪══════════╪══════════╪══════════╪═════════╣");

    let mut total_json = 0u64;
    let mut total_tson = 0u64;
    let mut results: Vec<FileResult> = Vec::new();

    for path_str in &entries {
        // Read JSON
        let json_text = match fs::read_to_string(path_str) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("  skip {} (read error: {})", path_str, e);
                continue;
            }
        };
        let json_size = json_text.len() as u64;

        // Compile
        let compile_start = Instant::now();
        let doc = match tson::compile_json(&json_text) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("  skip {} (compile error: {})", path_str, e);
                continue;
            }
        };
        let compile_elapsed = compile_start.elapsed();

        // Encode to binary
        let tson_bytes = match tson::to_bytes(&doc) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("  skip {} (encode error: {})", path_str, e);
                continue;
            }
        };
        let tson_size = tson_bytes.len() as u64;

        // Compression ratio
        let ratio = if json_size > 0 {
            (tson_size as f64 / json_size as f64) * 100.0
        } else {
            0.0
        };

        // Print row
        let fname = std::path::Path::new(path_str)
            .file_name()
            .unwrap()
            .to_string_lossy();
        println!(
            "║ {:<20} │ {:>8} │ {:>8} │ {:>7.1}% │ {:>8} │ {:>7} ║",
            fname,
            fmt_size(json_size),
            fmt_size(tson_size),
            ratio,
            doc.definitions.len(),
            doc.data.len()
        );

        total_json += json_size;
        total_tson += tson_size;
        results.push(FileResult {
            _name: fname.to_string(),
            json_size,
            tson_size,
            _compile_us: compile_elapsed.as_micros() as u64,
        });
    }

    // ── Footer ─────────────────────────────────────────────────────
    let overall_ratio = if total_json > 0 {
        (total_tson as f64 / total_json as f64) * 100.0
    } else {
        0.0
    };
    println!("╟──────────────────────┼──────────┼──────────┼──────────┼──────────┼─────────╢");
    println!(
        "║ TOTAL                │ {:>8} │ {:>8} │ {:>7.1}% │          │         ║",
        fmt_size(total_json),
        fmt_size(total_tson),
        overall_ratio
    );
    println!("╚══════════════════════╧══════════╧══════════╧══════════╧══════════╧═════════╝");

    println!(
        "\n  Compression: {:.1}% of original size ({:.1}% savings).",
        overall_ratio,
        100.0 - overall_ratio
    );

    // ── Observations ──────────────────────────────────────────────
    println!("\n  ── Observations ──");
    println!("  • Field names are stored once in the definition block — never repeated.");
    println!(
        "  • Identical object shapes share a single definition (deduplication)."
    );
    println!("  • Primitives (int, float, bool, null, string) are stored as raw bytes.");
    println!("  • The definition block is small — ideal for microcontroller RAM.");

    if !results.is_empty() {
        let largest_file = results.iter().max_by_key(|r| r.json_size).unwrap();
        let best_ratio = results.iter().min_by(|a, b| {
            (a.tson_size as f64 / a.json_size as f64)
                .partial_cmp(&(b.tson_size as f64 / b.json_size as f64))
                .unwrap()
        }).unwrap();
        println!(
            "  • Largest input: {} ({})",
            largest_file._name,
            fmt_size(largest_file.json_size)
        );
        println!(
            "  • Best compression: {} ({:.1}% of original)",
            best_ratio._name,
            (best_ratio.tson_size as f64 / best_ratio.json_size as f64) * 100.0
        );
    }

    // ── Performance mode (optional) ───────────────────────────────
    if perf_mode {
        println!("\n  ── Performance (p50 / p99 compile latency) ──");
        run_perf_bench(&entries);
    }
}

// ─── Performance benchmarking ────────────────────────────────────────

fn run_perf_bench(file_paths: &[String]) {
    let total_iterations = PERF_ITERATIONS;

    for path_str in file_paths {
        let json_text = match fs::read_to_string(path_str) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let fname = std::path::Path::new(path_str).file_name().unwrap().to_string_lossy();

        let mut durations: Vec<u64> = Vec::with_capacity(total_iterations as usize);

        for _ in 0..total_iterations {
            let start = Instant::now();
            let _doc = tson::compile_json(&json_text).unwrap();
            let elapsed = start.elapsed().as_nanos() as u64;
            durations.push(elapsed);
        }
        durations.sort_unstable();

        let p50 = durations[(durations.len() * 50 / 100).min(durations.len() - 1)];
        let p99 = durations[(durations.len() * 99 / 100).min(durations.len() - 1)];
        let avg = durations.iter().sum::<u64>() / durations.len() as u64;

        println!(
            "  {:<20} avg={:>7}   p50={:>7}   p99={:>7}   ({} iters)",
            fname,
            fmt_ns(avg),
            fmt_ns(p50),
            fmt_ns(p99),
            total_iterations
        );
    }

    println!("\n  Note: run in release mode for realistic numbers:");
    println!("    cargo run --release --bin tson-bench -- --perf");
}

// ─── Formatting helpers ───────────────────────────────────────────────

fn fmt_size(bytes: u64) -> String {
    if bytes >= 1024 {
        format!("{:.1}K", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

fn fmt_ns(ns: u64) -> String {
    if ns >= 1_000_000 {
        format!("{:.1}ms", ns as f64 / 1_000_000.0)
    } else if ns >= 1_000 {
        format!("{:.1}µs", ns as f64 / 1_000.0)
    } else {
        format!("{}ns", ns)
    }
}

struct FileResult {
    _name: String,
    json_size: u64,
    tson_size: u64,
    _compile_us: u64,
}
