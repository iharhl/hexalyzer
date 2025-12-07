//! The 'error' module defines the ['IntelHexError'] struct that describes the errors that
//! can occur when parsing Intel HEX files via ['IntelHex'].
//! It contains the three pieces of information:
//! 1. When the error occurs, e.g., during parsing or creating the record.
//! 2. What kind of error was encountered (via ['IntelHexErrorKind'] struct).
//! 3. What is the line number (if applicable), e.g., at which line in a hex file the parsing failed.

use crate::record::RecordType;
use std::error::Error;
use std::fmt;

#[derive(Debug, PartialEq)]
pub enum IntelHexError {
    ParseRecordError(IntelHexErrorKind, usize),
    CreateRecordError(IntelHexErrorKind),
    SetterError(IntelHexErrorKind),
}

impl fmt::Display for IntelHexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IntelHexError::ParseRecordError(base_err, line) => {
                write!(
                    f,
                    "Error encountered during record parsing at line #{line} of the hex file:\n{}",
                    base_err.to_string(),
                )
            }
            IntelHexError::CreateRecordError(base_err) => {
                write!(
                    f, "Error encountered during creation of hex record:\n{}",
                    base_err.to_string(),
                )
            }
            IntelHexError::SetterError(base_err) => {
                write!(
                    f,
                    "Error encountered during setting private field of IntelHex struct:\n{}",
                    base_err.to_string(),
                )
            }
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum IntelHexErrorKind {
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
    /// TBD
    RecordNotSupported,
    /// Record length is odd
    RecordNotEvenLength,
    /// Record checksum mismatch
    RecordChecksumMismatch(u8, u8),
    /// Invalid length of data bytes
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

impl fmt::Display for IntelHexErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IntelHexErrorKind::MissingStartCode => {
                write!(f, "Missing start code ':'")
            }
            IntelHexErrorKind::ContainsInvalidCharacters => {
                write!(f, "Record contains invalid character(s)")
            }
            IntelHexErrorKind::RecordTooShort => {
                write!(f, "Record too short")
            }
            IntelHexErrorKind::RecordTooLong => {
                write!(f, "Record too long")
            }
            IntelHexErrorKind::RecordLengthInvalidForType(rtype, expected, actual) => {
                write!(
                    f,
                    "For record type {rtype:?} expected data length is {expected} bytes, found {actual}"
                )
            }
            IntelHexErrorKind::RecordAddressInvalidForType(rtype, expected, actual) => {
                write!(
                    f,
                    "For record type {rtype:?} expected address is {expected}, found {actual}"
                )
            }
            IntelHexErrorKind::RecordAddressOverlap(address) => {
                write!(f, "Encountered duplicate address {address}")
            }
            IntelHexErrorKind::InvalidRecordType => {
                write!(f, "Invalid record type")
            }
            IntelHexErrorKind::RecordChecksumMismatch(expected, actual) => {
                write!(
                    f,
                    "Invalid record checksum - expected: {expected}, found: {actual}"
                )
            }
            IntelHexErrorKind::RecordInvalidPayloadLength => {
                write!(f, "Payload (data bytes) size differs from record's lengths")
            }
            IntelHexErrorKind::RecordNotEvenLength => {
                write!(f, "Record with uneven length")
            }
            IntelHexErrorKind::RecordNotSupported => {
                write!(f, "Record not supported")
            }
            IntelHexErrorKind::InvalidAddress(address) => {
                write!(f, "No data found at address {address}")
            }
            IntelHexErrorKind::DuplicateStartAddress => {
                write!(f, "Encountered second start address record")
            }
        }
    }
}

impl Error for IntelHexError {}
impl Error for IntelHexErrorKind {}
