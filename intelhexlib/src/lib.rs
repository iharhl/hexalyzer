//! # `intelhexlib`
//!
//! `intelhexlib` is a Rust library for parsing, validating, and working with Intel HEX files.
//!
//! The library provides:
//! - Parser for Intel HEX files (via [`IntelHex`] struct).
//! - Error handling with [`IntelHexError`].
//! - Easy access to hex data for its reading and modification.
//!
//! ## Example
//!
//! ```
//! use intelhexlib::IntelHex;
//!
//! let mut ih = IntelHex::from_hex("tests/fixtures/ih_valid_1.hex").unwrap();
//! ih.write_hex("build/ex1/ih.hex");
//! ```

mod error;
mod intelhex;
mod record;
mod search;

// Public APIs
pub use error::{IntelHexError, IntelHexErrorKind};
pub use intelhex::IntelHex;
pub use record::RecordType;
