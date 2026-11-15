//! The `intelhex` module provides the [`IntelHex`] struct, a high-level API for
//! managing Intel HEX data.
//!
//! It supports parsing HEX files into an internal sparse memory representation using
//! a `BTreeMap`, allowing for efficient manipulation of non-contiguous data chunks.
//! The module also provides utilities for binary file interop, memory relocation,
//! and generating valid Intel HEX output with configurable record sizes.

use crate::error::{IntelHexError, IntelHexErrorKind};
use crate::record::{Record, RecordType};
use crate::search;
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// UTF-8 byte order mark. Some editors (e.g. Notepad) put it at the start of text files.
const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

/// Counter that makes temporary file names unique within the process
static TEMP_FILE_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Number of characters of the target's file name kept in the temporary file name.
/// Keeps the temporary name within file system limits (usually 255 bytes) for long names.
const TEMP_FILE_NAME_PREFIX_LEN: usize = 32;

/// Write a file atomically (fully written or not changed at all).
///
/// The content is written to a temporary file next to the target, flushed and synced
/// to disk, and only then renamed over the target. If anything fails (e.g. disk full),
/// the temporary file is removed and an existing target is left untouched.
///
/// - Creates missing parent directories.
/// - If the target is a symlink, the file it points to is replaced (the link is kept).
/// - If the target exists, it must be writable, and its permissions are kept.
///
/// Refuses to write when:
/// - the directory does not allow creating the temporary file
/// - the target has other hard links (Unix only), which a rename would detach
/// - the target is a symlink to a file that does not exist
fn write_file_atomically(
    filepath: &Path,
    write: impl FnOnce(&mut BufWriter<File>) -> Result<(), IntelHexError>,
) -> Result<(), IntelHexError> {
    // Ensure the parent directory exists
    if let Some(parent) = filepath.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Resolve symlinks, so the link itself is not replaced by a regular file
    let target = match std::fs::canonicalize(filepath) {
        Ok(target) => target,
        // A symlink whose target does not exist (rename would replace the link)
        Err(_) if filepath.is_symlink() => {
            let link_target = std::fs::read_link(filepath)?;
            return Err(IntelHexError::WriteError(IntelHexErrorKind::BrokenSymlink(
                link_target,
            )));
        }
        // A new file
        Err(_) => filepath.to_path_buf(),
    };

    // An existing target must be writable
    let permissions = if target.exists() {
        let file = OpenOptions::new().write(true).open(&target)?;
        let metadata = file.metadata()?;

        // The rename would give this name a new file (other hard links keep the old data)
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if metadata.nlink() > 1 {
                return Err(IntelHexError::WriteError(
                    IntelHexErrorKind::FileHasHardLinks(metadata.nlink()),
                ));
            }
        }

        Some(metadata.permissions())
    } else {
        None
    };

    // Create temporary file in the same directory.
    // Only a prefix of the file name is used, so long names do not exceed the length limit.
    let file_name: String = target
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .chars()
        .take(TEMP_FILE_NAME_PREFIX_LEN)
        .collect();
    let temp_path = target.with_file_name(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));

    let result = (|| {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
            .map_err(|err| {
                if err.kind() == std::io::ErrorKind::PermissionDenied {
                    let dir = target.parent().unwrap_or(&target).to_path_buf();
                    IntelHexError::WriteError(IntelHexErrorKind::DirectoryNotWritable(dir))
                } else {
                    err.into()
                }
            })?;
        let mut writer = BufWriter::new(file);

        write(&mut writer)?;

        // Flush explicitly (dropping `BufWriter` silently discards write errors)
        let file = writer
            .into_inner()
            .map_err(std::io::IntoInnerError::into_error)?;

        if let Some(permissions) = permissions {
            file.set_permissions(permissions)?;
        }

        // Make sure the data is on disk before it replaces the target
        file.sync_all()?;
        drop(file);

        std::fs::rename(&temp_path, &target)?;
        Ok(())
    })();

    if result.is_err() {
        // Clean up
        let _ = std::fs::remove_file(&temp_path);
    }

    result
}

#[derive(Debug, Clone)]
pub struct IntelHex {
    /// Absolute path to the loaded file
    pub filepath: PathBuf,
    /// Size of the payload (data bytes).
    /// Does not represent the full size of the Intel HEX file.
    pub size: usize,
    /// Start address of the Intel HEX file (stores full record as raw bytes)
    pub start_addr: Option<[u8; 18]>,
    /// Maximum payload size for data records
    max_payload_size: usize,
    /// Offset of the linear address segment
    offset: usize,
    /// Data buffer of the Intel HEX file.
    /// Maps start address of each contiguous data chunk to a vector of bytes.
    buffer: BTreeMap<usize, Vec<u8>>,
}

impl Default for IntelHex {
    fn default() -> Self {
        Self::new()
    }
}

/// Iterator over individual bytes in an address range of the `IntelHex` buffer.
///
/// Yields `Some(byte)` for addresses that contain data, `None` for gaps. This mirrors
/// the behavior of [`IntelHex::read_range_safe`] but without allocating a `Vec`.
pub struct IterRange<'a> {
    /// Reference to the sparse buffer that stores contiguous data chunks.
    buffer: &'a BTreeMap<usize, Vec<u8>>,
    /// Next address to yield. Advances by 1 on each `next()` call.
    cur_addr: usize,
    /// One-past-the-end address. The iterator stops when `cur_addr >= end_addr`.
    end_addr: usize,
    /// Cached reference to the chunk we're currently iterating through.
    /// Avoids repeated `BTreeMap::range` lookups for contiguous data.
    /// Set to `None` when we advance past the chunk's end or enter a gap.
    current_chunk: Option<(usize, &'a Vec<u8>)>,
}

impl Iterator for IterRange<'_> {
    type Item = Option<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        // Iterator is exhausted once we've covered the entire address range
        if self.cur_addr >= self.end_addr {
            return None;
        }

        let addr = self.cur_addr;
        self.cur_addr += 1;

        // Fast path: check if the current address falls within the cached chunk.
        // This avoids a BTreeMap lookup for consecutive addresses in the same chunk.
        if let Some((chunk_start, chunk_data)) = self.current_chunk {
            let chunk_end = chunk_start + chunk_data.len();
            if addr >= chunk_start && addr < chunk_end {
                return Some(Some(chunk_data[addr - chunk_start]));
            }
            // We've moved past the end of this chunk - invalidate the cache
            if addr >= chunk_end {
                self.current_chunk = None;
            }
        }

        // Slow path: look up which chunk (if any) contains this address.
        // `range(..=addr).next_back()` finds the chunk with the highest start
        // address that is <= addr - i.e., the chunk that might contain addr.
        if let Some((&chunk_start, chunk_data)) = self.buffer.range(..=addr).next_back() {
            let chunk_end = chunk_start + chunk_data.len();
            if addr >= chunk_start && addr < chunk_end {
                // Cache this chunk for subsequent fast-path lookups
                self.current_chunk = Some((chunk_start, chunk_data));
                return Some(Some(chunk_data[addr - chunk_start]));
            }
        }

        // Address falls in a gap between (or before/after) chunks
        Some(None)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.end_addr - self.cur_addr;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for IterRange<'_> {}

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

    /// Check if a new range `[start, end)` overlaps with existing chunks.
    /// Returns `Err(addr)` with the overlapping address if overlap is found.
    fn check_no_overlap(&self, start: usize, end: usize) -> Result<(), usize> {
        // Find a neighbor (previous chunk) and check for overlaps
        if let Some((&prev_start, prev_data)) = self.buffer.range(..=start).next_back() {
            let prev_end = prev_start + prev_data.len();
            if start < prev_end {
                return Err(start);
            }
        }

        // Find a neighbor (next chunk) and check for overlaps
        if let Some((&next_start, _)) = self.buffer.range(start..).next()
            && next_start < end
        {
            return Err(next_start);
        }

        Ok(())
    }

    /// Insert a data chunk at `addr` and merge with adjacent neighbors if contiguous.
    fn insert_chunk_with_merge(&mut self, addr: usize, mut data: Vec<u8>) {
        let new_end = addr + data.len();

        let prev_key = self
            .buffer
            .range(..=addr)
            .next_back()
            .filter(|&(&s, d)| s + d.len() == addr)
            .map(|(&s, _)| s);

        let can_prepend = self
            .buffer
            .range(addr..)
            .next()
            .is_some_and(|(&s, _)| s == new_end);

        match (prev_key, can_prepend) {
            // BRIDGE: [prev][new][next]
            (Some(pk), true) => {
                // Remove the 'next' chunk from the buffer and get its data
                let next_data = self.buffer.remove(&new_end).unwrap_or_default();
                // Get the 'prev' chunk and append both 'new' and 'next' data to it.
                // Error cases are not handled here as they were checked above.
                if let Some(prev_data) = self.buffer.get_mut(&pk) {
                    prev_data.append(&mut data);
                    prev_data.extend(next_data);
                }
            }
            // APPEND: [prev][new]
            (Some(pk), false) => {
                // Get the 'prev' chunk and append 'new' data to it.
                // Error cases are not handled here as they were checked above.
                if let Some(prev_data) = self.buffer.get_mut(&pk) {
                    prev_data.append(&mut data);
                }
            }
            // PREPEND: [new][next]
            (None, true) => {
                // Remove the 'next' chunk from the buffer and get its data
                let mut next_data = self.buffer.remove(&new_end).unwrap_or_default();
                // Append 'next' data to the 'new' chunk and insert it into the buffer
                data.append(&mut next_data);
                self.buffer.insert(addr, data);
            }
            // NEW: [new]
            (None, false) => {
                self.buffer.insert(addr, data);
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    /// Parse the raw contents of the hex file and fill internal buffer.
    ///
    /// Parsing stops at the first end-of-file record, anything after it is ignored.
    ///
    /// # Errors
    /// - Returns an error if the record is corrupted
    /// - Returns an error if there is an issue during filling the internal buffer
    /// - Returns an error if there is no end-of-file record
    ///
    fn parse(&mut self, raw_bytes: &[u8]) -> Result<(), IntelHexError> {
        let raw_bytes = raw_bytes.strip_prefix(UTF8_BOM).unwrap_or(raw_bytes);

        // Line number of the last record seen (reported if the EOF record is missing)
        let mut last_line_no = 0;

        // Iterate over lines of records
        for (idx, line) in raw_bytes.split(|&b| b == b'\n').enumerate() {
            // Tolerate surrounding whitespace (CRLF, spaces, tabs)
            let line = line.trim_ascii();

            if line.is_empty() {
                continue;
            }

            let line_no = idx + 1;
            last_line_no = line_no;

            let record =
                Record::parse(line).map_err(|err| IntelHexError::ParseRecordError(err, line_no))?;

            // Fill in self
            match record.rtype {
                RecordType::Data => {
                    let addr = record.address as usize + self.offset;

                    if !record.data.is_empty() {
                        let new_end_addr = addr + record.data.len();

                        // Limit the address range to u32::MAX
                        if new_end_addr - 1 > u32::MAX as usize {
                            return Err(IntelHexError::ParseRecordError(
                                IntelHexErrorKind::AddressRangeOverflow,
                                line_no,
                            ));
                        }

                        // Check for overlaps
                        self.check_no_overlap(addr, new_end_addr)
                            .map_err(|overlap_addr| {
                                IntelHexError::ParseRecordError(
                                    IntelHexErrorKind::RecordAddressOverlap(overlap_addr),
                                    line_no,
                                )
                            })?;

                        // Insert data and merge with adjacent chunks
                        self.insert_chunk_with_merge(addr, record.data);
                    }
                }
                // EOF record must be the last record. Anything after it is ignored,
                // tolerating trailing junk some tools append (e.g. SUB / NUL padding).
                RecordType::EndOfFile => return Ok(()),
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
                            line_no,
                        ));
                    }

                    // Directly store the record slice.
                    // Line length is always 19 chars (9 bytes + ':').
                    // Error cases are not checked here as it was done during record parsing.
                    // TODO: split legacy and modern way of specifying start address?
                    if line.len() == 19
                        && let Ok(bytes) = line[1..19].try_into()
                    {
                        self.start_addr = Some(bytes);
                    }
                }
            }
        }

        // Reached the end without an EOF record
        Err(IntelHexError::ParseRecordError(
            IntelHexErrorKind::MissingEndOfFile,
            last_line_no,
        ))
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
    /// assert_eq!(ih.size, 68);
    /// ```
    pub fn from_hex<P: AsRef<Path>>(filepath: P) -> Result<Self, IntelHexError> {
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
    /// assert_eq!(ih.size, 68);
    /// ```
    pub fn load_hex<P: AsRef<Path>>(&mut self, filepath: P) -> Result<(), IntelHexError> {
        // Read the contents of the file
        let raw_bytes = std::fs::read(&filepath)?;

        // Clear provided IntelHex instance
        self.clear();

        // Load filepath
        self.filepath = filepath.as_ref().to_path_buf();

        // Parse contents
        self.parse(&raw_bytes)?;

        // Compute the payload size from the parsed buffer
        self.size = self.buffer.values().map(Vec::len).sum();

        Ok(())
    }

    /// Returns `true` if the content is Intel HEX, judged by its first record: the first
    /// non-blank line (after an optional UTF-8 BOM and surrounding whitespace) must be a
    /// valid record.
    ///
    /// Only the first record is checked to confirm the format. Records further down
    /// may still be invalid, loading the hex itself would report those.
    ///
    /// # Example
    /// ```
    /// use intelhexlib::IntelHex;
    ///
    /// assert!(IntelHex::is_intel_hex(b"\n:00000001FF\n"));
    /// assert!(!IntelHex::is_intel_hex(b":\x00\x01\x02 binary data"));
    /// ```
    #[must_use]
    pub fn is_intel_hex(raw_bytes: &[u8]) -> bool {
        let raw_bytes = raw_bytes.strip_prefix(UTF8_BOM).unwrap_or(raw_bytes);

        raw_bytes
            .split(|&b| b == b'\n')
            .map(<[u8]>::trim_ascii)
            .find(|line| !line.is_empty())
            .is_some_and(|line| Record::parse(line).is_ok())
    }

    /// Creates an `IntelHex` instance and fills it with data from the provided binary.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or parsed.
    /// Returns an error if the data extends beyond the 32-bit address range.
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
    ) -> Result<Self, IntelHexError> {
        let mut ih = Self::new();
        ih.load_bin(filepath, base_address)?;
        Ok(ih)
    }

    /// Fills an `IntelHex` instance with data from the provided binary.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or parsed.
    /// Returns an error if the data extends beyond the 32-bit address range.
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
    ) -> Result<(), IntelHexError> {
        // Read the contents of the file
        let data = std::fs::read(&filepath)?;

        // Limit the address range to u32::MAX
        if base_address > u32::MAX as usize || data.len() > u32::MAX as usize - base_address + 1 {
            return Err(IntelHexError::UpdateError(
                IntelHexErrorKind::AddressRangeOverflow,
            ));
        }

        // Clear provided IntelHex instance
        self.clear();

        // Compute the size (in bytes)
        self.size = data.len();

        // Load filepath
        self.filepath = filepath.as_ref().to_path_buf();

        // Load data bytes into the map as one chunk. Empty file holds no data.
        if !data.is_empty() {
            self.buffer.insert(base_address, data);
        }

        Ok(())
    }

    #[allow(clippy::cast_possible_truncation)]
    /// Generates an Intel HEX file at the specified path.
    ///
    /// > **NOTE**: Extended Segment Address (ESA) records are not supported,
    /// > Extended Linear Address (ELA) records are used instead.
    ///
    /// # Errors
    /// Returns an error if the file cannot be written. The file is replaced only after
    /// all data is written, so an existing file is left unchanged.
    /// Returns an error if the data extends beyond the 32-bit address range
    ///
    /// # Example
    /// ```
    /// use intelhexlib::IntelHex;
    ///
    /// let mut ih = IntelHex::from_hex("tests/fixtures/ih_valid_1.hex").unwrap();
    /// ih.write_hex("build/ex1/ih.hex");
    ///
    /// assert_eq!(ih.size, 68);
    /// ```
    pub fn write_hex<P: AsRef<Path>>(&mut self, filepath: P) -> Result<(), IntelHexError> {
        // Limit the address range to u32::MAX
        if let Some(max_addr) = self.get_max_addr()
            && max_addr > u32::MAX as usize
        {
            return Err(IntelHexError::CreateRecordError(
                IntelHexErrorKind::AddressRangeOverflow,
            ));
        }

        // Write to a temporary file first, so a failed write keeps the original file intact
        write_file_atomically(filepath.as_ref(), |writer| {
            // Write start address record (raw bytes + newline)
            if let Some(s) = self.start_addr {
                writer.write_all(b":")?;
                writer.write_all(&s)?;
                writeln!(writer)?;
            }

            let mut cur_high_addr = 0u16;

            for (&chunk_start, data) in &self.buffer {
                let mut chunk_offset = 0;

                // Iterate over data chunk
                while chunk_offset < data.len() {
                    let addr = chunk_start + chunk_offset;

                    // Split address into low and high
                    let high_addr = (addr >> 16) as u16;
                    let low_addr = (addr & 0xFFFF) as u16;

                    // If ELA segment changed -> emit ELA record
                    if cur_high_addr != high_addr {
                        let msb = (high_addr >> 8) as u8;
                        let lsb = (high_addr & 0xFF) as u8;

                        let record =
                            Record::create(0, RecordType::ExtendedLinearAddress, &[msb, lsb])?;

                        writeln!(writer, "{record}")?;

                        cur_high_addr = high_addr;
                    }

                    // Determine how many bytes can fit in this record
                    // - Can't exceed max_payload_size
                    // - Can't cross a 64KB boundary (must stay within current high_addr)
                    let remaining_in_segment = 0x10000 - low_addr as usize;
                    let chunk_size = std::cmp::min(
                        self.max_payload_size,
                        std::cmp::min(data.len() - chunk_offset, remaining_in_segment),
                    );

                    let record = Record::create(
                        low_addr,
                        RecordType::Data,
                        &data[chunk_offset..chunk_offset + chunk_size],
                    )?;
                    writeln!(writer, "{record}")?;

                    chunk_offset += chunk_size;
                }
            }

            // Write EOF record
            let record = Record::create(0, RecordType::EndOfFile, &[])?;
            write!(writer, "{record}")?; // write last line (no newline)

            Ok(())
        })
    }

    /// Generates a binary file at the specified path.
    /// Address gaps are filled with the provided `gap_fill` byte (usually 0x00 or 0xFF).
    ///
    /// # Errors
    /// Returns an error if the file cannot be written. The file is replaced only after
    /// all data is written, so an existing file is left unchanged.
    ///
    /// # Example
    /// ```
    /// use intelhexlib::IntelHex;
    ///
    /// let mut ih = IntelHex::from_hex("tests/fixtures/ih_valid_1.hex").unwrap();
    /// ih.write_bin("build/ex3/ih.bin", 0x00);
    ///
    /// assert_eq!(ih.size, 68);
    /// // Due to address gaps, the written bin file size is large
    /// assert_eq!(std::fs::metadata("build/ex3/ih.bin").unwrap().len(), 115264);
    /// ```
    pub fn write_bin<P: AsRef<Path>>(
        &mut self,
        filepath: P,
        gap_fill: u8,
    ) -> Result<(), IntelHexError> {
        // Write to a temporary file first, so a failed write keeps the original file intact
        write_file_atomically(filepath.as_ref(), |writer| {
            // Get the starting point
            let mut cur_addr = self.get_min_addr().unwrap_or(0);

            // Iterate over contiguous chunks of data
            for (&chunk_start, chunk_data) in &self.buffer {
                // Fill the gap between the last written byte and the start of this chunk
                if chunk_start > cur_addr {
                    let gap_size = chunk_start - cur_addr;

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

                // Write the entire contiguous chunk at once
                writer.write_all(chunk_data)?;

                // Advance the tracking address
                cur_addr = chunk_start + chunk_data.len();
            }

            Ok(())
        })
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
    /// let mut chunk_iter = ih.iter();
    /// let (first_key, first_chunk) = chunk_iter.next().unwrap();
    ///
    /// assert_eq!(*first_key, 0);
    /// assert_eq!(*first_chunk, vec![0xFA, 0x00, 0x00, 0x02]);
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
    /// let mut byte_iter = ih.bytes();
    /// let (first_key, first_byte) = byte_iter.next().unwrap();
    ///
    /// assert_eq!((first_key, first_byte), (0x00, 0xFA));
    /// ```
    pub fn bytes(&self) -> impl Iterator<Item = (usize, u8)> + '_ {
        self.buffer.iter().flat_map(|(&chunk_start, chunk_data)| {
            chunk_data
                .iter()
                .enumerate()
                .map(move |(offset, &byte)| (chunk_start + offset, byte))
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
    /// let mut byte_iter = ih.into_bytes();
    /// let (first_key, first_byte) = byte_iter.next().unwrap();
    ///
    /// assert_eq!((first_key, first_byte), (0x00, 0xFA));
    /// ```
    pub fn into_bytes(self) -> impl Iterator<Item = (usize, u8)> {
        self.buffer
            .into_iter()
            .flat_map(|(chunk_start, chunk_data)| {
                chunk_data
                    .into_iter()
                    .enumerate()
                    .map(move |(offset, byte)| (chunk_start + offset, byte))
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
    /// assert_eq!(min_addr, Some(0x0));
    /// ```
    #[must_use]
    pub fn get_min_addr(&self) -> Option<usize> {
        self.buffer
            .first_key_value()
            .map(|(chunk_start, _)| *chunk_start)
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
            .map(|(chunk_start, chunk_data)| *chunk_start + chunk_data.len() - 1)
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
    /// assert_eq!(byte, 0xFA);
    /// ```
    #[must_use]
    pub fn read_byte(&self, address: usize) -> Option<u8> {
        if let Some((&chunk_start, chunk_data)) = self.buffer.range(..=address).next_back()
            && address < chunk_start + chunk_data.len()
        {
            return Some(chunk_data[address - chunk_start]);
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
    /// assert_eq!(bytes, &[0xFA, 0x00, 0x00]);
    /// ```
    #[must_use]
    pub fn read_range(&self, start_addr: usize, len: usize) -> Option<Vec<u8>> {
        // Find the chunk that might contain the start_addr
        if let Some((&chunk_start, chunk_data)) = self.buffer.range(..=start_addr).next_back() {
            let end_addr = start_addr.checked_add(len)?;
            let chunk_end = chunk_start + chunk_data.len();

            // Check if the entire requested range is within this chunk
            if start_addr >= chunk_start && end_addr <= chunk_end {
                let chunk_offset = start_addr - chunk_start;
                return Some(chunk_data[chunk_offset..chunk_offset + len].to_vec());
            }
        }

        // If the range spans multiple chunks (contains gaps), return None
        None
    }

    /// Read a range of bytes, returning a `Vec<Option<u8>>`.
    /// Each element is `Some(byte)` if data exists at that address, or `None` if it is a gap.
    ///
    /// For a non-allocating alternative, see [`iter_range`](Self::iter_range).
    ///
    /// The range is clamped to the end of the address space (`usize::MAX`), so in case of
    /// an overflow fewer than `len` elements will be returned.
    ///
    /// # Example
    /// ```
    /// use intelhexlib::IntelHex;
    ///
    /// let ih = IntelHex::from_hex("tests/fixtures/ih_valid_1.hex").unwrap();
    /// let bytes: Vec<Option<u8>> = ih.read_range_safe(0x0, 5);
    ///
    /// assert_eq!(bytes, &[Some(0xFA), Some(0x00), Some(0x00), Some(0x02), None]);
    /// ```
    #[must_use]
    pub fn read_range_safe(&self, start_addr: usize, len: usize) -> Vec<Option<u8>> {
        let end_addr = start_addr.saturating_add(len);
        let mut result = Vec::with_capacity(end_addr - start_addr);
        let mut cur_addr = start_addr;

        while cur_addr < end_addr {
            // Find the chunk that contains or precedes current_addr
            if let Some((&chunk_start, chunk_data)) = self.buffer.range(..=cur_addr).next_back() {
                let chunk_end = chunk_start + chunk_data.len();

                // Case 1: current_addr is inside a chunk
                if cur_addr >= chunk_start && cur_addr < chunk_end {
                    let chunk_offset = cur_addr - chunk_start;

                    let available_size = chunk_end - cur_addr;
                    let needed_size = end_addr - cur_addr;
                    let len_used = std::cmp::min(available_size, needed_size);

                    // Extend with the actual data
                    result.extend(
                        chunk_data[chunk_offset..chunk_offset + len_used]
                            .iter()
                            .map(|&b| Some(b)),
                    );

                    cur_addr += len_used;
                    continue;
                }
            }

            // Case 2: current_addr is in a gap.
            // Find where the next chunk starts (if any) and fill the gap.
            let next_chunk_start = self
                .buffer
                .range(cur_addr..)
                .next()
                .map_or(end_addr, |(&s, _)| s);

            let gap_end = std::cmp::min(next_chunk_start, end_addr);
            let gap_len = gap_end - cur_addr;

            // Fill with None for the duration of the gap
            result.extend(std::iter::repeat_n(None, gap_len));
            cur_addr = gap_end;
        }

        result
    }

    /// Returns a borrowing iterator over individual bytes in the given address range.
    ///
    /// Yields `Some(byte)` for addresses that contain data, `None` for gaps.
    /// This is the iterator equivalent of [`read_range_safe`] which avoids heap
    /// allocation.
    ///
    /// The range is clamped to the end of the address space (`usize::MAX`), so in case of
    /// an overflow fewer than `len` elements will be yielded.
    ///
    /// # Example
    /// ```
    /// use intelhexlib::IntelHex;
    ///
    /// let ih = IntelHex::from_hex("tests/fixtures/ih_valid_1.hex").unwrap();
    /// let bytes: Vec<Option<u8>> = ih.iter_range(0x0, 5).collect();
    ///
    /// assert_eq!(bytes, &[Some(0xFA), Some(0x00), Some(0x00), Some(0x02), None]);
    /// ```
    #[must_use]
    pub const fn iter_range(&self, start_addr: usize, len: usize) -> IterRange<'_> {
        IterRange {
            buffer: &self.buffer,
            cur_addr: start_addr,
            end_addr: start_addr.saturating_add(len),
            current_chunk: None,
        }
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
        if let Some((&chunk_start, chunk_data)) = self.buffer.range_mut(..=address).next_back()
            && address < chunk_start + chunk_data.len()
        {
            chunk_data[address - chunk_start] = value;
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
    /// let res: Result<(), IntelHexError> = ih.update_slice(&[(0x0, 0xFF), (0x1, 0xFF), (0x2, 0xFF)]);
    ///
    /// assert!(res.is_ok());
    /// ```
    pub fn update_slice(&mut self, update_map: &[(usize, u8)]) -> Result<(), IntelHexError> {
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

        if let Some((&chunk_start, chunk_data)) = self.buffer.range_mut(..=start_addr).next_back()
            && let Some(end_addr) = start_addr.checked_add(len)
        {
            let chunk_end = chunk_start + chunk_data.len();

            // Check the entire range fits within a single chunk
            if start_addr >= chunk_start && end_addr <= chunk_end {
                let chunk_offset = start_addr - chunk_start;
                chunk_data[chunk_offset..chunk_offset + len].copy_from_slice(data);
                return Ok(());
            }
        }

        Err(IntelHexError::UpdateError(
            IntelHexErrorKind::InvalidAddress(start_addr),
        ))
    }

    /// Insert a new address range filled with 0x00 bytes into the `IntelHex` buffer.
    ///
    /// The range is inclusive: `[start_addr, end_addr]`.
    /// If the range overlaps with existing data, an error is returned and no changes are made.
    /// Adjacent chunks are merged automatically.
    ///
    /// # Errors
    /// Returns an error if `start_addr > end_addr`, the range exceeds 32-bit address space,
    /// or the range overlaps with existing data.
    ///
    /// # Example
    /// ```
    /// use intelhexlib::IntelHex;
    ///
    /// let mut ih = IntelHex::new();
    /// ih.write_range(0x1000, 0x100F).unwrap();
    ///
    /// assert!(ih.read_range(0x1000, 0xF).is_some());
    /// ```
    pub fn write_range(&mut self, start_addr: usize, end_addr: usize) -> Result<(), IntelHexError> {
        if start_addr > end_addr {
            return Err(IntelHexError::UpdateError(IntelHexErrorKind::InvalidRange(
                start_addr, end_addr,
            )));
        }

        if end_addr > u32::MAX as usize {
            return Err(IntelHexError::UpdateError(
                IntelHexErrorKind::AddressRangeOverflow,
            ));
        }

        let len = end_addr - start_addr + 1;

        // Check for overlaps
        self.check_no_overlap(start_addr, end_addr + 1)
            .map_err(|overlap_addr| {
                IntelHexError::UpdateError(IntelHexErrorKind::RecordAddressOverlap(overlap_addr))
            })?;

        // Insert data and merge with adjacent chunks
        self.insert_chunk_with_merge(start_addr, vec![0x00u8; len]);

        // Update total size
        self.size = self.buffer.values().map(Vec::len).sum();

        Ok(())
    }

    /// Remove data in the address range `[start_addr, end_addr]` from the `IntelHex` buffer.
    ///
    /// Chunks that overlap with the range are trimmed or split as needed.
    /// If the range contains no data, this is a no-op.
    ///
    /// # Errors
    /// Returns an error if `start_addr > end_addr`.
    ///
    /// # Example
    /// ```
    /// use intelhexlib::IntelHex;
    ///
    /// let mut ih = IntelHex::new();
    /// ih.write_range(0x1000, 0x100F).unwrap();
    /// ih.remove_range(0x1004, 0x100B).unwrap();
    ///
    /// assert_eq!(ih.read_byte(0x1003), Some(0x00));
    /// assert_eq!(ih.read_byte(0x1004), None);
    /// assert_eq!(ih.read_byte(0x100B), None);
    /// assert_eq!(ih.read_byte(0x100C), Some(0x00));
    /// ```
    pub fn remove_range(
        &mut self,
        start_addr: usize,
        end_addr: usize,
    ) -> Result<(), IntelHexError> {
        if start_addr > end_addr {
            return Err(IntelHexError::UpdateError(IntelHexErrorKind::InvalidRange(
                start_addr, end_addr,
            )));
        }

        // Collect keys of chunks that overlap with the removal range
        let affected_keys: Vec<usize> = self
            .buffer
            .range(..=end_addr)
            .filter(|&(&chunk_start, chunk_data)| chunk_start + chunk_data.len() > start_addr)
            .map(|(&k, _)| k)
            .collect();

        let mut new_chunks: Vec<(usize, Vec<u8>)> = Vec::new();

        for key in affected_keys {
            let Some(chunk_data) = self.buffer.remove(&key) else {
                continue;
            };
            let chunk_last = key + chunk_data.len() - 1;

            // Left fragment: portion before the removal range
            if key < start_addr {
                new_chunks.push((key, chunk_data[..start_addr - key].to_vec()));
            }

            // Right fragment: portion after the removal range.
            // `end_addr < chunk_last` guarantees `end_addr + 1` does not overflow.
            if chunk_last > end_addr {
                let right_start = end_addr + 1;
                new_chunks.push((right_start, chunk_data[right_start - key..].to_vec()));
            }
        }

        // Insert trimmed chunks
        for (addr, data) in new_chunks {
            self.buffer.insert(addr, data);
        }

        // Update total size
        self.size = self.buffer.values().map(Vec::len).sum();

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

    /// Relocate the entire data buffer to a new starting address.
    ///
    /// # Errors
    /// Returns an error if the relocated data would extend beyond the 32-bit address range.
    /// Returns an error if the `IntelHex` instance has no data.
    ///
    /// # Example
    /// ```
    /// use intelhexlib::IntelHex;
    ///
    /// let mut ih = IntelHex::from_hex("tests/fixtures/ih_valid_1.hex").unwrap();
    /// ih.relocate(0x1234);
    ///
    /// assert_eq!(ih.read_byte(0x0000), None);
    /// assert_eq!(ih.read_byte(0x1234), Some(0xFA));
    /// ```
    pub fn relocate(&mut self, new_start_address: usize) -> Result<(), IntelHexError> {
        let (min_addr, max_addr) =
            self.get_min_addr()
                .zip(self.get_max_addr())
                .ok_or(IntelHexError::UpdateError(
                    IntelHexErrorKind::IntelHexInstanceEmpty,
                ))?;

        // The relocated data must end within the 32-bit address range
        let max_allowed_start_address = (u32::MAX as usize).saturating_sub(max_addr - min_addr);

        if new_start_address > max_allowed_start_address {
            return Err(IntelHexError::UpdateError(
                IntelHexErrorKind::RelocateAddressOverflow(max_allowed_start_address),
            ));
        }

        self.buffer = std::mem::take(&mut self.buffer)
            .into_iter()
            .map(|(addr, data)| (addr - min_addr + new_start_address, data))
            .collect();

        Ok(())
    }

    /// Merge another `IntelHex` instance into this one.
    /// The merge is atomic: on error, this instance is left unchanged.
    ///
    /// # Errors
    /// Returns an error if any data chunks overlap.
    /// Returns an error if both this and other `IntelHex` instance have a start address
    /// record and the records differ. Identical records are accepted.
    ///
    /// # Example
    /// ```
    /// use intelhexlib::IntelHex;
    ///
    /// // Hex files do not have overlapping data chunks
    /// let mut ih1 = IntelHex::from_hex("tests/fixtures/ih_valid_1.hex").unwrap();
    /// let mut ih2 = IntelHex::from_hex("tests/fixtures/ih_valid_3.hex").unwrap();
    ///
    /// let res = ih1.merge_safe(&ih2);
    ///
    /// assert!(res.is_ok());
    /// ```
    pub fn merge_safe(&mut self, other: &Self) -> Result<(), IntelHexError> {
        // Validate everything first, so that on error this instance is left unchanged

        // Both files have a start address record, and they differ
        if let (Some(own), Some(incoming)) = (self.start_addr, other.start_addr)
            && own != incoming
        {
            return Err(IntelHexError::UpdateError(
                IntelHexErrorKind::DuplicateStartAddress,
            ));
        }

        // Chunks of `other` never overlap each other, so checking each one against
        // the current buffer is enough
        for (&chunk_start, chunk_data) in &other.buffer {
            self.check_no_overlap(chunk_start, chunk_start + chunk_data.len())
                .map_err(|overlap_addr| {
                    IntelHexError::UpdateError(IntelHexErrorKind::RecordAddressOverlap(
                        overlap_addr,
                    ))
                })?;
        }

        // Insert chunks (cannot fail). Adjacent ones are merged.
        for (&chunk_start, chunk_data) in &other.buffer {
            self.insert_chunk_with_merge(chunk_start, chunk_data.clone());
            self.size += chunk_data.len();
        }

        // Use the start address from the other file if we don't have one
        if self.start_addr.is_none() {
            self.start_addr = other.start_addr;
        }

        Ok(())
    }

    /// Merge another `IntelHex` instance into this one.
    ///
    /// Other `IntelHex` instance will overwrite any overlapping data and
    /// start address record (if present).
    ///
    /// # Example
    /// ```
    /// use intelhexlib::IntelHex;
    ///
    /// // Hex files have overlapping data chunk
    /// let mut ih1 = IntelHex::from_hex("tests/fixtures/ih_valid_2.hex").unwrap();
    /// let mut ih2 = IntelHex::from_hex("tests/fixtures/ih_valid_3.hex").unwrap();
    ///
    /// ih1.merge(&ih2);
    ///
    /// // Check for byte that was in `ih2` and not `ih1`
    /// assert_eq!(ih1.read_byte(0x100), Some(0x21));
    /// ```
    pub fn merge(&mut self, other: &Self) {
        for (&chunk_start, chunk_data) in &other.buffer {
            let mut new_start = chunk_start;
            let mut new_data = chunk_data.clone();
            let mut new_end = chunk_start + chunk_data.len();

            // Merge overlapping / adjacent keys in a dynamic loop to ensure
            // no contiguous chunks are split into separate vectors
            loop {
                // Find a chunk that overlaps or touches from the LEFT (prefix)
                let left_neighbor = self
                    .buffer
                    .range(..=new_start)
                    .next_back()
                    .filter(|&(&s, d)| s + d.len() >= new_start)
                    .map(|(&s, _)| s);

                if let Some(key) = left_neighbor
                    && let Some(cur_data) = self.buffer.remove(&key)
                {
                    let cur_end = key + cur_data.len();

                    // Merge prefix
                    let mut merged_data = cur_data[..new_start - key].to_vec();
                    merged_data.append(&mut new_data);

                    // Check if existing data also extended beyond the current end
                    if cur_end > new_end {
                        let mut suffix = cur_data[new_end - key..].to_vec();
                        merged_data.append(&mut suffix);
                    }

                    new_data = merged_data;
                    new_start = key;
                    new_end = new_start + new_data.len();
                    continue; // re-check with expanded range
                }

                // Find a chunk that starts WITHIN our range or touches the RIGHT (suffix)
                let right_neighbor = self
                    .buffer
                    .range(new_start..=new_end)
                    .next()
                    .map(|(&s, _)| s);

                if let Some(key) = right_neighbor
                    && let Some(cur_data) = self.buffer.remove(&key)
                {
                    let cur_end = key + cur_data.len();

                    // If the existing chunk extends beyond our current end, append its tail
                    if cur_end > new_end {
                        let mut suffix_data = cur_data[new_end - key..].to_vec();
                        new_data.append(&mut suffix_data);
                    }

                    new_end = new_start + new_data.len();
                    continue; // re-check with expanded range
                }
                break;
            }

            // Insert the contiguous chunk
            self.buffer.insert(new_start, new_data);
        }

        // Update total size (recompute to be safe with overwrites)
        self.size = self.buffer.values().map(Vec::len).sum();

        // Overwrite start address if the other has it
        if let Some(other_start_addr) = other.start_addr {
            self.start_addr = Some(other_start_addr);
        }
    }

    /// Window slide search for a byte array in the `IntelHex` data.
    /// Returns start addresses of matches, sorted in ascending order.
    /// Matches may overlap.
    ///
    /// # Example
    /// ```
    /// use intelhexlib::IntelHex;
    ///
    /// let mut ih = IntelHex::from_hex("tests/fixtures/ih_valid_1.hex").unwrap();
    /// let matches: Vec<usize> = ih.search_bytes(&[0x00, 0x00, 0x02]);
    ///
    /// assert_eq!(matches, vec![0x1]);
    /// ```
    #[must_use]
    pub fn search_bytes(&self, pattern: &[u8]) -> Vec<usize> {
        search::search_bytes(self.iter(), pattern)
    }

    /// Window slide search for an ASCII string in the `IntelHex` data.
    /// Returns start addresses of matches, sorted in ascending order.
    ///
    /// If `regex` flag is set to true, the pattern is interpreted as a regular expression
    /// (empty matches are skipped). Otherwise, it is interpreted as a literal string.
    ///
    /// # Errors
    /// Returns an error if `regex` is set and `pattern` is not a valid regular expression.
    ///
    /// # Example
    /// ```
    /// use intelhexlib::IntelHex;
    ///
    /// let mut ih = IntelHex::from_hex("tests/fixtures/ih_valid_1.hex").unwrap();
    /// let matches1 = ih.search_ascii("Kf", false).unwrap(); // [0x4B, 0x66] in hex bytes
    /// let matches2 = ih.search_ascii(r"H\d{1}\w{1}", true).unwrap(); // H0Y in the hex file
    ///
    /// assert_eq!(matches1, vec![0x1C216]);
    /// assert_eq!(matches2, vec![0x1C237]);
    /// assert!(ih.search_ascii("[", true).is_err());
    /// ```
    pub fn search_ascii(&self, pattern: &str, regex: bool) -> Result<Vec<usize>, IntelHexError> {
        if regex {
            search::search_regex(self.iter(), pattern).map_err(IntelHexError::SearchError)
        } else {
            Ok(search::search_bytes(self.iter(), pattern.as_bytes()))
        }
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
    fn test_read_byte_valid() {
        // Arrange
        let mut ih = IntelHex::new();
        let addr = 0x1234;
        let value = 0xFF;
        ih.buffer.insert(addr, vec![value]);

        // Act
        let byte = ih.read_byte(addr);

        // Assert
        assert_eq!(byte, Some(value));
    }

    #[test]
    fn test_read_byte_invalid() {
        // Arrange
        let mut ih = IntelHex::new();
        let addr = 0x1234;
        let value = 0xFF;
        ih.buffer.insert(addr, vec![value]);

        // Act
        let byte = ih.read_byte(addr - 1);

        // Assert
        assert!(byte.is_none());
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)]
    fn test_read_range_valid() {
        // Arrange
        let mut ih = IntelHex::new();

        let addr_start = 16;
        let length = 10;

        let mut addr_vec = Vec::with_capacity(length);

        for addr in addr_start..addr_start + length {
            // construct data vector where each byte is a corresponding address
            addr_vec.push(addr);
        }

        ih.buffer
            .insert(addr_start, addr_vec.iter().map(|&x| x as u8).collect());

        // Act
        let res_vec: Option<Vec<u8>> = ih.read_range(addr_start, length);

        // Assert
        assert_eq!(res_vec, Some(addr_vec.iter().map(|&x| x as u8).collect()));
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)]
    fn test_read_range_invalid() {
        // Arrange
        let mut ih = IntelHex::new();

        let addr_start = 16;
        let length = 10;

        let mut addr_vec = Vec::with_capacity(length);

        for addr in addr_start..addr_start + length {
            // construct data vector where each byte is a corresponding address
            addr_vec.push(addr);
        }

        ih.buffer.insert(
            addr_start,
            addr_vec
                .iter()
                .map(|&x| x as u8)
                .take(length - 1) // skip last byte
                .collect(),
        );

        // Act
        let res_vec: Option<Vec<u8>> = ih.read_range(addr_start, length);

        // Assert
        assert_eq!(res_vec, None);
    }

    #[test]
    #[allow(clippy::cast_possible_truncation, clippy::needless_collect)]
    fn test_iter_range_valid() {
        // Arrange: single contiguous chunk
        let mut ih = IntelHex::new();
        let addr_start = 16;
        let length = 10;
        let data: Vec<u8> = (addr_start..addr_start + length).map(|a| a as u8).collect();
        ih.buffer.insert(addr_start, data.clone());

        // Act
        let result: Vec<Option<u8>> = ih.iter_range(addr_start, length).collect();

        // Assert
        assert_eq!(result, data.iter().map(|&b| Some(b)).collect::<Vec<_>>());

        // Arrange: two chunks with a gap between them
        let mut ih2 = IntelHex::new();
        ih2.buffer.insert(0x00, vec![0xFA, 0x00, 0x00, 0x02]);
        ih2.buffer.insert(0x10, vec![0xAA, 0xBB, 0xCC, 0xDD]);

        // Act
        let result: Vec<Option<u8>> = ih2.iter_range(0x00, 0x14).collect();

        // Assert: range spanning both chunks yields None for the gap
        let expected: Vec<Option<u8>> = vec![
            Some(0xFA),
            Some(0x00),
            Some(0x00),
            Some(0x02), // chunk A
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None, // gap
            Some(0xAA),
            Some(0xBB),
            Some(0xCC),
            Some(0xDD), // chunk B
        ];
        assert_eq!(result, expected);

        // Act
        let safe_result: Vec<Option<u8>> = ih2.read_range_safe(0x00, 0x14);

        // Assert: consistency with read_range_safe
        assert_eq!(result, safe_result);

        // Act
        let result: Vec<Option<u8>> = ih2.iter_range(0x00, 0).collect();

        // Assert: zero-length range yields nothing
        assert!(result.is_empty());

        // Act
        let result: Vec<Option<u8>> = ih2.iter_range(0x10, 4).collect();

        // Assert: range exactly matching a chunk
        assert_eq!(result, vec![Some(0xAA), Some(0xBB), Some(0xCC), Some(0xDD)]);

        // Act
        let result: Vec<Option<u8>> = ih2.iter_range(0x00, 6).collect();

        // Assert: range starting before first chunk
        assert_eq!(
            result,
            vec![Some(0xFA), Some(0x00), Some(0x00), Some(0x02), None, None]
        );

        // Act + Assert: ExactSizeIterator::len() is correct
        let mut iter = ih2.iter_range(0x00, 0x14);
        assert_eq!(iter.len(), 0x14);
        iter.next();
        assert_eq!(iter.len(), 0x13);
        for _ in iter.by_ref().take(5) {}
        assert_eq!(iter.len(), 0x14 - 6);
    }

    #[test]
    fn test_iter_range_invalid() {
        // Arrange: single contiguous chunk
        let mut ih = IntelHex::new();
        ih.buffer.insert(0x10, vec![0xAA, 0xBB, 0xCC, 0xDD]);

        // Act
        let result: Vec<Option<u8>> = ih.iter_range(0x00, 4).collect();

        // Assert: range entirely outside any chunk yields all None
        assert_eq!(result, vec![None, None, None, None]);

        // Act
        let result: Vec<Option<u8>> = ih.iter_range(0x20, 3).collect();

        // Assert: range after all chunks
        assert_eq!(result, vec![None, None, None]);

        // Arrange: empty buffer
        let ih_empty = IntelHex::new();

        // Act
        let result: Vec<Option<u8>> = ih_empty.iter_range(0x00, 5).collect();

        // Assert: empty buffer yields all None
        assert_eq!(result, vec![None, None, None, None, None]);
    }

    #[test]
    fn test_update_byte_valid() {
        // Arrange
        let mut ih = IntelHex::new();
        let addr = 0x1234;
        let value = 0xFF;
        ih.buffer.insert(addr, vec![value]);

        // Act
        let res = ih.update_byte(addr, value - 1);

        // Assert
        assert!(res.is_ok());
        assert_eq!(*ih.buffer.get(&addr).unwrap_or(&vec![]), [value - 1]);
    }

    #[test]
    fn test_update_byte_invalid() {
        // Arrange
        let mut ih = IntelHex::new();
        let addr = 0x1234;
        let value = 0xFF;
        ih.buffer.insert(addr, vec![value]);

        // Act
        let res = ih.update_byte(addr - 1, value - 1);

        // Assert
        assert!(res.is_err());
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)]
    fn test_update_slice_valid() {
        // Arrange
        let mut ih = IntelHex::new();

        let addr_start = 16;
        let length = 10;

        let mut update_map: Vec<(usize, u8)> = Vec::with_capacity(length);

        for addr in addr_start..=addr_start + length {
            update_map.push((addr, (addr - 1) as u8));
        }

        ih.buffer
            .insert(addr_start, update_map.iter().map(|(_, x)| *x).collect());

        // Act
        let res = ih.update_slice(&update_map);

        // Assert
        assert!(res.is_ok());
        for addr in addr_start..=addr_start + length {
            assert_eq!(ih.read_byte(addr).unwrap_or_default(), (addr - 1) as u8);
        }
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)]
    fn test_update_slice_with_gap() {
        // Arrange
        let mut ih = IntelHex::new();

        let addr_start = 16;
        let length = 10;

        let mut update_map: Vec<(usize, u8)> = Vec::with_capacity(length);

        // todo: fail
        for addr in addr_start..addr_start + length {
            update_map.push((addr * 2, (addr + 2) as u8));
            ih.buffer.insert(addr * 2, vec![addr as u8]);
        }

        // Act
        let res = ih.update_slice(&update_map);

        // Assert
        assert!(res.is_ok());
        for (i, addr) in (addr_start..addr_start + length).enumerate() {
            assert_eq!(ih.read_byte(addr * 2), Some(update_map[i].1));
        }
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)]
    fn test_update_slice_invalid() {
        // Arrange
        let mut ih = IntelHex::new();

        let addr_start = 16;
        let length = 10;

        let mut update_map: Vec<(usize, u8)> = Vec::with_capacity(length);

        for addr in addr_start..=addr_start + length {
            update_map.push((addr, (addr - 1) as u8));
            ih.buffer.insert(addr, vec![addr as u8]);
        }
        ih.buffer.pop_last(); // pop the last addr

        // Act
        let res = ih.update_slice(&update_map);

        // Assert
        assert!(res.is_err());
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)]
    fn test_update_range_valid() {
        // Arrange
        let mut ih = IntelHex::new();

        let addr_start = 16;
        let length = 10;

        let mut update_map: Vec<(usize, u8)> = Vec::with_capacity(length);

        for addr in addr_start..=addr_start + length {
            update_map.push((addr, (addr + 7) as u8));
        }

        ih.buffer
            .insert(addr_start, update_map.iter().map(|(_, x)| *x).collect());

        // Act
        let res = ih.update_range(
            addr_start,
            update_map
                .iter()
                .map(|(_, x)| *x)
                .collect::<Vec<u8>>()
                .as_slice(),
        );

        // Assert
        assert!(res.is_ok());
        for addr in addr_start..=addr_start + length {
            assert_eq!(ih.read_byte(addr).unwrap_or_default(), (addr + 7) as u8);
        }
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)]
    fn test_update_range_invalid() {
        // Arrange
        let mut ih = IntelHex::new();

        let addr_start = 16;
        let length = 10;

        let mut update_map: Vec<(usize, u8)> = Vec::with_capacity(length);

        for addr in addr_start..=addr_start + length {
            update_map.push((addr, (addr + 7) as u8));
        }

        ih.buffer.insert(
            addr_start,
            update_map
                .iter()
                .map(|(_, x)| *x)
                .take(length - 1) // skip last byte
                .collect(),
        );

        // Act
        let res = ih.update_range(
            addr_start,
            update_map
                .iter()
                .map(|(_, x)| *x)
                .collect::<Vec<u8>>()
                .as_slice(),
        );

        // Assert
        assert!(res.is_err());
    }

    #[test]
    fn test_get_min_and_max_addr_valid() {
        // Arrange
        let mut ih = IntelHex::new();

        let addr_start = 10;
        let length = 10;

        ih.buffer.insert(addr_start, vec![0u8; 10]);

        // Act
        let min_addr = ih.get_min_addr();
        let max_addr = ih.get_max_addr();

        // Assert
        assert_eq!(min_addr, Some(addr_start));
        assert_eq!(max_addr, Some(addr_start + length - 1));

        // Arrange
        ih.buffer.clear();
        ih.buffer.insert(0, vec![0]);

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
        ih.buffer.insert(0xFFFF, vec![0xFF]);

        // Act
        let res = ih.relocate(0x0);

        // Assert
        assert!(res.is_ok());
        assert_eq!(ih.buffer.get(&0x0), Some(&vec![0xFFu8]));
    }

    #[test]
    fn test_relocate_invalid() {
        // Arrange
        let mut ih = IntelHex::new();
        ih.buffer.insert(0x0000, vec![0xFF]); // min addr
        ih.buffer.insert(0xFFFF, vec![0xFF]); // max addr

        let original = ih.buffer.clone();

        // Act & Assert - start addresses past the highest allowed one, incl. beyond 32-bit
        for new_start in [
            0xFFFF_0001,
            u32::MAX as usize,
            u32::MAX as usize + 1,
            usize::MAX,
        ] {
            let res = ih.relocate(new_start);

            assert_eq!(
                res,
                Err(IntelHexError::UpdateError(
                    IntelHexErrorKind::RelocateAddressOverflow(0xFFFF_0000)
                )),
                "new start 0x{new_start:X}"
            );
            assert_eq!(ih.buffer, original, "new start 0x{new_start:X}");
        }

        // Act & Assert - the highest allowed start address puts the last byte at u32::MAX
        assert!(ih.relocate(0xFFFF_0000).is_ok());
        assert_eq!(ih.get_min_addr(), Some(0xFFFF_0000));
        assert_eq!(ih.get_max_addr(), Some(u32::MAX as usize));
    }

    #[test]
    fn test_merge_valid() {
        // Arrange
        let mut ih1 = IntelHex::new();
        ih1.buffer.insert(0x00, vec![0x0, 0x0]);
        ih1.buffer.insert(0x20, vec![0x0, 0x0]);
        ih1.buffer.insert(0x24, vec![0x0, 0x0]);
        ih1.size = 6;

        let mut ih2 = IntelHex::new();
        ih2.buffer.insert(0x10, vec![0x1, 0x1]);
        ih2.buffer.insert(0x22, vec![0x1, 0x1]);
        ih2.start_addr = Some([
            48, 52, 48, 48, 48, 48, 48, 53, 49, 49, 50, 50, 51, 51, 52, 52, 52, 68,
        ]); // :04000005112233444D

        // Act
        ih1.merge(&ih2);

        // Assert
        assert_eq!(*ih1.buffer.get(&0x00).unwrap_or(&vec![]), vec![0x0, 0x0]);
        assert_eq!(*ih1.buffer.get(&0x10).unwrap_or(&vec![]), vec![0x1, 0x1]);
        assert_eq!(
            *ih1.buffer.get(&0x20).unwrap_or(&vec![]),
            vec![0x0, 0x0, 0x1, 0x1, 0x0, 0x0]
        );
        assert_eq!(ih1.start_addr, ih2.start_addr);
        assert_eq!(ih1.size, 10);

        // Arrange
        let mut ih3 = IntelHex::new();
        ih3.buffer.insert(0x04, vec![0x2]);
        ih3.buffer.insert(0x12, vec![0x2]);
        ih3.buffer.insert(0x24, vec![0x2, 0x2, 0x2]);
        ih3.start_addr = Some([
            48, 52, 48, 48, 48, 48, 48, 53, 49, 49, 50, 50, 51, 51, 52, 48, 53, 49,
        ]); // :040000051122334051

        // Act
        ih1.merge(&ih3);

        // Assert
        assert_eq!(*ih1.buffer.get(&0x00).unwrap_or(&vec![]), vec![0x0, 0x0]);
        assert_eq!(*ih1.buffer.get(&0x04).unwrap_or(&vec![]), vec![0x2]);
        assert_eq!(
            *ih1.buffer.get(&0x10).unwrap_or(&vec![]),
            vec![0x1, 0x1, 0x2]
        );
        assert_eq!(
            *ih1.buffer.get(&0x20).unwrap_or(&vec![]),
            vec![0x0, 0x0, 0x1, 0x1, 0x2, 0x2, 0x2]
        );
        assert_eq!(ih1.start_addr, ih3.start_addr);
        assert_eq!(ih1.size, 13);
    }

    #[test]
    fn test_merge_safe_valid() {
        // Arrange
        let mut ih1 = IntelHex::new();
        ih1.buffer.insert(0x00, vec![0x0, 0x0]);
        ih1.buffer.insert(0x20, vec![0x0, 0x0]);
        ih1.buffer.insert(0x24, vec![0x0, 0x0]);
        ih1.size = 6;

        let mut ih2 = IntelHex::new();
        ih2.buffer.insert(0x10, vec![0x1, 0x1]);
        ih2.buffer.insert(0x22, vec![0x1, 0x1]);
        ih2.start_addr = Some([
            48, 52, 48, 48, 48, 48, 48, 53, 49, 49, 50, 50, 51, 51, 52, 52, 52, 68,
        ]); // :04000005112233444D

        // Act
        let res = ih1.merge_safe(&ih2);

        // Assert
        assert!(res.is_ok());
        assert_eq!(*ih1.buffer.get(&0x00).unwrap_or(&vec![]), vec![0x0, 0x0]);
        assert_eq!(*ih1.buffer.get(&0x10).unwrap_or(&vec![]), vec![0x1, 0x1]);
        assert_eq!(
            *ih1.buffer.get(&0x20).unwrap_or(&vec![]),
            vec![0x0, 0x0, 0x1, 0x1, 0x0, 0x0]
        );
        assert_eq!(ih1.start_addr, ih2.start_addr);
        assert_eq!(ih1.size, 10);
    }

    #[test]
    fn test_merge_safe_invalid() {
        type Case = (
            &'static str,          // case name
            Vec<(usize, Vec<u8>)>, // chunks of the other file
            Option<[u8; 18]>,      // start address record
            IntelHexErrorKind,     // expected error
        );

        // :04000005112233444D
        let start_addr_a = [
            48, 52, 48, 48, 48, 48, 48, 53, 49, 49, 50, 50, 51, 51, 52, 52, 52, 68,
        ];
        // :04000005556677883D
        let start_addr_b = [
            48, 52, 48, 48, 48, 48, 48, 53, 53, 53, 54, 54, 55, 55, 56, 56, 51, 68,
        ];

        // Arrange
        let mut ih1 = IntelHex::new();
        ih1.buffer.insert(0x20, vec![0x0, 0x0, 0x0, 0x0]);
        ih1.size = 4;
        ih1.start_addr = Some(start_addr_a);
        let original = ih1.clone();

        let cases: Vec<Case> = vec![
            (
                "overlap from left side",
                vec![(0x1E, vec![0x1, 0x1, 0x1])],
                None,
                IntelHexErrorKind::RecordAddressOverlap(0x20),
            ),
            (
                "overlap within",
                vec![(0x22, vec![0x1])],
                None,
                IntelHexErrorKind::RecordAddressOverlap(0x22),
            ),
            (
                "overlap from right side",
                vec![(0x23, vec![0x1, 0x1, 0x1])],
                None,
                IntelHexErrorKind::RecordAddressOverlap(0x23),
            ),
            (
                "valid chunks (incl. adjacent) before an overlapping one",
                vec![
                    (0x00, vec![0x1, 0x1]),
                    (0x1E, vec![0x1, 0x1]),
                    (0x23, vec![0x1]),
                ],
                None,
                IntelHexErrorKind::RecordAddressOverlap(0x23),
            ),
            (
                "different start address, adjacent chunks",
                vec![(0x1E, vec![0x1, 0x1]), (0x24, vec![0x1, 0x1])],
                Some(start_addr_b),
                IntelHexErrorKind::DuplicateStartAddress,
            ),
        ];

        for (name, chunks, start_addr, expected) in cases {
            let mut ih2 = IntelHex::new();
            for (addr, data) in chunks {
                ih2.size += data.len();
                ih2.buffer.insert(addr, data);
            }
            ih2.start_addr = start_addr;

            // Act
            let res = ih1.merge_safe(&ih2);

            // Assert - error is returned and `ih1` is left unchanged
            assert_eq!(res, Err(IntelHexError::UpdateError(expected)), "{name}");
            assert_eq!(ih1.buffer, original.buffer, "{name}");
            assert_eq!(ih1.size, original.size, "{name}");
            assert_eq!(ih1.start_addr, original.start_addr, "{name}");
        }
    }

    #[test]
    fn test_merge_safe_identical_start_addr() {
        // Arrange
        let start_addr = Some([
            48, 52, 48, 48, 48, 48, 48, 53, 49, 49, 50, 50, 51, 51, 52, 52, 52, 68,
        ]); // :04000005112233444D

        let mut ih1 = IntelHex::new();
        ih1.buffer.insert(0x20, vec![0x0, 0x0]);
        ih1.size = 2;
        ih1.start_addr = start_addr;

        let mut ih2 = IntelHex::new();
        ih2.buffer.insert(0x22, vec![0x1, 0x1]);
        ih2.size = 2;
        ih2.start_addr = start_addr;

        // Act
        let res = ih1.merge_safe(&ih2);

        // Assert
        assert!(res.is_ok());
        assert_eq!(
            ih1.buffer,
            BTreeMap::from([(0x20, vec![0x0, 0x0, 0x1, 0x1])])
        );
        assert_eq!(ih1.size, 4);
        assert_eq!(ih1.start_addr, start_addr);
    }

    #[test]
    fn test_write_range_valid() {
        // Arrange - empty buffer
        let mut ih = IntelHex::new();

        // Act
        let res = ih.write_range(0x1000, 0x100F);

        // Assert
        assert!(res.is_ok());
        assert_eq!(ih.get_min_addr(), Some(0x1000));
        assert_eq!(ih.get_max_addr(), Some(0x100F));
        assert_eq!(ih.read_byte(0x1000), Some(0x00));
        assert_eq!(ih.read_byte(0x100F), Some(0x00));
        assert_eq!(ih.read_byte(0x1010), None);
        assert_eq!(ih.size, 16);
        assert_eq!(ih.buffer.len(), 1);

        // Arrange - single byte
        let mut ih = IntelHex::new();

        // Act
        let res = ih.write_range(0x00, 0x00);

        // Assert
        assert!(res.is_ok());
        assert_eq!(ih.get_min_addr(), Some(0x00));
        assert_eq!(ih.get_max_addr(), Some(0x00));
        assert_eq!(ih.read_byte(0x00), Some(0x00));
        assert_eq!(ih.size, 1);

        // Arrange - non-contiguous (gap between existing and new)
        let mut ih = IntelHex::new();
        ih.buffer.insert(0x00, vec![0xAA, 0xBB]);

        // Act
        let res = ih.write_range(0x1000, 0x1001);

        // Assert
        assert!(res.is_ok());
        assert_eq!(ih.get_min_addr(), Some(0x00));
        assert_eq!(ih.get_max_addr(), Some(0x1001));
        assert_eq!(ih.read_byte(0x00), Some(0xAA));
        assert_eq!(ih.read_byte(0x1000), Some(0x00));
        assert_eq!(ih.size, 4);
        assert_eq!(ih.buffer.len(), 2);

        // Arrange - merge with left neighbor
        let mut ih = IntelHex::new();
        ih.buffer.insert(0x1000, vec![0xAA, 0xBB]);
        ih.size = 2;

        // Act
        let res = ih.write_range(0x1002, 0x1003);

        // Assert
        assert!(res.is_ok());
        assert_eq!(ih.buffer.len(), 1);
        assert_eq!(ih.get_min_addr(), Some(0x1000));
        assert_eq!(ih.get_max_addr(), Some(0x1003));
        assert_eq!(ih.read_byte(0x1000), Some(0xAA));
        assert_eq!(ih.read_byte(0x1002), Some(0x00));
        assert_eq!(ih.size, 4);

        // Arrange - merge with right neighbor
        let mut ih = IntelHex::new();
        ih.buffer.insert(0x1002, vec![0xAA, 0xBB]);
        ih.size = 2;

        // Act
        let res = ih.write_range(0x1000, 0x1001);

        // Assert
        assert!(res.is_ok());
        assert_eq!(ih.buffer.len(), 1);
        assert_eq!(ih.get_min_addr(), Some(0x1000));
        assert_eq!(ih.get_max_addr(), Some(0x1003));
        assert_eq!(ih.read_byte(0x1000), Some(0x00));
        assert_eq!(ih.read_byte(0x1002), Some(0xAA));
        assert_eq!(ih.size, 4);

        // Arrange - bridge (merge with both neighbors)
        let mut ih = IntelHex::new();
        ih.buffer.insert(0x1000, vec![0xAA]);
        ih.buffer.insert(0x1004, vec![0xBB]);
        ih.size = 2;

        // Act
        let res = ih.write_range(0x1001, 0x1003);

        // Assert
        assert!(res.is_ok());
        assert_eq!(ih.buffer.len(), 1);
        assert_eq!(ih.get_min_addr(), Some(0x1000));
        assert_eq!(ih.get_max_addr(), Some(0x1004));
        assert_eq!(ih.read_byte(0x1000), Some(0xAA));
        assert_eq!(ih.read_byte(0x1001), Some(0x00));
        assert_eq!(ih.read_byte(0x1004), Some(0xBB));
        assert_eq!(ih.size, 5);
    }

    #[test]
    fn test_write_range_invalid() {
        // Arrange - start > end
        let mut ih = IntelHex::new();

        // Act
        let res = ih.write_range(0x100F, 0x1000);

        // Assert
        assert_eq!(
            res,
            Err(IntelHexError::UpdateError(IntelHexErrorKind::InvalidRange(
                0x100F, 0x1000
            )))
        );
        assert!(
            res.is_err_and(|e| e.to_string().ends_with(
                "Invalid range: start address 0x100F is greater than end address 0x1000"
            ))
        );

        // Arrange - overlap with existing chunk (start inside existing)
        let mut ih = IntelHex::new();
        ih.buffer.insert(0x1000, vec![0xAA, 0xBB, 0xCC]);
        ih.size = 3;

        // Act
        let res = ih.write_range(0x1001, 0x1005);

        // Assert
        assert_eq!(
            res,
            Err(IntelHexError::UpdateError(
                IntelHexErrorKind::RecordAddressOverlap(0x1001)
            ))
        );
        // Verify no mutation
        assert_eq!(ih.size, 3);
        assert_eq!(ih.read_byte(0x1001), Some(0xBB));

        // Arrange - overlap with next chunk (end overlaps next)
        let mut ih = IntelHex::new();
        ih.buffer.insert(0x1010, vec![0xAA]);
        ih.size = 1;

        // Act
        let res = ih.write_range(0x1000, 0x1010);

        // Assert
        assert_eq!(
            res,
            Err(IntelHexError::UpdateError(
                IntelHexErrorKind::RecordAddressOverlap(0x1010)
            ))
        );
        assert_eq!(ih.size, 1);
    }

    #[test]
    fn test_remove_range_valid() {
        // Arrange - remove entire chunk
        let mut ih = IntelHex::new();
        ih.buffer.insert(0x1000, vec![0xAA, 0xBB, 0xCC, 0xDD]);
        ih.size = 4;

        // Act
        let res = ih.remove_range(0x1000, 0x1003);

        // Assert
        assert!(res.is_ok());
        assert!(ih.buffer.is_empty());
        assert_eq!(ih.size, 0);

        // Arrange - split chunk in the middle
        let mut ih = IntelHex::new();
        ih.buffer.insert(0x1000, vec![0xAA, 0xBB, 0xCC, 0xDD, 0xEE]);
        ih.size = 5;

        // Act
        let res = ih.remove_range(0x1001, 0x1003);

        // Assert
        assert!(res.is_ok());
        assert_eq!(ih.buffer.len(), 2);
        assert_eq!(ih.read_byte(0x1000), Some(0xAA));
        assert_eq!(ih.read_byte(0x1001), None);
        assert_eq!(ih.read_byte(0x1003), None);
        assert_eq!(ih.read_byte(0x1004), Some(0xEE));
        assert_eq!(ih.size, 2);

        // Arrange - remove from start of chunk
        let mut ih = IntelHex::new();
        ih.buffer.insert(0x1000, vec![0xAA, 0xBB, 0xCC, 0xDD]);
        ih.size = 4;

        // Act
        let res = ih.remove_range(0x1000, 0x1001);

        // Assert
        assert!(res.is_ok());
        assert_eq!(ih.buffer.len(), 1);
        assert_eq!(ih.read_byte(0x1000), None);
        assert_eq!(ih.read_byte(0x1001), None);
        assert_eq!(ih.read_byte(0x1002), Some(0xCC));
        assert_eq!(ih.read_byte(0x1003), Some(0xDD));
        assert_eq!(ih.size, 2);

        // Arrange - remove from end of chunk
        let mut ih = IntelHex::new();
        ih.buffer.insert(0x1000, vec![0xAA, 0xBB, 0xCC, 0xDD]);
        ih.size = 4;

        // Act
        let res = ih.remove_range(0x1002, 0x1003);

        // Assert
        assert!(res.is_ok());
        assert_eq!(ih.buffer.len(), 1);
        assert_eq!(ih.read_byte(0x1000), Some(0xAA));
        assert_eq!(ih.read_byte(0x1001), Some(0xBB));
        assert_eq!(ih.read_byte(0x1002), None);
        assert_eq!(ih.read_byte(0x1003), None);
        assert_eq!(ih.size, 2);

        // Arrange - range spans multiple chunks with gap
        let mut ih = IntelHex::new();
        ih.buffer.insert(0x1000, vec![0xAA, 0xBB]);
        ih.buffer.insert(0x1008, vec![0xCC, 0xDD]);
        ih.size = 4;

        // Act
        let res = ih.remove_range(0x1000, 0x1009);

        // Assert
        assert!(res.is_ok());
        assert!(ih.buffer.is_empty());
        assert_eq!(ih.size, 0);

        // Arrange - trim edges of two chunks
        let mut ih = IntelHex::new();
        ih.buffer.insert(0x1000, vec![0xAA, 0xBB, 0xCC]);
        ih.buffer.insert(0x1008, vec![0xDD, 0xEE, 0xFF]);
        ih.size = 6;

        // Act
        let res = ih.remove_range(0x1001, 0x1008);

        // Assert
        assert!(res.is_ok());
        assert_eq!(ih.buffer.len(), 2);
        assert_eq!(ih.read_byte(0x1000), Some(0xAA));
        assert_eq!(ih.read_byte(0x1001), None);
        assert_eq!(ih.read_byte(0x1008), None);
        assert_eq!(ih.read_byte(0x1009), Some(0xEE));
        assert_eq!(ih.read_byte(0x100A), Some(0xFF));
        assert_eq!(ih.size, 3);

        // Arrange - range outside existing data
        let mut ih = IntelHex::new();
        ih.buffer.insert(0x1000, vec![0xAA, 0xBB]);
        ih.size = 2;

        // Act
        let res = ih.remove_range(0x2000, 0x200F);

        // Assert
        assert!(res.is_ok());
        assert_eq!(ih.buffer.len(), 1);
        assert_eq!(ih.read_byte(0x1000), Some(0xAA));
        assert_eq!(ih.size, 2);

        // Arrange - empty buffer
        let mut ih = IntelHex::new();

        // Act
        let res = ih.remove_range(0x1000, 0x100F);

        // Assert
        assert!(res.is_ok());
        assert!(ih.buffer.is_empty());
        assert_eq!(ih.size, 0);

        // Arrange - single byte
        let mut ih = IntelHex::new();
        ih.buffer.insert(0x1000, vec![0xAA]);
        ih.size = 1;

        // Act
        let res = ih.remove_range(0x1000, 0x1000);

        // Assert
        assert!(res.is_ok());
        assert!(ih.buffer.is_empty());
        assert_eq!(ih.size, 0);
    }

    #[test]
    fn test_remove_range_invalid() {
        // Arrange
        let mut ih = IntelHex::new();

        // Act
        let res = ih.remove_range(0x100F, 0x1000);

        // Assert
        assert_eq!(
            res,
            Err(IntelHexError::UpdateError(IntelHexErrorKind::InvalidRange(
                0x100F, 0x1000
            )))
        );
    }

    #[test]
    fn test_remove_range_near_usize_max() {
        // Arrange
        let mut ih = IntelHex::new();
        ih.buffer.insert(0x0, vec![0x1, 0x2, 0x3, 0x4]);
        ih.buffer.insert(0x1000, vec![0x5, 0x6, 0x7, 0x8]);
        ih.size = 8;

        // Act & Assert - range starting past all data is a no-op
        assert!(ih.remove_range(usize::MAX, usize::MAX).is_ok());
        assert_eq!(ih.size, 8);

        // Act & Assert - open-ended range trims the last chunk
        assert!(ih.remove_range(0x1002, usize::MAX).is_ok());
        assert_eq!(
            ih.buffer,
            BTreeMap::from([(0x0, vec![0x1, 0x2, 0x3, 0x4]), (0x1000, vec![0x5, 0x6])])
        );
        assert_eq!(ih.size, 6);

        // Act & Assert - full address space removes everything
        assert!(ih.remove_range(0, usize::MAX).is_ok());
        assert!(ih.buffer.is_empty());
        assert_eq!(ih.size, 0);
    }

    #[test]
    fn test_read_ops_near_usize_max() {
        // Arrange
        let mut ih = IntelHex::new();
        ih.buffer.insert(0x1000, vec![0x1, 0x2, 0x3, 0x4]);
        ih.size = 4;

        // `read_range` - a range past the end of the address space is never fully backed
        assert_eq!(ih.read_range(0x1000, usize::MAX), None);
        assert_eq!(ih.read_range(0x1002, usize::MAX - 0x1000), None);

        // `read_range_safe` - clamped to the end of the address space
        assert_eq!(ih.read_range_safe(usize::MAX - 2, 10), vec![None, None]);
        assert_eq!(ih.read_range_safe(usize::MAX, 1), vec![]);

        // `iter_range` - clamped to the end of the address space
        let iter = ih.iter_range(usize::MAX - 2, 10);
        assert_eq!(iter.len(), 2);
        assert_eq!(iter.collect::<Vec<_>>(), vec![None, None]);
        assert_eq!(
            ih.iter_range(0x1002, usize::MAX)
                .take(3)
                .collect::<Vec<_>>(),
            vec![Some(0x3), Some(0x4), None]
        );
    }

    #[test]
    fn test_update_range_near_usize_max() {
        // Arrange
        let mut ih = IntelHex::new();
        ih.buffer.insert(0x1000, vec![0x1, 0x2, 0x3, 0x4]);
        ih.size = 4;

        // Act & Assert - a range past the end of the address space is rejected, not a panic
        assert_eq!(
            ih.update_range(usize::MAX, &[0x0, 0x0]),
            Err(IntelHexError::UpdateError(
                IntelHexErrorKind::InvalidAddress(usize::MAX)
            ))
        );
        assert_eq!(
            ih.update_range(usize::MAX - 1, &[0x0, 0x0, 0x0]),
            Err(IntelHexError::UpdateError(
                IntelHexErrorKind::InvalidAddress(usize::MAX - 1)
            ))
        );
        assert_eq!(
            ih.buffer,
            BTreeMap::from([(0x1000, vec![0x1, 0x2, 0x3, 0x4])])
        );
    }

    // Valid records used by the parse / detection tests
    const REC_DATA_A: &str = ":0100000041BE"; // 1 byte 'A' at 0x0000
    const REC_DATA_B: &str = ":0100010042BC"; // 1 byte 'B' at 0x0001
    const REC_START: &str = ":0400000500000000F7"; // start linear address 0
    const REC_EOF: &str = ":00000001FF";

    #[test]
    fn test_parse_with_utf8_bom() {
        // Arrange
        let mut ih = IntelHex::new();
        let raw = format!("\u{FEFF}{REC_DATA_A}\n{REC_EOF}\n");

        // Act
        let res = ih.parse(raw.as_bytes());

        // Assert
        assert!(res.is_ok());
        assert_eq!(ih.read_byte(0x0), Some(b'A'));
    }

    #[test]
    fn test_parse_with_surrounding_whitespace() {
        // Arrange: blank lines, indentation, trailing spaces/tabs, CRLF
        let mut ih = IntelHex::new();
        let raw = format!("\n\r\n  {REC_DATA_A}  \r\n\t{REC_DATA_B}\t\n \n{REC_EOF}\r\n");

        // Act
        let res = ih.parse(raw.as_bytes());

        // Assert
        assert!(res.is_ok());
        assert_eq!(ih.read_byte(0x0), Some(b'A'));
        assert_eq!(ih.read_byte(0x1), Some(b'B'));
    }

    #[test]
    fn test_parse_error_line_counts_blank_lines() {
        // Arrange: bad record on physical line 4
        let mut ih = IntelHex::new();
        let raw = format!("{REC_DATA_A}\n\n\n:0100010042BD\n{REC_EOF}\n");

        // Act
        let res = ih.parse(raw.as_bytes());

        // Assert
        assert_eq!(
            res,
            Err(IntelHexError::ParseRecordError(
                IntelHexErrorKind::RecordChecksumMismatch(0xBC, 0xBD),
                4
            ))
        );
    }

    #[test]
    fn test_parse_duplicate_start_address_line() {
        // Arrange: second start address record on line 2
        let mut ih = IntelHex::new();
        let raw = format!("{REC_START}\n{REC_START}\n{REC_EOF}\n");

        // Act
        let res = ih.parse(raw.as_bytes());

        // Assert
        assert_eq!(
            res,
            Err(IntelHexError::ParseRecordError(
                IntelHexErrorKind::DuplicateStartAddress,
                2
            ))
        );
    }

    #[test]
    fn test_parse_address_range_limit() {
        // Arrange: ELA 0xFFFF, then 8 bytes at 0xFFFFFFF8 (last byte at exactly u32::MAX)
        let mut ih = IntelHex::new();
        let raw = format!(":02000004FFFFFC\n:08FFF800000000000000000001\n{REC_EOF}\n");

        // Act
        let res = ih.parse(raw.as_bytes());

        // Assert
        assert!(res.is_ok());
        assert_eq!(ih.get_max_addr(), Some(u32::MAX as usize));
    }

    #[test]
    fn test_parse_address_overflow_line() {
        // Arrange: ELA 0xFFFF, then 16 bytes at 0xFFFFFFF8 on line 2 (crosses u32::MAX)
        let mut ih = IntelHex::new();
        let raw =
            format!(":02000004FFFFFC\n:10FFF80000000000000000000000000000000000F9\n{REC_EOF}\n");

        // Act
        let res = ih.parse(raw.as_bytes());

        // Assert: the offending record's line is reported and not the last line
        assert_eq!(
            res,
            Err(IntelHexError::ParseRecordError(
                IntelHexErrorKind::AddressRangeOverflow,
                2
            ))
        );
    }

    #[test]
    fn test_parse_ignores_content_after_eof() {
        // Arrange: a valid record, a bad record and trailing junk after EOF
        let mut ih = IntelHex::new();
        let raw = format!("{REC_DATA_A}\n{REC_EOF}\n{REC_DATA_B}\n:0100010042BD\n\x1A\0\0");

        // Act
        let res = ih.parse(raw.as_bytes());

        // Assert
        assert!(res.is_ok());
        assert_eq!(ih.read_byte(0x0), Some(b'A'));
        assert_eq!(ih.read_byte(0x1), None);
    }

    #[test]
    fn test_parse_missing_eof() {
        // Arrange: last record on line 3, no EOF record
        let mut ih = IntelHex::new();
        let raw = format!("{REC_DATA_A}\n\n{REC_DATA_B}\n\n");

        // Act
        let res = ih.parse(raw.as_bytes());

        // Assert
        assert_eq!(
            res,
            Err(IntelHexError::ParseRecordError(
                IntelHexErrorKind::MissingEndOfFile,
                3
            ))
        );

        // Arrange: no records at all
        let mut ih = IntelHex::new();

        // Act
        let res = ih.parse(b" \r\n");

        // Assert
        assert_eq!(
            res,
            Err(IntelHexError::ParseRecordError(
                IntelHexErrorKind::MissingEndOfFile,
                0
            ))
        );
    }

    #[test]
    fn test_load_bin_address_overflow() {
        // Arrange
        let path = "tests/fixtures/ih_valid_1.bin";
        let len = 51596;
        let max_base = u32::MAX as usize - len + 1;

        // Act & Assert: last byte at exactly u32::MAX is accepted
        let mut ih = IntelHex::new();
        assert!(ih.load_bin(path, max_base).is_ok());
        assert_eq!(ih.get_max_addr(), Some(u32::MAX as usize));

        // Act & Assert: one byte beyond u32::MAX is rejected, instance unchanged
        let mut ih = IntelHex::new();
        ih.parse(format!("{REC_DATA_A}\n{REC_EOF}\n").as_bytes())
            .unwrap_or_default();
        let original = ih.clone();

        for base in [max_base + 1, u32::MAX as usize + 1, usize::MAX] {
            let res = ih.load_bin(path, base);

            assert_eq!(
                res,
                Err(IntelHexError::UpdateError(
                    IntelHexErrorKind::AddressRangeOverflow
                )),
                "base 0x{base:X}"
            );
            assert_eq!(ih.buffer, original.buffer, "base 0x{base:X}");
            assert_eq!(ih.size, original.size, "base 0x{base:X}");
        }
    }

    #[test]
    fn test_load_bin_empty_file() {
        // Arrange
        let path = "build/ut_load_bin_empty/empty.bin";
        std::fs::create_dir_all("build/ut_load_bin_empty").unwrap_or_default();
        std::fs::write(path, []).unwrap_or_default();

        // Act
        let mut ih = IntelHex::new();
        let res = ih.load_bin(path, 0x0);

        // Assert: no empty chunk is stored
        assert!(res.is_ok());
        assert!(ih.buffer.is_empty());
        assert_eq!(ih.size, 0);
        assert_eq!(ih.get_max_addr(), None);
    }

    #[test]
    fn test_write_hex_address_overflow() {
        // Arrange: data above the 32-bit range and an existing output file
        let mut ih = IntelHex::new();
        ih.buffer.insert(0xFFFF_FFF0, vec![0x0; 0x20]);
        ih.size = 0x20;

        let path = "build/ut_write_hex_overflow/ih.hex";
        std::fs::create_dir_all("build/ut_write_hex_overflow").unwrap_or_default();
        std::fs::write(path, "original").unwrap_or_default();

        // Act
        let res = ih.write_hex(path);

        // Assert: error is returned and the existing file is untouched
        assert_eq!(
            res,
            Err(IntelHexError::CreateRecordError(
                IntelHexErrorKind::AddressRangeOverflow
            ))
        );
        assert_eq!(
            std::fs::read_to_string(path).unwrap_or_default(),
            "original"
        );
    }

    /// Create an empty test directory under `build/` and return its path
    fn fresh_test_dir(name: &str) -> PathBuf {
        let dir = PathBuf::from("build").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap_or_default();
        dir
    }

    /// Number of entries in a directory (to check that no temporary files are left)
    fn dir_entry_count(dir: &Path) -> usize {
        std::fs::read_dir(dir).map_or(0, Iterator::count)
    }

    #[test]
    fn test_write_file_atomically_failure_keeps_original() {
        // Arrange
        let dir = fresh_test_dir("ut_atomic_failure");
        let path = dir.join("out.bin");
        std::fs::write(&path, "original").unwrap_or_default();

        // Act: some data is written, then the write fails
        let res = write_file_atomically(&path, |writer| {
            writer.write_all(b"partial data")?;
            Err(IntelHexError::Io(std::io::Error::other(
                "simulated failure",
            )))
        });

        // Assert: error, original untouched, no temporary file left behind
        assert!(matches!(res, Err(IntelHexError::Io(_))));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap_or_default(),
            "original"
        );
        assert_eq!(dir_entry_count(&dir), 1);
    }

    #[test]
    fn test_write_file_atomically_new_and_replace() {
        // Arrange - new file in a directory that does not exist yet
        let dir = fresh_test_dir("ut_atomic_replace");
        let path = dir.join("sub").join("out.bin");

        // Act & Assert - creates the file
        assert!(write_file_atomically(&path, |w| Ok(w.write_all(b"first")?)).is_ok());
        assert_eq!(std::fs::read_to_string(&path).unwrap_or_default(), "first");

        // Act & Assert - replaces the file
        assert!(write_file_atomically(&path, |w| Ok(w.write_all(b"2nd")?)).is_ok());
        assert_eq!(std::fs::read_to_string(&path).unwrap_or_default(), "2nd");
        assert_eq!(dir_entry_count(&dir.join("sub")), 1);
    }

    #[test]
    fn test_write_hex_read_only_target() {
        // Arrange
        let dir = fresh_test_dir("ut_atomic_read_only");
        let path = dir.join("ih.hex");
        std::fs::write(&path, "original").unwrap_or_default();
        if let Ok(meta) = std::fs::metadata(&path) {
            let mut permissions = meta.permissions();
            permissions.set_readonly(true);
            std::fs::set_permissions(&path, permissions).unwrap_or_default();
        }

        let mut ih = IntelHex::new();
        ih.buffer.insert(0x0, vec![0x1, 0x2]);
        ih.size = 2;

        // Act
        let res = ih.write_hex(&path);

        // Assert: refused, original untouched
        assert!(
            matches!(&res, Err(IntelHexError::Io(e)) if e.kind() == std::io::ErrorKind::PermissionDenied)
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap_or_default(),
            "original"
        );
        assert_eq!(dir_entry_count(&dir), 1);
    }

    #[cfg(unix)]
    #[test]
    fn test_write_bin_keeps_permissions_and_symlink() {
        use std::os::unix::fs::PermissionsExt;

        // Arrange: a file with custom permissions and a symlink pointing to it
        let dir = fresh_test_dir("ut_atomic_unix");
        let real = dir.join("real.bin");
        let link = dir.join("link.bin");
        std::fs::write(&real, "original").unwrap_or_default();
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o640)).unwrap_or_default();
        std::os::unix::fs::symlink("real.bin", &link).unwrap_or_default();

        let mut ih = IntelHex::new();
        ih.buffer.insert(0x0, vec![0x41, 0x42]);
        ih.size = 2;

        // Act: write through the symlink
        let res = ih.write_bin(&link, 0xFF);

        // Assert: target file replaced, link kept, permissions kept
        assert!(res.is_ok());
        assert_eq!(std::fs::read_to_string(&real).unwrap_or_default(), "AB");
        assert!(std::fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink()));
        assert_eq!(
            std::fs::metadata(&real)
                .map(|m| m.permissions().mode() & 0o777)
                .ok(),
            Some(0o640)
        );
        assert_eq!(dir_entry_count(&dir), 2);
    }

    #[test]
    fn test_write_file_atomically_long_file_name() {
        // Arrange: a name at the usual 255-byte limit
        let dir = fresh_test_dir("ut_atomic_long_name");
        let path = dir.join(format!("{}.bin", "a".repeat(251)));

        // Act & Assert: the temporary name stays within the limit
        assert!(write_file_atomically(&path, |w| Ok(w.write_all(b"data")?)).is_ok());
        assert_eq!(std::fs::read_to_string(&path).unwrap_or_default(), "data");
        assert_eq!(dir_entry_count(&dir), 1);
    }

    #[cfg(unix)]
    #[test]
    fn test_write_file_atomically_directory_not_writable() {
        use std::os::unix::fs::PermissionsExt;

        // Arrange: a writable file in a read-only directory
        let dir = fresh_test_dir("ut_atomic_read_only_dir");
        let path = dir.join("out.bin");
        std::fs::write(&path, "original").unwrap_or_default();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap_or_default();

        // Act
        let res = write_file_atomically(&path, |w| Ok(w.write_all(b"new")?));
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap_or_default();

        // Assert: refused with a clear error, original untouched
        assert_eq!(
            res,
            Err(IntelHexError::WriteError(
                IntelHexErrorKind::DirectoryNotWritable(
                    std::fs::canonicalize(&dir).unwrap_or_default()
                )
            ))
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap_or_default(),
            "original"
        );
        assert_eq!(dir_entry_count(&dir), 1);
    }

    #[cfg(unix)]
    #[test]
    fn test_write_file_atomically_hard_links() {
        // Arrange: a file with two hard links
        let dir = fresh_test_dir("ut_atomic_hard_link");
        let path = dir.join("a.bin");
        let other = dir.join("b.bin");
        std::fs::write(&path, "original").unwrap_or_default();
        std::fs::hard_link(&path, &other).unwrap_or_default();

        // Act
        let res = write_file_atomically(&path, |w| Ok(w.write_all(b"new")?));

        // Assert: refused, both names still show the original data
        assert_eq!(
            res,
            Err(IntelHexError::WriteError(
                IntelHexErrorKind::FileHasHardLinks(2)
            ))
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap_or_default(),
            "original"
        );
        assert_eq!(
            std::fs::read_to_string(&other).unwrap_or_default(),
            "original"
        );
        assert_eq!(dir_entry_count(&dir), 2);
    }

    #[cfg(unix)]
    #[test]
    fn test_write_file_atomically_broken_symlink() {
        // Arrange: a symlink to a file that does not exist
        let dir = fresh_test_dir("ut_atomic_broken_symlink");
        let link = dir.join("link.bin");
        std::os::unix::fs::symlink("missing.bin", &link).unwrap_or_default();

        // Act
        let res = write_file_atomically(&link, |w| Ok(w.write_all(b"new")?));

        // Assert: refused, the link is kept
        assert_eq!(
            res,
            Err(IntelHexError::WriteError(IntelHexErrorKind::BrokenSymlink(
                PathBuf::from("missing.bin")
            )))
        );
        assert!(std::fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink()));
        assert_eq!(dir_entry_count(&dir), 1);
    }

    #[test]
    fn test_is_intel_hex_valid() {
        assert!(IntelHex::is_intel_hex(REC_EOF.as_bytes()));
        assert!(IntelHex::is_intel_hex(
            format!("{REC_DATA_A}\n{REC_EOF}\n").as_bytes()
        ));
        // BOM, blank lines and indentation before the first record
        assert!(IntelHex::is_intel_hex(
            format!("\u{FEFF}\r\n\n  {REC_DATA_A}\r\n").as_bytes()
        ));
        // Only the first record is checked (a truncated file start is fine)
        assert!(IntelHex::is_intel_hex(
            format!("{REC_DATA_A}\n:10C20000E0A5").as_bytes()
        ));
    }

    #[test]
    fn test_is_intel_hex_invalid() {
        // Empty or blank
        assert!(!IntelHex::is_intel_hex(b""));
        assert!(!IntelHex::is_intel_hex(b" \r\n\t\n"));
        assert!(!IntelHex::is_intel_hex(b"\xEF\xBB\xBF"));
        // Binary that starts with ':' (0x3A)
        assert!(!IntelHex::is_intel_hex(b":\x00\x01\x02\xFF\xFE"));
        assert!(!IntelHex::is_intel_hex(b":"));
        // Text that starts with ':' but is not a valid record
        assert!(!IntelHex::is_intel_hex(b":hello world\n"));
        assert!(!IntelHex::is_intel_hex(b":0100000041BF\n")); // bad checksum
        assert!(!IntelHex::is_intel_hex(b":01000000\n")); // too short
        // First record must come first: junk before it is not skipped
        assert!(!IntelHex::is_intel_hex(
            format!("junk\n{REC_EOF}\n").as_bytes()
        ));
        // Other formats
        assert!(!IntelHex::is_intel_hex(b"\x7FELF\x02\x01\x01\x00"));
    }
}

// =====================  BENCH ACCESS FOR PRIVATE FUNCTIONS  =====================

#[allow(clippy::items_after_test_module)]
impl IntelHex {
    /// Parse raw Intel HEX bytes without file I/O (benchmark-only access to `parse`).
    ///
    /// # Errors
    /// Returns the parser's error if `raw_bytes` is not valid Intel HEX.
    #[cfg(feature = "benchmarking")]
    pub fn bench_priv_parse(ih: &mut Self, raw_bytes: &[u8]) -> Result<(), IntelHexError> {
        ih.parse(raw_bytes)
    }
}
