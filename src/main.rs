extern crate alloc;

mod error;
mod tson;  // tson.rs re-exports all modules (structure, encode, decode, stream)

// Binary target module tree — mirrors lib.rs so `crate::encode` etc. resolve.
mod encode;
mod decode;
mod stream;
mod structure;
#[cfg(feature = "json")]
mod compile;
#[cfg(feature = "json")]
mod decompile;

use std::io::Read;

fn is_json_file_name(file_name: &str) -> bool {
    file_name.ends_with(".json")
}

fn is_tson_file_name(file_name: &str) -> bool {
    file_name.ends_with(".tson")
}

fn print_usage(program: &str) {
    eprintln!("Usage:");
    eprintln!("  {program} <file.json>              Compile JSON → TSON binary");
    eprintln!("  {program} <file.tson>              Decompile TSON → JSON text");
    eprintln!("  {program} -s <file.tson>           Stream-print entries (debug)");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        print_usage(&args[0]);
        std::process::exit(1);
    }

    let stream_mode = args.len() > 1 && args[1] == "-s";
    let target_idx = if stream_mode { 2 } else { 1 };

    if target_idx >= args.len() {
        print_usage(&args[0]);
        std::process::exit(1);
    }

    let tson_file = &args[target_idx];

    if !std::path::Path::new(tson_file).exists() {
        eprintln!("File not found: {tson_file}");
        std::process::exit(1);
    }
    if !std::path::Path::new(tson_file).is_file() {
        eprintln!("Not a file: {tson_file}");
        std::process::exit(1);
    }

    // ── Streaming mode ──────────────────────────────────────────────
    if stream_mode {
        let mut buf = Vec::new();
        match std::fs::File::open(tson_file) {
            Ok(mut f) => {
                if let Err(e) = f.read_to_end(&mut buf) {
                    eprintln!("IO error: {e}");
                    std::process::exit(1);
                }
            }
            Err(e) => {
                eprintln!("Failed to open file: {e}");
                std::process::exit(1);
            }
        }

        match tson::TsonStreamReader::new(&buf) {
            Ok(reader) => {
                println!("Header version: {}", reader.header().version);
                println!(
                    "Def block offset: {}, Data block offset: {}",
                    reader.header().blk_definition,
                    reader.header().blk_data
                );
                println!("Definitions: {}", reader.definitions().len());
                for def in reader.definitions() {
                    println!("  Def #{}: {:?}", def.index, def.def_type);
                    if let Some(ref fields) = def.fields {
                        for (name, ftype) in fields {
                            println!("    - {name}: {:?}", ftype);
                        }
                    }
                }
                println!("Entries (streaming):");
                for result in reader {
                    match result {
                        Ok(chunk) => {
                            println!(
                                "  Entry[def={}]: {:?}",
                                chunk.definition_index,
                                chunk.data.type_tag()
                            );
                        }
                        Err(e) => {
                            eprintln!("Stream error: {e}");
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to parse TSON: {e}");
                std::process::exit(1);
            }
        }
        return;
    }

    // ── Compile / Decompile mode ────────────────────────────────────
    let file = match std::fs::File::open(tson_file) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to open file: {e}");
            std::process::exit(1);
        }
    };

    if is_json_file_name(tson_file) {
        #[cfg(feature = "json")]
        {
            match tson::compile_json_file(file) {
                Ok(doc) => {
                    // Encode to binary and write alongside the JSON file
                    let tson_path = tson_file.replace(".json", ".tson");
                    match tson::to_bytes(&doc) {
                        Ok(binary) => {
                            if let Err(e) = std::fs::write(&tson_path, &binary) {
                                eprintln!("Failed to write {tson_path}: {e}");
                                std::process::exit(1);
                            }
                            println!(
                                "Compiled {} → {} ({} bytes, {} defs, {} entries)",
                                tson_file,
                                tson_path,
                                binary.len(),
                                doc.definitions.len(),
                                doc.data.len()
                            );
                        }
                        Err(e) => {
                            eprintln!("Encode error: {e}");
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Compile error: {e}");
                    std::process::exit(1);
                }
            }
        }
        #[cfg(not(feature = "json"))]
        {
            eprintln!("JSON compilation requires the 'json' feature (build without --no-default-features)");
            std::process::exit(1);
        }
    } else if is_tson_file_name(tson_file) {
        #[cfg(feature = "json")]
        {
            match tson::decompile_tson_file(file) {
                Ok(value) => {
                    match serde_json::to_string_pretty(&value) {
                        Ok(json_str) => println!("{json_str}"),
                        Err(e) => {
                            eprintln!("JSON serialization error: {e}");
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Decompile error: {e}");
                    std::process::exit(1);
                }
            }
        }
        #[cfg(not(feature = "json"))]
        {
            let mut buf = Vec::new();
            use std::io::Read;
            if let Ok(mut f) = std::fs::File::open(tson_file) {
                let _ = f.read_to_end(&mut buf);
            }
            match tson::from_bytes(&buf) {
                Ok(doc) => {
                    println!("TSON decoded: {} entries, {} definitions",
                        doc.data.len(), doc.definitions.len());
                }
                Err(e) => {
                    eprintln!("Decompile error: {e}");
                    std::process::exit(1);
                }
            }
        }
    } else {
        eprintln!("Unknown file extension (use .json or .tson): {tson_file}");
        std::process::exit(1);
    }
}
