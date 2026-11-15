//! The `error` module defines the [`IntelHexError`] struct that describes the errors that
//! can occur when parsing, updating, or writing Intel HEX files via [`IntelHex`].
//! It may contain up to three types of information:
//! 1. When did the error occur, e.g., during parsing or creating the record.
//! 2. What kind of error was encountered (via [`IntelHexErrorKind`] struct).
//! 3. What is the line number (at which line in a hex file the parsing failed).

use crate::record::RecordType;
use std::error::Error;
use std::fmt;
use std::io;
use std::path::PathBuf;

/// New variants may be added in future versions
#[derive(Debug)]
#[non_exhaustive]
pub enum IntelHexError {
    ParseRecordError(IntelHexErrorKind, usize),
    CreateRecordError(IntelHexErrorKind),
    UpdateError(IntelHexErrorKind),
    SearchError(IntelHexErrorKind),
    WriteError(IntelHexErrorKind),
    Io(io::Error),
}

impl PartialEq for IntelHexError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::ParseRecordError(a, la), Self::ParseRecordError(b, lb)) => a == b && la == lb,
            (Self::CreateRecordError(a), Self::CreateRecordError(b))
            | (Self::UpdateError(a), Self::UpdateError(b))
            | (Self::SearchError(a), Self::SearchError(b))
            | (Self::WriteError(a), Self::WriteError(b)) => a == b,
            (Self::Io(a), Self::Io(b)) => a.kind() == b.kind(),
            _ => false,
        }
    }
}

impl Eq for IntelHexError {}

impl fmt::Display for IntelHexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // Error on line 0 means the error is not tied to a line (e.g. an empty file)
            Self::ParseRecordError(base_err, 0) => {
                write!(f, "Error encountered during record parsing:\n{base_err}")
            }
            Self::ParseRecordError(base_err, line) => {
                write!(
                    f,
                    "Error encountered during record parsing at line #{line} of the hex file:\n{base_err}",
                )
            }
            Self::CreateRecordError(base_err) => {
                write!(
                    f,
                    "Error encountered during creation of hex record:\n{base_err}",
                )
            }
            Self::UpdateError(base_err) => {
                write!(
                    f,
                    "Error encountered during update of IntelHex instance:\n{base_err}",
                )
            }
            Self::SearchError(base_err) => {
                write!(f, "Error encountered during search:\n{base_err}")
            }
            Self::WriteError(base_err) => {
                write!(f, "Error encountered during file write:\n{base_err}")
            }
            Self::Io(err) => {
                write!(f, "I/O error: {err}")
            }
        }
    }
}

impl Error for IntelHexError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<io::Error> for IntelHexError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

/// New variants may be added in future versions
#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]
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
    /// Record is intentionally unsupported for creation (e.g., ESA - library emits ELA instead)
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
    /// Range start address is greater than its end address
    InvalidRange(usize, usize),
    /// Encountered second start address record
    DuplicateStartAddress,
    /// `IntelHex` instance has no data
    IntelHexInstanceEmpty,
    /// Address relocation failed due to overflow
    RelocateAddressOverflow(usize),
    /// Parsed address range exceeds the maximum supported (32-bit)
    AddressRangeOverflow,
    /// Hex file has no end-of-file record
    MissingEndOfFile,
    /// Search pattern is not a valid regular expression
    InvalidRegex(String),
    /// Safe (atomic) write impossible: the directory does not allow creating a temporary file
    DirectoryNotWritable(PathBuf),
    /// Safe (atomic) write impossible: the file has other hard links that would keep the old data
    FileHasHardLinks(u64),
    /// Target path is a symbolic link to a file that does not exist
    BrokenSymlink(PathBuf),
    /// Max payload size per record must be at least 1 byte
    InvalidMaxPayloadSize,
}

impl fmt::Display for IntelHexErrorKind {
    #[allow(clippy::too_many_lines)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingStartCode => {
                write!(f, "Missing start code ':'")
            }
            Self::ContainsInvalidCharacters => {
                write!(f, "Record contains invalid character(s)")
            }
            Self::RecordTooShort => {
                write!(f, "Record too short")
            }
            Self::RecordTooLong => {
                write!(f, "Record too long")
            }
            Self::RecordLengthInvalidForType(rtype, expected, actual) => {
                write!(
                    f,
                    "For record type {rtype:?} expected data length is {expected} bytes, found {actual}"
                )
            }
            Self::RecordAddressInvalidForType(rtype, expected, actual) => {
                write!(
                    f,
                    "For record type {rtype:?} expected address is 0x{expected:X}, found 0x{actual:X}"
                )
            }
            Self::RecordAddressOverlap(address) => {
                write!(f, "Encountered duplicate address: 0x{address:X}")
            }
            Self::InvalidRecordType => {
                write!(f, "Invalid record type")
            }
            Self::RecordChecksumMismatch(expected, actual) => {
                write!(
                    f,
                    "Invalid record checksum - expected: 0x{expected:02X}, found: 0x{actual:02X}"
                )
            }
            Self::RecordInvalidPayloadLength => {
                write!(f, "Payload (data bytes) size differs from record's lengths")
            }
            Self::RecordNotEvenLength => {
                write!(f, "Record with uneven length")
            }
            Self::RecordNotSupported => {
                write!(f, "Record not supported")
            }
            Self::InvalidAddress(address) => {
                write!(f, "No data found at address: 0x{address:X}")
            }
            Self::InvalidRange(start, end) => {
                write!(
                    f,
                    "Invalid range: start address 0x{start:X} is greater than end address 0x{end:X}"
                )
            }
            Self::DuplicateStartAddress => {
                write!(f, "Encountered second start address record")
            }
            Self::IntelHexInstanceEmpty => {
                write!(f, "IntelHex instance has no data")
            }
            Self::RelocateAddressOverflow(address) => {
                write!(
                    f,
                    "Address relocation failed due to overflow. Max allowed start address: 0x{address:X}"
                )
            }
            Self::AddressRangeOverflow => {
                write!(f, "Maximum address exceeds 32-bit range")
            }
            Self::MissingEndOfFile => {
                write!(f, "Missing end-of-file record (file may be truncated)")
            }
            Self::InvalidRegex(msg) => {
                write!(f, "Invalid regular expression: {msg}")
            }
            Self::DirectoryNotWritable(dir) => {
                write!(
                    f,
                    "Cannot save safely: folder '{}' is not writable (a temporary file \
                     is created there first). Save to another location instead.",
                    dir.display()
                )
            }
            Self::FileHasHardLinks(count) => {
                write!(
                    f,
                    "Cannot save safely: the file has {count} hard links and saving would \
                     detach this one from the others. Save to another location instead."
                )
            }
            Self::InvalidMaxPayloadSize => {
                write!(f, "Max payload size per record must be at least 1 byte")
            }
            Self::BrokenSymlink(target) => {
                write!(
                    f,
                    "The file is a symbolic link to '{}', which does not exist",
                    target.display()
                )
            }
        }
    }
}

impl Error for IntelHexErrorKind {}
