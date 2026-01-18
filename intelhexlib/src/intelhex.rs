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
    /// Data buffer of the Intel HEX file.
    /// Maps start address of each contiguous data block to a vector of bytes.
    buffer: BTreeMap<usize, Vec<u8>>,
}

impl Default for IntelHex {
    fn default() -> Self {
        Self::new()
    }
}

/// Borrowing iterator over (address, data chunk) pairs in the `BTreeMap` buffer of the `IntelHex`.
/// Replicates the structure of the internal buffer and has the highest performance.
impl<'a> IntoIterator for &'a IntelHex {
    type Item = (&'a usize, &'a Vec<u8>);
    type IntoIter = std::collections::btree_map::Iter<'a, usize, Vec<u8>>;
    fn into_iter(self) -> Self::IntoIter {
        self.buffer.iter()
    }
}

/// Consuming iterator over (address, data chunk) pairs in the `BTreeMap` buffer of the `IntelHex`.
/// Replicates the structure of the internal buffer and has the highest performance.
impl IntoIterator for IntelHex {
    type Item = (usize, Vec<u8>);
    type IntoIter = std::collections::btree_map::IntoIter<usize, Vec<u8>>;
    fn into_iter(self) -> Self::IntoIter {
        self.buffer.into_iter()
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
    fn parse(&mut self, raw_bytes: &[u8]) -> Result<(), IntelHexError> {
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
                    let addr = record.address as usize + self.offset;

                    if !record.data.is_empty() {
                        let new_end_addr = addr + record.data.len();

                        // Find a neighbor (previous chunk) and check for overlaps
                        let (prev_key, can_append) = match self.buffer.range(..=addr).next_back() {
                            Some((&start, data)) => {
                                let end = start + data.len();
                                if addr < end {
                                    return Err(IntelHexError::ParseRecordError(
                                        IntelHexErrorKind::RecordAddressOverlap(addr),
                                        count,
                                    ));
                                }
                                (Some(start), end == addr)
                            }
                            None => (None, false),
                        };

                        // Find a neighbor (next chunk) and check for overlaps
                        let can_prepend = match self.buffer.range(addr..).next() {
                            Some((&start, _)) => {
                                if start < new_end_addr {
                                    return Err(IntelHexError::ParseRecordError(
                                        IntelHexErrorKind::RecordAddressOverlap(addr),
                                        count,
                                    ));
                                }
                                start == new_end_addr
                            }
                            None => false,
                        };

                        // Take ownership of the data (not required)
                        let mut current_data = record.data;

                        match (can_append, can_prepend) {
                            // BRIDGE: [prev][new][next] -> [prev_merged]
                            (true, true) => {
                                // Remove the 'next' block from the buffer and get its data
                                let mut next_data =
                                    self.buffer.remove(&new_end_addr).unwrap_or_default();
                                // Get the 'prev' block and append both 'new' and 'next' data to it.
                                // Error cases are not handled here as they were checked above.
                                if let Some(prev_data) =
                                    self.buffer.get_mut(&prev_key.unwrap_or_default())
                                {
                                    prev_data.append(&mut current_data);
                                    prev_data.append(&mut next_data);
                                }
                            }
                            // APPEND: [prev][new]
                            (true, false) => {
                                // Get the 'prev' block and append 'new' data to it.
                                // Error cases are not handled here as they were checked above.
                                if let Some(prev_data) =
                                    self.buffer.get_mut(&prev_key.unwrap_or_default())
                                {
                                    prev_data.append(&mut current_data);
                                }
                            }
                            // PREPEND: [new][next]
                            (false, true) => {
                                // Remove the 'next' block from the buffer and get its data
                                let mut next_data =
                                    self.buffer.remove(&new_end_addr).unwrap_or_default();
                                // Append 'next' data to the 'new' block and insert it into the buffer
                                current_data.append(&mut next_data);
                                self.buffer.insert(addr, current_data);
                            }
                            // NEW: [new]
                            (false, false) => {
                                self.buffer.insert(addr, current_data);
                            }
                        }
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
        // Read the contents of the file
        let data = std::fs::read(&filepath)?;

        // Clear provided IntelHex instance
        self.clear();

        // Compute the size (in bytes)
        self.size = data.len();

        // Load filepath
        self.filepath = filepath.as_ref().to_path_buf();

        // Load data bytes into the map as one chunk
        self.buffer.insert(base_address, data);

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
            writer.write_all(b":")?;
            writer.write_all(&s)?;
            writeln!(writer)?;
        }

        let mut curr_high_addr: Option<u16> = None;

        for (&block_start_addr, data) in &self.buffer {
            let mut block_offset = 0;

            // Iterate over data chunk
            while block_offset < data.len() {
                let addr = block_start_addr + block_offset;

                // Split address into low and high
                let high_addr = (addr >> 16) as u16;
                let low_addr = (addr & 0xFFFF) as u16;

                // If ELA segment changed -> emit ELA record
                if curr_high_addr != Some(high_addr) {
                    let msb = (high_addr >> 8) as u8;
                    let lsb = (high_addr & 0xFF) as u8;

                    let record = Record::create(0, RecordType::ExtendedLinearAddress, &[msb, lsb])?;

                    writeln!(writer, "{record}")?;

                    curr_high_addr = Some(high_addr);
                }

                // Determine how many bytes can fit in this record
                // - Can't exceed max_payload_size
                // - Can't cross a 64KB boundary (must stay within current high_addr)
                let remaining_in_segment = 0x10000 - low_addr as usize;
                let chunk_size = std::cmp::min(
                    self.max_payload_size,
                    std::cmp::min(data.len() - block_offset, remaining_in_segment),
                );

                let record = Record::create(
                    low_addr,
                    RecordType::Data,
                    &data[block_offset..block_offset + chunk_size],
                )?;
                writeln!(writer, "{record}")?;

                block_offset += chunk_size;
            }
        }

        // Write EOF record
        let record = Record::create(0, RecordType::EndOfFile, &[])?;
        write!(writer, "{record}")?; // write last line (no newline)

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

        // Get the starting point
        let mut current_addr = self.get_min_addr().unwrap_or(0);

        // Iterate over contiguous blocks of data
        for (&block_start_addr, data) in &self.buffer {
            // Fill the gap between the last written byte and the start of this block
            if block_start_addr > current_addr {
                let gap_size = block_start_addr - current_addr;

                // Use a small buffer to write gaps. Limit the buffer to 4096 KB as it is the
                // default / typical page size of most OS - more efficient + avoids large heap allocations.
                let gap_buf = vec![gap_fill; std::cmp::min(gap_size, 4096)];

                let mut remaining = gap_size;
                while remaining > 0 {
                    let to_write = std::cmp::min(remaining, gap_buf.len());
                    writer.write_all(&gap_buf[..to_write])?;
                    remaining -= to_write;
                }
            }

            // Write the entire contiguous block at once
            writer.write_all(data)?;

            // Advance the tracking address
            current_addr = block_start_addr + data.len();
        }

        writer.flush()?;
        Ok(())
    }

    /// Get an iterator over (address, contiguous data chunk) pairs in the
    /// `BTreeMap<usize, Vec<u8>` buffer of the `IntelHex`.
    /// For a more convenient way to iterate over the data, see [`IntelHex::bytes()`].
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
    pub fn iter(&self) -> std::collections::btree_map::Iter<'_, usize, Vec<u8>> {
        self.into_iter()
    }

    /// Returns an iterator over every (address, byte) pair, skipping gaps.
    /// More convenient to iterate over data but slower than [`IntelHex::iter()`].
    ///
    /// # Example
    /// ```
    /// use intelhexlib::IntelHex;
    ///
    /// let ih = IntelHex::from_hex("tests/fixtures/ih_valid_1.hex").unwrap();
    ///
    /// let mut byte_iter: impl Iterator<Item = (usize, u8)> = ih.bytes();
    /// let (first_key, first_value) = byte_iter.next().unwrap();
    ///
    /// assert_eq!((*first_key, *first_value), (0, 250));
    /// ```
    pub fn bytes(&self) -> impl Iterator<Item = (usize, u8)> + '_ {
        self.buffer.iter().flat_map(|(&start_addr, data)| {
            data.iter()
                .enumerate()
                .map(move |(offset, &byte)| (start_addr + offset, byte))
        })
    }

    /// Returns an iterator that yields owned (address, byte) pairs, skipping gaps.
    /// More convenient to iterate over data but slower than [`IntelHex::into_iter()`].
    ///
    /// # Example
    /// ```
    /// use intelhexlib::IntelHex;
    ///
    /// let ih = IntelHex::from_hex("tests/fixtures/ih_valid_1.hex").unwrap();
    ///
    /// let mut byte_iter: impl Iterator<Item = (usize, u8)> = ih.into_bytes();
    /// let (first_key, first_value) = byte_iter.next().unwrap();
    ///
    /// assert_eq!((*first_key, *first_value), (0, 250));
    /// ```
    pub fn into_bytes(self) -> impl Iterator<Item = (usize, u8)> {
        self.buffer.into_iter().flat_map(|(start_addr, data)| {
            data.into_iter()
                .enumerate()
                .map(move |(offset, byte)| (start_addr + offset, byte))
        })
    }

    /// Get the smallest address of the data.
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

    /// Get the highest address of the data.
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
        self.buffer
            .last_key_value()
            .map(|(key, data)| *key + data.len() - 1)
    }

    /// Read byte from `IntelHex` at the provided address.
    ///
    /// # Example
    /// ```
    /// use intelhexlib::IntelHex;
    ///
    /// let ih = IntelHex::from_hex("tests/fixtures/ih_valid_1.hex").unwrap();
    /// let byte: u8 = ih.read_byte(0x0).unwrap();
    ///
    /// assert_eq!(byte, 250);
    /// ```
    #[must_use]
    pub fn read_byte(&self, address: usize) -> Option<u8> {
        if let Some((&start_addr, data)) = self.buffer.range(..=address).next_back()
            && address < start_addr + data.len()
        {
            return Some(data[address - start_addr]);
        }
        None
    }

    /// Read an array of bytes from `IntelHex` at provided addresses.
    /// Returns `None` if any of the addresses are invalid.
    ///
    /// # Example
    /// ```
    /// use intelhexlib::IntelHex;
    ///
    /// let ih = IntelHex::from_hex("tests/fixtures/ih_valid_1.hex").unwrap();
    /// let bytes: Vec<u8> = ih.read_range(0x0, 3).unwrap();
    ///
    /// assert_eq!(bytes, &[250, 0, 0]);
    /// ```
    #[must_use]
    pub fn read_range(&self, start_addr: usize, len: usize) -> Option<Vec<u8>> {
        // Find the chunk that might contain the start_addr
        if let Some((&chunk_start, data)) = self.buffer.range(..=start_addr).next_back() {
            let end_addr = start_addr + len;
            let chunk_end = chunk_start + data.len();

            // Check if the entire requested range is within this chunk
            if start_addr >= chunk_start && end_addr <= chunk_end {
                let chunk_offset = start_addr - chunk_start;
                return Some(data[chunk_offset..chunk_offset + len].to_vec());
            }
        }

        // If the range spans multiple chunks (contains gaps), return None
        None
    }

    /// Read a range of bytes, returning a `Vec<Option<u8>>`.
    /// Each element is `Some(byte)` if data exists at that address, or `None` if it is a gap.
    ///
    /// # Example
    /// ```
    /// use intelhexlib::IntelHex;
    ///
    /// let ih = IntelHex::from_hex("tests/fixtures/ih_valid_1.hex").unwrap();
    /// let bytes: Vec<u8> = ih.read_range_safe(0x0, 3).unwrap();
    ///
    /// assert_eq!(bytes, &[250, 0, 0]);
    /// ```
    #[must_use]
    pub fn read_range_safe(&self, start_addr: usize, len: usize) -> Vec<Option<u8>> {
        let mut result = Vec::with_capacity(len);
        let end_addr = start_addr + len;
        let mut current_addr = start_addr;

        while current_addr < end_addr {
            // Find the chunk that contains or precedes current_addr
            if let Some((&chunk_start, data)) = self.buffer.range(..=current_addr).next_back() {
                let chunk_end = chunk_start + data.len();

                // Case 1: current_addr is inside a chunk
                if current_addr >= chunk_start && current_addr < chunk_end {
                    let chunk_offset = current_addr - chunk_start;

                    let available_size = chunk_end - current_addr;
                    let needed_size = end_addr - current_addr;
                    let len_used = std::cmp::min(available_size, needed_size);

                    // Extend with the actual data
                    result.extend(
                        data[chunk_offset..chunk_offset + len_used]
                            .iter()
                            .map(|&b| Some(b)),
                    );

                    current_addr += len_used;
                    continue;
                }
            }

            // Case 2: current_addr is in a gap.
            // Find where the next chunk starts (if any) and fill the gap.
            let next_chunk_start = self
                .buffer
                .range(current_addr..)
                .next()
                .map_or(end_addr, |(&s, _)| s);

            let gap_end = std::cmp::min(next_chunk_start, end_addr);
            let gap_len = gap_end - current_addr;

            // Fill with None for the duration of the gap
            result.extend(std::iter::repeat_n(None, gap_len));
            current_addr = gap_end;
        }

        result
    }

    #[allow(clippy::option_if_let_else)]
    /// Update a byte in `IntelHex` at the provided address.
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
        if let Some((&start_addr, data)) = self.buffer.range_mut(..=address).next_back()
            && address < start_addr + data.len()
        {
            data[address - start_addr] = value;
            return Ok(());
        }

        Err(IntelHexError::UpdateError(
            IntelHexErrorKind::InvalidAddress(address),
        ))
    }

    /// Update the array of bytes in `IntelHex` at provided addresses.
    ///
    /// If the address in the update map lands on the gap, an error is returned
    /// and no data is modified.
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
        // First pass: Verify all addresses exist before modifying anything
        for &(addr, _) in update_map {
            let exists = self
                .buffer
                .range(..=addr)
                .next_back()
                .is_some_and(|(&start, data)| addr < start + data.len());

            if !exists {
                return Err(IntelHexError::UpdateError(
                    IntelHexErrorKind::InvalidAddress(addr),
                ));
            }
        }

        // Second pass: Apply the updates
        for &(addr, value) in update_map {
            self.update_byte(addr, value)?;
        }

        Ok(())
    }

    /// Updates a contiguous range of bytes starting at `start_addr`.
    ///
    /// This operation is atomic: if the range spans across a gap in the
    /// sparse buffer, an error is returned and no data is modified.
    ///
    /// # Errors
    /// Returns `InvalidAddress` if any part of the range is not defined.
    ///
    /// # Example
    /// ```
    /// use intelhexlib::{IntelHex, IntelHexError};
    /// use std::io;
    ///
    /// let mut ih = IntelHex::from_hex("tests/fixtures/ih_valid_1.hex").unwrap();
    /// let res: Result<(), IntelHexError> = ih.update_range(0x0, &[0xFF, 0xFF, 0xFF]);
    ///
    /// assert!(res.is_ok());
    /// ```
    pub fn update_range(&mut self, start_addr: usize, data: &[u8]) -> Result<(), IntelHexError> {
        let len = data.len();

        if let Some((&chunk_start_addr, chunk_data)) =
            self.buffer.range_mut(..=start_addr).next_back()
        {
            let chunk_end = chunk_start_addr + chunk_data.len();
            let end_addr = start_addr + len;

            // Check the entire range fits within a single chunk
            if start_addr >= chunk_start_addr && end_addr <= chunk_end {
                let chunk_offset = start_addr - chunk_start_addr;
                chunk_data[chunk_offset..chunk_offset + len].copy_from_slice(data);
                return Ok(());
            }
        }

        Err(IntelHexError::UpdateError(
            IntelHexErrorKind::InvalidAddress(start_addr),
        ))
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
            .map(|(addr, data)| ((addr as i64 + offset) as usize, data))
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

// =====================  BENCH ACCESS FOR PRIVATE FUNCTIONS  =====================

impl IntelHex {
    #[cfg(feature = "benchmarking")]
    pub fn bench_priv_parse(ih: &mut Self, raw_bytes: &[u8]) {
        let _ = ih.parse(raw_bytes);
    }
}
