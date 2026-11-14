use crate::app::HexSession;
use crate::events::EventState;
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
    /// List of addresses where the match was found (sorted ascending)
    pub(crate) results: Vec<usize>,
    /// Length of the search pattern in bytes
    pub(crate) length: usize,
    /// Does the search text field have focus
    pub(crate) has_focus: bool,
    /// Index of the current search result
    idx: usize,
    /// Address of the current search result before a redo
    resume_from: Option<usize>,

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
        self.resume_from = None;
        self.force = false;
    }

    /// Redo the last search
    pub(crate) fn redo(&mut self) {
        // In case the current input field is not valid
        self.current = self.last.clone();

        // Remember the current hit to keep the position after the redo
        let resume_from = self.results.get(self.idx).copied();

        // Clear
        self.clear();
        self.resume_from = resume_from;

        // Set force flag
        self.force = true;
    }

    /// Force to loose focus from the text field
    pub(crate) const fn loose_focus(&mut self) {
        self.loose_focus = true;
    }

    /// Check if `addr` lies inside the search match.
    ///
    /// Relies on `results` being sorted ascending. The last match starting at
    /// or before `addr` is the only one that can cover it.
    pub(crate) fn is_hit(&self, addr: usize) -> bool {
        let n = self.results.partition_point(|&start| start <= addr);
        n > 0 && addr < self.results[n - 1].saturating_add(self.length)
    }
}

impl HexSession {
    /// Show content of the search menu
    pub(crate) fn show_search_contents(&mut self, ui: &mut egui::Ui, events: &EventState) {
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

        if (events.enter_released && self.search.has_focus) || self.search.force {
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

                // Verify that the results are sorted which is required for
                // correct highlighting of the search hit.
                debug_assert!(self.search.results.is_sorted());

                // Reset the state of search
                self.search.idx = 0;
                self.search.last = self.search.current.clone();

                // After a redo -> select closest hit
                if let Some(addr) = self.search.resume_from.take() {
                    let i = self.search.results.partition_point(|&start| start < addr);
                    if i < self.search.results.len() {
                        self.search.idx = i;
                    }
                }
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
    if s.is_ascii() && s.len().is_multiple_of(2) {
        return (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
            .collect();
    }
    None
}
