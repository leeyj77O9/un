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
use crate::native::regex::opts::*;
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
    fast: Option<FastRegex>,
}
#[derive(Debug, Clone)]
enum FastAtom {
    Class(usize),
    One(u32),
    NotOne(u32),
}
impl FastAtom {
    fn matches(&self, sets: &[CharClass], ch: char) -> bool {
        match self {
            FastAtom::Class(i) => sets[*i].char_in_class(ch),
            FastAtom::One(v) => ch as u32 == *v,
            FastAtom::NotOne(v) => ch as u32 != *v,
        }
    }
}
const FAST_FLAG_MASK: i32 = crate::native::regex::opts::OP_RTL
    | crate::native::regex::opts::OP_CASEINS
    | crate::native::regex::opts::OP_BACKTRACKING
    | crate::native::regex::opts::OP_BACKTRACKING_SECOND;

#[derive(Debug, Clone)]
struct FastRegex {
    pre: Vec<FastAtom>,
    greedy: Option<FastAtom>,
}
impl FastRegex {
    fn analyze(code: &Code) -> Option<FastRegex> {
        let codes = &code.codes;
        if codes.len() < 4 {
            return None;
        }
        let mut i = 0usize;
        for _ in 0..64 {
            let Some(&op) = codes.get(i) else { return None; };
            if (op & FAST_FLAG_MASK) != 0 {
                return None;
            }
            match op {
                x if x == OP_LAZYBRANCH && i == 0 => {
                    if codes.get(i + 1).is_none() {
                        return None;
                    }
                    i += 2;
                }
                OP_NULLMARK | OP_SETMARK => {
                    i += 1;
                }
                OP_SETCOUNT | OP_NULLCOUNT => {
                    if codes.get(i + 1).is_none() {
                        return None;
                    }
                    i += 2;
                }
                OP_CAPTUREMARK => {
                    if codes.get(i + 1) != Some(&0) {
                        return None;
                    }
                    if codes.get(i + 2).is_none() {
                        return None;
                    }
                    i += 3;
                }
                _ => break,
            }
        }
        let mut pre: Vec<FastAtom> = Vec::new();
        let mut greedy: Option<FastAtom> = None;
        for _ in 0..64 {
            let Some(&op) = codes.get(i) else { return None; };
            if (op & FAST_FLAG_MASK) != 0 {
                return None;
            }
            let base = op;
            match base {
                OP_SET | OP_SETREP => {
                    let idx = *codes.get(i + 1)?;
                    code.sets.get(idx as usize)?;
                    let n = if base == OP_SETREP {
                        let n = *codes.get(i + 2)?;
                        if !(1..=8).contains(&n) {
                            return None;
                        }
                        n
                    } else {
                        1
                    };
                    for _ in 0..n {
                        pre.push(FastAtom::Class(idx as usize));
                    }
                    i += if base == OP_SETREP { 3 } else { 2 };
                }
                OP_ONE | OP_ONEREP => {
                    let ch = *codes.get(i + 1)? as u32;
                    let n = if base == OP_ONEREP {
                        let n = *codes.get(i + 2)?;
                        if !(1..=8).contains(&n) {
                            return None;
                        }
                        n
                    } else {
                        1
                    };
                    for _ in 0..n {
                        pre.push(FastAtom::One(ch));
                    }
                    i += if base == OP_ONEREP { 3 } else { 2 };
                }
                OP_NOTONE | OP_NOTONEREP => {
                    let ch = *codes.get(i + 1)? as u32;
                    let n = if base == OP_NOTONEREP {
                        let n = *codes.get(i + 2)?;
                        if !(1..=8).contains(&n) {
                            return None;
                        }
                        n
                    } else {
                        1
                    };
                    for _ in 0..n {
                        pre.push(FastAtom::NotOne(ch));
                    }
                    i += if base == OP_NOTONEREP { 3 } else { 2 };
                }
                OP_SETLOOP | OP_ONELLOOP | OP_NOTONELLOOP => {
                    let v = *codes.get(i + 1)?;
                    let tail = *codes.get(i + 2)?;
                    if tail != i32::MAX {
                        return None;
                    }
                    let atom = match base {
                        OP_SETLOOP => {
                            code.sets.get(v as usize)?;
                            FastAtom::Class(v as usize)
                        }
                        OP_ONELLOOP => FastAtom::One(v as u32),
                        _ => FastAtom::NotOne(v as u32),
                    };
                    greedy = Some(atom);
                    i += 3;
                    for _ in 0..16 {
                        let Some(&op2) = codes.get(i) else { return None; };
                        if (op2 & FAST_FLAG_MASK) != 0 {
                            return None;
                        }
                        match op2 {
                            OP_CAPTUREMARK => {
                                if codes.get(i + 1) != Some(&0) {
                                    return None;
                                }
                                if codes.get(i + 2).is_none() {
                                    return None;
                                }
                                i += 3;
                            }
                            OP_STOP => {
                                return Some(FastRegex { pre, greedy });
                            }
                            _ => return None,
                        }
                    }
                    return None;
                }
                OP_CAPTUREMARK => {
                    if codes.get(i + 1) != Some(&0) {
                        return None;
                    }
                    if codes.get(i + 2).is_none() {
                        return None;
                    }
                    i += 3;
                }
                OP_STOP => {
                    return Some(FastRegex { pre, greedy });
                }
                _ => return None,
            }
        }
        None
    }

    fn end_at(&self, sets: &[CharClass], text: &[char], start: usize) -> Option<usize> {
        if start > text.len() {
            return None;
        }
        let mut i = start;
        for atom in &self.pre {
            if i >= text.len() {
                return None;
            }
            if !atom.matches(sets, text[i]) {
                return None;
            }
            i += 1;
        }
        if let Some(g) = &self.greedy {
            while i < text.len() && g.matches(sets, text[i]) {
                i += 1;
            }
        }
        Some(i)
    }

    fn find_all_direct(&self, sets: &[CharClass], text: &[char]) -> Vec<Match> {
        let mut out = Vec::new();
        let mut pos = 0usize;
        while pos <= text.len() {
            match self.end_at(sets, text, pos) {
                Some(end) => {
                    let empty = pos == end;
                    out.push(Match {
                        start: pos,
                        end,
                        captures: vec![vec![(pos, end)]],
                        capcount: vec![1],
                        matched: vec![true],
                    });
                    if empty {
                        if end == text.len() {
                            break;
                        }
                        pos = end + 1;
                    } else {
                        pos = end;
                    }
                }
                None => {
                    pos += 1;
                    continue;
                }
            }
        }
        out
    }
}

impl Regex {
    pub fn new(pattern: &str) -> Result<Self, String> {
        let parsed = parse::parse(pattern).map_err(|e| e.message)?;
        let sparse = parsed.sparse.clone();
        let code = writer::write(&parsed.root, &sparse);
        let fast = FastRegex::analyze(&code);
        Ok(Regex {
            code,
            capsize: parsed.capsize,
            capnames: parsed.capnames,
            capnumlist: parsed.capnumlist,
            fast,
        })
    }

    pub fn find_at(&self, text: &[char], from: usize) -> Result<Option<Match>, String> {
        let caps = self.capsize.max(1) as usize;
        let mut engine = interp::Engine::new(&self.code, text, caps);
        self.find_next(&mut engine, text, from)
    }
    fn find_next(&self, engine: &mut interp::Engine<'_>, text: &[char], from: usize) -> Result<Option<Match>, String> {
        let len = text.len();
        let mut start = from.min(len);
        loop {
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
                return Ok(Some(Match {
                    start: interval.0,
                    end: interval.1,
                    captures,
                    capcount: data.capcount,
                    matched,
                }));
            }
            if engine.limited() {
                return Err("regex backtracking limit exceeded".to_string());
            }
            if start == len {
                return Ok(None);
            }
            start += 1;
            engine.reset();
        }
    }

    pub fn find_all(&self, text: &[char]) -> Result<Vec<Match>, String> {
        if let Some(fast) = &self.fast {
            return Ok(fast.find_all_direct(&self.code.sets, text));
        }
        let len = text.len();
        let mut out = Vec::new();
        let caps = self.capsize.max(1) as usize;
        let mut engine = interp::Engine::new(&self.code, text, caps);
        let mut pos = 0usize;
        while pos <= len {
            match self.find_next(&mut engine, text, pos)? {
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
                    engine.reset();
                }
                None => break,
            }
        }
        Ok(out)
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
                    m.find_at(&chars, 0).expect("engine limit").is_some(),
                    expected[i],
                    "pattern {pattern} input {input:?}"
                );
            }
        }

        let m = super::Regex::new("\\p{L}").unwrap();
        assert!(m.find_at(&[], 0).expect("engine limit").is_none());
    }
}
