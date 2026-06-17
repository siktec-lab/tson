//! # TSON — Terse JSON Binary Format
//!
//! A compact binary format for JSON data, designed for microcontrollers and
//! constrained environments.
//!
//! ## Feature flags
//!
//! - `std` (default on) — enables `std::io`-based helpers and the `IoError`
//!   variant in `TsonError`.  When disabled, the library is `no_std` and only
//!   requires the `alloc` crate.
//! - `json` (default on) — enables JSON ↔ TSON compilation via `serde_json`.
//!
//! ## Quick start
//!
//! ```rust
//! # #[cfg(feature = "json")] {
//! let json = r#"{"name":"Alice","age":30}"#;
//! let doc = tson::compile_json(json).unwrap();
//! let bytes = tson::to_bytes(&doc).unwrap();
//! let restored = tson::from_bytes(&bytes).unwrap();
//! let value = tson::decompile_to_value(&restored).unwrap();
//! assert_eq!(value.to_string(), r#"{"age":30,"name":"Alice"}"#);
//! # }
//! ```

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

// ─── Module declarations (all at root level) ────────────────────────────────

pub mod error;
pub mod tson;

// Core modules — no_std compatible (only require alloc)
pub mod structure;
pub mod encode;
pub mod decode;
pub mod stream;

// JSON interop — requires serde_json (gated behind `json` feature)
#[cfg(feature = "json")]
pub mod compile;
#[cfg(feature = "json")]
pub mod decompile;

// ─── Root-level re-exports from `tson` module ─────────────────────────────

pub use tson::{
    TsonChunk, TsonData, TsonDefinition, TsonDocument, TsonHeader, TsonType,
    emit, emit_value, to_bytes, from_bytes, decode_definitions,
};
pub use stream::TsonStreamReader;

#[cfg(feature = "json")]
pub use tson::{
    compile_json, compile_json_file, compile_value,
    decompile_to_value, decompile_tson_file,
};
