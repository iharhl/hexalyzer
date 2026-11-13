use crate::app::{HexSession, HexViewerApp};
use crate::byteedit::ByteEdit;
use eframe::egui;
use intelhexlib::IntelHex;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileKind {
    Hex,
    Bin,
    Elf,
    Unknown,
}

/// Get the last modified time of the file
pub fn get_last_modified(path: &PathBuf) -> std::io::Result<std::time::SystemTime> {
    std::fs::metadata(path).map(|meta| meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH))
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

/// Determine file kind from a file path extension
pub fn kind_from_extension(path: &std::path::Path) -> FileKind {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("hex") => FileKind::Hex,
        _ => FileKind::Bin,
    }
}

/// Load a file into an `IntelHex` instance. Returns the detected `FileKind` on success.
fn load_file_into_ih(path: &PathBuf) -> Result<(IntelHex, FileKind), String> {
    let file_kind = detect_file_kind(path).map_err(|e| e.to_string())?;

    let mut ih = IntelHex::new();
    match file_kind {
        FileKind::Hex => ih.load_hex(path).map_err(|e| e.to_string()),
        FileKind::Bin => ih.load_bin(path, 0).map_err(|e| e.to_string()),
        FileKind::Elf => Err("ELF files are not yet supported".to_string()),
        FileKind::Unknown => Err("Could not determine the file type".to_string()),
    }?;

    Ok((ih, file_kind))
}

/// Write an `IntelHex` instance to a file in the given format
pub fn write_ih_to_path(
    ih: &mut IntelHex,
    path: &std::path::Path,
    kind: &FileKind,
    gap_fill: u8,
) -> Result<(), String> {
    match kind {
        FileKind::Hex => ih.write_hex(path).map_err(|e| e.to_string()),
        FileKind::Bin => ih.write_bin(path, gap_fill).map_err(|e| e.to_string()),
        _ => Err("Cannot write: unknown file format".to_string()),
    }
}

/// Gap-fill size above which a `.bin` write must be confirmed by the user
const LARGE_GAP_FILL_SIZE: usize = 64 * 1024 * 1024;

/// Number of gap-fill bytes a `.bin` write adds on top of the actual data.
pub fn bin_gap_fill_size(ih: &IntelHex) -> usize {
    match (ih.get_min_addr(), ih.get_max_addr()) {
        (Some(min), Some(max)) => (max - min).saturating_add(1).saturating_sub(ih.size),
        _ => 0,
    }
}

/// Format a byte count for display, e.g. `512 B`, `1.5 KiB`, `4.0 GiB`
pub fn format_size(bytes: usize) -> String {
    const UNITS: [&str; 4] = ["KiB", "MiB", "GiB", "TiB"];

    if bytes < 1024 {
        return format!("{bytes} B");
    }

    let bytes = bytes as u128;
    let mut unit = 1024u128;
    let mut idx = 0;
    while idx + 1 < UNITS.len() && bytes >= unit * 1024 {
        unit *= 1024;
        idx += 1;
    }
    let tenths = bytes * 10 / unit;
    format!("{}.{} {}", tenths / 10, tenths % 10, UNITS[idx])
}

/// Ask the user to confirm a `.bin` write that is mostly gap fill.
/// Returns `true` if the write should proceed.
fn confirm_large_write(ih: &IntelHex, kind: &FileKind) -> bool {
    if *kind != FileKind::Bin {
        return true;
    }

    let gap_fill = bin_gap_fill_size(ih);
    if gap_fill < LARGE_GAP_FILL_SIZE {
        return true;
    }

    let result = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Warning)
        .set_title("Large file")
        .set_description(format!(
            "This will write a {} binary file: {} of data, the rest is gap fill.\n\n\
             Continue?",
            format_size(gap_fill.saturating_add(ih.size)),
            format_size(ih.size),
        ))
        .set_buttons(rfd::MessageButtons::OkCancel)
        .show();

    matches!(result, rfd::MessageDialogResult::Ok)
}

/// Returns `true` if the file at `path` was modified on disk since `last_modified`
/// (the time it was loaded or last saved by this app).
fn changed_on_disk(path: &PathBuf, last_modified: std::time::SystemTime) -> bool {
    get_last_modified(path)
        .is_ok_and(|t| t != std::time::SystemTime::UNIX_EPOCH && t != last_modified)
}

/// Ask the user to confirm overwriting a file that was modified on disk since it was
/// loaded. Returns `true` if the write should proceed.
fn confirm_overwrite_if_changed(path: &PathBuf, last_modified: std::time::SystemTime) -> bool {
    if !changed_on_disk(path, last_modified) {
        return true;
    }

    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );

    let result = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Warning)
        .set_title("File changed on disk")
        .set_description(format!(
            "\"{name}\" has been modified on disk since it was opened.\n\n\
             Overwrite it with your version? The changes on disk will be lost."
        ))
        .set_buttons(rfd::MessageButtons::OkCancel)
        .show();

    matches!(result, rfd::MessageDialogResult::Ok)
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
            self.error = Some("Maximum number of tabs reached".into());
            return;
        }

        let (ih, file_kind) = match load_file_into_ih(path) {
            Ok(result) => result,
            Err(msg) => {
                self.error = Some(msg);
                return;
            }
        };

        // Get the last modified time of the file
        let last_modified = match get_last_modified(path) {
            Ok(time) => time,
            Err(err) => {
                self.error = Some(err.to_string());
                return;
            }
        };

        // Determine unique scroll widget id
        let scroll_id = self.next_scroll_id;
        self.next_scroll_id += 1;

        let mut new_session = HexSession {
            name: path.file_name().map_or_else(
                || "Untitled".to_string(),
                |n| n.to_string_lossy().into_owned(),
            ),
            last_modified,
            file_kind,
            scroll_id,
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

    /// Returns `true` if the session has unsaved in-memory edits
    pub(crate) fn has_unsaved_changes(&self, session_id: usize) -> bool {
        self.sessions
            .get(session_id)
            .is_some_and(|s| s.dirty || !s.editor.modified.is_empty())
    }

    /// Reload the file from disk, replacing in-memory data.
    /// Resets editor state and updates the last-modified timestamp.
    pub(crate) fn reload_file(&mut self, session_id: usize) {
        let Some(session) = self.sessions.get(session_id) else {
            return;
        };

        let path = session.ih.filepath.clone();
        if path.as_os_str().is_empty() {
            self.error = Some("No file path associated with this session".into());
            return;
        }

        let (ih, file_kind) = match load_file_into_ih(&path) {
            Ok(result) => result,
            Err(msg) => {
                self.error = Some(msg);
                return;
            }
        };

        let last_modified = match get_last_modified(&path) {
            Ok(time) => time,
            Err(err) => {
                self.error = Some(err.to_string());
                return;
            }
        };

        let Some(session) = self.sessions.get_mut(session_id) else {
            return;
        };

        session.ih = ih;
        session.addr =
            session.ih.get_min_addr().unwrap_or(0)..=session.ih.get_max_addr().unwrap_or(0);
        session.last_modified = last_modified;
        session.file_changed_on_disk = false;
        session.file_kind = file_kind;
        session.editor = ByteEdit::default();
        session.dirty = false;
        session.search.redo();
    }

    /// Save the current session back to its original file path.
    /// Writes in the same format the file was loaded as.
    /// Clears the dirty state on success.
    ///
    /// Returns `true` on success. Returns `false` if the write failed (error stored in
    /// `self.error`) or the user canceled a large-file confirmation.
    pub(crate) fn save_curr_session(&mut self) -> bool {
        let gap_fill = self.gap_fill;

        let Some(session) = self.get_curr_session_mut() else {
            return false;
        };

        let path = session.ih.filepath.clone();
        if path.as_os_str().is_empty() {
            self.error = Some("No file path associated with this session".into());
            return false;
        }

        let file_kind = session.file_kind.clone();

        if !confirm_overwrite_if_changed(&path, session.last_modified) {
            return false;
        }

        if !confirm_large_write(&session.ih, &file_kind) {
            return false;
        }

        if let Err(msg) = write_ih_to_path(&mut session.ih, &path, &file_kind, gap_fill) {
            self.error = Some(msg);
            return false;
        }

        let Some(session) = self.get_curr_session_mut() else {
            return false;
        };

        session.editor.modified.clear();
        session.dirty = false;
        session.last_modified =
            get_last_modified(&path).unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        session.file_changed_on_disk = false;

        true
    }

    /// Show an Export dialog and write a copy of the current session to the chosen path.
    /// The original session is untouched.
    ///
    /// The format is derived from the extension (defaults to `.bin`).
    ///
    /// Returns `true` on success.
    pub(crate) fn export_curr_session(&mut self) -> bool {
        let Some(curr_index) = self.active_index else {
            return false;
        };

        let path = self.get_curr_session().and_then(|s| {
            rfd::FileDialog::new()
                .set_title("Export")
                .set_file_name(s.name.clone())
                .save_file()
        });

        let Some(mut path) = path else {
            return false;
        };

        if path.extension().is_none() {
            path.set_extension("bin");
        }

        let gap_fill = self.gap_fill;
        let kind = kind_from_extension(&path);

        let Some(session) = self.get_curr_session_mut() else {
            return false;
        };

        if !confirm_large_write(&session.ih, &kind) {
            return false;
        }

        if let Err(msg) = write_ih_to_path(&mut session.ih, &path, &kind, gap_fill) {
            self.error = Some(msg);
            return false;
        }

        match self.sessions.iter().position(|s| s.ih.filepath == path) {
            // Export over the current file itself is equivalent to a save
            Some(index) if index == curr_index => {
                let session = &mut self.sessions[index];
                session.editor.modified.clear();
                session.dirty = false;
                session.file_kind = kind;
                session.last_modified =
                    get_last_modified(&path).unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                session.file_changed_on_disk = false;
            }
            // Export over a file in another tab - refresh that tab and switch to it
            Some(index) => {
                self.reload_file(index);
                self.active_index = Some(index);
            }
            // Open exported copy in a new tab and switch to it
            None => self.load_file(&path),
        }

        true
    }

    /// Intercepts save shortcut at the start of the frame and performs the save
    /// of the current session to its file if it has unsaved changes.
    pub(crate) fn handle_save_shortcut(&mut self, ctx: &egui::Context) {
        let pressed = ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S));
        if !pressed || self.popup.active {
            return;
        }

        let Some(session_id) = self.active_index else {
            return;
        };
        let has_filepath = self
            .get_curr_session()
            .is_some_and(|s| !s.ih.filepath.as_os_str().is_empty());

        if has_filepath && self.has_unsaved_changes(session_id) {
            self.save_curr_session();
        }
    }

    /// Merges the content of a file into the current `IntelHex` session.
    ///
    /// # Parameters
    /// - `path`: Reference to a `PathBuf` that represents the path of the file to be merged.
    /// - `addr1`: Optional start addr to which the current session's `IntelHex` instance should be relocated.
    /// - `addr2`: Optional start addr to which the contents of the new file should be relocated.
    pub(crate) fn merge_file_into_curr_session(
        &mut self,
        path: &PathBuf,
        addr1: Option<usize>,
        addr2: Option<usize>,
    ) {
        if let Some(cur_session) = self.get_curr_session_mut() {
            // Relocate the current file to a new start address
            if let Some(new_start_addr) = addr1 {
                let old_start_addr = cur_session.ih.get_min_addr();

                let res = cur_session.ih.relocate(new_start_addr);

                if let Err(msg) = res {
                    self.error = Some(msg.to_string());
                    return;
                }

                if let Some(old_start_addr) = old_start_addr {
                    cur_session
                        .editor
                        .remap_modified(new_start_addr, old_start_addr);
                }
            }

            // Load the selected file into a new IntelHex instance
            let (mut new_ih, _) = match load_file_into_ih(path) {
                Ok(result) => result,
                Err(msg) => {
                    self.error = Some(msg);
                    return;
                }
            };

            // Relocate the selected file to a new start address
            if let Some(new_start_addr) = addr2 {
                let res = new_ih.relocate(new_start_addr);

                if let Err(msg) = res {
                    self.error = Some(msg.to_string());
                    return;
                }
            }

            // Merge the two IntelHex instances
            cur_session.ih.merge(&new_ih);
        } else {
            self.error = Some("Could not get current hex session".to_string());
        }
    }
}
