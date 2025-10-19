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


#[derive(Debug, PartialEq)]
pub(crate) struct Record {
    pub(crate) length: u8,
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
    pub(crate) fn calculate_checksum(data: &[u8]) -> u8 {
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
            .unwrap(); // as hexdigit check is done above - assume safe unwrap

        // Check if record end is bigger than the record length itself
        let data_end =  ranges::RECORD_TYPE_RANGE.end + sizes::BYTE_CHAR_LEN * length as usize;
        let record_end = sizes::BYTE_CHAR_LEN + data_end; // last byte is checksum
        if record_end != line.len() {
            return Err(IntelHexError::RecordInvalidPayloadLength);
        }

        // Get record type
        let rtype = RecordType::parse(&line[ranges::RECORD_TYPE_RANGE])?;

        // Get record address
        let address = u16::from_str_radix(&line[ranges::RECORD_ADDR_RANGE], 16)
            .unwrap(); // as hexdigit check is done above - assume safe unwrap

        // More sanity checks (for length and address)
        match rtype {
            RecordType::EndOfFile => {
                if length != 0 {
                    return Err(IntelHexError::RecordLengthInvalidForType(rtype, 0, length as usize));
                }
            }
            RecordType::ExtendedSegmentAddress | RecordType::ExtendedLinearAddress => {
                if length != 2 {
                    return Err(IntelHexError::RecordLengthInvalidForType(rtype, 2, length as usize));
                }
            }
            RecordType::StartSegmentAddress | RecordType::StartLinearAddress => {
                if length != 4 {
                    return Err(IntelHexError::RecordLengthInvalidForType(rtype, 4, length as usize));
                }
            }
            _ => {}
        }
        if !matches!(rtype, RecordType::Data) && address != 0 {
            return Err(IntelHexError::RecordAddressInvalidForType(rtype, 0, address as usize));
        }

        // Get record data payload
        let mut data: Vec<u8> = Vec::with_capacity(length as usize);
        for i in (ranges::RECORD_TYPE_RANGE.end..data_end).step_by(sizes::BYTE_CHAR_LEN) {
            let byte = u8::from_str_radix(&line[i..i+sizes::BYTE_CHAR_LEN], 16)
                .unwrap(); // TODO: handler Err
            data.push(byte);
        }

        // Get checksum
        let checksum = u8::from_str_radix(&line[data_end..record_end], 16)
            .unwrap(); // TODO: handle Err

        // Validate checksum
        let _self = Self {
            length,
            address,
            rtype,
            data,
            checksum,
        };
        let calc_checksum = Self::calculate_checksum_from_self(&_self);
        if calc_checksum != checksum {
            return Err(IntelHexError::RecordChecksumMismatch(calc_checksum, checksum));
        }

        Ok(_self)
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    /// Returns valid instances of Record
    ///
    fn get_valid_struct_records() -> [Record; 5] {
        [
            Record {
                length: 0x10,
                address: 0x0100,
                rtype: RecordType::Data,
                data: vec![0x21, 0x46, 0x01, 0x36, 0x01, 0x21, 0x47, 0x01,
                           0x36, 0x00, 0x7E, 0xFE, 0x09, 0xD2, 0x19, 0x01],
                checksum: 0x40,
            },
            Record {
                length: 0x10,
                address: 0x0110,
                rtype: RecordType::Data,
                data: vec![0x21, 0x46, 0x01, 0x7E, 0x17, 0xC2, 0x00, 0x01,
                           0xFF, 0x5F, 0x16, 0x00, 0x21, 0x48, 0x01, 0x19],
                checksum: 0x28,
            },
            Record {
                length: 0x00,
                address: 0x0000,
                rtype: RecordType::EndOfFile,
                data: vec![],
                checksum: 0xFF,
            },
            Record {
                length: 0x02,
                address: 0x0000,
                rtype: RecordType::ExtendedSegmentAddress,
                data: vec![0x12, 0x00],
                checksum: 0xEA,
            },
            Record {
                length: 0x02,
                address: 0x0000,
                rtype: RecordType::ExtendedLinearAddress,
                data: vec![0x00, 0x03],
                checksum: 0xF7,
            },
        ]
    }

    /// Returns valid record strings
    ///
    fn get_valid_str_records() -> [&'static str; 5] {
        [
            ":10010000214601360121470136007EFE09D2190140",
            ":100110002146017E17C20001FF5F16002148011928",
            ":00000001FF",
            ":020000021200EA",
            ":020000040003F7",
        ]
    }

    /// Returns invalid record strings and corresponding errors
    ///
    fn get_invalid_str_records() -> [(&'static str, IntelHexError); 9] {
        [
            // Removed ':' from record str
            ("00000001FF", IntelHexError::MissingStartCode),
            // Payload shorter that record length byte
            (":100000000000FF", IntelHexError::RecordInvalidPayloadLength),
            // Payload longer that record length byte
            (":02000000000000FF", IntelHexError::RecordInvalidPayloadLength),
            // EOF record with fewer chars
            (":0000FF", IntelHexError::RecordTooShort),
            // EOF record with extra '0' added
            (":000000001FF", IntelHexError::RecordNotEvenLength),
            // Char 'Z' is not a hex digit
            (":0000000ZFF", IntelHexError::ContainsInvalidCharacters),
            // Checksum wrong - should be 0xF0
            (":1000000000000000000000000000000000000000AA",
             IntelHexError::RecordChecksumMismatch(0xF0, 0xAA)
            ),
            // Address non-zero for extended segment addr record
            (":020100021200EA",
             IntelHexError::RecordAddressInvalidForType(
                 RecordType::ExtendedSegmentAddress, 0, 0x0100
             )
            ),
            // Address non-zero for extended linear addr record
            (":020100041200EA",
             IntelHexError::RecordAddressInvalidForType(
                 RecordType::ExtendedLinearAddress, 0, 0x0100
             )
            ),
        ]
    }

    #[test]
    fn test_parse_valid_record_types() {
        assert_eq!(RecordType::parse("00"), Ok(RecordType::Data));
        assert_eq!(RecordType::parse("01"), Ok(RecordType::EndOfFile));
        assert_eq!(RecordType::parse("02"), Ok(RecordType::ExtendedSegmentAddress));
        assert_eq!(RecordType::parse("03"), Ok(RecordType::StartSegmentAddress));
        assert_eq!(RecordType::parse("04"), Ok(RecordType::ExtendedLinearAddress));
        assert_eq!(RecordType::parse("05"), Ok(RecordType::StartLinearAddress));
    }

    #[test]
    fn test_parse_invalid_record_type() {
        assert_eq!(RecordType::parse("0"), Err(IntelHexError::InvalidRecordType));
        assert_eq!(RecordType::parse("1"), Err(IntelHexError::InvalidRecordType));
        assert_eq!(RecordType::parse("06"), Err(IntelHexError::InvalidRecordType));
        assert_eq!(RecordType::parse("AB"), Err(IntelHexError::InvalidRecordType));
        assert_eq!(RecordType::parse("FF"), Err(IntelHexError::InvalidRecordType));
    }

    #[test]
    fn test_calculate_checksum() {
        // Each tuple = (record line, expected checksum)
        let cases = [
            (":10010000214601360121470136007EFE09D2190140", 0x40),
            (":100110002146017E17C20001FF5F16002148011928", 0x28),
            (":00000001FF", 0xFF),
            (":020000021200EA", 0xEA),
            (":020000040003F7", 0xF7),
        ];

        for (record, expected_checksum) in cases {
            // Strip information not used for checksum calculation
            let trimmed_record = &record[1..record.len() - 2];

            // Convert to byte Vec
            let bytes: Vec<u8> = (0..trimmed_record.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&trimmed_record[i..i + 2], 16).unwrap())
                .collect();

            assert_eq!(expected_checksum, Record::calculate_checksum(&bytes));
        }
    }

    #[test]
    fn test_calculate_checksum_from_self() {
        let records = get_valid_struct_records();
        for record in records {
            assert_eq!(record.checksum, Record::calculate_checksum_from_self(&record));
        }
    }

    #[test]
    fn test_parse_valid_records() {
        let records = get_valid_str_records();
        let expected_records = get_valid_struct_records();
        for (rec_str, rec) in records.iter().zip(expected_records.iter()) {
            assert_eq!(Record::parse(rec_str).unwrap(), *rec);
        }
    }

    #[test]
    fn test_parse_invalid_records() {
        let records_and_errors = get_invalid_str_records();
        for (record, expected_error) in records_and_errors {
            assert_eq!(Record::parse(record).unwrap_err(), expected_error);
        }
    }
}