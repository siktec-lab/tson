//! Shared prelude for `no_std` / `std` compatibility.
//!
//! When the `std` feature is disabled, the crate uses `no_std` and
//! imports heap types from `alloc`.  When `std` is enabled, the
//! standard prelude provides these types directly.
//!
//! Core modules should `use crate::prelude::*;` instead of importing
//! `Vec`, `String`, `format!`, `Box`, `ToString`, etc. individually.

#[cfg(not(feature = "std"))]
pub use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};

#[cfg(feature = "std")]
pub use std::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
