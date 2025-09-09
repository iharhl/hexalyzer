use std::io;
use std::io::Write;
use std::fs;
use std::ops::{Add, Range};
use std::path::Path;
use std::collections::{HashMap, BTreeMap};


const RECORD_START: char = ':';
const RECORD_LEN_RANGE: Range<usize> = 1..3;
const RECORD_ADDR_RANGE: Range<usize> = 3..7;
const RECORD_TYPE_RANGE: Range<usize> = 7..9;
const BYTE_CHAR_LEN: usize = 2;
const RECORD_CHKSUM_LEN: usize = BYTE_CHAR_LEN;


#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
enum RecordType {
    Data = 0x0,
    EndOfFile = 0x1,
    ExtendedSegmentAddress = 0x2,
    StartSegmentAddress = 0x3, // TODO: deprecate? Or allow to write with it?
    ExtendedLinearAddress = 0x4,
    StartLinearAddress = 0x5,
}

impl RecordType {
    fn parse(s: &str) -> Result<Self, io::Error> {
        match s {
            "00" => Ok(Self::Data),
            "01" => Ok(Self::EndOfFile),
            "02" => Ok(Self::ExtendedSegmentAddress),
            "03" => Ok(Self::StartSegmentAddress),
            "04" => Ok(Self::ExtendedLinearAddress),
            "05" => Ok(Self::StartLinearAddress),
            _ => Err(io::Error::from(io::ErrorKind::InvalidData)),
        }
    }
}


#[derive(Debug)]
struct Record {
    length: u8,
    address: u16,
    rtype: RecordType,
    data: Vec<u8>,
    checksum: u8,
}

impl Record {
    fn calculate_checksum_from_self(&self) -> u8 {
        // Get length, address and record type byte data
        let length = self.length as usize;
        let addr_high_byte = (self.address >> 8) as usize;
        let addr_low_byte = (self.address & 0xFF) as usize;
        let rtype = self.rtype as usize;
        // Sum it up with data vector
        let mut sum: usize = length + addr_high_byte + addr_low_byte + rtype;
        for &b in self.data.iter() {
            sum = sum.add(b as usize);
        }
        let checksum = (!sum as u8).wrapping_add(1); // two's complement
        checksum
    }

    fn calculate_checksum(record: &str) -> u8 {
        let hex_data: Result<Vec<u8>, _> = (0..record.len())
            .step_by(BYTE_CHAR_LEN)
            .map(|i| u8::from_str_radix(&record[i..i+BYTE_CHAR_LEN], 16))
            .collect();
        let mut sum: u8 = 0;
        for &b in hex_data.unwrap().iter() { // TODO: handle Err
            sum = sum.wrapping_add(b); // sum modulo 256
        }
        let checksum = (!sum).wrapping_add(1); // two's complement
        checksum
    }

    fn create(address: u16, rtype: RecordType, data: &Vec<u8>) -> Result<String, io::Error> {
        match rtype {
            RecordType::Data => {
                // Check for data length
                let length = data.len();
                if length > 16 {
                    return Err(io::Error::from(io::ErrorKind::InvalidData));
                }
                // Create record string (data length, address and record type)
                let mut record = String::from(&format!("{:02X}{:04X}00", length, address));
                // Add data bytes
                for byte in data {
                    record.push_str(&format!("{:02X}", byte));
                }
                // Calculate checksum
                let checksum = Self::calculate_checksum(&record);
                // Complete the record with start symbol and checksum
                record.insert(0, ':');
                record.push_str(&format!("{:02X}", checksum));
                Ok(record)
            }
            RecordType::EndOfFile => {
                Ok(String::from(":00000001FF"))
            }
            RecordType::ExtendedLinearAddress => {
                // Check for data length
                let length = data.len();
                if length != 2 {
                    return Err(io::Error::from(io::ErrorKind::InvalidData));
                }
                // Check for address
                if address != 0 {
                    return Err(io::Error::from(io::ErrorKind::InvalidData));
                }
                // Create record string (data length, address and record type)
                let mut record = String::from("02000004");
                // Add data bytes
                for byte in data {
                    record.push_str(&format!("{:02X}", byte));
                }
                // Calculate checksum
                let checksum = Self::calculate_checksum(&record);
                // Complete the record with start symbol and checksum
                record.insert(0, ':');
                record.push_str(&format!("{:02X}", checksum));

                Ok(record)
            }
            RecordType::StartLinearAddress | RecordType::StartSegmentAddress => {
                // Check for data length
                let length = data.len();
                if length != 4 {
                    return Err(io::Error::from(io::ErrorKind::InvalidData));
                }
                // Check for address
                if address != 0 {
                    return Err(io::Error::from(io::ErrorKind::InvalidData));
                }
                // Create record string (data length and address)
                let mut record = String::from("040000");
                // Add record type
                if rtype == RecordType::StartLinearAddress {
                    record.push_str("03")
                } else {
                    record.push_str("05")
                }
                // Add data bytes
                for byte in data {
                    record.push_str(&format!("{:02X}", byte));
                }
                // Calculate checksum
                let checksum = Self::calculate_checksum(&record);
                // Complete the record with start symbol and checksum
                record.insert(0, ':');
                record.push_str(&format!("{:02X}", checksum));
                Ok(record)
            }
            RecordType::ExtendedSegmentAddress => {
                Err(io::Error::from(io::ErrorKind::InvalidData)) // TODO: not supported
            }
        }
    }

    fn parse(line: &str) -> Result<Self, io::Error> {
        // Check for start record
        if !line.starts_with(RECORD_START) {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
        // Get record length
        let length = u8::from_str_radix(&line[RECORD_LEN_RANGE], 16)
            .unwrap(); // TODO: handle Err
        // Error if record end is bigger than the record length itself
        let data_end =  RECORD_TYPE_RANGE.end + BYTE_CHAR_LEN * length as usize;
        let record_end = RECORD_CHKSUM_LEN + data_end;
        if record_end > line.len() {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
        // Get record type
        let rtype = RecordType::parse(&line[RECORD_TYPE_RANGE])?;
        // Get record address
        let address = u16::from_str_radix(&line[RECORD_ADDR_RANGE], 16)
            .unwrap(); // TODO: handle Err
        // Get record data
        let mut data: Vec<u8> = Vec::new();
        if rtype == RecordType::EndOfFile {
            if length != 0 { return Err(io::Error::from(io::ErrorKind::InvalidData)); }
        } else {
            for i in (RECORD_TYPE_RANGE.end..data_end).step_by(BYTE_CHAR_LEN) {
                let byte = u8::from_str_radix(&line[i..i+BYTE_CHAR_LEN], 16)
                    .unwrap(); // TODO: handler Err
                data.push(byte);
            }
        }
        // Get checksum
        let checksum = u8::from_str_radix(&line[data_end..record_end], 16)
            .unwrap(); // TODO: handle Err
        // Return record instance
        Ok(Self {
            length,
            address,
            rtype,
            data,
            checksum,
        })
    }
}


pub struct IntelHex {
    pub filepath: String,
    pub size: usize, // TODO: implement
    offset: usize,
    start_addr: HashMap<RecordType, Vec<u8>>,
    buffer: BTreeMap<usize, u8>,
}

impl IntelHex {
    /// Creates empty IntelHex struct instance.
    ///
    /// # Examples
    /// ```
    /// use intelhex_parser::IntelHex;
    /// let ih = IntelHex::new();
    /// ```
    pub fn new() -> Self {
        Self {
            filepath: String::new(),
            size: 0,
            offset: 0,
            start_addr: HashMap::new(),
            buffer: BTreeMap::new(),
        }
    }

    /// Parses the raw contents of the hex file and fills internal record vector.
    ///
    fn parse(&mut self, raw_contents: &str) -> Result<(), io::Error> {
        // Iterate over lines of records
        for line in raw_contents.lines() {
            // Parse the record
            let r = match Record::parse(line) {
                Ok(rec) => rec,
                Err(e) => return Err(e)
            };
            // Validate checksum of the record
            if r.checksum != Record::calculate_checksum_from_self(&r) {
                return Err(io::Error::from(io::ErrorKind::InvalidData));
            }
            //
            match r.rtype {
                RecordType::Data => {
                    let mut addr = r.address as usize + self.offset;
                    for i in r.data.iter() {
                        // TODO: check for addr overlap
                        self.buffer.insert(addr, *i);
                        addr += 1;
                    }
                }
                RecordType::EndOfFile => {}
                RecordType::ExtendedSegmentAddress => {
                    self.offset = (r.data[0] as usize * 256 + r.data[1] as usize) * 16;
                }
                RecordType::ExtendedLinearAddress => {
                    self.offset = (r.data[0] as usize * 256 + r.data[1] as usize) * 65536;
                }
                RecordType::StartSegmentAddress => {
                    // TODO: check record length?
                    if !self.start_addr.is_empty() {
                        // TODO: duplicate StartSegmentAddress error
                    }
                    self.start_addr.insert(RecordType::StartSegmentAddress, r.data[0..4].to_owned());
                }
                RecordType::StartLinearAddress => {

                    self.start_addr.insert(RecordType::StartLinearAddress, r.data[0..4].to_owned());
                }
            }
        };
        Ok(())
    }

    /// Creates IntelHex struct instance and fills it with data from provided hex file.
    ///
    /// # Examples
    /// ```
    /// use intelhex_parser::IntelHex;
    /// let input_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ih_example_1.hex");
    /// let ih = IntelHex::from_hex(input_path).unwrap();
    /// ```
    pub fn from_hex(filepath: &str) -> Result<Self, io::Error> {
        let mut ih = IntelHex::new();
        ih.load_hex(filepath)?;
        Ok(ih)
    }

    /// Fills the IntelHex struct instance with data from provided hex file.
    ///
    /// # Examples
    /// ```
    /// use intelhex_parser::IntelHex;
    /// let mut ih = IntelHex::new();
    /// let input_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ih_example_1.hex");
    /// ih.load_hex(input_path).unwrap();
    /// ```
    pub fn load_hex(&mut self, filepath: &str) -> Result<(), io::Error> {
        //
        let raw_contents: String = fs::read_to_string(filepath)?;
        //
        let size = raw_contents.len();
        //
        self.filepath = String::from(filepath);
        self.parse(&raw_contents)?;
        self.size = size;
        Ok(())
    }

    /// Creates empty IntelHex struct instance.
    ///
    /// # Examples
    /// ```
    /// use intelhex_parser::IntelHex;
    /// let input_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ih_example_1.hex");
    /// let output_path = concat!(env!("CARGO_MANIFEST_DIR"), "/build/ex1/ih.hex");
    /// let mut ih = IntelHex::from_hex(input_path).unwrap();
    /// ih.write_hex(output_path);
    /// ```
    pub fn write_hex(&mut self, filepath: &str) -> Result<(), io::Error> {
        // Ensure the parent directory exists
        if let Some(parent) = Path::new(filepath).parent() {
            fs::create_dir_all(parent)?;
        }

        let file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(filepath)?;

        // Wrap in BufWriter for efficient line-by-line writing
        let mut writer = io::BufWriter::new(file);

        // Write start addr TODO: place it - start or end of file?
        if !self.start_addr.is_empty() {
            let (rtype, data) = self.start_addr.iter().next().unwrap();
            let record = Record::create(0, *rtype, data)?;
            writeln!(writer, "{}", record)?; // writes a line and adds newline
        }

        let mut curr_high_addr = 0;
        let mut chunk_start: Option<u16> = None;
        let mut prev_addr: Option<usize> = None;
        let mut chunk_data = Vec::new();

        for (addr, byte) in &self.buffer {
            // Split address into low and high
            let high_addr = (addr >> 16) as u16;
            let low_addr = (addr & 0xFFFF) as u16;

            // If ELA segment changed → flush current chunk and emit ELA
            if curr_high_addr != high_addr {
                if let Some(start) = chunk_start {
                    // Make data record
                    let record = Record::create(start, RecordType::Data, &chunk_data)?;
                    writeln!(writer, "{}", record)?;
                    // Make ELA record
                    let (msb, lsb) = (high_addr / 256, high_addr % 256);
                    let bin: Vec<u8> = vec![msb as u8, lsb as u8];
                    let record = Record::create(0, RecordType::ExtendedLinearAddress, &bin)?;
                    writeln!(writer, "{}", record)?;
                    // Update segment's current address
                    curr_high_addr = high_addr;
                    //
                    chunk_data.clear();
                    chunk_start = None;
                    prev_addr = None; // Reset continuity check
                }
            }

            // If gap detected or chunk full → flush
            if let Some(prev) = prev_addr {
                if (*addr != prev + 1) || chunk_data.len() >= 16 {
                    // Make data record
                    let record = Record::create(chunk_start.unwrap(), RecordType::Data, &chunk_data)?;
                    writeln!(writer, "{}", record)?;
                    //
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

        // Flush last chunk
        let record = Record::create(chunk_start.unwrap(), RecordType::Data, &chunk_data)?;
        writeln!(writer, "{}", record)?;

        let record = Record::create(0, RecordType::EndOfFile, &vec![])?;
        write!(writer, "{}", record)?; // writes a line (no newline)

        Ok(())
    }

    /// Get byte from IntelHex at provided address.
    ///
    /// # Examples
    /// ```
    /// use intelhex_parser::IntelHex;
    /// let input_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ih_example_1.hex");
    /// let ih = IntelHex::from_hex(input_path).unwrap();
    /// let b: u8 = ih.get_byte(0x0).unwrap();
    /// ```
    pub fn get_byte(&self, address: usize) -> Option<u8> {
        self.buffer.get(&address).copied()
    }

    /// Get array of bytes from IntelHex at provided addresses.
    ///
    /// # Examples
    /// ```
    /// use intelhex_parser::IntelHex;
    /// let input_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ih_example_1.hex");
    /// let ih = IntelHex::from_hex(input_path).unwrap();
    /// let b: Vec<u8> = ih.get_buffer_slice(&[0x0, 0x1, 0x2]).unwrap();
    /// ```
    pub fn get_buffer_slice(&self, addr_arr: &[usize]) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(addr_arr.len());
        for addr in addr_arr {
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
    /// # Examples
    /// ```
    /// use intelhex_parser::IntelHex;
    /// use std::io;
    /// let input_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ih_example_1.hex");
    /// let mut ih = IntelHex::from_hex(input_path).unwrap();
    /// let res: Result<(), io::Error> = ih.update_byte(0x0, 0xFF);
    /// ```
    pub fn update_byte(&mut self, address: usize, value: u8) -> Result<(), io::Error> {
        if let Some(v) = self.buffer.get_mut(&address) {
            *v = value;
            Ok(())
        } else {
            Err(io::Error::from(io::ErrorKind::InvalidData)) // invalid address
        }
    }

    /// Update array of bytes in IntelHex at provided addresses.
    ///
    /// # Examples
    /// ```
    /// use intelhex_parser::IntelHex;
    /// use std::io;
    /// let input_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ih_example_1.hex");
    /// let mut ih = IntelHex::from_hex(input_path).unwrap();
    /// let res: Result<(), io::Error> = ih.update_buffer_slice(&[(0x0, 0xFF), (0x1, 0xFF), (0x2, 0xFF)]);
    /// ```
    pub fn update_buffer_slice(&mut self, updates_arr: &[(usize, u8)]) -> Result<(), io::Error> {
        for &(addr, value) in updates_arr {
            if let Some(byte) = self.buffer.get_mut(&addr) {
                *byte = value;
            } else {
                return Err(io::Error::from(io::ErrorKind::InvalidData)); // invalid address
            }
        }
        Ok(())
    }
}
