use regex::bytes::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchType {
    Hex(Vec<u8>),
    Ascii(String),
    Regex(String),
}

/// Searches for a pattern in the hex data.
pub fn search<'a>(
    iter: impl Iterator<Item = (&'a usize, &'a Vec<u8>)>,
    search_type: &SearchType,
) -> Vec<usize> {
    match search_type {
        SearchType::Hex(p) => search_bytes(iter, p),
        SearchType::Ascii(s) => search_bytes(iter, s.as_bytes()),
        SearchType::Regex(p) => search_regex(iter, p),
    }
}

/// Slide window search for `BTreeMap<usize, Vec<u8>>`.
/// Returns the starting addresses of all matches.
fn search_bytes<'a>(
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
/// Returns the starting addresses of all matches.
fn search_regex<'a>(
    iter: impl Iterator<Item = (&'a usize, &'a Vec<u8>)>,
    pattern: &str,
) -> Vec<usize> {
    let Ok(re) = Regex::new(pattern) else {
        return vec![];
    };
    let mut matches = Vec::new();

    for (&addr, data) in iter {
        for mtch in re.find_iter(data) {
            matches.push(addr + mtch.start());
        }
    }

    matches
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;
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
        let res = search(map.iter(), &SearchType::Hex(pattern));

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
        let res = search(map.iter(), &SearchType::Ascii("20.71".to_string()));

        // Assert
        assert_eq!(res, vec![start_addr + 203]);
    }

    #[test]
    fn test_search_ascii_regex() {
        // Arrange
        let rng = rand::rng();
        let start_addr = 0x1000;
        let pattern = vec![0x37, 0x37, 0x4C, 0x6F, 0x4C]; // "77LoL" in ASCII

        let mut random_bytes: Vec<u8> = rng
            .sample_iter(rand::distr::StandardUniform)
            .take(1000)
            .collect();
        random_bytes[203..208].copy_from_slice(&pattern); // replace elements with the pattern

        let map: BTreeMap<usize, Vec<u8>> = BTreeMap::from([(start_addr, random_bytes)]);

        // Act
        let res = search(map.iter(), &SearchType::Regex(r"\d{2}\D{2}L".to_string()));

        // Assert
        assert_eq!(res, vec![start_addr + 203]);
    }
}
