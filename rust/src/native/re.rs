//! `Un.Native.Re` — the native `re` module.

use crate::runtime::collections;
use crate::runtime::context::Context;
use crate::runtime::obj::{self, Obj, Payload};
use crate::runtime::scope::{self, Scope};

use super::regex::{self, Regex};
use super::{arg, native};

pub fn members(closure: &Context) -> Vec<(String, Obj)> {
    let two = |a: &str, b: &str| vec![arg(a, true, false, false), arg(b, true, false, false)];
    let three = |a: &str, b: &str, c: &str| {
        vec![
            arg(a, true, false, false),
            arg(b, true, false, false),
            arg(c, true, false, false),
        ]
    };

    vec![
        (
            "test".to_owned(),
            native(closure, "test", two("pattern", "text"), test_impl),
        ),
        (
            "match".to_owned(),
            native(closure, "match", two("pattern", "text"), match_impl),
        ),
        (
            "search".to_owned(),
            native(closure, "search", two("pattern", "text"), search_impl),
        ),
        (
            "find_all".to_owned(),
            native(closure, "find_all", two("pattern", "text"), find_all_impl),
        ),
        (
            "groups".to_owned(),
            native(closure, "groups", two("pattern", "text"), groups_impl),
        ),
        (
            "replace".to_owned(),
            native(
                closure,
                "replace",
                three("pattern", "text", "replacement"),
                replace_impl,
            ),
        ),
        (
            "replace_all".to_owned(),
            native(
                closure,
                "replace_all",
                three("pattern", "text", "replacement"),
                replace_all_impl,
            ),
        ),
        (
            "split".to_owned(),
            native(closure, "split", two("pattern", "text"), split_impl),
        ),
        (
            "escape".to_owned(),
            native(closure, "escape", vec![arg("text", true, false, false)], escape_impl),
        ),
    ]
}

/// `GetString` — the argument has to be a `Str`.
fn get_string(value: &Obj) -> Result<String, Obj> {
    match obj::payload(value) {
        Payload::Str(s) => Ok(s.to_string()),
        _ => Err(obj::err("expected value is string")),
    }
}

/// the pattern and the text of a call.
fn pattern_and_text(scope: &Scope) -> Result<(String, String), Obj> {
    let pattern = get_string(&scope::get(scope, "pattern"))?;
    let text = get_string(&scope::get(scope, "text"))?;
    Ok((pattern, text))
}

/// the compiled pattern, the reference reports the .NET compile error.
fn compile(pattern: &str) -> Result<Regex, Obj> {
    match Regex::new(pattern) {
        Ok(regex) => Ok(regex),
        Err(message) => Err(obj::err(format!("invalid regex pattern: {message}"))),
    }
}

fn test_impl(scope: &Scope) -> Obj {
    let (pattern, text) = match pattern_and_text(scope) {
        Ok(v) => v,
        Err(e) => return e,
    };

    let regex = match compile(&pattern) {
        Ok(regex) => regex,
        Err(e) => return e,
    };

    let chars: Vec<char> = text.chars().collect();
    obj::boolean(regex.find_at(&chars, 0).is_some())
}

fn match_impl(scope: &Scope) -> Obj {
    let (pattern, text) = match pattern_and_text(scope) {
        Ok(v) => v,
        Err(e) => return e,
    };

    let regex = match compile(&pattern) {
        Ok(regex) => regex,
        Err(e) => return e,
    };

    let chars: Vec<char> = text.chars().collect();
    match regex.find_at(&chars, 0) {
        Some(found) if found.start == 0 => {
            obj::str(&chars[found.start..found.end].iter().collect::<String>())
        }
        _ => obj::none(),
    }
}

fn search_impl(scope: &Scope) -> Obj {
    let (pattern, text) = match pattern_and_text(scope) {
        Ok(v) => v,
        Err(e) => return e,
    };

    let regex = match compile(&pattern) {
        Ok(regex) => regex,
        Err(e) => return e,
    };

    let chars: Vec<char> = text.chars().collect();
    match regex.find_at(&chars, 0) {
        Some(found) => obj::str(&chars[found.start..found.end].iter().collect::<String>()),
        None => obj::none(),
    }
}

fn find_all_impl(scope: &Scope) -> Obj {
    let (pattern, text) = match pattern_and_text(scope) {
        Ok(v) => v,
        Err(e) => return e,
    };

    let regex = match compile(&pattern) {
        Ok(regex) => regex,
        Err(e) => return e,
    };

    let chars: Vec<char> = text.chars().collect();
    let matches: Vec<Obj> = regex
        .find_all(&chars)
        .into_iter()
        .map(|found| obj::str(&chars[found.start..found.end].iter().collect::<String>()))
        .collect();

    collections::list(matches)
}

fn groups_impl(scope: &Scope) -> Obj {
    let (pattern, text) = match pattern_and_text(scope) {
        Ok(v) => v,
        Err(e) => return e,
    };

    let regex = match compile(&pattern) {
        Ok(regex) => regex,
        Err(e) => return e,
    };

    let chars: Vec<char> = text.chars().collect();
    let Some(found) = regex.find_at(&chars, 0) else {
        return obj::none();
    };

    let groups: Vec<Obj> = found
        .captures
        .iter()
        .map(|capture| match capture {
            Some((start, end)) => obj::str(&chars[*start..*end].iter().collect::<String>()),
            None => obj::none(),
        })
        .collect();

    collections::list(groups)
}

/// the replacement of `replace` and `replace_all`, the `$1` groups of the reference.
fn expand(replacement: &str, chars: &[char], found: &regex::Match) -> String {
    let mut out = String::new();
    let mut chars_iter = replacement.chars().peekable();

    while let Some(c) = chars_iter.next() {
        if c != '$' {
            out.push(c);
            continue;
        }

        match chars_iter.peek().copied() {
            Some('$') => {
                chars_iter.next();
                out.push('$');
            }
            Some(digit) if digit.is_ascii_digit() => {
                chars_iter.next();
                let mut number = digit.to_digit(10).unwrap_or(0) as usize;

                // a two digit group number
                if let Some(next) = chars_iter.peek().copied() {
                    if next.is_ascii_digit() {
                        let two = number * 10 + next.to_digit(10).unwrap_or(0) as usize;
                        if two < found.captures.len() {
                            chars_iter.next();
                            number = two;
                        }
                    }
                }

                match found.captures.get(number).copied().flatten() {
                    Some((start, end)) => out.push_str(&chars[start..end].iter().collect::<String>()),
                    None => {}
                }
            }
            _ => out.push('$'),
        }
    }

    out
}

fn replace_impl(scope: &Scope) -> Obj {
    let (pattern, text) = match pattern_and_text(scope) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let replacement = match get_string(&scope::get(scope, "replacement")) {
        Ok(replacement) => replacement,
        Err(e) => return e,
    };

    let regex = match compile(&pattern) {
        Ok(regex) => regex,
        Err(e) => return e,
    };

    let chars: Vec<char> = text.chars().collect();
    let Some(found) = regex.find_at(&chars, 0) else {
        return obj::str(&text);
    };

    let mut out: String = chars[..found.start].iter().collect();
    out.push_str(&expand(&replacement, &chars, &found));
    out.push_str(&chars[found.end..].iter().collect::<String>());

    obj::str(&out)
}

fn replace_all_impl(scope: &Scope) -> Obj {
    let (pattern, text) = match pattern_and_text(scope) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let replacement = match get_string(&scope::get(scope, "replacement")) {
        Ok(replacement) => replacement,
        Err(e) => return e,
    };

    let regex = match compile(&pattern) {
        Ok(regex) => regex,
        Err(e) => return e,
    };

    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut pos = 0usize;

    for found in regex.find_all(&chars) {
        out.push_str(&chars[pos..found.start].iter().collect::<String>());
        out.push_str(&expand(&replacement, &chars, &found));
        pos = found.end;
    }

    out.push_str(&chars[pos..].iter().collect::<String>());

    obj::str(&out)
}

fn split_impl(scope: &Scope) -> Obj {
    let (pattern, text) = match pattern_and_text(scope) {
        Ok(v) => v,
        Err(e) => return e,
    };

    let regex = match compile(&pattern) {
        Ok(regex) => regex,
        Err(e) => return e,
    };

    let chars: Vec<char> = text.chars().collect();
    let mut parts: Vec<Obj> = Vec::new();
    let mut pos = 0usize;

    for found in regex.find_all(&chars) {
        parts.push(obj::str(&chars[pos..found.start].iter().collect::<String>()));
        pos = found.end;
    }

    parts.push(obj::str(&chars[pos..].iter().collect::<String>()));

    collections::list(parts)
}

fn escape_impl(scope: &Scope) -> Obj {
    let text = match get_string(&scope::get(scope, "text")) {
        Ok(text) => text,
        Err(e) => return e,
    };

    obj::str(&regex::escape(&text))
}
