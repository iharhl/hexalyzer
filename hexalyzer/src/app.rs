use crate::byteedit::ByteEdit;
use crate::events::EventState;
use crate::fingerprint::Fingerprint;
use crate::loader::FileKind;
use crate::selection::Selection;
use crate::ui_converter::HexConverter;
use crate::ui_jumpto::JumpTo;
use crate::ui_popup::Popup;
use crate::ui_search::Search;
use intelhexlib::IntelHex;
use std::ops::RangeInclusive;

/// Smallest window size (the side panel collapsed): fits the hex bytes of a 16-byte row
pub const MIN_WINDOW_SIZE: [f32; 2] = [480.0, 320.0];

pub mod colors {
    use eframe::egui::Color32;

    pub const LIGHT_BLUE: Color32 = Color32::from_rgba_premultiplied(33, 81, 109, 20);
    pub const MUD: Color32 = Color32::from_rgba_premultiplied(54, 44, 19, 20);
    pub const GREEN: Color32 = Color32::from_rgba_premultiplied(35, 53, 38, 20);
    pub const BRIGHT_GREEN: Color32 = Color32::from_rgba_premultiplied(50, 85, 55, 20);
    pub const GRAY_160: Color32 = Color32::from_gray(160);
    pub const GRAY_210: Color32 = Color32::from_gray(210);
    pub const SHADOW: Color32 = Color32::from_black_alpha(150);
    pub const WARNING: Color32 = Color32::from_rgb(160, 160, 16);
    pub const ERROR: Color32 = Color32::from_rgb(230, 80, 80);
}

#[derive(PartialEq, Eq)]
pub enum Endianness {
    Little,
    Big,
}

pub struct HexSession {
    /// Name of the session (same as filename)
    pub name: String,
    /// `IntelHex` object returned by `intelhexlib`
    pub ih: IntelHex,
    /// Address range of the hex data
    pub addr: RangeInclusive<usize>,
    /// Endianness of the hex data
    pub endianness: Endianness,
    /// Handler for bytes editing
    pub editor: ByteEdit,
    /// Handler for GUI feature of bytes selection
    pub selection: Selection,
    /// Handler for GUI feature to search for byte string
    pub search: Search,
    /// Handler for GUI feature to jump to selected address
    pub jump_to: JumpTo,
    /// Last modified time of the file. Used to detect file changes.
    pub last_modified: std::time::SystemTime,
    /// Last time the file modification was checked. Used to throttle `fs::metadata` calls.
    pub last_mod_check: std::time::Instant,
    /// Whether the file on disk has diverged from the loaded data (sticky until next poll).
    pub file_changed_on_disk: bool,
    /// Tracks whether the data differs from the last loaded or saved version
    pub fingerprint: Fingerprint,
    /// Detected file format at load time
    pub file_kind: FileKind,
    /// Unique id of the session
    pub id: usize,
}

pub struct HexViewerApp {
    /// Vector of opened sessions. Each session is represented by a `HexSession` struct.
    pub sessions: Vec<HexSession>,
    /// Index of the currently active session. If `None`, no session is active.
    pub active_index: Option<usize>,
    /// Session for the (disabled) side panel while no file is open
    pub(crate) placeholder_session: HexSession,
    /// Maximum number of tabs that can be opened.
    pub max_tabs: usize,
    /// Displayed bytes per row
    pub bytes_per_row: usize,
    /// Pop up handler
    pub popup: Popup,
    /// Monotonically increasing counter for unique session IDs
    pub(crate) next_session_id: usize,
    /// Whether the side panel is expanded or collapsed
    pub side_panel_expanded: bool,
    /// Side panel state the minimum window width was last set for
    pub(crate) min_width_set_for: Option<bool>,

    /// Hex/Dec/Bin/ASCII converter tool
    pub converter: HexConverter,

    // -- UI states
    /// Per-frame state of user inputs
    pub events: EventState,
    /// Errors during parsing, editing, or writing `IntelHex` file
    pub error: Option<String>,

    /// Gap fill byte used when exporting to binary
    pub gap_fill: u8,

    /// Set once the user has confirmed quitting with unsaved changes
    pub(crate) quit_confirmed: bool,
}

impl Default for HexSession {
    fn default() -> Self {
        Self {
            name: "Untitled".to_string(),
            ih: IntelHex::default(),
            addr: 0..=0,
            endianness: Endianness::Little,
            editor: ByteEdit::default(),
            selection: Selection::default(),
            search: Search::default(),
            jump_to: JumpTo::default(),
            last_modified: std::time::SystemTime::UNIX_EPOCH,
            last_mod_check: std::time::Instant::now(),
            file_changed_on_disk: false,
            fingerprint: Fingerprint::default(),
            file_kind: FileKind::Unknown,
            id: 0,
        }
    }
}

impl HexSession {
    /// Update the session state after an operation that changes the data layout
    /// (relocate, insert/remove range, merge, reload).
    pub(crate) fn on_layout_changed(&mut self) {
        self.addr = self.ih.get_min_addr().unwrap_or(0)..=self.ih.get_max_addr().unwrap_or(0);
        // Forget edits of bytes that no longer exist (e.g. removed range), so they are
        // neither highlighted nor written back into a range inserted later by "Restore"
        let ih = &self.ih;
        self.editor
            .modified
            .retain(|&addr, _| ih.read_byte(addr).is_some());
        self.fingerprint.recompute(&self.ih);
        self.clear_marks_if_saved();
        self.selection.clear();
        self.editor.clear();
        self.search.redo();
    }

    /// Mark the current data as saved (after load, reload or save)
    pub(crate) fn mark_saved(&mut self) {
        self.fingerprint.mark_saved(&self.ih);
        self.editor.modified.clear();
    }

    /// Returns `true` if the data differs from the last loaded or saved version
    pub(crate) const fn has_unsaved_changes(&self) -> bool {
        self.fingerprint.is_changed()
    }

    /// Clear the byte edit marks once the data matches the saved version again,
    /// since there is nothing left to highlight or restore.
    ///
    /// Limitation: the marks track bytes typed over since the last save or layout change
    /// (with their values before that), not the difference from the saved file.
    /// E.g. after removing a range and inserting it back, the data differs from the saved
    /// file with no bytes marked. Typing the original values back then marks each byte
    /// (Restore would revert it to the inserted value) until the last one makes the data
    /// match the saved file and all marks are cleared here.
    pub(crate) fn clear_marks_if_saved(&mut self) {
        if !self.has_unsaved_changes() {
            self.editor.modified.clear();
        }
    }
}

impl Default for HexViewerApp {
    fn default() -> Self {
        Self {
            sessions: Vec::new(),
            active_index: None,
            placeholder_session: HexSession::default(),
            max_tabs: 5,
            bytes_per_row: 16,
            popup: Popup::default(),
            converter: HexConverter::default(),
            next_session_id: 1,
            side_panel_expanded: true,
            min_width_set_for: None,
            events: EventState::default(),
            error: None,
            gap_fill: 0xFF,
            quit_confirmed: false,
        }
    }
}

impl HexViewerApp {
    /// Index of the session with the given `HexSession::id`, if it is still open
    pub(crate) fn get_session_index(&self, id: usize) -> Option<usize> {
        self.sessions.iter().position(|s| s.id == id)
    }

    /// Get the currently active session, if any
    pub(crate) fn get_curr_session(&self) -> Option<&HexSession> {
        self.active_index.and_then(|i| self.sessions.get(i))
    }

    /// Get a mutable reference to the currently active session, if any
    pub(crate) fn get_curr_session_mut(&mut self) -> Option<&mut HexSession> {
        self.active_index.and_then(|i| self.sessions.get_mut(i))
    }
}
