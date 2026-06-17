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
    /// Clear selection and cancel any in-progress byte edit.
    pub(crate) fn clear_selection(&mut self) {
        if self.editor.in_progress {
            self.editor.clear();
        }
        self.selection.clear();
    }

    /// Update edit buffer used for temporary storage of user key inputs
    /// during byte editing process
    pub(crate) fn update_edit_buffer(&mut self, typed_chars: &[char]) {
        if self.editor.blocked {
            if self.editor.in_progress {
                self.editor.clear();
            }
            return;
        }

        for &ch in typed_chars {
            let ch = ch.to_ascii_uppercase();
            if !ch.is_ascii_hexdigit() {
                continue;
            }

            if self.editor.in_progress {
                if self.editor.addr != self.selection.range {
                    self.editor.clear();
                } else {
                    self.append_edit_nibble(ch);
                    continue;
                }
            }

            if self.selection.range.is_some() && self.selection.released {
                self.editor.in_progress = true;
                self.editor.addr = self.selection.range;
                self.editor.buffer.clear();
                self.editor.buffer.push(ch);
            }
        }
    }

    /// Append one hex digit to the in-progress edit buffer; commit when two digits are entered.
    fn append_edit_nibble(&mut self, ch: char) {
        if self.editor.buffer.len() >= 2 {
            self.editor.buffer.clear();
        }
        self.editor.buffer.push(ch);

        if self.editor.buffer.len() == 2 {
            self.commit_byte_edit();
        }
    }

    fn commit_byte_edit(&mut self) {
        if let Ok(value) = u8::from_str_radix(&self.editor.buffer, 16)
            && let Some([start, end]) = self.editor.addr
        {
            let s = start.min(end);
            let e = start.max(end);

            for addr in s..=e {
                let prev_value = self.ih.read_byte(addr);
                if self.ih.update_byte(addr, value).ok() == Some(())
                    && let Some(prev) = prev_value
                    && value != prev
                {
                    self.editor.modified.entry(addr).or_insert(prev);
                }
            }

            if !self.search.results.is_empty() {
                self.search.redo();
            }
        }
        self.editor.clear();
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
