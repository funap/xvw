//! Parsing helpers for byte-oriented clipboard operations.

/// Parses common hex-editor clipboard representations into bytes.
///
/// The parser accepts continuous or separated hexadecimal values, C/Rust
/// style `0xNN` arrays, binary octets, escaped `\xNN` strings, JSON-like
/// decimal arrays, and finally falls back to UTF-8 text bytes. The fallback
/// makes pasting ordinary text into the ASCII column useful without making
/// the clipboard format platform-specific.
pub fn parse_paste_bytes(input: &str) -> Option<Vec<u8>> {
    let text = input.trim();
    if text.is_empty() {
        return Some(Vec::new());
    }

    if let Some(bytes) = parse_escaped_bytes(text) {
        return Some(bytes);
    }

    let is_array = (text.starts_with('[') && text.ends_with(']')) || (text.starts_with('{') && text.ends_with('}'));
    if is_array && let Some(bytes) = parse_decimal_array(&text[1..text.len() - 1]) {
        return Some(bytes);
    }

    if let Some(bytes) = parse_prefixed_hex(text)
        && !bytes.is_empty()
    {
        return Some(bytes);
    }

    if let Some(bytes) = parse_binary_tokens(text)
        && !bytes.is_empty()
    {
        return Some(bytes);
    }

    if let Some(bytes) = parse_hex(text)
        && !bytes.is_empty()
    {
        return Some(bytes);
    }

    Some(text.as_bytes().to_vec())
}

fn parse_escaped_bytes(text: &str) -> Option<Vec<u8>> {
    if !text.contains("\\x") && !text.contains("\\X") {
        return None;
    }

    let bytes = text.as_bytes();
    let mut result = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if index + 3 < bytes.len() && bytes[index] == b'\\' && (bytes[index + 1] == b'x' || bytes[index + 1] == b'X') {
            let high = hex_digit(bytes[index + 2])?;
            let low = hex_digit(bytes[index + 3])?;
            result.push((high << 4) | low);
            index += 4;
        } else {
            result.extend(&bytes[index..]);
            break;
        }
    }
    Some(result)
}

fn parse_decimal_array(text: &str) -> Option<Vec<u8>> {
    let tokens: Vec<&str> = text
        .split(|c: char| c == ',' || c.is_ascii_whitespace())
        .filter(|token| !token.is_empty())
        .collect();
    if tokens.is_empty() {
        return Some(Vec::new());
    }

    let mut result = Vec::with_capacity(tokens.len());
    for token in tokens {
        let value = if let Some(hex) = token.strip_prefix("0x").or_else(|| token.strip_prefix("0X")) {
            u16::from_str_radix(hex, 16).ok()?
        } else {
            token.parse::<u16>().ok()?
        };
        result.push(u8::try_from(value).ok()?);
    }
    Some(result)
}

fn parse_prefixed_hex(text: &str) -> Option<Vec<u8>> {
    let mut result = Vec::new();
    let bytes = text.as_bytes();
    let mut index = 0;
    while index + 2 <= bytes.len() {
        if bytes[index] != b'0' || !matches!(bytes[index + 1], b'x' | b'X') {
            index += 1;
            continue;
        }

        let value_start = index + 2;
        let mut value_end = value_start;
        while value_end < bytes.len() && bytes[value_end].is_ascii_hexdigit() {
            value_end += 1;
        }
        let digits = &bytes[value_start..value_end];
        if !(1..=2).contains(&digits.len()) {
            return None;
        }
        result.push(u8::from_str_radix(std::str::from_utf8(digits).ok()?, 16).ok()?);
        index = value_end;
    }

    (!result.is_empty()).then_some(result)
}

fn parse_binary_tokens(text: &str) -> Option<Vec<u8>> {
    let tokens: Vec<&str> = text
        .split(|c: char| c == ',' || c.is_ascii_whitespace())
        .filter(|token| !token.is_empty())
        .collect();
    if tokens.is_empty() || tokens.iter().any(|token| token.len() != 8 || token.chars().any(|c| c != '0' && c != '1')) {
        return None;
    }
    tokens.into_iter().map(|token| u8::from_str_radix(token, 2).ok()).collect()
}

fn parse_hex(text: &str) -> Option<Vec<u8>> {
    struct ParsedHexLine<'a> {
        tokens: Vec<&'a str>,
        has_ascii_panel: bool,
        offset_candidate: Option<usize>,
        rest_all_two_digit_bytes: bool,
    }

    let mut parsed_lines = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let (hex_part, has_ascii_panel) = if trimmed.ends_with('|')
            && let Some((before_pipe, _after_pipe)) = trimmed[..trimmed.len() - 1].split_once('|')
        {
            if before_pipe.trim().is_empty() {
                return None;
            }
            (before_pipe, true)
        } else {
            (line, false)
        };

        let tokens: Vec<&str> = hex_part.split_ascii_whitespace().collect();
        if tokens.is_empty() {
            continue;
        }

        let offset_candidate = if tokens.len() > 1 && tokens[0].len() >= 6 && is_hex(tokens[0]) {
            usize::from_str_radix(tokens[0], 16).ok()
        } else {
            None
        };
        let rest_all_two_digit_bytes = tokens.len() > 1
            && tokens[1..].iter().all(|token| {
                let token = token.trim_matches(|c| c == ',' || c == ';');
                token.len() == 2 && is_hex(token)
            });

        parsed_lines.push(ParsedHexLine {
            tokens,
            has_ascii_panel,
            offset_candidate,
            rest_all_two_digit_bytes,
        });
    }

    if parsed_lines.is_empty() {
        return None;
    }

    // Only skip a leading address column across un-piped lines when multiple lines form
    // a consistent contiguous hexdump sequence (offset + line byte count == next offset).
    let is_contiguous_offset_dump = parsed_lines.len() >= 2
        && parsed_lines.iter().all(|line| line.offset_candidate.is_some() && line.rest_all_two_digit_bytes)
        && parsed_lines.windows(2).all(|window| {
            let Some(addr0) = window[0].offset_candidate else {
                return false;
            };
            let Some(addr1) = window[1].offset_candidate else {
                return false;
            };
            addr0.checked_add(window[0].tokens.len() - 1) == Some(addr1)
        });

    let mut result = Vec::new();
    let mut saw_hex = false;

    for parsed in parsed_lines {
        let skip_first = parsed.offset_candidate.is_some() && parsed.rest_all_two_digit_bytes && (parsed.has_ascii_panel || is_contiguous_offset_dump);
        let start_idx = usize::from(skip_first);

        for &raw_token in &parsed.tokens[start_idx..] {
            let token = raw_token.trim_matches(|c| c == ',' || c == ';');
            if token.is_empty() {
                continue;
            }
            if token.len() == 2 && is_hex(token) {
                result.push(u8::from_str_radix(token, 16).ok()?);
                saw_hex = true;
            } else if token.len() > 2 && token.len().is_multiple_of(2) && is_hex(token) {
                for pair in token.as_bytes().as_chunks::<2>().0 {
                    result.push(u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?);
                }
                saw_hex = true;
            } else {
                return None;
            }
        }
    }

    saw_hex.then_some(result)
}

fn is_hex(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::parse_paste_bytes;

    #[test]
    fn parses_common_hex_formats() {
        assert_eq!(parse_paste_bytes("deadbeef"), Some(vec![0xde, 0xad, 0xbe, 0xef]));
        assert_eq!(parse_paste_bytes("DE AD\nBE EF"), Some(vec![0xde, 0xad, 0xbe, 0xef]));
        assert_eq!(parse_paste_bytes("{ 0x41, 0x42 }"), Some(vec![0x41, 0x42]));
        assert_eq!(parse_paste_bytes("01000001 01000010"), Some(vec![0x41, 0x42]));
    }

    #[test]
    fn parses_hex_dump_and_arrays() {
        let dump = "00000000  48 65 6c 6c 6f  |Hello|\n00000005  21              |!|";
        assert_eq!(parse_paste_bytes(dump), Some(b"Hello!".to_vec()));
        assert_eq!(parse_paste_bytes("[65, 66, 255]"), Some(vec![65, 66, 255]));
        assert_eq!(parse_paste_bytes("{65, 66}"), Some(vec![65, 66]));
        assert_eq!(parse_paste_bytes("const unsigned char data[] = {0x41, 0x42};"), Some(vec![0x41, 0x42]));
        assert_eq!(parse_paste_bytes("const DATA: [u8; 2] = [0x41, 0x42];"), Some(vec![0x41, 0x42]));
        assert_eq!(parse_paste_bytes("\\x48\\x69"), Some(b"Hi".to_vec()));
    }

    #[test]
    fn falls_back_to_text() {
        assert_eq!(parse_paste_bytes("hello"), Some(b"hello".to_vec()));
        assert_eq!(parse_paste_bytes("aa:bb 01"), Some(b"aa:bb 01".to_vec()));
        assert_eq!(parse_paste_bytes("01 02-03"), Some(b"01 02-03".to_vec()));
        assert_eq!(parse_paste_bytes("foo-bar dead beef"), Some(b"foo-bar dead beef".to_vec()));
        assert_eq!(parse_paste_bytes("|hello|"), Some(b"|hello|".to_vec()));
    }

    #[test]
    fn preserves_long_first_hex_token_and_handles_hexdump_variants() {
        assert_eq!(
            parse_paste_bytes("DEADBEEF CAFEBABE"),
            Some(vec![0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE])
        );
        assert_eq!(parse_paste_bytes("deadbeef 01 02"), Some(vec![0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02]));
        assert_eq!(parse_paste_bytes("00000000  48 65 6c 6c 6f\n00000005  21"), Some(b"Hello!".to_vec()));
        assert_eq!(parse_paste_bytes("00000000  7c 41  ||A|"), Some(vec![0x7c, 0x41]));
    }
}
