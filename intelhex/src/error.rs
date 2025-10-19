//! The 'error' module defines the ['IntelHexError'] struct which contains errors that
//! can occur when parsing Intel HEX files via ['IntelHex'].

use std::error::Error;
use std::fmt;
use crate::record::RecordType;


#[derive(Debug, PartialEq)]
pub enum IntelHexError {
    /// Record does not begin with a ':'
    MissingStartCode,
    /// Record contains non-hexadecimal characters
    ContainsInvalidCharacters,
    /// Record is shorter than the smallest valid
    RecordTooShort,
    /// Record is longer than the largest valid
    RecordTooLong,
    /// Record's payload length does not match the record type
    RecordLengthInvalidForType(RecordType, usize, usize),
    /// Record's address does not match the record type
    RecordAddressInvalidForType(RecordType, usize, usize),
    RecordNotSupported,
    /// Record length is odd
    RecordNotEvenLength,
    /// Record checksum mismatch
    RecordChecksumMismatch(u8, u8),
    /// Invalid payload length
    RecordInvalidPayloadLength,
    /// Encountered address that already contains data
    RecordAddressOverlap(usize),
    /// Provided record type does not exist
    InvalidRecordType,
    /// Provided address is invalid (e.g. does not hold any data)
    InvalidAddress(usize),
    /// Encountered second start address record
    DuplicateStartAddress,
}

impl fmt::Display for IntelHexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IntelHexError::MissingStartCode => write!(f, "Missing start code ':'"),
            IntelHexError::ContainsInvalidCharacters => write!(f, "Record contains invalid characters"),
            IntelHexError::RecordTooShort => write!(f, "Record too short"),
            IntelHexError::RecordTooLong => write!(f, "Record too long"),
            IntelHexError::RecordLengthInvalidForType(rtype, expected, actual) => {
                write!(f, "For record type {rtype:?} expected data length is {expected} bytes, encountered {actual}")
            }
            IntelHexError::RecordAddressInvalidForType(rtype, expected, actual) => {
                write!(f, "For record type {rtype:?} expected address is {expected}, encountered {actual}")
            }
            IntelHexError::RecordAddressOverlap(address) => {
                write!(f, "Encountered data at the address {address} already used by another record")
            }
            IntelHexError::InvalidRecordType => write!(f, "Invalid record type"),
            IntelHexError::RecordChecksumMismatch(expected, actual) => {
                write!(f, "Invalid record checksum, expected: {expected}, actual: {actual}")
            },
            IntelHexError::RecordInvalidPayloadLength => {
                write!(f, "Payload (data bytes) size differs from record's lengths")
            },
            IntelHexError::RecordNotEvenLength => {
                write!(f, "Record with uneven length")
            }
            IntelHexError::RecordNotSupported => write!(f, "Record not supported"),
            IntelHexError::InvalidAddress(address) => {
                write!(f, "No data found at address {address}")
            },
            IntelHexError::DuplicateStartAddress => {
                write!(f, "Encountered second start address")
            }
        }
    }
}

impl Error for IntelHexError {}
