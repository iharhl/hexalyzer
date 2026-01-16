//! The `intelhex` module provides the [`IntelHex`] struct, a high-level API for
//! managing Intel HEX data.
//!
//! It supports parsing HEX files into an internal sparse memory representation using
//! a `BTreeMap`, allowing for efficient manipulation of non-contiguous data blocks.
//! The module also provides utilities for binary file interop, memory relocation,
//! and generating valid Intel HEX output with configurable record sizes.

use crate::error::{IntelHexError, IntelHexErrorKind};
use crate::record::{Record, RecordType};
use std::collections::BTreeMap;
use std::error::Error;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct IntelHex {
    /// Intel HEX file path
    pub filepath: PathBuf,
    /// Intel HEX file size in bytes
    pub size: usize,
    /// Start address of the Intel HEX file (stores full record as a byte slice)
    pub start_addr: Option<[u8; 10]>,
    /// Maximum payload size for data records
    max_payload_size: usize,
    /// Offset of the linear address segment
    offset: usize,
    /// Data buffer of the Intel HEX file
    buffer: BTreeMap<usize, u8>,
}

impl Default for IntelHex {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> IntoIterator for &'a IntelHex {
    type Item = (&'a usize, &'a u8);
    type IntoIter = std::collections::btree_map::Iter<'a, usize, u8>;
    fn into_iter(self) -> Self::IntoIter {
        self.buffer.iter()
    }
}

impl IntelHex {
    /// Creates empty `IntelHex` struct instance.
    ///
    /// # Examples
    /// ```
    /// use intelhexlib::IntelHex;
    ///
    /// let ih = IntelHex::new();
    /// assert_eq!(ih.size, 0);
    /// ```
    #[must_use]
    pub const fn new() -> Self {
        Self {
            filepath: PathBuf::new(),
            size: 0,
            offset: 0,
            max_payload_size: 16,
            start_addr: None,
            buffer: BTreeMap::new(),
        }
    }

    /// Clears loaded data from the `IntelHex` struct instance.
    ///
    /// # Examples
    /// ```
    /// use intelhexlib::IntelHex;
    ///
    /// let mut ih = IntelHex::from_hex("tests/fixtures/ih_valid_1.hex").unwrap();
    /// assert_ne!(ih.size, 0);
    ///
    /// ih.clear();
    /// assert_eq!(ih.size, 0);
    /// ```
    pub fn clear(&mut self) {
        self.filepath.clear();
        self.size = 0;
        self.start_addr = None;
        self.offset = 0;
        self.buffer.clear();
    }

    /// Parse the raw contents of the hex file and fill internal record vector.
    ///
    /// # Errors
    /// - Returns an error if the record is corrupted
    /// - Returns an error if there is an issue during filling the internal buffer
    ///
    pub fn parse(&mut self, raw_bytes: &[u8]) -> Result<(), IntelHexError> {
        let mut count: usize = 0;

        // Iterate over lines of records
        for line in raw_bytes.split(|&b| b == b'\n') {
            let line = line.strip_suffix(b"\r").unwrap_or(line);

            if line.is_empty() {
                continue;
            }

            count += 1;

            let record =
                Record::parse(line).map_err(|err| IntelHexError::ParseRecordError(err, count))?;

            // Fill in self
            match record.rtype {
                RecordType::Data => {
                    let mut addr = record.address as usize + self.offset;
                    for byte in &record.data {
                        if self.buffer.insert(addr, *byte).is_some() {
                            // Address overlap
                            return Err(IntelHexError::ParseRecordError(
                                IntelHexErrorKind::RecordAddressOverlap(addr),
                                count + 1,
                            ));
                        }
                        addr += 1;
                    }
                }
                RecordType::EndOfFile => {}
                RecordType::ExtendedSegmentAddress => {
                    self.offset = (record.data[0] as usize * 256 + record.data[1] as usize) * 16;
                }
                RecordType::ExtendedLinearAddress => {
                    self.offset = (record.data[0] as usize * 256 + record.data[1] as usize) * 65536;
                }
                RecordType::StartSegmentAddress | RecordType::StartLinearAddress => {
                    if self.start_addr.is_some() {
                        return Err(IntelHexError::ParseRecordError(
                            IntelHexErrorKind::DuplicateStartAddress,
                            count + 1,
                        ));
                    }
                    // Directly store the record slice.
                    // Error cases are not checked here as it was done during record parsing.
                    // TODO: split legacy and modern way of specifying start address?
                    if line.len() == 10
                        && let Ok(bytes) = line[1..=10].try_into()
                    {
                        self.start_addr = Some(bytes);
                    }
                }
            }
        }
        Ok(())
    }

    /// Creates an `IntelHex` instance and fills it with data from the provided hex file.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or parsed.
    ///
    /// # Example
    /// ```
    /// use intelhexlib::IntelHex;
    ///
    /// let ih = IntelHex::from_hex("tests/fixtures/ih_valid_1.hex").unwrap();
    /// assert_eq!(ih.size, 239);
    /// ```
    pub fn from_hex<P: AsRef<Path>>(filepath: P) -> Result<Self, Box<dyn Error>> {
        let mut ih = Self::new();
        ih.load_hex(filepath)?;
        Ok(ih)
    }

    /// Fills an `IntelHex` instance with data from the provided hex file.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or parsed.
    ///
    /// # Example
    /// ```
    /// use intelhexlib::IntelHex;
    ///
    /// let mut ih = IntelHex::new();
    /// ih.load_hex("tests/fixtures/ih_valid_1.hex").unwrap();
    ///
    /// assert_eq!(ih.size, 239);
    /// ```
    pub fn load_hex<P: AsRef<Path>>(&mut self, filepath: P) -> Result<(), Box<dyn Error>> {
        // Read the contents of the file
        let raw_bytes = std::fs::read(&filepath)?;

        // Clear provided IntelHex instance
        self.clear();

        // Compute the size (in bytes)
        self.size = raw_bytes.len();

        // Load filepath
        self.filepath = filepath.as_ref().to_path_buf();

        // Parse contents
        self.parse(&raw_bytes)?;

        Ok(())
    }

    /// Creates an `IntelHex` instance and fills it with data from the provided binary.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or parsed.
    ///
    /// # Example
    /// ```
    /// use intelhexlib::IntelHex;
    ///
    /// let base_addr = 0x1000;
    /// let ih = IntelHex::from_bin("tests/fixtures/ih_valid_1.bin", base_addr).unwrap();
    ///
    /// assert_eq!(ih.size, 51596);
    /// ```
    pub fn from_bin<P: AsRef<Path>>(
        filepath: P,
        base_address: usize,
    ) -> Result<Self, Box<dyn Error>> {
        let mut ih = Self::new();
        ih.load_bin(filepath, base_address)?;
        Ok(ih)
    }

    /// Fills an `IntelHex` instance with data from the provided binary.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or parsed.
    ///
    /// # Example
    /// ```
    /// use intelhexlib::IntelHex;
    ///
    /// let mut ih = IntelHex::new();
    /// let base_addr = 0x1000;
    /// ih.load_bin("tests/fixtures/ih_valid_1.bin", base_addr).unwrap();
    ///
    /// assert_eq!(ih.size, 51596);
    /// ```
    pub fn load_bin<P: AsRef<Path>>(
        &mut self,
        filepath: P,
        base_address: usize,
    ) -> Result<(), Box<dyn Error>> {
        // Read contents of the file. Bin only contains data bytes, thus read as Vec<u8>.
        let data = std::fs::read(&filepath)?;

        // Clear provided IntelHex instance
        self.clear();

        // Compute the size (in bytes)
        self.size = data.len();

        // Load filepath
        self.filepath = filepath.as_ref().to_path_buf();

        // Load data bytes into the map and return
        self.buffer.extend(
            data.into_iter()
                .enumerate()
                .map(|(i, byte)| (base_address + i, byte)),
        );
        Ok(())
    }

    #[allow(clippy::cast_possible_truncation)]
    /// Generates an Intel HEX file at the specified path.
    ///
    /// # Errors
    /// Returns an error if the file cannot be written.
    ///
    /// # Example
    /// ```
    /// use intelhexlib::IntelHex;
    ///
    /// let mut ih = IntelHex::from_hex("tests/fixtures/ih_valid_1.hex").unwrap();
    /// ih.write_hex("build/ex1/ih.hex");
    ///
    /// assert_eq!(ih.size, 239);
    /// ```
    pub fn write_hex<P: AsRef<Path>>(&mut self, filepath: P) -> Result<(), Box<dyn Error>> {
        // Ensure the parent directory exists
        if let Some(parent) = filepath.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(filepath)?;

        // Wrap in BufWriter for efficient line-by-line writing
        let mut writer = std::io::BufWriter::new(file);

        // Write start address record (raw bytes + newline)
        if let Some(s) = self.start_addr {
            writer.write_all(&s)?;
            writeln!(writer)?;
        }

        let mut curr_high_addr = 0;
        let mut chunk_start: Option<u16> = None;
        let mut prev_addr: Option<usize> = None;
        let mut chunk_data = Vec::with_capacity(self.max_payload_size);

        for (addr, byte) in &self.buffer {
            // Split address into low and high
            let high_addr = (addr >> 16) as u16;
            let low_addr = (addr & 0xFFFF) as u16;

            // If ELA segment changed -> flush current chunk and emit ELA
            if curr_high_addr != high_addr
                && let Some(start) = chunk_start
            {
                // Write data record
                let record = Record::create(start, RecordType::Data, &chunk_data)?;
                writeln!(writer, "{record}")?;

                // Write ELA record
                let msb = (high_addr >> 8) as u8;
                let lsb = (high_addr & 0xFF) as u8;
                let record = Record::create(0, RecordType::ExtendedLinearAddress, &[msb, lsb])?;
                writeln!(writer, "{record}")?;

                // Update segment's current address
                curr_high_addr = high_addr;

                // Clean up
                chunk_data.clear();
                chunk_start = None;
                prev_addr = None; // resets continuity check
            }

            // If gap detected or chunk full -> flush
            if let Some(prev) = prev_addr
                && ((*addr != prev + 1) || chunk_data.len() >= self.max_payload_size)
            {
                // Write data record
                let record = Record::create(
                    chunk_start.unwrap_or_default(),
                    RecordType::Data,
                    &chunk_data,
                )?;
                writeln!(writer, "{record}")?;

                // Clean up
                chunk_data.clear();
                chunk_start = None;
            }

            // Start new chunk if empty
            if chunk_start.is_none() {
                chunk_start = Some(low_addr);
            }

            // Push byte into data chunk
            chunk_data.push(*byte);

            // Update address
            prev_addr = Some(*addr);
        }

        // Flush last data chunk
        let record = Record::create(
            chunk_start.unwrap_or_default(),
            RecordType::Data,
            &chunk_data,
        )?;
        writeln!(writer, "{record}")?;

        // Write EOF record
        let record = Record::create(0, RecordType::EndOfFile, &[])?;
        write!(writer, "{record}")?; // writes a line (no newline)

        Ok(())
    }

    /// Generates a binary file at the specified path.
    /// Address gaps are filled with the provided `gap_fill` byte (usually 0x00 or 0xFF).
    ///
    /// # Errors
    /// Returns an error if the file cannot be written.
    ///
    /// # Example
    /// ```
    /// use intelhexlib::IntelHex;
    ///
    /// let mut ih = IntelHex::from_hex("tests/fixtures/ih_valid_1.hex").unwrap();
    /// ih.write_bin("build/ex3/ih.bin", 0x00);
    ///
    /// assert_eq!(ih.size, 239);
    /// // Due to address gaps, the written bin file size is large
    /// assert_eq!(std::fs::metadata("build/ex3/ih.bin").unwrap().len(), 115264);
    /// ```
    pub fn write_bin<P: AsRef<Path>>(
        &mut self,
        filepath: P,
        gap_fill: u8,
    ) -> Result<(), Box<dyn Error>> {
        // Ensure the parent directory exists
        if let Some(parent) = filepath.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(filepath)?;

        // Wrap in BufWriter for efficient line-by-line writing
        let mut writer = std::io::BufWriter::new(file);

        let start = self.get_min_addr().unwrap_or(0);
        let mut current_addr = start;

        for (addr, byte) in &self.buffer {
            // Fill gaps
            while current_addr < *addr {
                writer.write_all(&[gap_fill])?;
                current_addr += 1;
            }

            // Write actual byte
            writer.write_all(&[*byte])?;
            current_addr += 1;
        }

        Ok(())
    }

    /// Get copy of the data buffer as `BTreeMap` from `IntelHex`.
    ///
    /// # Example
    /// ```
    /// use std::collections::BTreeMap;
    /// use intelhexlib::IntelHex;
    ///
    /// let ih = IntelHex::from_hex("tests/fixtures/ih_valid_1.hex").unwrap();
    /// let addr_byte_map: BTreeMap<usize, u8> = ih.to_btree_map();
    ///
    /// assert!(!addr_byte_map.is_empty());
    /// ```
    #[must_use]
    pub fn to_btree_map(&self) -> BTreeMap<usize, u8> {
        self.buffer.clone()
    }

    /// Get an iterator over (address, byte) pairs in the `BTreeMap` buffer of the `IntelHex`.
    ///
    /// # Example
    /// ```
    /// use std::collections::{BTreeMap, btree_map};
    /// use intelhexlib::IntelHex;
    ///
    /// let ih = IntelHex::from_hex("tests/fixtures/ih_valid_1.hex").unwrap();
    ///
    /// let mut map_iter: btree_map::Iter<'_, usize, u8> = ih.iter();
    /// let (first_key, first_value) = map_iter.next().unwrap();
    ///
    /// assert_eq!((*first_key, *first_value), (0, 250));
    /// ```
    pub fn iter(&self) -> std::collections::btree_map::Iter<'_, usize, u8> {
        self.into_iter()
    }

    /// Get the smallest address present in the data buffer.
    ///
    /// # Example
    /// ```
    /// use intelhexlib::IntelHex;
    ///
    /// let ih = IntelHex::from_hex("tests/fixtures/ih_valid_1.hex").unwrap();
    /// let min_addr: Option<usize> = ih.get_min_addr();
    ///
    /// assert_eq!(min_addr, Some(0));
    /// ```
    #[must_use]
    pub fn get_min_addr(&self) -> Option<usize> {
        self.buffer.first_key_value().map(|(key, _)| *key)
    }

    /// Get the highest address present in the data buffer.
    ///
    /// # Example
    /// ```
    /// use intelhexlib::IntelHex;
    ///
    /// let ih = IntelHex::from_hex("tests/fixtures/ih_valid_1.hex").unwrap();
    /// let max_addr: Option<usize> = ih.get_max_addr();
    ///
    /// assert_eq!(max_addr, Some(0x1C23F));
    /// ```
    #[must_use]
    pub fn get_max_addr(&self) -> Option<usize> {
        self.buffer.last_key_value().map(|(key, _)| *key)
    }

    /// Get byte from `IntelHex` at the provided address.
    ///
    /// # Example
    /// ```
    /// use intelhexlib::IntelHex;
    ///
    /// let ih = IntelHex::from_hex("tests/fixtures/ih_valid_1.hex").unwrap();
    /// let byte: u8 = ih.get_byte(0x0).unwrap();
    ///
    /// assert_eq!(byte, 250);
    /// ```
    #[must_use]
    pub fn get_byte(&self, address: usize) -> Option<u8> {
        self.buffer.get(&address).copied()
    }

    /// Get an array of bytes from `IntelHex` at provided addresses.
    /// Returns `None` if any of the addresses are invalid.
    ///
    /// # Example
    /// ```
    /// use intelhexlib::IntelHex;
    ///
    /// let ih = IntelHex::from_hex("tests/fixtures/ih_valid_1.hex").unwrap();
    /// let bytes: Vec<u8> = ih.get_buffer_slice(&[0x0, 0x1, 0x2]).unwrap();
    ///
    /// assert_eq!(bytes, &[250, 0, 0]);
    /// ```
    #[must_use]
    pub fn get_buffer_slice(&self, addr_vec: &[usize]) -> Option<Vec<u8>> {
        addr_vec
            .iter()
            .map(|addr| self.buffer.get(addr).copied())
            .collect()
    }

    #[allow(clippy::option_if_let_else)]
    /// Update byte in `IntelHex` at provided address.
    ///
    /// # Errors
    /// Returns an error if the provided address is invalid.
    ///
    /// # Example
    /// ```
    /// use intelhexlib::{IntelHex, IntelHexError};
    /// use std::io;
    ///
    /// let mut ih = IntelHex::from_hex("tests/fixtures/ih_valid_1.hex").unwrap();
    /// let res: Result<(), IntelHexError> = ih.update_byte(0x0, 0xFF);
    ///
    /// assert!(res.is_ok());
    /// ```
    pub fn update_byte(&mut self, address: usize, value: u8) -> Result<(), IntelHexError> {
        if let Some(v) = self.buffer.get_mut(&address) {
            *v = value;
        } else {
            return Err(IntelHexError::UpdateError(
                IntelHexErrorKind::InvalidAddress(address),
            ));
        }

        Ok(())
    }

    /// Update the array of bytes in `IntelHex` at provided addresses.
    ///
    /// # Errors
    /// Returns an error if any of the provided addresses are invalid.
    ///
    /// # Example
    /// ```
    /// use intelhexlib::{IntelHex, IntelHexError};
    /// use std::io;
    ///
    /// let mut ih = IntelHex::from_hex("tests/fixtures/ih_valid_1.hex").unwrap();
    /// let res: Result<(), IntelHexError> = ih.update_buffer_slice(&[(0x0, 0xFF), (0x1, 0xFF), (0x2, 0xFF)]);
    ///
    /// assert!(res.is_ok());
    /// ```
    pub fn update_buffer_slice(&mut self, update_map: &[(usize, u8)]) -> Result<(), IntelHexError> {
        // Iteration 1 - check if all addresses are valid
        for &(addr, _) in update_map {
            if !self.buffer.contains_key(&addr) {
                return Err(IntelHexError::UpdateError(
                    IntelHexErrorKind::InvalidAddress(addr),
                ));
            }
        }

        // Iteration 2 - update bytes
        for &(addr, value) in update_map {
            if let Some(byte) = self.buffer.get_mut(&addr) {
                *byte = value;
            }
        }

        Ok(())
    }

    /// Update the max payload size (number of bytes) per record when writing `IntelHex` file.
    /// Default = 16.
    ///
    /// # Errors
    /// Returns an error if the provided payload size is 0.
    ///
    /// # Example
    /// ```
    /// use intelhexlib::IntelHex;
    ///
    /// let mut ih = IntelHex::from_hex("tests/fixtures/ih_valid_1.hex").unwrap();
    /// ih.set_max_payload_size(0xFF);      // set to u8 max
    /// ih.write_hex("build/ex2/ih.hex");   // now data records can have up to 255 bytes of payload
    ///
    /// // Due to larger payload size, there are fewer records in the hex file.
    /// // Hence, the size is smaller (originally = 239 bytes).
    /// assert_eq!(std::fs::metadata("build/ex2/ih.hex").unwrap().len(), 187);
    /// ```
    pub const fn set_max_payload_size(&mut self, size: u8) -> Result<(), IntelHexError> {
        if size == 0 {
            return Err(IntelHexError::UpdateError(
                IntelHexErrorKind::RecordInvalidPayloadLength,
            ));
        }
        self.max_payload_size = size as usize;
        Ok(())
    }

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss
    )]
    /// Relocate the entire data buffer to a new starting address.
    ///
    /// # Errors
    /// Returns an error if the new starting address is out of bounds or
    /// if the `IntelHex` instance has no data.
    ///
    /// # Example
    /// ```
    /// use intelhexlib::IntelHex;
    ///
    /// let mut ih = IntelHex::from_hex("tests/fixtures/ih_valid_1.hex").unwrap();
    /// ih.relocate(0x1234);
    ///
    /// assert_eq!(ih.get_byte(0), None);
    /// assert_eq!(ih.get_byte(0x1234), Some(250));
    /// ```
    pub fn relocate(&mut self, new_start_address: usize) -> Result<(), IntelHexError> {
        let (min_addr, max_addr) =
            self.get_min_addr()
                .zip(self.get_max_addr())
                .ok_or(IntelHexError::UpdateError(
                    IntelHexErrorKind::IntelHexInstanceEmpty,
                ))?;

        // Cap start address to u32::MAX (to not break API for now)
        if new_start_address > u32::MAX as usize {
            return Err(IntelHexError::UpdateError(
                IntelHexErrorKind::InvalidAddress(new_start_address),
            ));
        }

        let offset = new_start_address as i64 - min_addr as i64;

        if max_addr as i64 + offset > i64::from(u32::MAX) {
            let max_allowed_start_address = u32::MAX - max_addr as u32 + min_addr as u32;
            return Err(IntelHexError::UpdateError(
                IntelHexErrorKind::RelocateAddressOverflow(max_allowed_start_address as usize),
            ));
        }

        self.buffer = std::mem::take(&mut self.buffer)
            .into_iter()
            .map(|(addr, byte)| ((addr as i64 + offset) as usize, byte))
            .collect();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_max_payload_size_valid() {
        // Arrange
        let mut ih = IntelHex::new();
        let new_payload_length = 2;

        // Act
        let res = ih.set_max_payload_size(new_payload_length);

        // Assert
        assert!(res.is_ok());
        assert_eq!(ih.max_payload_size, new_payload_length as usize);
    }

    #[test]
    fn test_set_max_payload_size_invalid() {
        // Arrange
        let mut ih = IntelHex::new();
        let new_payload_length = 0;
        let default_payload_length = ih.max_payload_size;

        // Act
        let res = ih.set_max_payload_size(new_payload_length);

        // Assert
        assert!(res.is_err());
        assert_eq!(ih.max_payload_size, default_payload_length);
    }

    #[test]
    fn test_get_byte_valid() {
        // Arrange
        let mut ih = IntelHex::new();
        let addr = 0x1234;
        let value = 0xFF;
        ih.buffer.insert(addr, value);

        // Act
        let byte = ih.get_byte(addr);

        // Assert
        assert_eq!(byte, Some(value));
    }

    #[test]
    fn test_get_byte_invalid() {
        // Arrange
        let mut ih = IntelHex::new();
        let addr = 0x1234;
        let value = 0xFF;
        ih.buffer.insert(addr, value);

        // Act
        let byte = ih.get_byte(addr - 1);

        // Assert
        assert!(byte.is_none());
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)]
    fn test_get_buffer_slice_valid() {
        // Arrange
        let mut ih = IntelHex::new();

        let addr_start = 16;
        let length = 10;

        let mut addr_vec = Vec::with_capacity(length);
        let mut expected_res_vec = Vec::with_capacity(length);

        for addr in addr_start..=addr_start + length {
            addr_vec.push(addr); // construct addr vector
            ih.buffer.insert(addr, addr as u8); // insert key-value pair into the map
            expected_res_vec.push(addr as u8); // push the value into expected result vec
        }

        // Act
        let res_vec: Option<Vec<u8>> = ih.get_buffer_slice(&addr_vec);

        // Assert
        assert_eq!(res_vec, Some(expected_res_vec));
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)]
    fn test_get_buffer_slice_with_gaps() {
        // Arrange
        let mut ih = IntelHex::new();

        let addr_start = 16;
        let length = 10;

        let mut addr_vec = Vec::with_capacity(length);
        let mut expected_res_vec = Vec::with_capacity(length);

        for addr in addr_start..=addr_start + length {
            addr_vec.push(addr * 2); // construct addr vector
            ih.buffer.insert(addr * 2, addr as u8); // insert key-value pair into the map
            expected_res_vec.push(addr as u8); // push the value into expected result vec
        }

        // Act
        let res_vec: Option<Vec<u8>> = ih.get_buffer_slice(&addr_vec);

        // Assert
        assert_eq!(res_vec, Some(expected_res_vec));
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)]
    fn test_get_buffer_slice_invalid() {
        // Arrange
        let mut ih = IntelHex::new();

        let addr_start = 16;
        let length = 10;

        let mut addr_vec = Vec::with_capacity(length);

        for addr in addr_start..=addr_start + length {
            addr_vec.push(addr); // construct addr vector
            ih.buffer.insert(addr, addr as u8); // insert key-value pair into the map
        }
        ih.buffer.pop_last(); // pop the last addr

        // Act
        let res_vec: Option<Vec<u8>> = ih.get_buffer_slice(&addr_vec);

        // Assert
        assert_eq!(res_vec, None);
    }

    #[test]
    fn test_update_byte_valid() {
        // Arrange
        let mut ih = IntelHex::new();
        let addr = 0x1234;
        let value = 0xFF;
        ih.buffer.insert(addr, value);

        // Act
        let res = ih.update_byte(addr, value - 1);

        // Assert
        assert!(res.is_ok());
        assert_eq!(*ih.buffer.get(&addr).unwrap_or(&0), value - 1);
    }

    #[test]
    fn test_update_byte_invalid() {
        // Arrange
        let mut ih = IntelHex::new();
        let addr = 0x1234;
        let value = 0xFF;
        ih.buffer.insert(addr, value);

        // Act
        let res = ih.update_byte(addr - 1, value - 1);

        // Assert
        assert!(res.is_err());
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)]
    fn test_update_buffer_slice_valid() {
        // Arrange
        let mut ih = IntelHex::new();

        let addr_start = 16;
        let length = 10;

        let mut update_map: Vec<(usize, u8)> = Vec::with_capacity(length);

        for addr in addr_start..=addr_start + length {
            update_map.push((addr, (addr - 1) as u8)); // construct vector with addr & new value
            ih.buffer.insert(addr, addr as u8); // insert key-value pair into the map
        }

        // Act
        let res = ih.update_buffer_slice(&update_map);

        // Assert
        assert!(res.is_ok());
        for addr in addr_start..=addr_start + length {
            assert_eq!(*ih.buffer.get(&addr).unwrap_or(&0), (addr - 1) as u8);
        }
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)]
    fn test_update_buffer_slice_with_gap() {
        // Arrange
        let mut ih = IntelHex::new();

        let addr_start = 16;
        let length = 10;

        let mut update_map: Vec<(usize, u8)> = Vec::with_capacity(length);

        for addr in addr_start..=addr_start + length {
            update_map.push((addr * 2, (addr - 1) as u8)); // construct vector with addr & new value
            ih.buffer.insert(addr * 2, addr as u8); // insert key-value pair into the map
        }

        // Act
        let res = ih.update_buffer_slice(&update_map);

        // Assert
        assert!(res.is_ok());
        for addr in addr_start..=addr_start + length {
            assert_eq!(*ih.buffer.get(&(addr * 2)).unwrap_or(&0), (addr - 1) as u8);
        }
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)]
    fn test_update_buffer_slice_invalid() {
        // Arrange
        let mut ih = IntelHex::new();

        let addr_start = 16;
        let length = 10;

        let mut update_map: Vec<(usize, u8)> = Vec::with_capacity(length);

        for addr in addr_start..=addr_start + length {
            update_map.push((addr, (addr - 1) as u8)); // construct vector with addr & new value
            ih.buffer.insert(addr, addr as u8); // insert key-value pair into the map
        }
        ih.buffer.pop_last(); // pop the last addr

        // Act
        let res = ih.update_buffer_slice(&update_map);

        // Assert
        assert!(res.is_err());
    }

    #[test]
    fn test_get_min_and_max_addr_valid() {
        // Arrange
        let mut ih = IntelHex::new();

        let addr_start = 10;
        let length = 10;

        for addr in addr_start..=addr_start + length {
            ih.buffer.insert(addr, 0); // insert key-value pair into the map
        }

        // Act
        let min_addr = ih.get_min_addr();
        let max_addr = ih.get_max_addr();

        // Assert
        assert_eq!(min_addr, Some(addr_start));
        assert_eq!(max_addr, Some(addr_start + length));

        // Arrange
        ih.buffer.clear();
        ih.buffer.insert(0, 0);

        // Act
        let min_addr = ih.get_min_addr();
        let max_addr = ih.get_max_addr();

        // Assert
        assert_eq!(min_addr, Some(0));
        assert_eq!(max_addr, Some(0));
    }

    #[test]
    fn test_get_min_and_max_addr_invalid() {
        // Arrange
        let ih = IntelHex::new();

        // Act
        let min_addr = ih.get_min_addr();
        let max_addr = ih.get_max_addr();

        // Assert
        assert!(min_addr.is_none());
        assert!(max_addr.is_none());
    }

    #[test]
    fn test_relocate_valid() {
        // Arrange
        let mut ih = IntelHex::new();
        ih.buffer.insert(0xFFFF, 0xFF);

        // Act
        let res = ih.relocate(0x0);

        // Assert
        assert!(res.is_ok());
        assert_eq!(ih.buffer.get(&0x0), Some(&0xFF));
    }

    #[test]
    fn test_relocate_invalid() {
        // Arrange
        let mut ih = IntelHex::new();
        ih.buffer.insert(0x0000, 0xFF); // min addr
        ih.buffer.insert(0xFFFF, 0xFF); // max addr

        // Act
        let res = ih.relocate(u32::MAX as usize);

        // Assert
        assert_eq!(
            res,
            Err(IntelHexError::UpdateError(
                IntelHexErrorKind::RelocateAddressOverflow(0xFFFF_0000)
            ))
        );
    }
}
