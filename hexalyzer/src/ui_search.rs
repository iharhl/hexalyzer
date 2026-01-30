use crate::app::HexSession;
use eframe::egui;

#[derive(Default, PartialEq, Clone)]
enum SearchMode {
    #[default]
    Hex,
    Ascii,
    Regex,
}

#[derive(Default, PartialEq, Clone)]
struct SearchState {
    /// User input
    input: String,
    /// Search mode: byte / ASCII literals / ASCII regex
    mode: SearchMode,
}

#[derive(Default)]
pub struct Search {
    /// Start address of the search results
    pub(crate) addr: Option<usize>,
    /// List of addresses where the match was found
    pub(crate) results: Vec<usize>,
    /// Length of the search pattern in bytes
    pub(crate) length: usize,
    /// Does the search text field have focus
    pub(crate) has_focus: bool,
    /// Index of the current search result
    idx: usize,

    // -- UI control flags
    /// Force the search to be performed even if the input is the same as the last one
    force: bool,
    /// Force to loose focus from the text field
    loose_focus: bool,

    // -- Input states
    /// Current search state
    current: SearchState,
    /// Previous search state. Used to detect if the input changed and the search should be repeated.
    last: SearchState,
}

impl Search {
    /// Clear the search state
    pub(crate) fn clear(&mut self) {
        self.has_focus = false;
        self.addr = None;
        self.results.clear();
        self.length = 0;
        // Do not clear current to preserve text box content
        self.last = SearchState::default();
        self.idx = 0;
        self.force = false;
    }

    /// Redo the last search
    pub(crate) fn redo(&mut self) {
        // In case the current input field is not valid
        self.current = self.last.clone();

        // Clear
        self.clear();

        // Set force flag
        self.force = true;
    }

    /// Force to loose focus from the text field
    pub(crate) const fn loose_focus(&mut self) {
        self.loose_focus = true;
    }
}

impl HexSession {
    /// Show content of the search menu
    pub(crate) fn show_search_contents(&mut self, ui: &mut egui::Ui) {
        // RadioButtons to select between byte and ascii search
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.search.current.mode, SearchMode::Hex, "Hex")
                .on_hover_text("Search for a byte pattern");
            ui.add_space(5.0);
            ui.radio_value(&mut self.search.current.mode, SearchMode::Ascii, "Ascii")
                .on_hover_text("Search ASCII literals");
            ui.add_space(5.0);
            ui.radio_value(&mut self.search.current.mode, SearchMode::Regex, "Regex")
                .on_hover_text(
                    "Search ASCII with regex\n\
                Highlights only the first byte of the match",
                );
        });

        ui.add_space(3.0);

        let textedit = ui.add(
            egui::TextEdit::singleline(&mut self.search.current.input)
                .desired_width(ui.available_width() - 30.0),
        );

        if self.search.loose_focus {
            textedit.surrender_focus();
            self.search.loose_focus = false;
        }

        if textedit.has_focus() {
            self.jump_to.has_focus = false;
            self.search.has_focus = true;

            // Clear the selection to avoid modifying bytes
            // while typing in the search area
            self.selection.clear();
        }

        if (self.events.borrow().enter_released && self.search.has_focus) || self.search.force {
            // Same input -> move to next result, otherwise -> search again
            if self.search.current == self.search.last {
                if !self.search.results.is_empty() {
                    self.search.idx = (self.search.idx + 1) % self.search.results.len();
                }
            } else {
                let input = self.search.current.input.as_str();

                // Search if pattern is valid
                let valid = match self.search.current.mode {
                    SearchMode::Hex => {
                        if let Some(pattern) = parse_str_into_bytes(input) {
                            self.search.results = self.ih.search_bytes(&pattern);
                            self.search.length = pattern.len();
                            true
                        } else {
                            false
                        }
                    }
                    SearchMode::Ascii => {
                        if input.is_empty() {
                            false
                        } else {
                            self.search.results = self.ih.search_ascii(input, false);
                            self.search.length = input.len();
                            true
                        }
                    }
                    SearchMode::Regex => {
                        if input.is_empty() {
                            false
                        } else {
                            self.search.results = self.ih.search_ascii(input, true);
                            self.search.length = 1; // highlight the first byte of the match
                            true
                        }
                    }
                };

                // Clear the result if pattern is not valid
                if !valid {
                    self.search.results.clear();
                }

                // Reset the state of search
                self.search.idx = 0;
                self.search.last = self.search.current.clone();
            }

            // Set address to scroll to (only if not forced)
            if !self.search.force {
                self.search.addr = self.search.results.get(self.search.idx).copied();
            }

            self.search.force = false;
        }

        ui.add_space(5.0);

        // Show matches count if any
        let label_text = if self.search.results.is_empty() {
            "No results".to_string()
        } else {
            format!(
                "Hits: {} (Current: {})",
                self.search.results.len(),
                self.search.idx + 1
            )
        };

        ui.label(label_text);
    }
}

fn parse_str_into_bytes(s: &str) -> Option<Vec<u8>> {
    if s.len().is_multiple_of(2) {
        return (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
            .collect();
    }
    None
}
