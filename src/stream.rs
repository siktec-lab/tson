use alloc::vec::Vec;
use crate::decode;
use crate::error::TsonError;
use crate::structure::*;

/// A streaming TSON reader that yields data entries one at a time.
///
/// Memory model:
/// - The **definition block** is fully materialized once (small - usually
///   tens to hundreds of bytes).
/// - The **data block** is scanned entry-by-entry; only one entry is held
///   at a time.
///
/// Usage:
/// ```ignore
/// let mut reader = TsonStreamReader::new(bytes)?;
/// println!("Definitions: {}", reader.definitions().len());
/// for result in reader {
///     let chunk = result?;
///     // process one entry...
/// }
/// ```
pub struct TsonStreamReader<'a> {
    /// Parsed definitions - kept alive so each entry can reference its schema.
    pub(crate) definitions: Vec<TsonDefinition>,
    /// Parsed string interning table.
    #[allow(dead_code)]
    pub(crate) dict: Vec<String>,
    /// The header for this document.
    pub(crate) header: TsonHeader,
    /// Remaining data-bytes to scan.
    data_slice: &'a [u8],
    /// Total entries declared in the data block header.
    total_entries: u32,
    /// How many entries we have yielded so far.
    yielded: u32,
}

impl<'a> TsonStreamReader<'a> {
    /// Parse the header and definition block from a TSON byte slice, then
    /// prepare for streaming data entries.
    ///
    /// The definitions *are* loaded eagerly (they are small and essential for
    /// interpreting each entry). Only the data entries are streamed lazily.
    pub fn new(bytes: &'a [u8]) -> Result<Self, TsonError> {
        let header = TsonHeader::from_bytes(bytes)?;
        header.validate()?;

        let def_off = header.blk_definition as usize;
        let dict_off = header.blk_dict as usize;
        let data_off = header.blk_data as usize;

        if def_off > bytes.len() {
            return Err(TsonError::ParseError(format!(
                "Definition block offset {} exceeds buffer length {}",
                def_off, bytes.len()
            )));
        }
        if dict_off > bytes.len() {
            return Err(TsonError::ParseError(format!(
                "Dict block offset {} exceeds buffer length {}",
                dict_off, bytes.len()
            )));
        }
        if data_off > bytes.len() {
            return Err(TsonError::ParseError(format!(
                "Data block offset {} exceeds buffer length {}",
                data_off, bytes.len()
            )));
        }

        let definitions = decode::decode_definitions(&bytes[def_off..dict_off])?;
        let dict = decode::decode_dict_slice(&bytes[dict_off..data_off])?;

        // The data slice starts at data_off. We need to read entry_count
        // (u32 LE) before we start yielding entries.
        let data_bytes = &bytes[data_off..];
        if data_bytes.len() < 4 {
            return Err(TsonError::ParseError(format!(
                "Data block too short: {} bytes, need 4 for entry count",
                data_bytes.len()
            )));
        }
        let total_entries = u32::from_le_bytes(data_bytes[0..4].try_into().unwrap());

        Ok(Self {
            definitions,
            dict,
            header,
            data_slice: &data_bytes[4..],
            total_entries,
            yielded: 0,
        })
    }

    /// Get a reference to the parsed definitions (the schema).
    pub fn definitions(&self) -> &[TsonDefinition] {
        &self.definitions
    }

    /// Get a reference to the parsed string dict.
    #[allow(dead_code)]
    pub fn dict(&self) -> &[String] {
        &self.dict
    }

    /// Get a reference to the parsed header.
    pub fn header(&self) -> &TsonHeader {
        &self.header
    }

    /// Total number of entries declared in the data block header.
    #[allow(dead_code)]
    pub fn total_entries(&self) -> u32 {
        self.total_entries
    }

    /// Number of entries yielded so far.
    #[allow(dead_code)]
    pub fn yielded(&self) -> u32 {
        self.yielded
    }

    /// Read the next data entry from the stream, advancing the internal
    /// cursor. Returns `None` when no more entries remain.
    ///
    /// This is a lower-level alternative to the `Iterator` impl.
    pub fn read_entry(&mut self) -> Option<Result<TsonChunk, TsonError>> {
        if self.yielded >= self.total_entries {
            return None;
        }

        // Each entry header is 6 bytes: def_index (u16) + payload_len (u32)
        if self.data_slice.len() < 6 {
            self.yielded = self.total_entries; // mark exhausted
            return Some(Err(TsonError::ParseError(format!(
                "Truncated data entry {}: need 6 bytes for header, got {}",
                self.yielded,
                self.data_slice.len()
            ))));
        }

        let def_index =
            u16::from_le_bytes(self.data_slice[0..2].try_into().unwrap());
        let payload_len =
            u32::from_le_bytes(self.data_slice[2..6].try_into().unwrap()) as usize;

        if 6 + payload_len > self.data_slice.len() {
            self.yielded = self.total_entries;
            return Some(Err(TsonError::ParseError(format!(
                "Truncated data entry {}: payload claims {} bytes, but only {} remain",
                self.yielded,
                payload_len,
                self.data_slice.len() - 6
            ))));
        }

        let payload = &self.data_slice[6..6 + payload_len];

        // Decode the value using the root-value decoder from decode.rs
        let result = decode::decode_root_value(payload, def_index, &self.definitions, 0);

        match result {
            Ok((data, consumed)) => {
                if consumed != payload_len {
                    self.yielded = self.total_entries;
                    return Some(Err(TsonError::ParseError(format!(
                        "Entry {} payload mismatch: consumed {} of {} bytes",
                        self.yielded, consumed, payload_len
                    ))));
                }
                self.data_slice = &self.data_slice[6 + payload_len..];
                self.yielded += 1;
                Some(Ok(TsonChunk {
                    definition_index: def_index,
                    data,
                }))
            }
            Err(e) => {
                self.yielded = self.total_entries;
                Some(Err(e))
            }
        }
    }
}

/// Iterate over data entries one at a time.
///
/// Each call to `next()` decodes and yields exactly one entry without
/// allocating space for all entries.
impl<'a> Iterator for TsonStreamReader<'a> {
    type Item = Result<TsonChunk, TsonError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.read_entry()
    }
}

// Standalone streaming convenience

/// Parse a TSON byte slice, returning the header and definitions immediately
/// for inspection, and a `TsonStreamReader` for iterating data entries.
///
/// This is useful when the caller wants to inspect the schema before reading
/// entries.
#[allow(dead_code)]
pub fn open_stream(bytes: &[u8]) -> Result<TsonStreamReader<'_>, TsonError> {
    TsonStreamReader::new(bytes)
}

// Multi-Document Reader

#[cfg(feature = "std")]
pub mod multi_doc {
    //! Multi-document reader (requires `std` feature for `io::Read`).
    use crate::prelude::*;
    use crate::error::TsonError;
    use crate::structure::TsonDocument;
    use crate::decode;

    /// Reads length-prefixed TSON documents from a byte source.
    pub struct TsonDocReader<R: std::io::Read> {
        source: R,
        buf: Vec<u8>,
    }

    impl<R: std::io::Read> TsonDocReader<R> {
        pub fn new(source: R) -> Self {
            TsonDocReader { source, buf: Vec::with_capacity(4096) }
        }

        pub fn read_next(&mut self) -> Result<Option<TsonDocument>, TsonError> {
            let mut len_buf = [0u8; 4];
            let n = self.source.read(&mut len_buf)
                .map_err(|e| TsonError::IoError(e))?;
            if n == 0 { return Ok(None); }
            if n < 4 {
                return Err(TsonError::ParseError(
                    "Truncated length prefix in TSON document stream".into(),
                ));
            }
            let len = u32::from_le_bytes(len_buf) as usize;
            self.buf.resize(len, 0u8);
            self.source.read_exact(&mut self.buf)
                .map_err(|e| TsonError::IoError(e))?;
            let doc = decode::decode_document(&self.buf)?;
            Ok(Some(doc))
        }
    }

    impl<R: std::io::Read> Iterator for TsonDocReader<R> {
        type Item = Result<TsonDocument, TsonError>;
        fn next(&mut self) -> Option<Self::Item> {
            match self.read_next() {
                Ok(Some(doc)) => Some(Ok(doc)),
                Ok(None) => None,
                Err(e) => Some(Err(e)),
            }
        }
    }
}

#[cfg(feature = "std")]
pub use multi_doc::TsonDocReader;
