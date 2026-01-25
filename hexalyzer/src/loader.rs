use crate::app::{HexSession, HexViewerApp};
use intelhexlib::IntelHex;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

#[derive(Debug)]
pub enum FileKind {
    Hex,
    Bin,
    Elf,
    Unknown,
}

/// Get the last modified time of the file
pub fn get_last_modified(path: &PathBuf) -> std::io::Result<std::time::SystemTime> {
    std::fs::metadata(path)
        .map(|meta| meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH))
}

fn detect_file_kind(path: &PathBuf) -> std::io::Result<FileKind> {
    let mut f = File::open(path)?;

    // Read the first 32 bytes (OK if file has less)
    let mut buf = [0u8; 32];
    let n = f.read(&mut buf)?;
    let buf = &buf[..n];

    // If read 0 bytes -> Unknown
    if n == 0 {
        return Ok(FileKind::Unknown);
    }

    // ELF magic check
    if buf.len() >= 4 && &buf[..4] == b"\x7FELF" {
        return Ok(FileKind::Elf);
    }

    // Intel HEX record start check
    if buf[0] == b':' {
        return Ok(FileKind::Hex);
    }

    // Otherwise consider the file as raw binary
    Ok(FileKind::Bin)
}

impl HexViewerApp {
    /// Load hex file from disk and add it to the list of opened sessions.
    /// If the file is already open, switch to it.
    /// If the maximum number of tabs is reached, display an error message.
    pub(crate) fn load_file(&mut self, path: &PathBuf) {
        // Check if the file is already open
        if let Some(index) = self.sessions.iter().position(|s| s.ih.filepath == *path) {
            self.active_index = Some(index);
            return;
        }

        // Prevent loading more files than allowed by the app settings
        if self.sessions.len() >= self.max_tabs {
            self.error
                .borrow_mut()
                .replace("Maximum number of tabs reached".into());
            return;
        }

        let mut ih = IntelHex::new();

        let file_type = match detect_file_kind(path) {
            Ok(kind) => kind,
            Err(err) => {
                self.error.borrow_mut().replace(err.to_string());
                return;
            }
        };

        let res = match file_type {
            FileKind::Hex => ih.load_hex(path),
            FileKind::Bin => {
                // Set base addr to 0 to avoid complex logic around waiting
                // to fill the pop-up. Can re-addr later.
                ih.load_bin(path, 0)
            }
            FileKind::Elf => Err("ELF files are not yet supported".into()),
            FileKind::Unknown => Err("Could not determine the file type".into()),
        };

        if let Err(msg) = res {
            self.error.borrow_mut().replace(msg.to_string());
            return;
        }

        // Get the last modified time of the file
        let last_modified = match get_last_modified(path) {
            Ok(time) => time,
            Err(err) => {
                self.error.borrow_mut().replace(err.to_string());
                return;
            }
        };

        let mut new_session = HexSession {
            name: path.file_name().map_or_else(
                || "Untitled".to_string(),
                |n| n.to_string_lossy().into_owned(),
            ),
            last_modified,
            events: self.events.clone(), // clone the pointer
            error: self.error.clone(),   // clone the pointer
            ..HexSession::default()
        };

        // Load the IntelHex
        new_session.ih = ih;

        // Re-calculate address range
        new_session.addr =
            new_session.ih.get_min_addr().unwrap_or(0)..=new_session.ih.get_max_addr().unwrap_or(0);

        // Add the new session and switch to it
        self.sessions.push(new_session);
        self.active_index = Some(self.sessions.len() - 1);
    }

    /// Close the file with the given ID. When the file is closed, switch to the first one.
    pub(crate) fn close_file(&mut self, session_id: usize) {
        self.sessions.remove(session_id);

        if self.sessions.is_empty() {
            self.active_index = None;
        } else {
            self.active_index = Some(0);
        }
    }
}
