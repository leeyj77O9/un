use std::sync::OnceLock;
use crate::native::regex::{defined_cats, prop_table, unicode_table};
pub const SPACE_CONST: i16 = 100;
pub const NOT_SPACE_CONST: i16 = -100;
pub fn category_of(ch: char) -> u8 {
    static TABLE: OnceLock<Vec<Vec<(u32, u32)>>> = OnceLock::new();
    let table = TABLE.get_or_init(build_table);
    let cp = ch as u32;
    for (i, ranges) in table.iter().enumerate() {
        let mut lo = 0usize;
        let mut hi = ranges.len();
        while lo < hi {
            let mid = (lo + hi) / 2;
            let (a, b) = ranges[mid];
            if cp < a {
                hi = mid;
            } else if cp > b {
                lo = mid + 1;
            } else {
                return i as u8;
            }
        }
    }
    29 
}
fn build_table() -> Vec<Vec<(u32, u32)>> {
    let mut out: Vec<Vec<(u32, u32)>> = Vec::new();
    for line in unicode_table::BMP_RANGES.iter() {
        let v: Vec<(u32, u32)> = line.iter().map(|(a, b)| (*a as u32, *b as u32)).collect();
        out.push(v);
    }
    for line in unicode_table::ASTRAL_RANGES.iter() {
        if line.cat >= out.len() {
            continue;
        }
        for (a, b) in line.ranges.iter() {
            out[line.cat].push((*a, *b));
        }
    }
    for v in out.iter_mut() {
        v.sort();
        let mut res: Vec<(u32, u32)> = Vec::with_capacity(v.len());
        for &(a, b) in v.iter() {
            if let Some(last) = res.last_mut() {
                if a <= last.1 + 1 {
                    if b > last.1 {
                        last.1 = b;
                    }
                    continue;
                }
            }
            res.push((a, b));
        }
        *v = res;
    }
    out
}
pub fn is_whitespace(ch: char) -> bool {
    ch.is_whitespace()
}
const WORD_ASCII: [u8; 16] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0x03,
    0xFE, 0xFF, 0xFF, 0x87, 0xFE, 0xFF, 0xFF, 0x07,
];
const WORD_MASK: u32 = (1 << 0) | (1 << 1) | (1 << 2) | (1 << 3) | (1 << 4) | (1 << 5) | (1 << 8) | (1 << 18);
pub fn is_word_char(ch: char) -> bool {
    let cp = ch as u32;
    if cp < 128 {
        WORD_ASCII[(cp >> 3) as usize] & (1 << (cp & 7)) != 0
    } else {
        (WORD_MASK & (1 << category_of(ch))) != 0
    }
}
pub fn is_boundary_word_char(ch: char) -> bool {
    is_word_char(ch) || ch == '\u{200C}' || ch == '\u{200D}'
}
pub fn is_ecma_word_char(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch.is_ascii_digit() || ch == '_' || ch == '\u{130}'
}
pub fn case_equivalents(ch: char) -> Vec<char> {
    let mut out: Vec<char> = Vec::new();
    out.push(ch);
    let mut push_first = |s: String| {
        if let Some(f) = s.chars().next() {
            if !out.contains(&f) {
                out.push(f);
            }
        }
    };
    push_first(ch.to_lowercase().collect::<String>());
    push_first(ch.to_uppercase().collect::<String>());
    out
}
pub fn participates_in_case_conversion(text: &[char]) -> bool {
    text.iter().any(|&c| case_equivalents(c).len() > 1)
}
pub fn prop_ranges(name: &str) -> Option<Vec<(u16, u16)>> {
    let t = prop_table::PROP_TABLE;
    let mut lo = 0usize;
    let mut hi = t.len();
    while lo < hi {
        let mid = (lo + hi) / 2;
        use std::cmp::Ordering;
        match name.cmp(t[mid].0) {
            Ordering::Less => hi = mid,
            Ordering::Greater => lo = mid + 1,
            Ordering::Equal => {
                let e = t[mid].1;
                let mut out: Vec<(u16, u16)> = Vec::with_capacity(e.len() / 2);
                let mut i = 0;
                while i < e.len() {
                    out.push((e[i], e[i + 1]));
                    i += 2;
                }
                return Some(out);
            }
        }
    }
    None
}
pub fn defined_category(name: &str) -> Option<&'static [i16]> {
    let t = defined_cats::DEFINED_CATEGORIES;
    let mut lo = 0usize;
    let mut hi = t.len();
    while lo < hi {
        let mid = (lo + hi) / 2;
        use std::cmp::Ordering;
        match name.cmp(t[mid].0) {
            Ordering::Less => hi = mid,
            Ordering::Greater => lo = mid + 1,
            Ordering::Equal => return Some(t[mid].1),
        }
    }
    None
}
