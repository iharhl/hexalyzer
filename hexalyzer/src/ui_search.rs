use crate::app::{HexSession, colors};
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
    /// Index of the current search result
    idx: usize,
    /// Error of the last search (invalid hex pattern or regex)
    error: Option<(&'static str, String)>,
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
        self.addr = None;
        self.results.clear();
        self.length = 0;
        // Do not clear current to preserve text box content
        self.last = SearchState::default();
        self.idx = 0;
        self.error = None;
        self.resume_from = None;
        self.force = false;
    }

    /// Redo the last search
    pub(crate) fn redo(&mut self) {
        // Nothing was searched yet -> keep the text box and mode as they are
        if self.last == SearchState::default() {
            return;
        }

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

    /// Check if `addr` lies inside the current (active) search match
    pub(crate) fn is_current_hit(&self, addr: usize) -> bool {
        self.results
            .get(self.idx)
            .is_some_and(|&start| start <= addr && addr < start.saturating_add(self.length))
    }
}

/// Accepted hex pattern formats, shown on hover
const HEX_PATTERN_HINT: &str = "Enter pairs of hex digits, optionally separated by spaces\n\
    or commas, lower cased or prefixed with 0x.\nExamples: `0A1B` | `0a 1b` | `0x0A, 0x1B`";

impl HexSession {
    /// Run the search for the current input and mode, filling `results` and `length`.
    /// On invalid input, the results are cleared and `error` is set.
    fn run_search(&mut self) {
        let input = self.search.current.input.as_str();
        self.search.error = None;

        // Search if pattern is valid
        let valid = match self.search.current.mode {
            SearchMode::Hex => {
                if let Some(pattern) = parse_str_into_bytes(input) {
                    self.search.results = self.ih.search_bytes(&pattern);
                    self.search.length = pattern.len();
                    true
                } else {
                    self.search.error = Some(("Invalid hex pattern", HEX_PATTERN_HINT.to_string()));
                    false
                }
            }
            SearchMode::Ascii | SearchMode::Regex => {
                let regex = self.search.current.mode == SearchMode::Regex;

                if input.is_empty() {
                    false
                } else {
                    match self.ih.search_ascii(input, regex) {
                        Ok(results) => {
                            self.search.results = results;
                            // Regex: highlight the first byte of the match
                            self.search.length = if regex { 1 } else { input.len() };
                            true
                        }
                        Err(err) => {
                            self.search.error = Some(("Invalid regex", err.to_string()));
                            false
                        }
                    }
                }
            }
        };

        // Clear the result if pattern is not valid
        if !valid {
            self.search.results.clear();
        }
    }

    /// Show content of the search menu
    pub(crate) fn show_search_contents(&mut self, ui: &mut egui::Ui) {
        // RadioButtons to select between byte and ascii search
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.search.current.mode, SearchMode::Hex, "Hex")
                .on_hover_text(format!("Search for a byte pattern\n{HEX_PATTERN_HINT}"));
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

        let enter = textedit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

        // Keep the focus, so pressing Enter again moves to the next hit
        if enter {
            textedit.request_focus();
        }

        if textedit.has_focus() {
            // Clear the selection to avoid modifying bytes
            // while typing in the search area
            self.selection.clear();
        }

        if enter || self.search.force {
            // Same input -> move to next result, otherwise -> search again
            if self.search.current == self.search.last {
                if !self.search.results.is_empty() {
                    self.search.idx = (self.search.idx + 1) % self.search.results.len();
                }
            } else {
                self.run_search();

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

        // Show the search error (details on hover)
        if let Some((label, details)) = &self.search.error {
            ui.label(egui::RichText::new(*label).color(colors::WARNING))
                .on_hover_text(details);
            return;
        }

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

/// Parse a hex byte pattern.
///
/// Groups of digits may be separated by whitespace or commas, and each group may have
/// a `0x` / `0X` prefix, e.g. `0A1B`, `0A 1B`, `0x0A1B` or `0x0A, 0x1B`. A group must
/// have an even number of digits, except a single digit, which is one byte (`0xA` = `0A`).
///
/// Returns `None` if the input is not a valid pattern.
fn parse_str_into_bytes(s: &str) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();

    let groups = s
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|g| !g.is_empty());

    for group in groups {
        let digits = group
            .strip_prefix("0x")
            .or_else(|| group.strip_prefix("0X"))
            .unwrap_or(group);

        // Only ASCII hex digits
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }

        match digits.len() {
            1 => bytes.push(u8::from_str_radix(digits, 16).ok()?),
            len if len.is_multiple_of(2) => {
                for i in (0..len).step_by(2) {
                    bytes.push(u8::from_str_radix(&digits[i..i + 2], 16).ok()?);
                }
            }
            // Ambiguous, e.g. ABC could be 0A BC or AB 0C
            _ => return None,
        }
    }

    Some(bytes)
}
