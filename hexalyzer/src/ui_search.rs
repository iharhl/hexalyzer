use crate::app::HexSession;
use eframe::egui;
use std::collections::btree_map;

#[derive(Default, PartialEq, Clone)]
struct SearchState {
    /// User input
    input: String,
    /// Is search text in ASCII representation
    is_ascii: bool,
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
            ui.radio_value(&mut self.search.current.is_ascii, false, "hex");
            ui.add_space(5.0);
            ui.radio_value(&mut self.search.current.is_ascii, true, "ascii");
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
                let is_ascii = self.search.current.is_ascii;

                // If pattern valid -> search, otherwise -> clear results
                if let Some(pattern) = parse_str_into_bytes(input, is_ascii) {
                    self.search.results = search_bmh(self.ih.iter(), &pattern);
                    self.search.length = pattern.len();
                } else {
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
            "--".to_string()
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

// TODO: 1) add SIMD acceleration; 2) Replace with KMP search?

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
/// Boyer–Moore–Horspool algorithm for `BTreeMap<usize, u8>`.
/// Returns the starting addresses of all matches.
fn search_bmh(map_iter: btree_map::Iter<usize, u8>, pattern: &[u8]) -> Vec<usize> {
    let m = pattern.len();
    if m == 0 || m > u8::MAX as usize {
        return vec![];
    }

    // Consume the iterator once into an indexable representation.
    // This does not clone the BTreeMap, only copies (usize, u8) pairs.
    let haystack: Vec<(usize, u8)> = map_iter.map(|(&addr, &byte)| (addr, byte)).collect();

    // Check if length of address is less than the pattern
    let n = haystack.len();
    if n < m {
        return vec![];
    }

    // Build bad match table
    let mut bad_match = [m as u8; 256];
    for i in 0..m - 1 {
        bad_match[pattern[i] as usize] = (m - 1 - i) as u8;
    }

    // Prepare result collection
    let mut results = Vec::new();

    // Main BMH loop
    let mut i = 0; // index into addrs[]
    while i <= n - m {
        // Compare pattern from right to left
        let mut j = (m - 1) as isize;
        while j >= 0 && haystack[i + j as usize].1 == pattern[j as usize] {
            j -= 1;
        }

        if j < 0 {
            // Match found
            results.push(haystack[i].0);
            i += 1; // advance minimally
        } else {
            // Mismatch -> skip using last byte of window
            let last_byte = haystack[i + m - 1].1;
            i += bad_match[last_byte as usize] as usize;
        }
    }

    results
}

fn parse_str_into_bytes(s: &str, is_ascii_repr: bool) -> Option<Vec<u8>> {
    if is_ascii_repr {
        return Some(s.as_bytes().to_vec());
    }

    if s.len().is_multiple_of(2) {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
            .collect()
    } else {
        None
    }
}
