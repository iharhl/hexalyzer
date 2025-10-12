//! The 'record' module defines the ['Record'] and ['RecordType'] which are used for parsing
//! (and generating) Intel HEX records.

use std::ops::Add;
use crate::error::IntelHexError;


mod ranges {
    use std::ops::Range;
    pub const RECORD_LEN_RANGE: Range<usize> = 1..3;
    pub const RECORD_ADDR_RANGE: Range<usize> = 3..7;
    pub const RECORD_TYPE_RANGE: Range<usize> = 7..9;
}
mod sizes {
    pub const BYTE_CHAR_LEN: usize = 2;
    pub const SMALLEST_RECORD: usize = (1 + 2 + 1 + 1) * 2; // len + addr + rtype + checksum
    pub const LARGEST_RECORD: usize = SMALLEST_RECORD + 255 * 2;
}


#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum RecordType {
    Data = 0x0,
    EndOfFile = 0x1,
    ExtendedSegmentAddress = 0x2,
    StartSegmentAddress = 0x3, // TODO: deprecate? Or allow to write with it?
    ExtendedLinearAddress = 0x4,
    StartLinearAddress = 0x5,
}

impl RecordType {
    fn parse(s: &str) -> Result<Self, IntelHexError> {
        match s {
            "00" => Ok(Self::Data),
            "01" => Ok(Self::EndOfFile),
            "02" => Ok(Self::ExtendedSegmentAddress),
            "03" => Ok(Self::StartSegmentAddress),
            "04" => Ok(Self::ExtendedLinearAddress),
            "05" => Ok(Self::StartLinearAddress),
            _ => Err(IntelHexError::InvalidRecordType),
        }
    }
}


#[derive(Debug)]
pub struct Record {
    length: u8,
    pub(crate) address: u16,
    pub(crate) rtype: RecordType,
    pub(crate) data: Vec<u8>,
    pub(crate) checksum: u8,
}


impl Record {
    /// Calculate checksum from the Record instance.
    ///
    pub(crate) fn calculate_checksum_from_self(&self) -> u8 {
        // Get length, address and record type byte data
        let length = self.length as usize;
        let addr_high_byte = (self.address >> 8) as usize;
        let addr_low_byte = (self.address & 0xFF) as usize;
        let rtype = self.rtype as usize;

        // Sum it up with data vector
        let mut sum: usize = length + addr_high_byte + addr_low_byte + rtype;

        for b in &self.data {
            sum = sum.add(*b as usize);
        }
        let checksum = (!sum as u8).wrapping_add(1); // two's complement
        checksum
    }

    /// Calculate checksum from u8 array.
    ///
    fn calculate_checksum(data: &[u8]) -> u8 {
        let mut sum: u8 = 0;
        for b in data {
            sum = sum.wrapping_add(*b);
        }
        let checksum = (!sum).wrapping_add(1); // two's complement
        checksum
    }

    /// Create the record string from address, type and data vector.
    ///
    pub(crate) fn create(address: u16, rtype: RecordType, data: &[u8]) -> Result<String, IntelHexError> {
        // Get length of payload data
        let length = data.len();

        // Create a vector of data for checksum calculation
        let mut v = vec![length as u8, (address >> 8) as u8, (address & 0xFF) as u8];
        v.extend_from_slice(&data);

        // Checksum
        let checksum = Self::calculate_checksum(&v);

        match rtype {
            RecordType::Data => {
                // Check for data length
                if length > u8::MAX as usize {
                    return Err(IntelHexError::RecordTooLong);
                }

                // Create record string
                let record = format!(
                    ":{:02X}{:04X}00{}{:02X}",
                    length,
                    address,
                    data.iter().map(|b| format!("{:02X}", b)).collect::<String>(),
                    checksum
                );

                Ok(record)
            }
            RecordType::EndOfFile => {
                Ok(String::from(":00000001FF"))
            }
            RecordType::ExtendedLinearAddress => {
                // Check for data length (has to be 1 byte)
                if length != 2 {
                    return Err(IntelHexError::RecordLengthInvalidForType(rtype, 2, length));
                }

                // Check for address (has to be 0x0)
                if address != 0 {
                    return Err(IntelHexError::RecordAddressInvalidForType(rtype, 0, address as usize));
                }

                // Create record string
                let record = format!(
                    ":{:02X}{:04X}00{}{:02X}",
                    length,
                    address,
                    data.iter().map(|b| format!("{:02X}", b)).collect::<String>(),
                    checksum
                );

                Ok(record)
            }
            RecordType::StartLinearAddress | RecordType::StartSegmentAddress => {
                // Check for data length
                if length != 4 {
                    return Err(IntelHexError::RecordLengthInvalidForType(rtype, 4, length));
                }

                // Check for address
                if address != 0 {
                    return Err(IntelHexError::RecordAddressInvalidForType(rtype, 0, address as usize));
                }

                // Create record string
                let record = format!(
                    ":{:02X}{:04X}{}{}{:02X}",
                    length,
                    address,
                    rtype as u8,
                    data.iter().map(|b| format!("{:02X}", b)).collect::<String>(),
                    checksum
                );

                Ok(record)
            }
            RecordType::ExtendedSegmentAddress => {
                Err(IntelHexError::RecordNotSupported)
            }
        }
    }

    /// Parse the record string into Record.
    ///
    pub(crate) fn parse(line: &str) -> Result<Self, IntelHexError> {
        // Check for start record
        if !line.starts_with(':') {
            return Err(IntelHexError::MissingStartCode);
        }

        let hexdigit_part = &line[1..];
        let hexdigit_part_len = hexdigit_part.len();

        // Validate all characters are hexadecimal
        if !hexdigit_part.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return Err(IntelHexError::ContainsInvalidCharacters);
        }

        // Validate record's size
        if hexdigit_part_len < sizes::SMALLEST_RECORD {
            return Err(IntelHexError::RecordTooShort);
        } else if hexdigit_part_len > sizes::LARGEST_RECORD {
            return Err(IntelHexError::RecordTooLong);
        } else if (hexdigit_part_len % 2) != 0 {
            return Err(IntelHexError::RecordNotEvenLength);
        }

        // Get record length
        let length = u8::from_str_radix(&line[ranges::RECORD_LEN_RANGE], 16)
            .unwrap(); // TODO: handle Err

        // Check if record end is bigger than the record length itself
        let data_end =  ranges::RECORD_TYPE_RANGE.end + sizes::BYTE_CHAR_LEN * length as usize;
        let record_end = sizes::BYTE_CHAR_LEN + data_end; // last byte is checksum
        if record_end > line.len() {
            return Err(IntelHexError::RecordInvalidChecksumLength);
        }

        // Get record type
        let rtype = RecordType::parse(&line[ranges::RECORD_TYPE_RANGE])?;

        // Get record address
        let address = u16::from_str_radix(&line[ranges::RECORD_ADDR_RANGE], 16)
            .unwrap(); // TODO: handle Err

        // TODO: add sanity checks (e.g. addr length)

        // Get record data payload
        let mut data: Vec<u8> = Vec::new();
        if rtype == RecordType::EndOfFile {
            if length != 0 {
                return Err(IntelHexError::RecordLengthInvalidForType(rtype, 0, length as usize));
            }
        } else {
            for i in (ranges::RECORD_TYPE_RANGE.end..data_end).step_by(sizes::BYTE_CHAR_LEN) {
                let byte = u8::from_str_radix(&line[i..i+sizes::BYTE_CHAR_LEN], 16)
                    .unwrap(); // TODO: handler Err
                data.push(byte);
            }
        }

        // Get checksum
        let checksum = u8::from_str_radix(&line[data_end..record_end], 16)
            .unwrap(); // TODO: handle Err

        // Validate checksum
        // TODO: ...

        Ok(Self {
            length,
            address,
            rtype,
            data,
            checksum,
        })
    }
}
