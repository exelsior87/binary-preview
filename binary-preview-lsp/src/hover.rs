use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position, Url};

use crate::binary;

pub fn create(text: &str, uri: &Url, position: Position) -> Option<Hover> {
    let line = text.lines().nth(position.line as usize)?;
    let byte_idx = utf16_col_to_byte_idx(line, position.character)?;

    if let Some(token) = extract_numeric_literal(line, byte_idx) {
        if let Some(preview) = format_numeric_literal(token) {
            return Some(markdown_hover(preview));
        }
    }

    let raw_path = extract_path(line, byte_idx)?;
    let source_path = uri.to_file_path().ok()?;
    let binary_path = resolve_path(&source_path, raw_path)?;
    if !binary_path.is_file() {
        return None;
    }

    let file_size = std::fs::metadata(&binary_path).ok()?.len();
    let header = match read_header(&binary_path, 64) {
        Ok(header) => header,
        Err(err) => {
            return Some(markdown_hover(format!(
                "### Binary Preview\n\n\
                 **Path:** `{}`\n\n\
                 Failed to read file: \n\n\
                 `{}`",
                binary_path.display(),
                err
            )));
        }
    };

    let hex_dump = format_hex_dump(&header);
    let binary_summary = binary::inspect(&binary_path)
        .ok()
        .map(|summary| format!("```text\n{summary}\n```\n\n"))
        .unwrap_or_default();
    Some(markdown_hover(format!(
        "### Binary Preview\n\n\
         **Path:** `{}`\n\n\
         **Size:** `{}` bytes\n\n\
         {}\
         **Header:** `{}` bytes\n\n\
         ```text\n\
         {}\
         ```",
        binary_path.display(),
        file_size,
        binary_summary,
        header.len(),
        hex_dump,
    )))
}

fn markdown_hover(value: String) -> Hover {
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        }),
        range: None,
    }
}

fn utf16_col_to_byte_idx(line: &str, utf16_col: u32) -> Option<usize> {
    let mut utf16_count = 0u32;
    for (byte_idx, ch) in line.char_indices() {
        if utf16_count == utf16_col {
            return Some(byte_idx);
        }
        utf16_count += ch.len_utf16() as u32;
        if utf16_count > utf16_col {
            return None;
        }
    }
    (utf16_count == utf16_col).then_some(line.len())
}

fn is_identifier_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn extract_numeric_literal(line: &str, byte_idx: usize) -> Option<&str> {
    let bytes = line.as_bytes();
    let mut start = 0;

    while start < bytes.len() {
        let starts_with_digit = bytes[start].is_ascii_digit();
        let starts_with_dot =
            bytes[start] == b'.' && bytes.get(start + 1).is_some_and(u8::is_ascii_digit);
        let follows_identifier = start > 0 && is_identifier_char(bytes[start - 1] as char);

        if (starts_with_digit || starts_with_dot) && !follows_identifier {
            let end = scan_numeric_literal(bytes, start);
            let has_identifier_suffix = end < bytes.len() && is_identifier_char(bytes[end] as char);
            if end > start && !has_identifier_suffix {
                if byte_idx >= start && byte_idx <= end {
                    return Some(&line[start..end]);
                }
                start = end;
                continue;
            }
        }
        start += 1;
    }
    None
}

fn scan_numeric_literal(bytes: &[u8], start: usize) -> usize {
    let mut index = start;

    if bytes.get(index) == Some(&b'0') && matches!(bytes.get(index + 1), Some(b'x' | b'X')) {
        index += 2;
        index = consume_digits(bytes, index, 16);
        return consume_integer_suffix(bytes, index);
    }
    if bytes.get(index) == Some(&b'0') && matches!(bytes.get(index + 1), Some(b'b' | b'B')) {
        index += 2;
        index = consume_digits(bytes, index, 2);
        return consume_integer_suffix(bytes, index);
    }

    index = consume_digits(bytes, index, 10);
    let mut is_float = start < bytes.len() && bytes[start] == b'.';
    if bytes.get(index) == Some(&b'.') {
        is_float = true;
        index = consume_digits(bytes, index + 1, 10);
    }
    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        let exponent_start = index;
        index += 1;
        if matches!(bytes.get(index), Some(b'+' | b'-')) {
            index += 1;
        }
        let exponent_end = consume_digits(bytes, index, 10);
        if exponent_end > index {
            is_float = true;
            index = exponent_end;
        } else {
            index = exponent_start;
        }
    }

    if is_float {
        if matches!(bytes.get(index), Some(b'f' | b'F' | b'l' | b'L')) {
            index += 1;
        }
        index
    } else {
        consume_integer_suffix(bytes, index)
    }
}

fn consume_digits(bytes: &[u8], mut index: usize, radix: u32) -> usize {
    while let Some(byte) = bytes.get(index) {
        if *byte == b'\'' || (*byte as char).is_digit(radix) {
            index += 1;
        } else {
            break;
        }
    }
    index
}

fn consume_integer_suffix(bytes: &[u8], mut index: usize) -> usize {
    while matches!(
        bytes.get(index),
        Some(b'u' | b'U' | b'l' | b'L' | b'z' | b'Z')
    ) {
        index += 1;
    }
    index
}

fn format_numeric_literal(token: &str) -> Option<String> {
    let cleaned = token.replace('\'', "");
    let has_radix_prefix = cleaned.starts_with("0x")
        || cleaned.starts_with("0X")
        || cleaned.starts_with("0b")
        || cleaned.starts_with("0B");
    if !has_radix_prefix
        && (cleaned.contains('.') || cleaned.contains('e') || cleaned.contains('E'))
    {
        return format_float_literal(token, &cleaned);
    }

    let digits = cleaned.trim_end_matches(['u', 'U', 'l', 'L', 'z', 'Z']);
    let (radix, digits) = if let Some(digits) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        (16, digits)
    } else if let Some(digits) = digits
        .strip_prefix("0b")
        .or_else(|| digits.strip_prefix("0B"))
    {
        (2, digits)
    } else {
        (10, digits)
    };
    let value = u64::from_str_radix(digits, radix).ok()?;
    Some(format!(
        "**HEX**  `0x{value:X}`\n\n\
         **DEC**  `{value}`\n\n\
         **BIN**  `0b{value:b}`"
    ))
}

fn format_float_literal(token: &str, cleaned: &str) -> Option<String> {
    let is_f32 = cleaned.ends_with('f') || cleaned.ends_with('F');
    let number = cleaned.trim_end_matches(['f', 'F', 'l', 'L']);

    if is_f32 {
        let value = number.parse::<f32>().ok()?;
        let bits = value.to_bits();
        Some(format!(
            "**FLOAT (f32)**  `{token}`\n\n\
             **DEC**  `{value}`\n\n\
             **HEX**  `0x{bits:08X}`\n\n\
             **BIN**  `0b{bits:032b}`"
        ))
    } else {
        let value = number.parse::<f64>().ok()?;
        let bits = value.to_bits();
        Some(format!(
            "**FLOAT (f64)**  `{token}`\n\n\
             **DEC**  `{value}`\n\n\
             **HEX**  `0x{bits:016X}`\n\n\
             **BIN**  `0b{bits:064b}`"
        ))
    }
}

fn is_path_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '/' | '\\' | '.' | '_' | '-' | ':')
}

fn extract_path(line: &str, byte_idx: usize) -> Option<&str> {
    if line.is_empty() {
        return None;
    }
    if let Some(path) = extract_quoted_path(line, byte_idx) {
        return Some(path);
    }

    let mut pos = byte_idx.min(line.len().saturating_sub(1));
    if !is_path_char(line[pos..].chars().next()?) {
        if pos == 0 {
            return None;
        }
        pos -= 1;
    }
    if !is_path_char(line[pos..].chars().next()?) {
        return None;
    }

    let mut start = pos;
    while start > 0 {
        let ch = line[..start].chars().next_back()?;
        if !is_path_char(ch) {
            break;
        }
        start -= ch.len_utf8();
    }

    let mut end = pos;
    if let Some(ch) = line[end..].chars().next() {
        end += ch.len_utf8();
    }
    while end < line.len() {
        let ch = line[end..].chars().next()?;
        if !is_path_char(ch) {
            break;
        }
        end += ch.len_utf8();
    }

    let token = &line[start..end];
    looks_like_path(token).then_some(token)
}

fn extract_quoted_path(line: &str, byte_idx: usize) -> Option<&str> {
    let mut opening: Option<(usize, char)> = None;
    for (index, ch) in line.char_indices() {
        if !matches!(ch, '\'' | '"') || is_escaped(line, index) {
            continue;
        }
        match opening {
            Some((start, quote)) if quote == ch => {
                let content_start = start + quote.len_utf8();
                if byte_idx >= content_start && byte_idx <= index {
                    let candidate = &line[content_start..index];
                    return looks_like_path(candidate).then_some(candidate);
                }
                opening = None;
            }
            None => opening = Some((index, ch)),
            _ => {}
        }
    }
    None
}

fn is_escaped(text: &str, byte_idx: usize) -> bool {
    !text[..byte_idx]
        .chars()
        .rev()
        .take_while(|ch| *ch == '\\')
        .count()
        .is_multiple_of(2)
}

fn looks_like_path(token: &str) -> bool {
    token.contains('.') || token.contains('/') || token.contains('\\')
}

fn resolve_path(source_file: &Path, raw_path: &str) -> Option<PathBuf> {
    let path = Path::new(raw_path);
    if path.is_absolute() {
        return Some(path.to_path_buf());
    }
    Some(source_file.parent()?.join(path))
}

fn read_header(path: &Path, size: usize) -> std::io::Result<Vec<u8>> {
    let file = File::open(path)?;
    let mut buffer = vec![0u8; size];
    let read_size = file.take(size as u64).read(&mut buffer)?;
    buffer.truncate(read_size);
    Ok(buffer)
}

fn format_hex_dump(data: &[u8]) -> String {
    let mut output = String::new();
    for (offset, chunk) in data.chunks(16).enumerate() {
        output.push_str(&format!("{:08X}  ", offset * 16));
        for i in 0..16 {
            if let Some(byte) = chunk.get(i) {
                output.push_str(&format!("{byte:02X} "));
            } else {
                output.push_str("    ");
            }
        }
        output.push(' ');
        for byte in chunk {
            output.push(if byte.is_ascii_graphic() {
                *byte as char
            } else {
                '.'
            });
        }
        output.push('\n');
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_integer_literals_in_all_bases() {
        assert_eq!(
            format_numeric_literal("42").as_deref(),
            Some("**HEX**  `0x2A`\n\n**DEC**  `42`\n\n**BIN**  `0b101010`")
        );
        assert_eq!(
            format_numeric_literal("0b101010"),
            format_numeric_literal("42")
        );
        assert_eq!(format_numeric_literal("0x2A"), format_numeric_literal("42"));
        assert_eq!(
            format_numeric_literal("0xDEAD"),
            format_numeric_literal("57005")
        );
    }

    #[test]
    fn extracts_decimal_binary_and_float_literals() {
        let line = "auto values = 42 + 0b1010 + 1.5f + 6.02e23;";
        assert_eq!(extract_numeric_literal(line, 14), Some("42"));
        assert_eq!(extract_numeric_literal(line, 21), Some("0b1010"));
        assert_eq!(extract_numeric_literal(line, 30), Some("1.5f"));
        assert_eq!(extract_numeric_literal(line, 39), Some("6.02e23"));
    }

    #[test]
    fn formats_float_as_ieee_754_bits() {
        let f32_preview = format_numeric_literal("1.5f").unwrap();
        assert!(f32_preview.contains("**FLOAT (f32)**"));
        assert!(f32_preview.contains("0x3FC00000"));

        let f64_preview = format_numeric_literal("1.5").unwrap();
        assert!(f64_preview.contains("**FLOAT (f64)**"));
        assert!(f64_preview.contains("0x3FF8000000000000"));
    }

    #[test]
    fn ignores_digits_inside_identifiers() {
        let line = "uint32_t value = 7;";
        assert_eq!(extract_numeric_literal(line, 5), None);
        assert_eq!(extract_numeric_literal(line, 17), Some("7"));
    }

    #[test]
    fn extracts_extensionless_unix_executable_path() {
        let line = "const char* tool = \"/usr/local/bin/tool\";";
        assert_eq!(extract_path(line, 30), Some("/usr/local/bin/tool"));
    }

    #[test]
    fn extracts_quoted_path_with_spaces() {
        let line = "std::string path = \"C:/Program Files/tool.exe\";";
        assert_eq!(extract_path(line, 32), Some("C:/Program Files/tool.exe"));
    }

    #[test]
    fn extracts_unquoted_relative_executable_path() {
        let line = "run build/output/tool now";
        assert_eq!(extract_path(line, 12), Some("build/output/tool"));
    }
}
