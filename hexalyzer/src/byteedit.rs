use crate::app::HexSession;
use std::collections::HashMap;
use std::collections::hash_map::Entry;

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
    /// When `true`, `update_edit_buffer` is a no-op. In-progress edit is cleared.
    pub(crate) blocked: bool,
}

impl ByteEdit {
    /// Clear the editor when edit process complete/canceled
    pub(crate) fn clear(&mut self) {
        self.in_progress = false;
        self.addr = None;
        self.buffer.clear();
    }

    /// Remap all addresses in the `modified` map by the offset (diff between new and old addr).
    /// Used after operations like relocate that shift memory addresses.
    pub(crate) fn remap_modified(&mut self, new_addr: usize, old_addr: usize) {
        if new_addr == old_addr {
            return;
        }

        if new_addr > old_addr {
            let offset = new_addr - old_addr;
            self.modified = self
                .modified
                .drain()
                .map(|(addr, val)| (addr + offset, val))
                .collect();
        } else {
            let offset = old_addr - new_addr;
            self.modified = self
                .modified
                .drain()
                .map(|(addr, val)| (addr - offset, val))
                .collect();
        }
    }
}

impl HexSession {
    /// Update edit buffer used for temporary storage of user key inputs
    /// during byte editing process
    pub(crate) fn update_edit_buffer(&mut self, typed_chars: &[char]) {
        if self.editor.blocked {
            if self.editor.in_progress {
                self.editor.clear();
            }
            return;
        }

        // If other bytes got selected mid-edit - clear the pending edit
        if self.editor.in_progress && self.editor.addr != self.selection.range {
            self.editor.clear();
        }

        for &ch in typed_chars {
            // Allow only hex chars
            if !ch.is_ascii_hexdigit() {
                continue;
            }

            if self.editor.in_progress {
                self.editor.buffer.push(ch.to_ascii_uppercase());

                // When two hex chars are entered - commit automatically
                if self.editor.buffer.len() == 2 {
                    if let Ok(value) = u8::from_str_radix(&self.editor.buffer, 16)
                        && let Some([start, end]) = self.editor.addr
                    {
                        // Handle reversed range
                        let s = start.min(end);
                        let e = start.max(end);

                        // Update the bytes. Track the original value of each changed byte,
                        // and stop tracking it once it is set back to that original value.
                        for addr in s..=e {
                            let Some(prev) = self.set_byte(addr, value) else {
                                continue;
                            };
                            match self.editor.modified.entry(addr) {
                                Entry::Occupied(orig) if *orig.get() == value => {
                                    orig.remove();
                                }
                                Entry::Occupied(_) => {}
                                Entry::Vacant(orig) => {
                                    orig.insert(prev);
                                }
                            }
                        }
                        self.clear_marks_if_saved();

                        // If there are search results - redo it
                        if !self.search.results.is_empty() {
                            self.search.redo();
                        }
                    }
                    self.editor.clear();
                }
            } else if self.selection.range.is_some() && self.selection.released {
                // Start editing if there is a finished selection
                self.editor.in_progress = true;
                self.editor.addr = self.selection.range;
                self.editor.buffer = ch.to_ascii_uppercase().to_string();
            }
        }
    }

    /// Write `value` to the byte at `addr` and update the data fingerprint.
    /// Returns the previous value if the byte exists and was actually changed.
    fn set_byte(&mut self, addr: usize, value: u8) -> Option<u8> {
        let prev = self.ih.read_byte(addr)?;
        if prev == value || self.ih.update_byte(addr, value).is_err() {
            return None;
        }
        self.fingerprint.byte_changed(addr, prev, value);
        Some(prev)
    }

    /// Restore all modified bytes to their original values
    pub(crate) fn restore(&mut self) {
        for (addr, orig_value) in std::mem::take(&mut self.editor.modified) {
            self.set_byte(addr, orig_value);
        }

        if !self.search.results.is_empty() {
            self.search.redo();
        }
    }
}
