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
    /// Check if the provided address is within the selection range
    pub(crate) const fn is_addr_within_range(&self, addr: usize) -> bool {
        if let Some(range) = self.range {
            if range[0] < range[1] {
                return range[0] <= addr && range[1] >= addr;
            }
            return range[1] <= addr && range[0] >= addr;
        }
        false
    }

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
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Selection of `[anchor, end]` that is still being dragged
    const fn dragging(anchor: usize, end: usize) -> Selection {
        Selection {
            range: Some([anchor, end]),
            released: false,
            blocked: false,
        }
    }

    /// Selection of `[anchor, end]` after the mouse button was released
    const fn finished(anchor: usize, end: usize) -> Selection {
        Selection {
            range: Some([anchor, end]),
            released: true,
            blocked: false,
        }
    }

    // ======================== is_addr_within_range ========================

    #[test]
    fn test_is_addr_within_range_no_selection() {
        // Arrange
        let sel = Selection::default();

        // Act
        let within = sel.is_addr_within_range(0);

        // Assert
        assert!(!within);
    }

    #[test]
    fn test_is_addr_within_range_forward() {
        // Arrange
        let sel = finished(0x10, 0x14);

        // Act
        let within = [0x0F, 0x10, 0x12, 0x14, 0x15].map(|a| sel.is_addr_within_range(a));

        // Assert: both ends are inclusive
        assert_eq!(within, [false, true, true, true, false]);
    }

    #[test]
    fn test_is_addr_within_range_backward() {
        // Arrange: dragged right-to-left
        let sel = finished(0x14, 0x10);

        // Act
        let within = [0x0F, 0x10, 0x12, 0x14, 0x15].map(|a| sel.is_addr_within_range(a));

        // Assert
        assert_eq!(within, [false, true, true, true, false]);
    }

    #[test]
    fn test_is_addr_within_range_single_byte() {
        // Arrange
        let sel = finished(0x10, 0x10);

        // Act
        let within = [0x0F, 0x10, 0x11].map(|a| sel.is_addr_within_range(a));

        // Assert
        assert_eq!(within, [false, true, false]);
    }

    #[test]
    fn test_is_addr_within_range_address_space_ends() {
        // Arrange
        let sel = finished(usize::MAX, 0);

        // Act
        let within = [0, usize::MAX / 2, usize::MAX].map(|a| sel.is_addr_within_range(a));

        // Assert
        assert_eq!(within, [true; 3]);
    }

    // =============================== update ===============================

    #[test]
    fn test_update_starts_selection() {
        // Arrange
        let mut sel = Selection::default();

        // Act
        sel.update(0x10);

        // Assert
        assert_eq!(sel.range, Some([0x10, 0x10]));
    }

    #[test]
    fn test_update_while_dragging_keeps_anchor() {
        // Arrange
        let mut sel = dragging(0x10, 0x12);

        // Act
        sel.update(0x08);

        // Assert
        assert_eq!(sel.range, Some([0x10, 0x08]));
    }

    #[test]
    fn test_update_after_release_starts_new_selection() {
        // Arrange
        let mut sel = finished(0x10, 0x14);

        // Act
        sel.update(0x20);

        // Assert
        assert_eq!(sel.range, Some([0x20, 0x20]));
        assert!(!sel.released);
    }

    #[test]
    fn test_update_blocked() {
        // Arrange
        let mut sel = finished(0x10, 0x14);
        sel.blocked = true;

        // Act
        sel.update(0x20);

        // Assert
        assert_eq!(sel.range, Some([0x10, 0x14]));
        assert!(sel.released);
    }

    // ============================ shift_update ============================

    #[test]
    fn test_shift_update_without_selection() {
        // Arrange
        let mut sel = Selection::default();

        // Act
        sel.shift_update(0x10);

        // Assert
        assert_eq!(sel.range, Some([0x10, 0x10]));
    }

    #[test]
    fn test_shift_update_after_release_extends_from_anchor() {
        // Arrange
        let mut sel = finished(0x10, 0x14);

        // Act
        sel.shift_update(0x08);

        // Assert: unlike `update`, the selection is extended
        assert_eq!(sel.range, Some([0x10, 0x08]));
        assert!(!sel.released);
    }

    #[test]
    fn test_shift_update_blocked() {
        // Arrange
        let mut sel = finished(0x10, 0x14);
        sel.blocked = true;

        // Act
        sel.shift_update(0x20);

        // Assert
        assert_eq!(sel.range, Some([0x10, 0x14]));
    }

    // =============================== clear ================================

    #[test]
    fn test_clear() {
        // Arrange
        let mut sel = finished(0x10, 0x14);

        // Act
        sel.clear();

        // Assert
        assert_eq!(sel.range, None);
        assert!(!sel.released);
    }

    #[test]
    fn test_clear_keeps_blocked() {
        // Arrange
        let mut sel = finished(0x10, 0x14);
        sel.blocked = true;

        // Act
        sel.clear();

        // Assert: blocking is controlled by the popups, not by clearing
        assert!(sel.blocked);
    }
}
