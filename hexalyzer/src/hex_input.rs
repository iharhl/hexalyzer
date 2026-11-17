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

/// Keep the hex digits (uppercased) and group separators of an input field's text,
/// in the format `parse_hex_pattern` reads, e.g. `0x0a, 0x1b` -> `0A 1B`
pub fn sanitize_hex_pattern_input(value: &mut String) {
    *value = filter_digits(&strip_radix_prefixes(value, 'x'), 16);
}

/// Remove C-style radix prefixes (`0x`, `0b`; `marker` is the letter) that start a
/// group, e.g. `"0x12, 0X34"` -> `"12, 34"`
pub fn strip_radix_prefixes(s: &str, marker: char) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        let at_group_start = i == 0 || !chars[i - 1].is_ascii_alphanumeric();
        let is_prefix = chars[i] == '0'
            && chars
                .get(i + 1)
                .is_some_and(|c| c.eq_ignore_ascii_case(&marker));
        if at_group_start && is_prefix {
            i += 2;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// Keep digits of the given radix (uppercased) and turn group separators (whitespace,
/// `,` `;` `:` `-` `_`) into single spaces between digits. Anything else is dropped.
pub fn filter_digits(s: &str, radix: u32) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_digit(radix) {
            out.push(c.to_ascii_uppercase());
        } else if (c.is_whitespace() || matches!(c, ',' | ';' | ':' | '-' | '_'))
            && !out.is_empty()
            && !out.ends_with(' ')
        {
            out.push(' ');
        }
    }
    out
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
        assert_eq!(
            values,
            ["1F00", "abcdef", "0123456789ABCDEF"].map(String::from)
        );
    }

    #[test]
    fn test_sanitize_hex_input_drops_prefix() {
        // Arrange
        let inputs = ["0x1F00", "0X1f00", " 0x1F00 ", "0x", "0xFF"];

        // Act
        let values = inputs.map(sanitized);

        // Assert: the `0` of the prefix is not kept as a digit
        assert_eq!(values, ["1F00", "1f00", "1F00", "", "FF"].map(String::from));
    }

    #[test]
    fn test_sanitize_hex_input_drops_other_chars() {
        // Arrange
        let inputs = ["0800_0000", "1F 00", "1F00h", "-12", "zz", "x1F"];

        // Act
        let values = inputs.map(sanitized);

        // Assert
        assert_eq!(
            values,
            ["08000000", "1F00", "1F00", "12", "", "1F"].map(String::from)
        );
    }

    #[test]
    fn test_sanitize_hex_input_prefix_only_at_start() {
        // Arrange
        let inputs = ["10x2", "00x"];

        // Act
        let values = inputs.map(sanitized);

        // Assert: a `0x` after the first digit is not a prefix
        assert_eq!(values, ["102", "00"].map(String::from));
    }

    // ===================== sanitize_hex_pattern_input =====================

    /// `sanitize_hex_pattern_input` applied to a copy of `input`
    fn sanitized_pattern(input: &str) -> String {
        let mut value = input.to_string();
        sanitize_hex_pattern_input(&mut value);
        value
    }

    #[test]
    fn test_sanitize_hex_pattern_input_keeps_groups() {
        // Arrange
        let inputs = ["0a 1b", "0A,1B", "de:ad:be:ef", "0A  ,  1B"];

        // Act
        let values = inputs.map(sanitized_pattern);

        // Assert: separators become single spaces, digits are uppercased
        assert_eq!(
            values,
            ["0A 1B", "0A 1B", "DE AD BE EF", "0A 1B"].map(String::from)
        );
    }

    #[test]
    fn test_sanitize_hex_pattern_input_drops_prefixes() {
        // Arrange
        let inputs = ["0x0A, 0x1B", "0X0a", "0x", "0x1,0x23"];

        // Act
        let values = inputs.map(sanitized_pattern);

        // Assert
        assert_eq!(values, ["0A 1B", "0A", "", "1 23"].map(String::from));
    }

    #[test]
    fn test_sanitize_hex_pattern_input_prefix_only_at_group_start() {
        // Arrange
        let inputs = ["10x2", "A0x5", "1F 0x2"];

        // Act
        let values = inputs.map(sanitized_pattern);

        // Assert: a `0x` after a digit is not a prefix
        assert_eq!(values, ["102", "A05", "1F 2"].map(String::from));
    }

    #[test]
    fn test_sanitize_hex_pattern_input_drops_other_chars() {
        // Arrange
        let inputs = ["0G1H", "zz", "x1F", "1+2", "#FF", "1=2*3"];

        // Act
        let values = inputs.map(sanitized_pattern);

        // Assert: only separators become spaces, other symbols are not
        assert_eq!(
            values,
            ["01", "", "1F", "12", "FF", "123"].map(String::from)
        );
    }

    #[test]
    fn test_sanitize_hex_pattern_input_no_leading_separator() {
        // Arrange
        let inputs = [" 0A", ", 0A", "+0A"];

        // Act
        let values = inputs.map(sanitized_pattern);

        // Assert
        assert_eq!(values, ["0A"; 3].map(String::from));
    }

    #[test]
    fn test_sanitize_hex_pattern_input_output_parses() {
        // Arrange
        let inputs = ["0x0a, 0x1b", "0A:1B", "0a-1b", "0a1b"];

        // Act
        let patterns = inputs.map(|s| parse_hex_pattern(&sanitized_pattern(s)));

        // Assert
        assert!(
            patterns
                .iter()
                .all(|p| p.as_deref() == Some(&[0x0A, 0x1B][..]))
        );
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
