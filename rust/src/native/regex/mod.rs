mod charclass;
mod defined_cats;
mod interp;
mod node;
mod opts;
mod parse;
mod prop_table;
mod unicode;
mod unicode_table;
mod writer;
pub use charclass::CharClass;
pub use interp::{Engine, MatchData};
pub use node::Node;
pub use parse::{parse, ParseResult, RegexError};
pub use writer::{write, Code};
#[derive(Debug, Clone)]
pub struct Match {
    pub start: usize,
    pub end: usize,

    pub captures: Vec<Vec<(usize, usize)>>,

    pub capcount: Vec<i32>,

    pub matched: Vec<bool>,
}
impl Match {
    pub fn is_matched(&self, slot: usize) -> bool {
        self.matched.get(slot).copied().unwrap_or(false)
    }
    pub fn group_start(&self, slot: usize) -> usize {
        self.captures
            .get(slot)
            .and_then(|s| s.last())
            .map(|p| p.0)
            .unwrap_or(0)
    }
    pub fn group_end(&self, slot: usize) -> usize {
        self.captures
            .get(slot)
            .and_then(|s| s.last())
            .map(|p| p.1)
            .unwrap_or(0)
    }
}
#[derive(Debug, Clone)]
pub struct Regex {
    code: Code,
    capsize: i32,
    capnames: std::collections::HashMap<String, i32>,
    capnumlist: Option<Vec<i32>>,
}
impl Regex {
    pub fn new(pattern: &str) -> Result<Self, String> {
        let parsed = parse::parse(pattern).map_err(|e| e.message)?;
        let sparse = parsed.sparse.clone();
        let code = writer::write(&parsed.root, &sparse);
        Ok(Regex {
            code,
            capsize: parsed.capsize,
            capnames: parsed.capnames,
            capnumlist: parsed.capnumlist,
        })
    }

    pub fn find_at(&self, text: &[char], from: usize) -> Option<Match> {
        let len = text.len();
        let mut start = from.min(len);
        loop {
            let caps = self.capsize.max(1) as usize;
            let mut engine = interp::Engine::new(&self.code, text, caps);
            if engine.run(start as i64) {
                let data = engine.finish();
                let mut captures: Vec<Vec<(usize, usize)>> = Vec::with_capacity(data.captures.len());
                let mut matched: Vec<bool> = Vec::with_capacity(data.captures.len());
                for raw in data.captures {
                    let slot_matched = !raw.is_empty() && raw.last().map(|p| p.1).unwrap_or(-2) != -2;
                    matched.push(slot_matched);
                    let slot: Vec<(usize, usize)> = raw
                        .into_iter()
                        .filter(|&(_, l)| l != -2)
                        .map(|(s, l)| (s.max(0) as usize, (s + l) as usize))
                        .collect();
                    captures.push(slot);
                }
                let interval = captures[0].last().copied().unwrap_or((start, start));
                return Some(Match {
                    start: interval.0,
                    end: interval.1,
                    captures,
                    capcount: data.capcount,
                    matched,
                });
            }
            if start == len {
                return None;
            }
            start += 1;
        }
    }

    pub fn find_all(&self, text: &[char]) -> Vec<Match> {
        let len = text.len();
        let mut out = Vec::new();
        let mut pos = 0usize;
        while pos <= len {
            match self.find_at(text, pos) {
                Some(found) => {
                    let empty = found.start == found.end;
                    let end = found.end;
                    out.push(found);
                    if empty {
                        if end == len {
                            break;
                        }
                        pos = end + 1;
                    } else {
                        pos = end;
                    }
                }
                None => break,
            }
        }
        out
    }

    pub fn capsize(&self) -> usize {
        self.capsize as usize
    }

    pub fn map_capture(&self, raw: i32) -> i32 {
        match &self.capnumlist {
            Some(list) => list.binary_search(&raw).expect("slot present") as i32,
            None => raw,
        }
    }

    pub fn is_valid_slot(&self, raw: i32) -> bool {
        match &self.capnumlist {
            Some(list) => list.binary_search(&raw).is_ok(),
            None => raw >= 0 && raw < self.capsize,
        }
    }

    pub fn group_number_from_name(&self, name: &str) -> Option<usize> {
        let raw = *self.capnames.get(name)?;
        Some(self.map_capture(raw) as usize)
    }

    pub fn has_group(&self, name: &str) -> bool {
        self.capnames.contains_key(name)
    }
}
pub fn escape(text: &str) -> String {
    const METCHARS: &[char] = &[
        '\t', '\n', '\u{C}', '\r', ' ', '#', '$', '(', ')', '*', '+', '.', '?', '[', '\\', '^', '{', '|',
    ];
    if !text.chars().any(|c| METCHARS.contains(&c)) {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len() + 16);
    for c in text.chars() {
        match c {
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\u{C}' => out.push_str("\\f"),
            c if METCHARS.contains(&c) => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out
}
#[cfg(test)]
mod tests {
    #[test]
    fn char_class_category_groups() {
        use crate::native::regex::charclass::CharClass;
        let mut l = CharClass::new();
        l.add_categories(&[0, 2, 4, 5, 3, 1, 0]); 
        assert!(!l.char_in_class('1'));
        assert!(l.char_in_class('a'));
        assert!(!l.char_in_class('-'));
        assert!(l.char_in_class('Z'));
        let mut p = CharClass::new();
        p.add_categories(&[0, 19, 20, 22, 25, 21, 24, 23, 0]); 
        assert!(p.char_in_class('-'));
        assert!(!p.char_in_class('a'));
        assert!(p.char_in_class('.'));
        let mut s = CharClass::new();
        s.add_categories(&[0, 27, 28, 26, 29, 0]); 
        assert!(s.char_in_class('+'));
        assert!(!s.char_in_class('a'));
        assert!(!s.char_in_class('1'));
        let mut backspace = CharClass::new();
        backspace.add_char('\u{8}');
        assert!(backspace.char_in_class('\u{8}'));
        assert!(!backspace.char_in_class('a'));
    }
    #[test]
    fn class_shorthand_escapes() {
        use crate::native::regex::parse::parse;
        use crate::native::regex::writer;
        for (pattern, cat_entries) in [
            ("[\\p{L}]", vec![0, 2, 4, 5, 3, 1, 0i16]),
            ("[\\P{L}]", vec![0, -2, -4, -5, -3, -1, 0i16]),
            ("[^\\p{L}]", vec![0, 2, 4, 5, 3, 1, 0i16]),
        ] {
            let parsed = parse(pattern).unwrap();
            let sparse = parsed.sparse.clone();
            let code = writer::write(&parsed.root, &sparse);
            assert_eq!(code.sets[0].categories, cat_entries, "pattern {pattern}");
        }

        let parsed = parse("[\\D]").unwrap();
        let code = writer::write(&parsed.root, &parsed.sparse.clone());
        assert_eq!(code.sets[0].categories, vec![-9i16]);
    }
    #[test]
    fn match_and_find_all() {
        let cases: &[(&str, &[&str], &[bool])] = &[
            ("(?(x)b)", &["b", "ab", "aab"], &[true, true, true]),
            ("(a)?b", &["b", "ab", "aab"], &[true, true, true]),
            ("(a)?b\\1", &["b", "ab", "aab"], &[false, false, false]),
            ("\\p{L}", &["1"], &[false]),
            ("[\\b]", &["a\u{8}b"], &[true]),
        ];
        for (pattern, inputs, expected) in cases {
            let m = super::Regex::new(pattern).unwrap();
            for (i, input) in inputs.iter().enumerate() {
                let chars: Vec<char> = input.chars().collect();
                assert_eq!(
                    m.find_at(&chars, 0).is_some(),
                    expected[i],
                    "pattern {pattern} input {input:?}"
                );
            }
        }

        let m = super::Regex::new("\\p{L}").unwrap();
        assert!(m.find_at(&[], 0).is_none());
    }
}
