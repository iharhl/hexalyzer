use crate::app::HexSession;
use std::collections::HashMap;

#[derive(Default)]
pub struct ByteEdit {
    /// Is byte being edited
    pub(crate) in_progress: bool,
    /// Buffer to store byte data during editing
    pub(crate) buffer: String,
    /// Address range of the bytes being edited
    pub(crate) addr: Option<[usize; 2]>,
    /// Tracks the modified bytes by storing addresses and original values before modification
    pub(crate) modified: HashMap<usize, u8>,
}

impl ByteEdit {
    /// Clear the editor when edit process complete/canceled
    pub(crate) fn clear(&mut self) {
        self.in_progress = false;
        self.addr = None;
        self.buffer.clear();
    }
}

impl HexSession {
    /// Update edit buffer used for temporary storage of user key inputs
    /// during byte editing process
    pub(crate) fn update_edit_buffer(&mut self, typed_char: Option<char>) {
        if self.selection.range.is_some()
            && self.selection.released
            && !self.editor.in_progress
            && let Some(ch) = typed_char
        {
            // Start editing if user types a hex char
            if ch.is_ascii_hexdigit() {
                self.editor.in_progress = true;
                self.editor.addr = self.selection.range;
                self.editor.buffer = ch.to_ascii_uppercase().to_string();
            }
        } else if self.editor.in_progress {
            // If other bytes got selected - clear and return
            if self.editor.addr != self.selection.range {
                self.editor.clear();
            }

            if let Some(ch) = typed_char {
                self.editor.buffer.insert(1, ch);
            }

            // Allow only hex chars
            self.editor.buffer.retain(|c| c.is_ascii_hexdigit());

            // When two hex chars are entered - commit automatically
            if self.editor.buffer.len() == 2 {
                if let Ok(value) = u8::from_str_radix(&self.editor.buffer, 16)
                    && let Some([start, end]) = self.editor.addr
                {
                    // Handle reversed range
                    let s = start.min(end);
                    let e = start.max(end);

                    // Update the bytes in the map. If the byte is actually changed -
                    // insert its address into Vec that tracks modified bytes.
                    for addr in s..=e {
                        let prev_value = self.ih.read_byte(addr);
                        if self.ih.update_byte(addr, value).ok() == Some(())
                            && let Some(prev) = prev_value
                            && value != prev
                        {
                            self.editor.modified.entry(addr).or_insert(prev);
                        }
                    }

                    // If there are search results - redo it
                    if !self.search.results.is_empty() {
                        self.search.redo();
                    }
                }
                self.editor.clear();
            }
        }
    }

    /// Restore all modified bytes to their original values
    pub(crate) fn restore(&mut self) {
        for (&addr, &orig_value) in &self.editor.modified {
            let _ = self.ih.update_byte(addr, orig_value);
        }

        self.editor.modified.clear();

        if !self.search.results.is_empty() {
            self.search.redo();
        }
    }
}
