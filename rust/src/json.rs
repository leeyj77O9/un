//! Minimal JSON string escaping shared by the LSP stdio server and the
//! playground FFI. Field names, ordering and framing stay with their callers.

/// Appends `text` as a quoted JSON string to `out`.
pub fn write_json_string(text: &str, out: &mut String) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            ch if (ch as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
}

/// Returns `text` as a quoted JSON string.
pub fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    write_json_string(text, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_control_characters_and_quotes() {
        assert_eq!(json_string("a\"b\\c\nd\t\u{8}\u{1}"), "\"a\\\"b\\\\c\\nd\\t\\b\\u0001\"");
    }
}
