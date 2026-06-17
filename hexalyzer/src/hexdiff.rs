//! Comparison helpers for a future side-by-side diff view.
//!
//! Produces address sets consumed by [`crate::hexview::PageContext::diff_addrs`] and
//! rendered via [`crate::hexview::CellFlags::DIFFERENT`].

use intelhexlib::IntelHex;
use std::collections::HashSet;
use std::ops::RangeInclusive;

/// Addresses where two byte sources disagree (value mismatch or present vs gap).
#[derive(Debug, Default)]
pub struct DiffSet {
    pub different: HashSet<usize>,
}

impl DiffSet {
    /// Compare every address in `range`, marking value or presence mismatches.
    #[must_use]
    pub fn compare(left: &IntelHex, right: &IntelHex, range: RangeInclusive<usize>) -> Self {
        let mut different = HashSet::new();
        for addr in range {
            if left.read_byte(addr) != right.read_byte(addr) {
                different.insert(addr);
            }
        }
        Self { different }
    }

    /// Compare from min to max address across both files (includes gap-only regions).
    #[must_use]
    pub fn compare_union(left: &IntelHex, right: &IntelHex) -> Self {
        let Some(start) = left
            .get_min_addr()
            .into_iter()
            .chain(right.get_min_addr())
            .min()
        else {
            return Self::default();
        };
        let Some(end) = left
            .get_max_addr()
            .into_iter()
            .chain(right.get_max_addr())
            .max()
        else {
            return Self::default();
        };
        Self::compare(left, right, start..=end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compare_marks_value_and_presence_mismatches() {
        // Arrange
        let mut left = IntelHex::new();
        left.write_range(0x00, 0x02).unwrap();
        left.update_range(0x00, &[0xAA, 0xBB, 0xCC]).unwrap();

        let mut right = IntelHex::new();
        right.write_range(0x00, 0x01).unwrap();
        right.update_range(0x00, &[0xAA, 0x00]).unwrap();
        right.write_range(0x02, 0x02).unwrap();
        right.update_range(0x02, &[0xCC]).unwrap();

        // Act
        let diff = DiffSet::compare(&left, &right, 0x00..=0x02);

        // Assert
        assert!(!diff.different.contains(&0x00));
        assert!(diff.different.contains(&0x01));
        assert!(!diff.different.contains(&0x02));
    }

    #[test]
    fn compare_union_spans_both_files() {
        // Arrange
        let mut left = IntelHex::new();
        left.write_range(0x10, 0x10).unwrap();
        left.update_range(0x10, &[0x01]).unwrap();

        let mut right = IntelHex::new();
        right.write_range(0x20, 0x20).unwrap();
        right.update_range(0x20, &[0x02]).unwrap();

        // Act
        let diff = DiffSet::compare_union(&left, &right);

        // Assert
        assert!(diff.different.contains(&0x10));
        assert!(diff.different.contains(&0x20));
    }
}
