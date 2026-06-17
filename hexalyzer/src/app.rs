use crate::byteedit::ByteEdit;
use crate::events::EventState;
use crate::hexview::PageBuilder;
use crate::selection::Selection;
use crate::ui_converter::HexConverter;
use crate::ui_jumpto::JumpTo;
use crate::ui_popup::Popup;
use crate::ui_search::Search;
use intelhexlib::IntelHex;
use std::ops::RangeInclusive;

pub mod colors {
    use eframe::egui::Color32;

    pub const LIGHT_BLUE: Color32 = Color32::from_rgba_premultiplied(33, 81, 109, 20);
    pub const MUD: Color32 = Color32::from_rgba_premultiplied(54, 44, 19, 20);
    pub const GREEN: Color32 = Color32::from_rgba_premultiplied(35, 53, 38, 20);
    pub const GRAY_160: Color32 = Color32::from_gray(160);
    pub const GRAY_210: Color32 = Color32::from_gray(210);
    pub const SHADOW: Color32 = Color32::from_black_alpha(150);
    pub const WARNING: Color32 = Color32::from_rgb(160, 160, 16);
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
    /// Cached page builder for the hex view renderer
    pub page_builder: PageBuilder,
    /// Last modified time of the file. Used to detect file changes.
    pub last_modified: std::time::SystemTime,
    /// Last time the file modification was checked. Used to throttle `fs::metadata` calls.
    pub last_mod_check: std::time::Instant,
    /// Whether the file on disk has diverged from the loaded data (sticky until next poll).
    pub file_changed_on_disk: bool,
    /// Scroll id that allows each tab to keep its own scroll position
    pub scroll_id: usize,
}

pub struct HexViewerApp {
    /// Vector of opened sessions. Each session is represented by a `HexSession` struct.
    pub sessions: Vec<HexSession>,
    /// Index of the currently active session. If `None`, no session is active.
    pub active_index: Option<usize>,
    /// Maximum number of tabs that can be opened.
    pub max_tabs: usize,
    /// Displayed bytes per row
    pub bytes_per_row: usize,
    /// Pop up handler
    pub popup: Popup,
    /// Monotonically increasing counter for unique scroll IDs
    pub(crate) next_scroll_id: usize,
    /// Whether the side panel is expanded or collapsed
    pub side_panel_expanded: bool,

    /// Hex/Dec/Bin/ASCII converter tool
    pub converter: HexConverter,

    // -- UI states
    /// Per-frame state of user inputs
    pub events: EventState,
    /// Errors during parsing, editing, or writing `IntelHex` file
    pub error: Option<String>,
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
            page_builder: PageBuilder::new(),
            last_modified: std::time::SystemTime::UNIX_EPOCH,
            last_mod_check: std::time::Instant::now(),
            file_changed_on_disk: false,
            scroll_id: 0,
        }
    }
}

impl Default for HexViewerApp {
    fn default() -> Self {
        Self {
            sessions: Vec::new(),
            active_index: None,
            max_tabs: 5,
            bytes_per_row: 16,
            popup: Popup::default(),
            converter: HexConverter::default(),
            next_scroll_id: 1,
            side_panel_expanded: true,
            events: EventState::default(),
            error: None,
        }
    }
}

impl HexViewerApp {
    /// Get the currently active session, if any
    pub(crate) fn get_curr_session(&self) -> Option<&HexSession> {
        self.active_index.and_then(|i| self.sessions.get(i))
    }

    /// Get a mutable reference to the currently active session, if any
    pub(crate) fn get_curr_session_mut(&mut self) -> Option<&mut HexSession> {
        self.active_index.and_then(|i| self.sessions.get_mut(i))
    }
}
