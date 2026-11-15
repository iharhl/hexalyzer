use crate::IntelHexErrorKind;
use regex::bytes::RegexBuilder;

/// Slide window search for `BTreeMap<usize, Vec<u8>>`.
/// Returns the starting addresses of all matches, sorted in ascending order.
pub fn search_bytes<'a>(
    iter: impl Iterator<Item = (&'a usize, &'a Vec<u8>)>,
    pattern: &[u8],
) -> Vec<usize> {
    let size = pattern.len();
    if size == 0 {
        return vec![];
    }

    let mut matches = Vec::new();

    for (&addr, data) in iter {
        // Since contiguous data is guaranteed to be in one Vec,
        // search within the slice.
        for (offset, window) in data.windows(size).enumerate() {
            if window == pattern {
                matches.push(addr + offset);
            }
        }
    }

    matches
}

/// Regex search for `BTreeMap<usize, Vec<u8>>`.
/// Returns the starting addresses of all matches, sorted in ascending order.
///
/// Empty matches are skipped, as they do not point at any data.
///
/// # Errors
/// Returns an error if the pattern is not a valid regular expression.
pub fn search_regex<'a>(
    iter: impl Iterator<Item = (&'a usize, &'a Vec<u8>)>,
    pattern: &str,
) -> Result<Vec<usize>, IntelHexErrorKind> {
    // Unicode mode off: the data is raw bytes, so `\xFF` matches the byte 0xFF (not the
    // UTF-8 encoding of U+00FF). The `.` matches any byte, including 0x0A (newline).
    let re = RegexBuilder::new(pattern)
        .unicode(false)
        .dot_matches_new_line(true)
        .build()
        .map_err(|err| IntelHexErrorKind::InvalidRegex(err.to_string()))?;

    let mut matches = Vec::new();

    for (&addr, data) in iter {
        for mtch in re.find_iter(data).filter(|m| !m.is_empty()) {
            matches.push(addr + mtch.start());
        }
    }

    Ok(matches)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::RngExt;
    use std::collections::BTreeMap;

    #[test]
    fn test_search_bytes() {
        // Arrange
        let rng = rand::rng();
        let start_addr = 0x1000;
        let pattern = vec![0xAA, 0xBB, 0xCC, 0xDD, 0xEE];

        let mut random_bytes: Vec<u8> = rng
            .sample_iter(rand::distr::StandardUniform)
            .take(1000)
            .collect();
        random_bytes[203..208].copy_from_slice(&pattern); // replace elements with the pattern

        let map: BTreeMap<usize, Vec<u8>> = BTreeMap::from([(start_addr, random_bytes)]);

        // Act
        let res = search_bytes(map.iter(), &pattern);

        // Assert
        assert_eq!(res, vec![start_addr + 203]);
    }

    #[test]
    fn test_search_ascii_literals() {
        // Arrange
        let rng = rand::rng();
        let start_addr = 0x1000;
        let pattern = vec![0x32, 0x30, 0x2E, 0x37, 0x31]; // "20.71" in ASCII

        let mut random_bytes: Vec<u8> = rng
            .sample_iter(rand::distr::StandardUniform)
            .take(1000)
            .collect();
        random_bytes[203..208].copy_from_slice(&pattern); // replace elements with the pattern

        let map: BTreeMap<usize, Vec<u8>> = BTreeMap::from([(start_addr, random_bytes)]);

        // Act
        let res = search_bytes(map.iter(), b"20.71");

        // Assert
        assert_eq!(res, vec![start_addr + 203]);
    }

    #[test]
    fn test_search_ascii_regex() {
        // Arrange
        let mut rng = rand::rng();
        let start_addr = 0x1000;
        let pattern = vec![0x37, 0x37, 0x4C, 0x6F, 0x4C]; // "77LoL" in ASCII

        // Filler is limited to 0x40..0x80: ASCII without digits, so it can never
        // contain `\d` and the regex cannot match anywhere except the planted pattern
        let mut random_bytes: Vec<u8> = (0..1000).map(|_| rng.random_range(0x40..0x80)).collect();
        random_bytes[203..208].copy_from_slice(&pattern); // replace elements with the pattern

        let map: BTreeMap<usize, Vec<u8>> = BTreeMap::from([(start_addr, random_bytes)]);

        // Act
        let res = search_regex(map.iter(), r"\d{2}\D{2}L");

        // Assert
        assert_eq!(res, Ok(vec![start_addr + 203]));
    }

    #[test]
    fn test_search_regex_invalid() {
        // Arrange
        let map: BTreeMap<usize, Vec<u8>> = BTreeMap::from([(0x1000, b"abc".to_vec())]);

        // Act
        let res = search_regex(map.iter(), "[");

        // Assert
        assert!(matches!(res, Err(IntelHexErrorKind::InvalidRegex(_))));
    }

    #[test]
    fn test_search_regex_skips_empty_matches() {
        // Arrange
        let map: BTreeMap<usize, Vec<u8>> =
            BTreeMap::from([(0x10, b"abxxxcx".to_vec()), (0x20, b"yy".to_vec())]);

        // Act
        let res = search_regex(map.iter(), "x*");

        // Assert: only non-empty matches, no hit at every byte
        assert_eq!(res, Ok(vec![0x12, 0x16]));
    }

    #[test]
    fn test_search_regex_dot_matches_newline() {
        // Arrange
        let map: BTreeMap<usize, Vec<u8>> = BTreeMap::from([(0x10, b"xa\nb".to_vec())]);

        // Act
        let res = search_regex(map.iter(), "a.b");

        // Assert
        assert_eq!(res, Ok(vec![0x11]));
    }
}
