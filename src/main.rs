mod error;
mod tson;

pub fn is_json_file_name(file_name: &str) -> bool {
    file_name.ends_with(".json")
}

pub fn is_tson_file_name(file_name: &str) -> bool {
    file_name.ends_with(".tson")
}

fn main() {
    
    // get arguments
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <tson_file>", args[0]);
        std::process::exit(1);
    }

    // The file name:
    let tson_file = &args[1];

    // It exists?
    if !std::path::Path::new(tson_file).exists() {
        eprintln!("File not found: {}", tson_file);
        std::process::exit(1);
    }

    // We want open it and seek through it, so we need to check if it's a file and not a directory.
    if !std::path::Path::new(tson_file).is_file() {
        eprintln!("Not a file: {}", tson_file);
        std::process::exit(1);
    }

    // open the file and parse it
    let file = std::fs::File::open(tson_file).expect("Failed to open file");

    if is_json_file_name(tson_file) {
        match tson::compile_json_file(file) {
            Ok(document) => println!("Parsed TSON document: {:?}", document),
            Err(e) => eprintln!("Error parsing TSON file: {}", e),
        }
    } else if is_tson_file_name(tson_file) {
        match tson::decompile_tson_file(file) {
            Ok(document) => println!("Parsed TSON document: {:?}", document),
            Err(e) => eprintln!("Error parsing TSON file: {}", e),
        }
    }
}
