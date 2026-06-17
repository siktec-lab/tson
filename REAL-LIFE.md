# TSON — Real-Life Usage Guide

A walkthrough from remote JSON ingestion to local persisted struct data, showing how TSON fits into a realistic data pipeline (serialise, transfer, store, query, save to disk).

---

## Scenario

An IoT platform receives telemetry from 500 sensors every second. Each payload is ~900 bytes of JSON with repeating field structure (temperatures, humidity, GPS coords, timestamps, device metadata). The data is:

1. Received from remote sensors as JSON over HTTPS  
2. Compiled to TSON on the server (compression + schema dedup)  
3. Stored to the local file system as a rolling TSON archive  
4. Read back later with the streaming reader to query and replay data

---

## 1. Fetch the Data (Plain JSON Over Network)

A remote API returns telemetry for a single device.

```rust
use std::io::Read;

fn fetch_sensor_data() -> String {
    let mut response = reqwest::blocking::get(
        "https://api.iot-platform.example/sensors/device-001/latest"
    )
    .expect("network error")
    .text()
    .expect("utf-8 error");

    response
}
// response: '{"device_id":"sensor-001","temp":22.5,"humidity":61,...}'
```

The response is standard JSON — perfectly readable, easy to debug with `curl`, compatible with every HTTP client in existence.

---

## 2. Compile to TSON (Schema Discovery + Interning)

The server compiles the JSON to TSON once before storing it. The compiler
automatically discovers the schema, deduplicates repeated strings, and
produces a compact binary.

```rust
use tson;

fn json_to_tson(json_text: &str) -> Vec<u8> {
    let doc = tson::compile_json(json_text)
        .expect("valid JSON should compile");

    // Inspect what was found
    println!("Definitions: {}", doc.definitions.len());
    println!("Dict entries: {}", doc.dict.len());
    println!("Data entries: {}", doc.data.len());

    tson::to_bytes(&doc).expect("binary encode")
}
```

For one sensor reading, the output might show:

```
Definitions: 11       (6 primitives + 5 compound shapes)
Dict entries: 63       (repeated strings like "temperature", "humidity", "nominal")
Data entries: 1        (one root entry — could be many if from a batch)
```

TSON automatically discovered the structure without a schema file, and
included only the strings that repeat (lazy-promotion ensures no waste from
unique strings like sensor IDs).

---

## 3. Save to Disk (Rolling Archive)

A typical IoT server writes each TSON binary blob to a flat file, one per
event. For large-scale data, a rolling log approach works well.

```rust
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;

struct RollingArchive {
    dir: String,
    current: File,
    current_size: u64,
    max_size: u64,
    file_index: u64,
}

impl RollingArchive {
    fn new(dir: &str, max_size: u64) -> Self {
        fs::create_dir_all(dir).unwrap();
        let path = format!("{dir}/archive-000.tson");
        let file = OpenOptions::new()
            .create(true).append(true).open(&path).unwrap();
        let size = file.metadata().map(|m| m.len()).unwrap_or(0);
        RollingArchive {
            dir: dir.to_string(),
            current: file,
            current_size: size,
            max_size,
            file_index: 0,
        }
    }

    fn append(&mut self, tson_bytes: &[u8]) {
        if self.current_size + tson_bytes.len() as u64 > self.max_size {
            self.file_index += 1;
            let path = format!(
                "{}/archive-{:03}.tson",
                self.dir, self.file_index
            );
            self.current = File::create(&path).unwrap();
            self.current_size = 0;
            println!("Rolling to new archive: {path}");
        }

        // Write: [4-byte LE length prefix] [TSON binary]
        self.current
            .write_all(&(tson_bytes.len() as u32).to_le_bytes())
            .unwrap();
        self.current.write_all(tson_bytes).unwrap();
        self.current_size += tson_bytes.len() as u64 + 4;
    }
}
```

Each archive entry is a length-prefixed TSON blob — simple, seekable, and
self-describing.

---

## 4. Main Loop

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut archive = RollingArchive::new("./sensor-archive", 64 * 1024 * 1024);

    loop {
        let json_resp = fetch_sensor_data();
        let tson_bin = json_to_tson(&json_resp);
        archive.append(&tson_bin);

        println!(
            "Stored: JSON {} B → TSON {} B ({:.1}% of original)",
            json_resp.len(),
            tson_bin.len(),
            tson_bin.len() as f64 / json_resp.len() as f64 * 100.0
        );

        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
```

Each second: fetch JSON from the network, compile to TSON (field names and
strings reduced by up to 70%), save to the rolling archive system.

---

## 5. Reading Back Data (Streaming)

Days later, a downstream service needs to replay sensor data to compute daily averages, filter alarms, or export to CSV.

### 5.1 Open an Archive File

```rust
use std::fs;

fn read_archive(file_path: &str) -> Vec<Vec<u8>> {
    let raw = fs::read(file_path).expect("read archive");
    let mut entries = Vec::new();
    let mut pos = 0usize;

    while pos + 4 <= raw.len() {
        let len = u32::from_le_bytes(
            raw[pos..pos + 4].try_into().unwrap()
        ) as usize;
        pos += 4;

        if pos + len > raw.len() { break; }

        entries.push(raw[pos..pos + len].to_vec());
        pos += len;
    }

    entries
}
```

### 5.2 Stream-Decode Each Entry

```rust
use tson::TsonStreamReader;

fn compute_average_temperature(archive_path: &str) -> f64 {
    let entries = read_archive(archive_path);
    let mut total_temp = 0.0;
    let mut count = 0u64;

    for entry_bytes in &entries {
        let mut reader = TsonStreamReader::new(entry_bytes).unwrap();
        let defs = reader.definitions();
        let dict = reader.dict();

        // Pre-compute field indices for "temp" (avoids string lookup per entry)
        let temp_field_index = defs.iter()
            .position(|d| {
                d.fields.as_ref().map_or(false, |f| {
                    f.iter().any(|(name, _)| name == "temp")
                })
            })
            .unwrap_or(usize::MAX);

        for result in reader {
            let chunk = result.unwrap();

            match &chunk.data {
                tson::TsonData::Object(def_idx, fields) => {
                    let def = &defs[*def_idx as usize];
                    if let Some(fields_spec) = &def.fields {
                        for (i, (fname, _)) in fields_spec.iter().enumerate() {
                            if fname == "temp" {
                                if let tson::TsonData::Float(f) = &fields[i] {
                                    total_temp += *f as f64;
                                    count += 1;
                                }
                            }
                        }
                    }
                }
                tson::TsonData::Array(_, _, items) => {
                    for item in items {
                        // Nested readings inside a batch
                        if let tson::TsonData::Object(def_idx, fields) = item {
                            // ... same field extraction
                        }
                    }
                }
                _ => {}
            }
        }
    }

    if count > 0 { total_temp / count as f64 } else { 0.0 }
}
```

### 5.3 Filter Alarms

```rust
fn find_alarm_events(archive_path: &str) -> Vec<String> {
    let entries = read_archive(archive_path);
    let mut alarms = Vec::new();

    for entry_bytes in &entries {
        let mut reader = TsonStreamReader::new(entry_bytes).unwrap();
        let defs = reader.definitions().to_vec();
        let dict = reader.dict().to_vec();

        for result in reader {
            let chunk = result.unwrap();
            let json = tson::decompile_to_value(
                &tson::TsonDocument {
                    header: reader.header().clone(),
                    definitions: defs.clone(),
                    dict: dict.clone(),
                    data: vec![chunk],
                }
            ).unwrap();

            // Now you can query with familiar JSON-like access
            if let Some(readings) = json.get("readings").and_then(|r| r.as_array()) {
                for reading in readings {
                    if let Some(temp) = reading.get("temp").and_then(|t| t.as_f64()) {
                        if temp > 35.0 {
                            alarms.push(format!(
                                "ALARM: temp={temp} at ts={:?}",
                                reading.get("ts")
                            ));
                        }
                    }
                }
            }
        }
    }

    alarms
}
```

---

## 6. Example: The Full Round-Trip in One Script

```rust
//! sensor-pipeline.rs — Daily temperature report
//!
//! Reads the rolling TSON archive, extracts all temperature readings,
//! computes hourly averages, and writes a summary CSV.

fn main() {
    let archive_entries = read_archive("./sensor-archive/archive-000.tson");
    let mut hourly_temps: Vec<Vec<f64>> = vec![Vec::new(); 24];

    for entry_bytes in &archive_entries {
        let mut reader = TsonStreamReader::new(entry_bytes).unwrap();
        for result in reader {
            let chunk = result.unwrap();
            let json = decompile_chunk_to_json(&chunk, &reader);
            // extract hour and temp...
        }
    }

    // Write CSV summary
    println!("Hour,AvgTemp,MinTemp,MaxTemp,Count");
    for (hour, temps) in hourly_temps.iter().enumerate() {
        if temps.is_empty() { continue; }
        let avg: f64 = temps.iter().sum::<f64>() / temps.len() as f64;
        let min = temps.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = temps.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        println!("{hour},{avg:.1},{min:.1},{max:.1},{}", temps.len());
    }
}
```

---

## 7. Key Take-Aways

1. **JSON is the wire format** for external systems.
   TSON is the **internal format** for storage, transport, and replay.

2. **Compilation happens on ingestion**. The cost of schema discovery and string interning is paid once, then
   all downstream processing benefits from cheaper decoding.

3. **Streaming reads avoid OOM**. Even for archives containing millions of entries, the streaming reader processes one entry at a time.

4. **Schema dedup with the dict** combined can shrink repetitive data by 70%.
   This makes TSON especially suitable for telemetry, API responses, and log files.

5. **No schema files needed**. Unlike Protobuf, TSON discovers the schema from data — you can start using
   it with existing JSON APIs without pre-declaring types.

6. **Zero-copy strings**. When a string is repeated hundreds of times (like "temperature" in sensor data), it is stored once in the dict block.  All data entries
   reference it via `StrRef`.  This saves both memory and wire bandwidth.

---

## 8. Performance (Perspective)

On a server receiving 500 sensor readings per second, the time to compile each reading to TSON is ~15–20 microseconds (release build, users‑t1 scale).  This is fast enough to run inline, on every event, with no batching required.  The resulting binary is about 30–40% the size of the original JSON, which reduces disk I/O and bandwidth proportionally.
