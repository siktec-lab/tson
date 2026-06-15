use alloc::string::String;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TsonError {
    #[error("Failed to parse TSON: {0}")]
    ParseError(String),
    #[allow(dead_code)]
    #[error("Unsupported data type: {0}")]
    UnsupportedType(String),
    /// Only available when the `std` feature is enabled.
    #[cfg(feature = "std")]
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}
