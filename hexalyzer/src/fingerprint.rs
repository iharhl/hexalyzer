//! Fingerprint of the session data. Tells whether it differs from the saved file.
//!
//! The fingerprint is a wrapping sum of a hash per `(address, byte)` pair, so it:
//! - does not depend on how the data is split into chunks (e.g. after remove + insert);
//! - can be updated in O(1) when a single byte changes (see [`Fingerprint::byte_changed`]).

use intelhexlib::IntelHex;
use std::hash::{DefaultHasher, Hash, Hasher};

/// Tracks whether the data differs from the last loaded or saved version
#[derive(Debug, Default)]
pub struct Fingerprint {
    /// Fingerprint of the current data
    current: u64,
    /// Fingerprint of the data as last loaded or saved
    saved: u64,
}

impl Fingerprint {
    /// Recompute from the whole data (after layout changes)
    pub fn recompute(&mut self, ih: &IntelHex) {
        self.current = content_hash(ih);
    }

    /// Update `current` hash in O(1) after the byte at `addr` changed from `old` to `new`
    pub const fn byte_changed(&mut self, addr: usize, old: u8, new: u8) {
        self.current = self
            .current
            .wrapping_sub(byte_hash(addr, old))
            .wrapping_add(byte_hash(addr, new));
    }

    /// Recompute `current` hash and remember as the `saved` state (after load, reload or save)
    pub fn mark_saved(&mut self, ih: &IntelHex) {
        self.recompute(ih);
        self.saved = self.current;
    }

    /// Returns `true` if the data differs from the last loaded or saved version
    pub const fn is_changed(&self) -> bool {
        self.current != self.saved
    }
}

/// `SplitMix64` scrambles a 64-bit value: each input gives a different output, and similar
/// inputs give unrelated outputs. Without this, the per-byte hashes would be linear
/// and different data could add up to the same sum (e.g. two swapped bytes).
const fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// Returns hash of a single byte at its address
const fn byte_hash(addr: usize, value: u8) -> u64 {
    splitmix64(((addr as u64) << 8) | value as u64)
}

/// Fingerprint of the whole data, including the start address record
fn content_hash(ih: &IntelHex) -> u64 {
    let start_addr_hash = ih.start_addr.map_or(0, |record| {
        let mut hasher = DefaultHasher::new();
        record.hash(&mut hasher);
        hasher.finish()
    });

    ih.bytes().fold(start_addr_hash, |hash, (addr, value)| {
        hash.wrapping_add(byte_hash(addr, value))
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Returns `IntelHex` with zero-filled inclusive ranges
    fn create_ih(ranges: &[(usize, usize)]) -> IntelHex {
        let mut ih = IntelHex::new();
        for &(start, end) in ranges {
            ih.write_range(start, end).unwrap();
        }
        ih
    }

    /// Returns fingerprint marked as saved for `ih`
    fn saved(ih: &IntelHex) -> Fingerprint {
        let mut fp = Fingerprint::default();
        fp.mark_saved(ih);
        fp
    }

    /// Write a byte and update the fingerprint the way the editor does
    fn set(ih: &mut IntelHex, fp: &mut Fingerprint, addr: usize, value: u8) {
        let old = ih.read_byte(addr).unwrap();
        ih.update_byte(addr, value).unwrap();
        fp.byte_changed(addr, old, value);
    }

    #[test]
    fn test_default_and_empty_are_unchanged() {
        // Arrange
        let ih = IntelHex::new();

        // Act
        let default_fp = Fingerprint::default();
        let empty_fp = saved(&ih);

        // Assert
        assert!(!default_fp.is_changed());
        assert!(!empty_fp.is_changed());
    }

    #[test]
    fn test_mark_saved() {
        // Arrange
        let mut ih = create_ih(&[(0x1000, 0x10FF)]);
        let mut fp = Fingerprint::default();

        // Act
        fp.recompute(&ih);

        // Assert
        assert!(fp.is_changed(), "data differs from the empty default");

        // Act
        fp.mark_saved(&ih);

        // Assert
        assert!(!fp.is_changed());

        // Act
        ih.remove_range(0x1000, 0x100F).unwrap();
        fp.recompute(&ih);

        // Assert
        assert!(fp.is_changed());

        // Act
        fp.mark_saved(&ih);

        // Assert
        assert!(!fp.is_changed(), "the new state is the saved one");
    }

    #[test]
    fn test_byte_edit_and_back() {
        // Arrange
        let mut ih = create_ih(&[(0x1000, 0x100F)]);
        let mut fp = saved(&ih);

        // Act
        set(&mut ih, &mut fp, 0x1005, 0x55);

        // Assert
        assert!(fp.is_changed());

        // Act
        set(&mut ih, &mut fp, 0x1005, 0x77);

        // Assert
        assert!(fp.is_changed());

        // Act - type the original value back
        set(&mut ih, &mut fp, 0x1005, 0x00);

        // Assert
        assert!(!fp.is_changed());
    }

    #[test]
    fn test_byte_changed_matches_recompute() {
        // Arrange
        let mut ih = create_ih(&[(0x0, 0xFF), (0x8000, 0x80FF)]);
        let mut fp = saved(&ih);
        let mut full = Fingerprint::default();

        // Act
        let mut value: u8 = 0;
        for addr in (0x0..0x100).chain(0x8000..0x8100).step_by(7) {
            value = value.wrapping_add(37);
            set(&mut ih, &mut fp, addr, value);
        }
        full.recompute(&ih);

        // Assert
        assert_eq!(fp.current, full.current);
    }

    #[test]
    fn test_edit_order_does_not_matter() {
        // Arrange
        let mut ih1 = create_ih(&[(0x0, 0xF)]);
        let mut fp1 = saved(&ih1);
        let mut ih2 = create_ih(&[(0x0, 0xF)]);
        let mut fp2 = saved(&ih2);

        // Act
        set(&mut ih1, &mut fp1, 0x1, 0xAA);
        set(&mut ih1, &mut fp1, 0x2, 0xBB);

        set(&mut ih2, &mut fp2, 0x2, 0xBB);
        set(&mut ih2, &mut fp2, 0x1, 0xAA);

        // Assert
        assert_eq!(fp1.current, fp2.current);
    }

    #[test]
    fn test_remove_and_reinsert_is_unchanged() {
        // Arrange
        let mut ih = create_ih(&[(0x1000, 0x103F)]);
        let mut fp = saved(&ih);

        // Act
        ih.remove_range(0x1010, 0x101F).unwrap();
        fp.recompute(&ih);

        // Assert
        assert!(fp.is_changed());

        // Act
        ih.write_range(0x1010, 0x101F).unwrap();
        fp.recompute(&ih);

        // Assert
        assert!(!fp.is_changed());
    }

    #[test]
    fn test_independent_of_chunk_layout() {
        // Arrange - same bytes, inserted as one range or as pieces in a different order
        let ih1 = create_ih(&[(0x0, 0x3F)]);
        let ih2 = create_ih(&[(0x20, 0x3F), (0x0, 0xF), (0x10, 0x1F)]);

        // Act
        let hash1 = content_hash(&ih1);
        let hash2 = content_hash(&ih2);

        // Assert
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_detects_differences() {
        // Arrange
        let base = create_ih(&[(0x0, 0xF)]);
        let extra_byte = create_ih(&[(0x0, 0x10)]);
        let missing_byte = create_ih(&[(0x0, 0xE)]);
        let shifted = create_ih(&[(0x1, 0x10)]);
        let mut changed_value = create_ih(&[(0x0, 0xF)]);
        changed_value.update_byte(0x3, 0x01).unwrap();

        // Act
        let base_hash = content_hash(&base);

        // Assert
        assert_ne!(base_hash, content_hash(&extra_byte));
        assert_ne!(base_hash, content_hash(&missing_byte));
        assert_ne!(base_hash, content_hash(&shifted));
        assert_ne!(base_hash, content_hash(&changed_value));
    }

    #[test]
    fn test_swapped_values_differ() {
        // Arrange - same values at the same addresses, but swapped
        let mut ih1 = create_ih(&[(0x0, 0x1)]);
        ih1.update_byte(0x0, 0x01).unwrap();
        ih1.update_byte(0x1, 0x02).unwrap();

        let mut ih2 = create_ih(&[(0x0, 0x1)]);
        ih2.update_byte(0x0, 0x02).unwrap();
        ih2.update_byte(0x1, 0x01).unwrap();

        // Act
        let hash1 = content_hash(&ih1);
        let hash2 = content_hash(&ih2);

        // Assert
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_start_address_record() {
        // Arrange
        let mut ih = create_ih(&[(0x0, 0xF)]);
        let mut fp = saved(&ih);

        // Act - add start address
        ih.start_addr = Some([0x11; 18]);
        fp.recompute(&ih);

        // Assert
        assert!(fp.is_changed());

        // Act - change start address
        fp.mark_saved(&ih);
        ih.start_addr = Some([0x22; 18]);
        fp.recompute(&ih);

        // Assert
        assert!(fp.is_changed());

        // Act - restore start address
        ih.start_addr = Some([0x11; 18]);
        fp.recompute(&ih);

        // Assert
        assert!(!fp.is_changed());
    }

    #[test]
    fn test_zero_byte_at_zero_address_is_detected() {
        // Arrange: hashes of `(0x0, 0x00)` must not be zero, or this byte would be invisible
        let empty = saved(&IntelHex::new());
        let mut fp = empty;
        let ih = create_ih(&[(0x0, 0x0)]);

        // Act
        fp.recompute(&ih);

        // Assert
        assert!(fp.is_changed());
    }

    #[test]
    fn test_byte_changed_to_same_value() {
        // Arrange
        let ih = create_ih(&[(0x0, 0xF)]);
        let mut fp = saved(&ih);

        // Act
        fp.byte_changed(0x3, 0x00, 0x00);

        // Assert
        assert!(!fp.is_changed());
    }

    #[test]
    fn test_start_address_removed() {
        // Arrange
        let mut ih = create_ih(&[(0x0, 0xF)]);
        ih.start_addr = Some([0x11; 18]);
        let mut fp = saved(&ih);
        ih.start_addr = None;

        // Act
        fp.recompute(&ih);

        // Assert
        assert!(fp.is_changed());
    }
}
