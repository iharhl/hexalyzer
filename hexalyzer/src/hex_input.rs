//! Parsing of hex values typed or pasted by the user

/// Strip a `0x` / `0X` prefix (if any)
fn strip_hex_prefix(s: &str) -> &str {
    s.strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s)
}

/// Keep only the hex digits of an input field's text
pub fn sanitize_hex_input(value: &mut String) {
    *value = strip_hex_prefix(value.trim_start())
        .chars()
        .filter(char::is_ascii_hexdigit)
        .collect();
}

/// Parse a hex address, e.g. `1F00`, `0x1F00`, ` 0X1f00 `
pub fn parse_hex_addr(input: &str) -> Option<usize> {
    let s = strip_hex_prefix(input.trim());
    // `from_str_radix` accepts a leading sign -> not a valid address
    if s.starts_with(['+', '-']) {
        return None;
    }
    usize::from_str_radix(s, 16).ok()
}

/// Parse a hex byte pattern.
///
/// Groups of digits may be separated by whitespace or commas, and each group may have
/// a `0x` / `0X` prefix, e.g. `0A1B`, `0A 1B`, `0x0A1B` or `0x0A, 0x1B`. A group must
/// have an even number of digits, except a single digit, which is one byte (`0xA` = `0A`).
///
/// Returns `None` if the input is not a valid pattern.
pub fn parse_hex_pattern(s: &str) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();

    let groups = s
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|g| !g.is_empty());

    for group in groups {
        let digits = strip_hex_prefix(group);

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

#[cfg(test)]
mod tests {
    use super::*;

    /// `sanitize_hex_input` applied to a copy of `input`
    fn sanitized(input: &str) -> String {
        let mut value = input.to_string();
        sanitize_hex_input(&mut value);
        value
    }

    // ========================= sanitize_hex_input =========================

    #[test]
    fn test_sanitize_hex_input_keeps_hex_digits() {
        // Arrange
        let inputs = ["1F00", "abcdef", "0123456789ABCDEF"];

        // Act
        let values = inputs.map(sanitized);

        // Assert
        assert_eq!(values, ["1F00", "abcdef", "0123456789ABCDEF"]);
    }

    #[test]
    fn test_sanitize_hex_input_drops_prefix() {
        // Arrange
        let inputs = ["0x1F00", "0X1f00", " 0x1F00 ", "0x", "0xFF"];

        // Act
        let values = inputs.map(sanitized);

        // Assert: the `0` of the prefix is not kept as a digit
        assert_eq!(values, ["1F00", "1f00", "1F00", "", "FF"]);
    }

    #[test]
    fn test_sanitize_hex_input_drops_other_chars() {
        // Arrange
        let inputs = ["0800_0000", "1F 00", "1F00h", "-12", "zz", "x1F"];

        // Act
        let values = inputs.map(sanitized);

        // Assert
        assert_eq!(values, ["08000000", "1F00", "1F00", "12", "", "1F"]);
    }

    #[test]
    fn test_sanitize_hex_input_prefix_only_at_start() {
        // Arrange
        let inputs = ["10x2", "00x"];

        // Act
        let values = inputs.map(sanitized);

        // Assert: a `0x` after the first digit is not a prefix
        assert_eq!(values, ["102", "00"]);
    }

    // =========================== parse_hex_addr ===========================

    #[test]
    fn test_parse_hex_addr_valid() {
        // Arrange
        let inputs = ["1F00", "1f00", "0", "08000000", "FFFFFFFF"];

        // Act
        let addrs = inputs.map(parse_hex_addr);

        // Assert
        assert_eq!(
            addrs,
            [
                Some(0x1F00),
                Some(0x1F00),
                Some(0),
                Some(0x0800_0000),
                Some(0xFFFF_FFFF)
            ]
        );
    }

    #[test]
    fn test_parse_hex_addr_prefix_and_whitespace() {
        // Arrange
        let inputs = ["0x1F00", "0X1f00", " 0x1F00 ", "\t1F00\n"];

        // Act
        let addrs = inputs.map(parse_hex_addr);

        // Assert
        assert_eq!(addrs, [Some(0x1F00); 4]);
    }

    #[test]
    fn test_parse_hex_addr_invalid() {
        // Arrange
        let inputs = ["", " ", "0x", "zz", "1G", "1F 00", "0x0x1F", "1_000"];

        // Act
        let addrs = inputs.map(parse_hex_addr);

        // Assert
        assert_eq!(addrs, [None; 8]);
    }

    #[test]
    fn test_parse_hex_addr_rejects_sign() {
        // Arrange
        let inputs = ["-1", "+1", "0x-1", "0x+1"];

        // Act
        let addrs = inputs.map(parse_hex_addr);

        // Assert
        assert_eq!(addrs, [None; 4]);
    }

    #[test]
    fn test_parse_hex_addr_overflow() {
        // Arrange
        let input = "1".repeat(usize::BITS as usize / 4 + 1);

        // Act
        let addr = parse_hex_addr(&input);

        // Assert
        assert_eq!(addr, None);
    }

    // ========================== parse_hex_pattern =========================

    #[test]
    fn test_parse_hex_pattern_contiguous() {
        // Arrange
        let inputs = ["0A1B", "0a1b", "0x0A1B", "0X0A1B"];

        // Act
        let patterns = inputs.map(parse_hex_pattern);

        // Assert
        assert!(
            patterns
                .iter()
                .all(|p| p.as_deref() == Some(&[0x0A, 0x1B][..]))
        );
    }

    #[test]
    fn test_parse_hex_pattern_separated_groups() {
        // Arrange
        let inputs = ["0A 1B", "0A,1B", "0x0A, 0x1B", "  0A \t 1B  ", "0A,,1B"];

        // Act
        let patterns = inputs.map(parse_hex_pattern);

        // Assert
        assert!(
            patterns
                .iter()
                .all(|p| p.as_deref() == Some(&[0x0A, 0x1B][..]))
        );
    }

    #[test]
    fn test_parse_hex_pattern_single_digit_group() {
        // Arrange
        let inputs = ["A", "0xA", "A B"];

        // Act
        let patterns = inputs.map(parse_hex_pattern);

        // Assert: a single digit is one byte
        assert_eq!(
            patterns,
            [Some(vec![0x0A]), Some(vec![0x0A]), Some(vec![0x0A, 0x0B])]
        );
    }

    #[test]
    fn test_parse_hex_pattern_odd_group_is_ambiguous() {
        // Arrange
        let inputs = ["ABC", "0xABC", "0A ABC"];

        // Act
        let patterns = inputs.map(parse_hex_pattern);

        // Assert
        assert_eq!(patterns, [None, None, None]);
    }

    #[test]
    fn test_parse_hex_pattern_invalid_chars() {
        // Arrange
        let inputs = ["0G", "0x", "-1", "+1", "0A;1B", "0A 0x"];

        // Act
        let patterns = inputs.map(parse_hex_pattern);

        // Assert
        assert_eq!(patterns, [const { None }; 6]);
    }

    #[test]
    fn test_parse_hex_pattern_empty() {
        // Arrange
        let inputs = ["", "   ", ","];

        // Act
        let patterns = inputs.map(parse_hex_pattern);

        // Assert: no groups is an empty (valid) pattern
        assert_eq!(patterns, [Some(vec![]), Some(vec![]), Some(vec![])]);
    }
}
