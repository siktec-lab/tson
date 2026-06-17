fn main() {
    use std::fs;
    use std::io::Write;

    // Telemetry file: 500 sensor readings with repeating structure
    let mut f = fs::File::create("examples/telemetry.json").unwrap();
    writeln!(f, "{{").unwrap();
    writeln!(f, r#"  "device_id": "sensor-array-001","#).unwrap();
    writeln!(f, r#"  "firmware": "v2.4.1","#).unwrap();
    writeln!(f, r#"  "location": {{"lat":37.7749,"lon":-122.4194,"alt":15.2}},"#).unwrap();
    writeln!(f, r#"  "calibration": {{"temp_offset":0.1,"humidity_offset":-1.0,"last_cal":"2026-05-01T08:00:00Z"}},"#).unwrap();
    writeln!(f, r#"  "readings": ["#).unwrap();

    for i in 0..500 {
        let temp = 20.0 + 15.0 * ((i as f64 * 0.7).sin() + 1.0) / 2.0;
        let hum = 50.0 + 20.0 * ((i as f64 * 0.3).cos());
        let batt = 3.3 + 0.4 * ((i as f64 * 0.1 + std::f64::consts::PI).sin() + 1.0) / 2.0;
        let status = if i > 480 { "critical" } else if temp > 33.0 { "warning" } else { "nominal" };
        let comma = if i < 499 { "," } else { "" };
        writeln!(f, r#"    {{"ts":"2026-06-15T00:00:{:02}Z","temp":{:.1},"humidity":{:.0},"pressure":{},"battery":{:.2},"status":"{}"}}{}"#,
            i % 60, (temp * 10.0).round() / 10.0, hum.round(),
            1000 + (i % 40), ((batt * 100.0).round() / 100.0), status, comma
        ).unwrap();
    }

    writeln!(f, "  ]").unwrap();
    writeln!(f, "}}").unwrap();

    // Config file: deeply nested with 200 routing rules
    let mut f = fs::File::create("examples/config.json").unwrap();
    writeln!(f, "{{").unwrap();
    writeln!(f, r#"  "app": {{"name":"EdgeGateway","version":"3.2.1","features":["cache","compression","auto-retry","rate-limit"],"limits":{{"max_connections":1000,"timeout_ms":5000,"retry_count":3}},"endpoints":"#).unwrap();
    writeln!(f, r#"    [{{"path":"/api/v1/data","method":"POST","auth":true}},{{"path":"/api/v1/status","method":"GET","auth":false}},{{"path":"/api/v1/config","method":"PUT","auth":true}},{{"path":"/api/v1/metrics","method":"GET","auth":true}}]"#).unwrap();
    writeln!(f, r#"  }},"#).unwrap();
    writeln!(f, r#"  "ssl": {{"enabled":true,"cert_path":"/etc/ssl/cert.pem","key_path":"/etc/ssl/key.pem","min_version":"TLSv1.2"}},"#).unwrap();
    writeln!(f, r#"  "logging": {{"level":"info","destinations":["stdout","syslog","elasticsearch"],"format":"json","retention_days":30}},"#).unwrap();
    writeln!(f, r#"  "rules": ["#).unwrap();

    let sources = ["sensor", "api", "mqtt", "websocket"];
    let targets = ["storage", "alerts", "dashboard", "ml-engine"];
    let ops = ["gt", "lt", "eq"];
    let transforms = ["pass", "scale", "normalize", "aggregate"];
    let fields = ["temp", "humidity", "pressure", "battery"];

    for i in 0..200 {
        let comma = if i < 199 { "," } else { "" };
        writeln!(f, r#"    {{"id":{},"source":"{}","target":"{}","filter":{{"field":"{}","op":"{}","value":{}}},"transform":"{}","active":{}}}{}"#,
            i,
            sources[i % 4], targets[(i + 1) % 4],
            fields[i % 4], ops[i % 3], 100 + (i * 7) % 1000,
            transforms[i % 4], i < 180, comma
        ).unwrap();
    }

    writeln!(f, "  ]").unwrap();
    writeln!(f, "}}").unwrap();

    println!("Generated telemetry.json and config.json");
}
