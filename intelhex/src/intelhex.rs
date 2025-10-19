//! The 'intelhex' module defines the ['IntelHex'] struct which provides APIs for
//! loading, modifying and writing Intel HEX files.

use std::collections::BTreeMap;
use std::error::Error;
use std::{fs, io};
use std::io::Write;
use std::path::{Path, PathBuf};
use crate::record::{Record, RecordType};
use crate::error::IntelHexError;


#[derive(Debug, Clone)]
pub struct StartAddress {
    /// Type of the start address
    rtype: Option<RecordType>,
    /// Data bytes (the address itself stored as byte array)
    bytes: Option<[u8; 4]>,
}

impl StartAddress {
    pub fn new(rtype: RecordType, bytes: [u8; 4]) -> Self {
        Self { rtype: Some(rtype), bytes: Some(bytes) }
    }
    pub fn is_empty(&self) -> bool {
        self.rtype.is_none() && self.bytes.is_none()
    }
}


#[derive(Debug, Clone)]
pub struct IntelHex {
    pub filepath: PathBuf,
    pub size: usize,
    pub start_addr: StartAddress,
    offset: usize,
    buffer: BTreeMap<usize, u8>,
}

impl Default for IntelHex {
    fn default() -> Self {
        Self::new()
    }
}

impl IntelHex {
    /// Creates empty IntelHex struct instance.
    ///
    /// # Examples
    /// ```
    /// use intelhex::IntelHex;
    ///
    /// let ih = IntelHex::new();
    /// ```
    pub fn new() -> Self {
        Self {
            filepath: PathBuf::new(),
            size: 0,
            offset: 0,
            start_addr: StartAddress {
                rtype: None,
                bytes: None,
            },
            buffer: BTreeMap::new(),
        }
    }

    /// Parse the raw contents of the hex file and fill internal record vector.
    ///
    fn parse(&mut self, raw_contents: &str) -> Result<(), IntelHexError> {
        // Iterate over lines of records
        for line in raw_contents.lines() {
            // Parse the record
            let record = match Record::parse(line) {
                Ok(rec) => rec,
                Err(e) => return Err(e)
            };

            // Validate checksum of the record
            let expected_checksum = Record::calculate_checksum_from_self(&record);
            if record.checksum != expected_checksum{
                return Err(IntelHexError::RecordChecksumMismatch(expected_checksum, record.checksum));
            }

            // Fill in self
            match record.rtype {
                RecordType::Data => {
                    let mut addr = record.address as usize + self.offset;
                    for byte in &record.data {
                        if let Some(_) = self.buffer.insert(addr, *byte) {
                            // Address overlap
                            return Err(IntelHexError::RecordAddressOverlap(addr));
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
                    if !self.start_addr.is_empty() {
                        return Err(IntelHexError::DuplicateStartAddress);
                    }
                    self.start_addr.rtype = Some(record.rtype);
                    // Sanity checking is done during record parsing, thus directly
                    // putting Vec data into byte array
                    self.start_addr.bytes = Some(record.data.try_into().unwrap());
                }
            }
        };
        Ok(())
    }

    /// Creates IntelHex struct instance and fills it with data from provided hex file.
    ///
    /// # Example
    /// ```
    /// use intelhex::IntelHex;
    ///
    /// let ih = IntelHex::from_hex("tests/fixtures/ih_example_1.hex").unwrap();
    /// ```
    pub fn from_hex<P: AsRef<Path>>(filepath: P) -> Result<Self, Box<dyn Error>> {
        let mut ih = IntelHex::new();
        ih.load_hex(filepath)?;
        Ok(ih)
    }

    /// Fills the IntelHex struct instance with data from provided hex file.
    ///
    /// # Example
    /// ```
    /// use intelhex::IntelHex;
    ///
    /// let mut ih = IntelHex::new();
    /// ih.load_hex("tests/fixtures/ih_example_1.hex").unwrap();
    /// ```
    pub fn load_hex<P: AsRef<Path>>(&mut self, filepath: P) -> Result<(), Box<dyn Error>> {
        // Read contents of the file
        let raw_contents: String = fs::read_to_string(&filepath)?;

        // Compute the size (in bytes)
        self.size = raw_contents.len();

        // Load filepath
        self.filepath = filepath.as_ref().to_path_buf();

        // Parse contents and return
        self.parse(&raw_contents)?;
        Ok(())
    }

    /// Generates an Intel HEX file at the specified path.
    ///
    /// # Example
    /// ```
    /// use intelhex::IntelHex;
    ///
    /// let mut ih = IntelHex::from_hex("tests/fixtures/ih_example_1.hex").unwrap();
    /// ih.write_hex("build/ex1/ih.hex");
    /// ```
    pub fn write_hex<P: AsRef<Path>>(&mut self, filepath: P) -> Result<(), Box<dyn Error>> {
        // Ensure the parent directory exists
        if let Some(parent) = filepath.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }

        let file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(filepath)?;

        // Wrap in BufWriter for efficient line-by-line writing
        let mut writer = io::BufWriter::new(file);

        // Write start address record
        // TODO: place it - start or end of file?
        if !self.start_addr.is_empty() {
            // Sanity checks were done beforehand, assume it is safe to unwrap
            let rtype = self.start_addr.rtype.unwrap();
            let data = &self.start_addr.bytes.unwrap();
            let record = Record::create(0, rtype, data)?;
            writeln!(writer, "{}", record)?;
        }

        let mut curr_high_addr = 0;
        let mut chunk_start: Option<u16> = None;
        let mut prev_addr: Option<usize> = None;
        let mut chunk_data = Vec::new();

        for (addr, byte) in &self.buffer {
            // Split address into low and high
            let high_addr = (addr >> 16) as u16;
            let low_addr = (addr & 0xFFFF) as u16;

            // If ELA segment changed -> flush current chunk and emit ELA
            if curr_high_addr != high_addr {
                if let Some(start) = chunk_start {
                    // Write data record
                    let record = Record::create(start, RecordType::Data, &chunk_data)?;
                    writeln!(writer, "{}", record)?;

                    // Write ELA record
                    let (msb, lsb) = (high_addr / 256, high_addr % 256);
                    let bin: Vec<u8> = vec![msb as u8, lsb as u8];
                    let record = Record::create(0, RecordType::ExtendedLinearAddress, &bin)?;
                    writeln!(writer, "{}", record)?;

                    // Update segment's current address
                    curr_high_addr = high_addr;

                    // Clean up
                    chunk_data.clear();
                    chunk_start = None;
                    prev_addr = None; // resets continuity check
                }
            }

            // If gap detected or chunk full -> flush
            if let Some(prev) = prev_addr {
                if (*addr != prev + 1) || chunk_data.len() >= 16 {
                    // Write data record
                    let record = Record::create(chunk_start.unwrap(), RecordType::Data, &chunk_data)?;
                    writeln!(writer, "{}", record)?;

                    // Clean up
                    chunk_data.clear();
                    chunk_start = None;
                }
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
        let record = Record::create(chunk_start.unwrap(), RecordType::Data, &chunk_data)?;
        writeln!(writer, "{}", record)?;

        // Write EOL record
        let record = Record::create(0, RecordType::EndOfFile, &vec![])?;
        write!(writer, "{}", record)?; // writes a line (no newline)

        Ok(())
    }

    /// Get copy of the data buffer as BTreeMap from IntelHex.
    ///
    /// # Example
    /// ```
    /// use std::collections::BTreeMap;
    /// use intelhex::IntelHex;
    ///
    /// let ih = IntelHex::from_hex("tests/fixtures/ih_example_1.hex").unwrap();
    /// let addr_byte_map: BTreeMap<usize, u8> = ih.to_btree_map();
    /// ```
    pub fn to_btree_map(&self) -> BTreeMap<usize, u8> {
        self.buffer.clone()
    }

    /// Get byte from IntelHex at provided address.
    ///
    /// # Example
    /// ```
    /// use intelhex::IntelHex;
    ///
    /// let ih = IntelHex::from_hex("tests/fixtures/ih_example_1.hex").unwrap();
    /// let b: u8 = ih.get_byte(0x0).unwrap();
    /// ```
    pub fn get_byte(&self, address: usize) -> Option<u8> {
        self.buffer.get(&address).copied()
    }

    /// Get array of bytes from IntelHex at provided addresses.
    ///
    /// # Example
    /// ```
    /// use intelhex::IntelHex;
    ///
    /// let ih = IntelHex::from_hex("tests/fixtures/ih_example_1.hex").unwrap();
    /// let b: Vec<u8> = ih.get_buffer_slice(&[0x0, 0x1, 0x2]).unwrap();
    /// ```
    pub fn get_buffer_slice(&self, addr_vec: &[usize]) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(addr_vec.len());
        for addr in addr_vec {
            if let Some(&byte) = self.buffer.get(addr) {
                out.push(byte);
            } else {
                return None; // invalid address
            }
        }
        Some(out)
    }

    /// Update byte in IntelHex at provided address.
    ///
    /// # Example
    /// ```
    /// use intelhex::{IntelHex, IntelHexError};
    /// use std::io;
    ///
    /// let mut ih = IntelHex::from_hex("tests/fixtures/ih_example_1.hex").unwrap();
    /// let res: Result<(), IntelHexError> = ih.update_byte(0x0, 0xFF);
    /// ```
    pub fn update_byte(&mut self, address: usize, value: u8) -> Result<(), IntelHexError> {
        if let Some(v) = self.buffer.get_mut(&address) {
            *v = value;
            Ok(())
        } else {
            Err(IntelHexError::InvalidAddress(address))
        }
    }

    /// Update array of bytes in IntelHex at provided addresses.
    ///
    /// # Example
    /// ```
    /// use intelhex::{IntelHex, IntelHexError};
    /// use std::io;
    ///
    /// let mut ih = IntelHex::from_hex("tests/fixtures/ih_example_1.hex").unwrap();
    /// let res: Result<(), IntelHexError> = ih.update_buffer_slice(&[(0x0, 0xFF), (0x1, 0xFF), (0x2, 0xFF)]);
    /// ```
    pub fn update_buffer_slice(&mut self, updates_map: &[(usize, u8)]) -> Result<(), IntelHexError> {
        for &(addr, value) in updates_map {
            if let Some(byte) = self.buffer.get_mut(&addr) {
                *byte = value;
            } else {
                return Err(IntelHexError::InvalidAddress(addr));
            }
        }
        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_foo() {
        assert!(true);
    }
}