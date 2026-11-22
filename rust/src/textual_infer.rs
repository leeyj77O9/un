//! Lightweight, text-only type inference shared by the LSP and the playground.
//!
//! This is deliberately not the real type checker: it scans declaration shapes
//! line by line so it still returns useful information while a document is too
//! broken to parse. Both consumers used to carry a private copy of these
//! helpers; keeping one canonical version avoids the two drifting apart.

use std::collections::HashMap;

/// Variable and function types recovered from textual declarations.
#[derive(Debug, Default)]
pub struct InferredTypes {
    pub vars: HashMap<String, String>,
    pub fn_params: HashMap<String, Vec<(String, String)>>,
    pub fn_returns: HashMap<String, String>,
}

/// Recognises `b"..."` / `b'...'` byte-string literals (uppercase `B` is
/// accepted by the lexer as well).
pub fn is_bytes_literal(text: &str) -> bool {
    matches!(text.as_bytes(), [b'b' | b'B', b'"' | b'\'', ..])
}

/// Base types the textual passes understand in annotations.
pub fn is_builtin_type(name: &str) -> bool {
    matches!(name, "list" | "dict" | "set" | "iter" | "str" | "bytes" | "date" | "stopwatch")
}

/// Infers the type of an expression from its textual shape only.
pub fn infer_expr_type(expr: &str) -> Option<String> {
    let text = expr.trim();
    if text.is_empty() {
        return None;
    }
    if is_bytes_literal(text) {
        return Some("bytes".to_string());
    }
    if text.starts_with('"') || text.starts_with('\'') || text.starts_with('`') {
        return Some("str".to_string());
    }
    if text == "true" || text == "false" {
        return Some("bool".to_string());
    }
    if text.starts_with('[') {
        return Some("list".to_string());
    }
    if text.starts_with('{') {
        return Some("dict".to_string());
    }
    if text.starts_with("set(") {
        return Some("set".to_string());
    }
    if text.starts_with("iter.") || text.starts_with("iter(") {
        return Some("iter".to_string());
    }
    if text.chars().all(|ch| ch.is_ascii_digit() || ch == '_' || ch == '-')
        && text.chars().any(|ch| ch.is_ascii_digit())
    {
        return Some("int".to_string());
    }
    if text.chars().all(|ch| ch.is_ascii_digit() || ch == '_' || ch == '-' || ch == '.')
        && text.contains('.')
    {
        return Some("float".to_string());
    }
    None
}

/// Byte index of the `)` matching the `(` at `open` in `text`.
pub fn matching_paren(text: &str, open: usize) -> Option<usize> {
    matching_close(text.char_indices().skip_while(|(index, _)| *index < open))
}

/// Char-index variant of [`matching_paren`], for callers working on
/// `Source::code` (a `Vec<char>`).
pub fn matching_paren_chars(code: &[char], open: usize) -> Option<usize> {
    matching_close((open.min(code.len())..code.len()).map(|index| (index, code[index])))
}

fn matching_close<I: IntoIterator<Item = (usize, char)>>(items: I) -> Option<usize> {
    let mut depth = 0i64;
    for (index, ch) in items {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// Char index of the innermost unmatched `(` before `before`.
pub fn innermost_open_paren(code: &[char], before: usize) -> Option<usize> {
    let mut depth = 0i64;
    let mut index = before.min(code.len());
    while index > 0 {
        index -= 1;
        match code[index] {
            ')' => depth += 1,
            '(' if depth == 0 => return Some(index),
            '(' => depth -= 1,
            _ => {}
        }
    }
    None
}

/// Top-level segments of `text` split on `sep` (bracket depth zero).
pub fn split_top_level(text: &str, sep: char) -> Vec<&str> {
    split_segments(text.char_indices().map(|(index, ch)| (index, ch, index + ch.len_utf8())), sep)
        .into_iter()
        .map(|(start, end)| &text[start..end])
        .collect()
}

/// Char-index variant of [`split_top_level`] over `code[start..end]`.
pub fn split_top_level_chars(code: &[char], start: usize, end: usize, sep: char) -> Vec<(usize, usize)> {
    let end = end.min(code.len());
    let start = start.min(end);
    split_segments((start..end).map(|index| (index, code[index], index + 1)), sep)
}

fn split_segments<I: IntoIterator<Item = (usize, char, usize)>>(items: I, sep: char) -> Vec<(usize, usize)> {
    let mut result = Vec::new();
    let mut start = 0usize;
    let mut end = 0usize;
    let mut depth = 0i64;
    for (index, ch, next) in items {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            c if c == sep && depth == 0 => {
                result.push((start, index));
                start = next;
            }
            _ => {}
        }
        end = next;
    }
    result.push((start, end));
    result
}

/// Scans `code` for top-level `name = expr` / `name: Type = expr` assignments
/// and `fn name(params) -> ret` declarations.
pub fn infer(code: &[char]) -> InferredTypes {
    let mut result = InferredTypes::default();
    let mut i = 0usize;
    while i < code.len() {
        let line_start = i;
        while i < code.len() && code[i] != '\n' {
            i += 1;
        }
        if i < code.len() {
            i += 1;
        }
        let line: String = code[line_start..i].iter().collect();
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }

        if let Some(eq) = trimmed.find('=') {
            let left = trimmed[..eq].trim();
            let rhs = trimmed[eq + 1..].trim();
            let (name, declared) = match left.find(':') {
                Some(colon) => (&left[..colon], Some(left[colon + 1..].trim().to_string())),
                None => (left, None),
            };
            if !name.is_empty()
                && name.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
                && !name.chars().next().unwrap().is_ascii_digit()
            {
                let inferred = declared.filter(|ty| is_builtin_type(ty)).or_else(|| infer_expr_type(rhs));
                if let Some(ty) = inferred {
                    result.vars.insert(name.to_string(), ty);
                }
            }
        }

        if let Some(rest) = trimmed.strip_prefix("fn ") {
            let name: String = rest
                .chars()
                .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
                .collect();
            if !name.is_empty() {
                if let Some(open) = rest.find('(') {
                    if let Some(close) = matching_paren(rest, open) {
                        let mut params = Vec::new();
                        for part in split_top_level(&rest[open + 1..close], ',') {
                            let param = part.trim();
                            if param.is_empty() {
                                continue;
                            }
                            let pname: String = param
                                .chars()
                                .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
                                .collect();
                            let ptype = if let Some(colon) = param.find(':') {
                                param[colon + 1..]
                                    .trim()
                                    .split(['=', ' '])
                                    .next()
                                    .unwrap_or("")
                                    .to_string()
                            } else {
                                let default = param.find('=').map(|eq| param[eq + 1..].trim()).unwrap_or("");
                                infer_expr_type(default).unwrap_or_else(|| "any".to_string())
                            };
                            params.push((pname, ptype));
                        }
                        result.fn_params.insert(name.clone(), params);
                        if let Some(ret) = rest[close + 1..].trim().strip_prefix("->") {
                            let ret = ret.trim().split([' ', ':']).next().unwrap_or("").to_string();
                            if !ret.is_empty() {
                                result.fn_returns.insert(name, ret);
                            }
                        }
                    }
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_bytes_literals_and_annotations() {
        let code: Vec<char> = "raw = b\"hi\"\nother = B'x'\ntyped: bytes = raw\n".chars().collect();
        let types = infer(&code);
        assert_eq!(types.vars.get("raw").map(String::as_str), Some("bytes"));
        assert_eq!(types.vars.get("other").map(String::as_str), Some("bytes"));
        assert_eq!(types.vars.get("typed").map(String::as_str), Some("bytes"));
    }

    #[test]
    fn paren_and_split_helpers_match_on_chars_and_str() {
        let text = "f(a, g(b, c), d)";
        let open = text.find('(').unwrap();
        assert_eq!(matching_paren(text, open), Some(text.rfind(')').unwrap()));
        assert_eq!(split_top_level(&text[open + 1..text.len() - 1], ','), vec!["a", " g(b, c)", " d"]);

        let code: Vec<char> = text.chars().collect();
        let open = matching_paren_chars(&code, open).unwrap();
        assert_eq!(open, code.len() - 1);
        assert_eq!(innermost_open_paren(&code, open), Some(text.find('(').unwrap()));
        assert_eq!(split_top_level_chars(&code, text.find('(').unwrap() + 1, open, ',').len(), 3);
    }
}
