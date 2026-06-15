use thiserror::Error;

#[derive(Error, Debug)]
pub enum TsonError {
    #[error("Failed to parse TSON: {0}")]
    ParseError(String),
    #[error("Unsupported data type: {0}")]
    UnsupportedType(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}