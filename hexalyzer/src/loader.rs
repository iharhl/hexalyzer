use crate::app::{HexSession, HexViewerApp};
use crate::byteedit::ByteEdit;
use crate::ui_popup::PopupState;
use eframe::egui;
use intelhexlib::IntelHex;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileKind {
    Hex,
    Bin,
    Elf,
    Unknown,
}

/// Returns `true` if both paths point to the same file, even if spelled differently.
/// Falls back to comparing the paths as-is when either file cannot be accessed
/// (e.g. does not exist).
fn is_same_file(a: &Path, b: &Path) -> bool {
    a == b || same_file::is_same_file(a, b).unwrap_or(false)
}

/// Get the last modified time of the file
pub fn get_last_modified(path: &PathBuf) -> std::io::Result<std::time::SystemTime> {
    std::fs::metadata(path).map(|meta| meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH))
}

/// Number of bytes read from the start of a file to detect its kind
const DETECT_LEN: u64 = 4096;

/// Detect the file kind. A known extension decide the type, otherwise the content is checked.
fn detect_file_kind(path: &PathBuf) -> std::io::Result<FileKind> {
    let mut buf = Vec::new();
    File::open(path)?.take(DETECT_LEN).read_to_end(&mut buf)?;

    // A known extension is detected
    if let Some(kind) = kind_from_extension(path) {
        return Ok(kind);
    }

    // Empty file -> consider as raw binary with no data
    if buf.is_empty() {
        return Ok(FileKind::Bin);
    }

    // ELF magic check
    if buf.starts_with(b"\x7FELF") {
        return Ok(FileKind::Elf);
    }

    // Intel HEX: the first non-blank line must be a valid record
    if IntelHex::is_intel_hex(&buf) {
        return Ok(FileKind::Hex);
    }

    // Otherwise consider the file as raw binary
    Ok(FileKind::Bin)
}

/// File kind named by the path extension, or `None` for other/no extension
fn kind_from_extension(path: &Path) -> Option<FileKind> {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("hex" | "ihx" | "ihex") => Some(FileKind::Hex),
        Some("bin") => Some(FileKind::Bin),
        _ => None,
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
    path: &Path,
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

/// Ask the user to confirm overwriting a file that is open in another tab with
/// unsaved changes. Returns `true` if the write should proceed.
fn confirm_overwrite_unsaved_tab(path: &Path) -> bool {
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );

    let result = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Warning)
        .set_title("Unsaved changes")
        .set_description(format!(
            "\"{name}\" is open in another tab with unsaved changes.\n\n\
             Overwrite it? The changes in that tab will be lost."
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
        if let Some(index) = self
            .sessions
            .iter()
            .position(|s| is_same_file(&s.ih.filepath, path))
        {
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

        // Determine unique session id
        let id = self.next_session_id;
        self.next_session_id += 1;

        let mut new_session = HexSession {
            name: path.file_name().map_or_else(
                || "Untitled".to_string(),
                |n| n.to_string_lossy().into_owned(),
            ),
            last_modified,
            file_kind,
            id,
            ..HexSession::default()
        };

        // Load the IntelHex
        new_session.ih = ih;

        // Re-calculate address range
        new_session.addr =
            new_session.ih.get_min_addr().unwrap_or(0)..=new_session.ih.get_max_addr().unwrap_or(0);
        new_session.mark_saved();

        // Add the new session and switch to it
        self.sessions.push(new_session);
        self.active_index = Some(self.sessions.len() - 1);
    }

    /// Close the file at a given index. When the file is closed, switch to the first one.
    /// Does nothing if the index is out of range.
    pub(crate) fn close_file(&mut self, index: usize) {
        if index >= self.sessions.len() {
            return;
        }
        self.sessions.remove(index);

        if self.sessions.is_empty() {
            self.active_index = None;
        } else {
            self.active_index = Some(0);
        }
    }

    /// Returns `true` if the session at the given index has unsaved in-memory edits
    pub(crate) fn has_unsaved_changes(&self, index: usize) -> bool {
        self.sessions
            .get(index)
            .is_some_and(HexSession::has_unsaved_changes)
    }

    /// Reload the file of the session at the given index from disk, replacing in-memory data.
    /// Resets editor state and updates the last-modified timestamp.
    pub(crate) fn reload_file(&mut self, index: usize) {
        let Some(session) = self.sessions.get(index) else {
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

        let Some(session) = self.sessions.get_mut(index) else {
            return;
        };

        session.ih = ih;
        session.last_modified = last_modified;
        session.file_changed_on_disk = false;
        session.file_kind = file_kind;
        session.editor = ByteEdit::default();
        session.on_layout_changed();
        session.mark_saved();
    }

    /// Save the current session back to its original file path.
    /// Writes in the same format the file was loaded as.
    /// Marks the data as saved on success.
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

        session.mark_saved();
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
        let kind = kind_from_extension(&path).unwrap_or(FileKind::Bin);

        // The file is open in another tab with unsaved changes
        let other_tab_unsaved = self.sessions.iter().enumerate().any(|(index, s)| {
            index != curr_index && s.has_unsaved_changes() && is_same_file(&s.ih.filepath, &path)
        });
        if other_tab_unsaved && !confirm_overwrite_unsaved_tab(&path) {
            return false;
        }

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

        match self
            .sessions
            .iter()
            .position(|s| is_same_file(&s.ih.filepath, &path))
        {
            // Export over the current file itself is equivalent to a save
            Some(index) if index == curr_index => {
                let session = &mut self.sessions[index];
                session.mark_saved();
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
            // Open exported copy in a new tab and switch to it (if there is room for it)
            None if self.sessions.len() < self.max_tabs => self.load_file(&path),
            None => {}
        }

        true
    }

    /// Intercepts the request to close the app window.
    /// If any tab has unsaved changes, cancels the close and asks the user first.
    /// Once the user confirms, closes the window.
    ///
    /// Known limitation: on macOS, Cmd+Q and Quit from the Dock bypass this check.
    pub(crate) fn handle_close_request(&mut self, ctx: &egui::Context) {
        if self.quit_confirmed {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        if !ctx.input(|i| i.viewport().close_requested()) {
            return;
        }

        let unsaved: Vec<String> = self
            .sessions
            .iter()
            .filter(|s| s.has_unsaved_changes())
            .map(|s| s.name.clone())
            .collect();

        if unsaved.is_empty() {
            return;
        }

        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);

        // Clear error (in case it was set before), so that its popup is not
        // re-opened over this one
        self.error = None;
        self.popup.open(PopupState::QuitConfirm { names: unsaved });
    }

    /// Intercepts save shortcut at the start of the frame and performs the save
    /// of the current session to its file if it has unsaved changes.
    pub(crate) fn handle_save_shortcut(&mut self, ctx: &egui::Context) {
        let pressed = ctx.input_mut(|i| {
            !i.modifiers.shift
                && !i.modifiers.alt
                && i.consume_key(egui::Modifiers::COMMAND, egui::Key::S)
        });
        if !pressed || self.popup.active {
            return;
        }

        let Some(index) = self.active_index else {
            return;
        };
        let has_filepath = self
            .get_curr_session()
            .is_some_and(|s| !s.ih.filepath.as_os_str().is_empty());

        if has_filepath && self.has_unsaved_changes(index) {
            self.save_curr_session();
        }
    }

    /// Merges the content of a file into the current `IntelHex` session.
    ///
    /// # Parameters
    /// - `path`: Reference to a `PathBuf` that represents the path of the file to be merged.
    /// - `addr1`: Optional start addr to which the current session's `IntelHex` instance should be relocated.
    /// - `addr2`: Optional start addr to which the contents of the new file should be relocated.
    ///
    /// Returns `true` if the merge was applied. On failure, the current session is left
    /// unchanged and the error is stored.
    pub(crate) fn merge_file_into_curr_session(
        &mut self,
        path: &PathBuf,
        addr1: Option<usize>,
        addr2: Option<usize>,
    ) -> bool {
        // Load and relocate the selected file first. These steps can fail and must not
        // leave the current session half-modified.
        let (mut incoming_ih, _) = match load_file_into_ih(path) {
            Ok(result) => result,
            Err(msg) => {
                self.error = Some(msg);
                return false;
            }
        };

        if let Some(new_start_addr) = addr2
            && let Err(err) = incoming_ih.relocate(new_start_addr)
        {
            self.error = Some(err.to_string());
            return false;
        }

        let Some(cur_session) = self.get_curr_session_mut() else {
            self.error = Some("Could not get current hex session".to_string());
            return false;
        };

        // Relocate the current file to a new start address.
        // `relocate` validates before changing anything, so on error the session is unchanged.
        if let Some(new_start_addr) = addr1 {
            let old_start_addr = cur_session.ih.get_min_addr();

            if let Err(err) = cur_session.ih.relocate(new_start_addr) {
                self.error = Some(err.to_string());
                return false;
            }

            if let Some(old_start_addr) = old_start_addr {
                cur_session
                    .editor
                    .remap_modified(new_start_addr, old_start_addr);
            }
        }

        // Merge the two IntelHex instances (cannot fail)
        cur_session.ih.merge(&incoming_ih);

        // Forget edits of bytes overwritten by the merged file, so "Restore" does not
        // write the pre-edit values over the merged data
        cur_session
            .editor
            .modified
            .retain(|&addr, _| incoming_ih.read_byte(addr).is_none());

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// Temporary directory, removed when dropped
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> std::io::Result<Self> {
            let path = std::env::temp_dir()
                .join(format!("hexalyzer_loader_{name}_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path)?;
            Ok(Self(path))
        }

        /// Create a file with the given content in this directory
        fn file(&self, name: &str, data: &[u8]) -> std::io::Result<PathBuf> {
            let path = self.0.join(name);
            std::fs::write(&path, data)?;
            Ok(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Build an `IntelHex` from `(start address, data)` chunks
    fn ih_with(chunks: &[(usize, &[u8])]) -> Result<IntelHex, intelhexlib::IntelHexError> {
        let mut ih = IntelHex::new();
        for &(addr, data) in chunks {
            ih.write_range(addr, addr + data.len() - 1)?;
            ih.update_range(addr, data)?;
        }
        Ok(ih)
    }

    /// Minimal valid Intel HEX: 4 bytes `01 02 03 04` at address 0
    const HEX_CONTENT: &[u8] = b":0400000001020304F2\n:00000001FF\n";

    /// Start of an ELF file
    const ELF_CONTENT: &[u8] = b"\x7FELF\x02\x01\x01";

    // ============================ is_same_file ============================

    #[test]
    fn test_is_same_file_same_path() -> TestResult {
        // Arrange
        let dir = TempDir::new("same_path")?;
        let file = dir.file("a.bin", b"a")?;

        // Act
        let res = is_same_file(&file, &file);

        // Assert
        assert!(res);

        Ok(())
    }

    #[test]
    fn test_is_same_file_different_spelling() -> TestResult {
        // Arrange
        let dir = TempDir::new("spelling")?;
        let file = dir.file("a.bin", b"a")?;
        std::fs::create_dir(dir.0.join("sub"))?;
        let other = dir.0.join("sub").join("..").join("a.bin");

        // Act
        let res = is_same_file(&file, &other);

        // Assert
        assert_ne!(file, other);
        assert!(res);

        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn test_is_same_file_symlink() -> TestResult {
        // Arrange
        let dir = TempDir::new("symlink")?;
        let file = dir.file("a.bin", b"a")?;
        let link = dir.0.join("link.bin");
        std::os::unix::fs::symlink(&file, &link)?;

        // Act
        let res = is_same_file(&file, &link);

        // Assert
        assert!(res);

        Ok(())
    }

    #[test]
    fn test_is_same_file_different_files() -> TestResult {
        // Arrange
        let dir = TempDir::new("different")?;
        let a = dir.file("a.bin", b"a")?;
        let b = dir.file("b.bin", b"a")?;

        // Act
        let res = is_same_file(&a, &b);

        // Assert
        assert!(!res);

        Ok(())
    }

    #[test]
    fn test_is_same_file_missing_same_path() -> TestResult {
        // Arrange
        let dir = TempDir::new("missing_same")?;
        let path = dir.0.join("a.bin");

        // Act
        let res = is_same_file(&path, &path);

        // Assert: falls back to comparing the paths as-is
        assert!(res);

        Ok(())
    }

    #[test]
    fn test_is_same_file_missing_different_paths() -> TestResult {
        // Arrange
        let dir = TempDir::new("missing_different")?;
        let a = dir.0.join("a.bin");
        let b = dir.0.join("b.bin");

        // Act
        let res = is_same_file(&a, &b);

        // Assert
        assert!(!res);

        Ok(())
    }

    // ========================= kind_from_extension ========================

    #[test]
    fn test_kind_from_extension_hex() {
        // Arrange
        let names = ["a.hex", "a.ihx", "a.ihex", "a.HEX", "a.IHex"];

        // Act
        let kinds = names.map(|name| kind_from_extension(Path::new(name)));

        // Assert
        assert_eq!(kinds, [const { Some(FileKind::Hex) }; 5]);
    }

    #[test]
    fn test_kind_from_extension_bin() {
        // Arrange
        let names = ["a.bin", "a.BIN"];

        // Act
        let kinds = names.map(|name| kind_from_extension(Path::new(name)));

        // Assert
        assert_eq!(kinds, [const { Some(FileKind::Bin) }; 2]);
    }

    #[test]
    fn test_kind_from_extension_unknown() {
        // Arrange
        let names = ["a", "a.elf", "a.txt", "a.hex.bak", ".hex"];

        // Act
        let kinds = names.map(|name| kind_from_extension(Path::new(name)));

        // Assert
        assert_eq!(kinds, [const { None }; 5]);
    }

    // ========================== detect_file_kind ==========================

    #[test]
    fn test_detect_file_kind_extension_wins_over_content() -> TestResult {
        // Arrange
        let dir = TempDir::new("detect_ext")?;
        let path = dir.file("a.bin", HEX_CONTENT)?;

        // Act
        let kind = detect_file_kind(&path)?;

        // Assert
        assert_eq!(kind, FileKind::Bin);

        Ok(())
    }

    #[test]
    fn test_detect_file_kind_empty_file() -> TestResult {
        // Arrange
        let dir = TempDir::new("detect_empty")?;
        let path = dir.file("empty", b"")?;

        // Act
        let kind = detect_file_kind(&path)?;

        // Assert
        assert_eq!(kind, FileKind::Bin);

        Ok(())
    }

    #[test]
    fn test_detect_file_kind_elf_content() -> TestResult {
        // Arrange
        let dir = TempDir::new("detect_elf")?;
        let path = dir.file("firmware", ELF_CONTENT)?;

        // Act
        let kind = detect_file_kind(&path)?;

        // Assert
        assert_eq!(kind, FileKind::Elf);

        Ok(())
    }

    #[test]
    fn test_detect_file_kind_hex_content() -> TestResult {
        // Arrange
        let dir = TempDir::new("detect_hex")?;
        let path = dir.file("firmware", HEX_CONTENT)?;

        // Act
        let kind = detect_file_kind(&path)?;

        // Assert
        assert_eq!(kind, FileKind::Hex);

        Ok(())
    }

    #[test]
    fn test_detect_file_kind_binary_content() -> TestResult {
        // Arrange
        let dir = TempDir::new("detect_bin")?;
        let path = dir.file("firmware", &[0x00, 0x3A, 0xFF, 0x10])?;

        // Act
        let kind = detect_file_kind(&path)?;

        // Assert
        assert_eq!(kind, FileKind::Bin);

        Ok(())
    }

    #[test]
    fn test_detect_file_kind_missing_file() -> TestResult {
        // Arrange
        let dir = TempDir::new("detect_missing")?;
        let path = dir.0.join("a.hex");

        // Act
        let res = detect_file_kind(&path);

        // Assert
        assert!(res.is_err());

        Ok(())
    }

    // ========================= load_file_into_ih ==========================

    #[test]
    fn test_load_file_into_ih_bin() -> TestResult {
        // Arrange
        let dir = TempDir::new("load_bin")?;
        let path = dir.file("a.bin", &[0xAA, 0xBB, 0xCC])?;

        // Act
        let (ih, kind) = load_file_into_ih(&path)?;

        // Assert
        assert_eq!(kind, FileKind::Bin);
        assert_eq!(ih.size, 3);
        assert_eq!(ih.read_range(0, 3), Some(vec![0xAA, 0xBB, 0xCC]));

        Ok(())
    }

    #[test]
    fn test_load_file_into_ih_hex() -> TestResult {
        // Arrange
        let dir = TempDir::new("load_hex")?;
        let path = dir.file("a.hex", HEX_CONTENT)?;

        // Act
        let (ih, kind) = load_file_into_ih(&path)?;

        // Assert
        assert_eq!(kind, FileKind::Hex);
        assert_eq!(ih.read_range(0, 4), Some(vec![0x01, 0x02, 0x03, 0x04]));
        assert_eq!(ih.filepath, path);

        Ok(())
    }

    #[test]
    fn test_load_file_into_ih_elf_not_supported() -> TestResult {
        // Arrange
        let dir = TempDir::new("load_elf")?;
        let path = dir.file("firmware", ELF_CONTENT)?;

        // Act
        let res = load_file_into_ih(&path);

        // Assert
        assert_eq!(
            res.err(),
            Some("ELF files are not yet supported".to_string())
        );

        Ok(())
    }

    #[test]
    fn test_load_file_into_ih_invalid_hex() -> TestResult {
        // Arrange
        let dir = TempDir::new("load_invalid_hex")?;
        let path = dir.file("bad.hex", b":04000000010203FF\n")?;

        // Act
        let res = load_file_into_ih(&path);

        // Assert
        assert!(res.is_err());

        Ok(())
    }

    #[test]
    fn test_load_file_into_ih_missing_file() -> TestResult {
        // Arrange
        let dir = TempDir::new("load_missing")?;
        let path = dir.0.join("missing.bin");

        // Act
        let res = load_file_into_ih(&path);

        // Assert
        assert!(res.is_err());

        Ok(())
    }

    // ========================== write_ih_to_path ==========================

    #[test]
    fn test_write_ih_to_path_hex() -> TestResult {
        // Arrange
        let dir = TempDir::new("write_hex")?;
        let path = dir.0.join("out.hex");
        let mut ih = ih_with(&[(0x10, &[1, 2, 3]), (0x100, &[4, 5])])?;

        // Act
        write_ih_to_path(&mut ih, &path, &FileKind::Hex, 0x00)?;

        // Assert: the written file loads back with the same data
        let (loaded, kind) = load_file_into_ih(&path)?;
        assert_eq!(kind, FileKind::Hex);
        assert_eq!(
            loaded.bytes().collect::<Vec<_>>(),
            ih.bytes().collect::<Vec<_>>()
        );

        Ok(())
    }

    #[test]
    fn test_write_ih_to_path_bin_gap_fill() -> TestResult {
        // Arrange
        let dir = TempDir::new("write_bin")?;
        let path = dir.0.join("out.bin");
        let mut ih = ih_with(&[(0x10, &[1, 2]), (0x14, &[3])])?;

        // Act
        write_ih_to_path(&mut ih, &path, &FileKind::Bin, 0xFF)?;

        // Assert
        assert_eq!(std::fs::read(&path)?, vec![1, 2, 0xFF, 0xFF, 3]);

        Ok(())
    }

    #[test]
    fn test_write_ih_to_path_elf_not_supported() -> TestResult {
        // Arrange
        let dir = TempDir::new("write_elf")?;
        let path = dir.0.join("out.elf");
        let mut ih = ih_with(&[(0, &[1])])?;

        // Act
        let res = write_ih_to_path(&mut ih, &path, &FileKind::Elf, 0x00);

        // Assert
        assert!(res.is_err());
        assert!(!path.exists());

        Ok(())
    }

    #[test]
    fn test_write_ih_to_path_unknown_kind() -> TestResult {
        // Arrange
        let dir = TempDir::new("write_unknown")?;
        let path = dir.0.join("out");
        let mut ih = ih_with(&[(0, &[1])])?;

        // Act
        let res = write_ih_to_path(&mut ih, &path, &FileKind::Unknown, 0x00);

        // Assert
        assert!(res.is_err());
        assert!(!path.exists());

        Ok(())
    }

    // ========================= bin_gap_fill_size ==========================

    #[test]
    fn test_bin_gap_fill_size_empty() {
        // Arrange
        let ih = IntelHex::new();

        // Act
        let size = bin_gap_fill_size(&ih);

        // Assert
        assert_eq!(size, 0);
    }

    #[test]
    fn test_bin_gap_fill_size_contiguous() -> TestResult {
        // Arrange
        let ih = ih_with(&[(0x10, &[1, 2, 3])])?;

        // Act
        let size = bin_gap_fill_size(&ih);

        // Assert
        assert_eq!(size, 0);

        Ok(())
    }

    #[test]
    fn test_bin_gap_fill_size_with_gap() -> TestResult {
        // Arrange: data at 0x10..=0x11 and 0x20 -> gap 0x12..=0x1F
        let ih = ih_with(&[(0x10, &[1, 2]), (0x20, &[3])])?;

        // Act
        let size = bin_gap_fill_size(&ih);

        // Assert
        assert_eq!(size, 0x0E);

        Ok(())
    }

    // ============================ format_size =============================

    const KIB: usize = 1024;
    const MIB: usize = 1024 * KIB;
    const GIB: usize = 1024 * MIB;
    const TIB: usize = 1024 * GIB;

    #[test]
    fn test_format_size_bytes() {
        // Arrange
        let sizes = [0, 1, KIB - 1];

        // Act
        let formatted = sizes.map(format_size);

        // Assert
        assert_eq!(formatted, ["0 B", "1 B", "1023 B"]);
    }

    #[test]
    fn test_format_size_units() {
        // Arrange
        let sizes = [KIB, KIB + KIB / 2, MIB, 64 * MIB, 4 * GIB, TIB];

        // Act
        let formatted = sizes.map(format_size);

        // Assert
        assert_eq!(
            formatted,
            [
                "1.0 KiB", "1.5 KiB", "1.0 MiB", "64.0 MiB", "4.0 GiB", "1.0 TiB"
            ]
        );
    }

    #[test]
    fn test_format_size_rounds_down() {
        // Arrange
        let sizes = [2 * KIB - 1, MIB - 1];

        // Act
        let formatted = sizes.map(format_size);

        // Assert
        assert_eq!(formatted, ["1.9 KiB", "1023.9 KiB"]);
    }

    #[test]
    fn test_format_size_tib_is_largest_unit() {
        // Arrange
        let sizes = [2048 * TIB, usize::MAX];

        // Act
        let formatted = sizes.map(format_size);

        // Assert
        assert_eq!(formatted, ["2048.0 TiB", "16777215.9 TiB"]);
    }

    // ========================= get_last_modified ==========================

    #[test]
    fn test_get_last_modified_existing_file() -> TestResult {
        // Arrange
        let dir = TempDir::new("last_modified")?;
        let path = dir.file("a.bin", b"a")?;

        // Act
        let res = get_last_modified(&path);

        // Assert
        assert!(res.is_ok());

        Ok(())
    }

    #[test]
    fn test_get_last_modified_missing_file() -> TestResult {
        // Arrange
        let dir = TempDir::new("last_modified_missing")?;
        let path = dir.0.join("missing.bin");

        // Act
        let res = get_last_modified(&path);

        // Assert
        assert!(res.is_err());

        Ok(())
    }

    // ========================== changed_on_disk ===========================

    #[test]
    fn test_changed_on_disk_unchanged() -> TestResult {
        // Arrange
        let dir = TempDir::new("changed_unchanged")?;
        let path = dir.file("a.bin", b"a")?;
        let loaded_at = get_last_modified(&path)?;

        // Act
        let changed = changed_on_disk(&path, loaded_at);

        // Assert
        assert!(!changed);

        Ok(())
    }

    #[test]
    fn test_changed_on_disk_modified_later() -> TestResult {
        // Arrange: another program writes the file after it was loaded
        let dir = TempDir::new("changed_later")?;
        let path = dir.file("a.bin", b"a")?;
        let loaded_at = get_last_modified(&path)?;
        File::options()
            .write(true)
            .open(&path)?
            .set_modified(loaded_at + Duration::from_secs(10))?;

        // Act
        let changed = changed_on_disk(&path, loaded_at);

        // Assert
        assert!(changed);

        Ok(())
    }

    #[test]
    fn test_changed_on_disk_missing_file() -> TestResult {
        // Arrange
        let dir = TempDir::new("changed_missing")?;
        let path = dir.0.join("a.bin");

        // Act
        let changed = changed_on_disk(&path, SystemTime::now());

        // Assert: a deleted file is not reported as changed
        assert!(!changed);

        Ok(())
    }

    // ================= confirm_* (paths without a dialog) =================

    #[test]
    fn test_confirm_large_write_hex_never_asks() -> TestResult {
        // Arrange: HEX files have no gap fill, however sparse the data is
        let ih = ih_with(&[(0, &[1]), (2 * LARGE_GAP_FILL_SIZE, &[2])])?;

        // Act
        let confirmed = confirm_large_write(&ih, &FileKind::Hex);

        // Assert
        assert!(confirmed);

        Ok(())
    }

    #[test]
    fn test_confirm_large_write_small_gap_fill() -> TestResult {
        // Arrange
        let ih = ih_with(&[(0, &[1]), (LARGE_GAP_FILL_SIZE - 1, &[2])])?;

        // Act
        let confirmed = confirm_large_write(&ih, &FileKind::Bin);

        // Assert
        assert!(bin_gap_fill_size(&ih) < LARGE_GAP_FILL_SIZE);
        assert!(confirmed);

        Ok(())
    }

    #[test]
    fn test_confirm_overwrite_if_changed_unchanged_file() -> TestResult {
        // Arrange
        let dir = TempDir::new("confirm_unchanged")?;
        let path = dir.file("a.bin", b"a")?;
        let loaded_at = get_last_modified(&path)?;

        // Act
        let confirmed = confirm_overwrite_if_changed(&path, loaded_at);

        // Assert
        assert!(confirmed);

        Ok(())
    }

    #[test]
    fn test_confirm_overwrite_if_changed_missing_file() -> TestResult {
        // Arrange
        let dir = TempDir::new("confirm_missing")?;
        let path = dir.0.join("missing.bin");

        // Act
        let confirmed = confirm_overwrite_if_changed(&path, SystemTime::now());

        // Assert: a deleted file can be written again without asking
        assert!(confirmed);

        Ok(())
    }
}
