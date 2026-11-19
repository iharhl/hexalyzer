//! # `intelhexlib`
//!
//! `intelhexlib` is a Rust library for parsing, validating, and working with Intel HEX files.
//!
//! The library provides:
//! - Parser for Intel HEX files (via [`IntelHex`] struct).
//! - Error handling with [`IntelHexError`].
//! - Easy access to hex data for its reading and modification.
//!
//! <div class="warning">
//!
//! `intelhexlib` is still maturing and is not yet production ready. It is tested with
//! unit, integration and doc tests, but it has not been fully verified against every
//! Intel HEX variant and toolchain, and its API may change between `0.x` releases.
//! Bug reports and feedback are very welcome on the
//! [issue tracker](https://github.com/iharhl/hexalyzer/issues).
//!
//! </div>
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
pub use intelhex::{IntelHex, IterRange};
pub use record::{RecordType, StartAddress};

// Compile and run the Rust code blocks of the crate README as doctests, so the usage
// example stays in sync with the API. Only compiled when doctests run.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
