#[derive(Debug, Default)]
pub struct Selection {
    /// Range is start and end addresses of selected bytes.
    /// Inverted if selection is moving right-to-left.
    pub(crate) range: Option<[usize; 2]>,
    /// Returns `true` when cursor click removed after being pressed
    pub(crate) released: bool,
    /// When `true`, `update` and `shift_update` are no-ops.
    pub(crate) blocked: bool,
}

impl Selection {
    /// Extend selection range with provided address
    pub(crate) fn update(&mut self, addr: usize) {
        if self.blocked {
            return;
        }
        if self.released {
            self.released = false;
            self.range = None;
        }
        let sel = self.range.get_or_insert([addr, addr]);
        sel[1] = addr;
    }

    /// Extend selection range to the provided address without clearing.
    /// Used for Shift+Click range selection.
    pub(crate) fn shift_update(&mut self, addr: usize) {
        if self.blocked {
            return;
        }
        self.released = false;
        let sel = self.range.get_or_insert([addr, addr]);
        sel[1] = addr;
    }

    /// Clear selection range
    pub(crate) const fn clear(&mut self) {
        self.range = None;
        self.released = false;
    }

    /// Get the selected address range as a normalized (min, max) tuple.
    #[must_use]
    pub(crate) fn get_normalized_range(&self) -> Option<(usize, usize)> {
        self.range
            .map(|[start, end]| (start.min(end), start.max(end)))
    }
}
