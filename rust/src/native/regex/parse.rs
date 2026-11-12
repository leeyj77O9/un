//! The .NET `RegexParser` equivalent — produces a tree of `Node`s from a pattern.

use std::collections::HashMap;

use crate::native::regex::charclass::{invert_ranges, CharClass};
use crate::native::regex::node::Node;
use crate::native::regex::opts::*;
use crate::native::regex::unicode;

/// A parse error. `message` carries the final text — the .NET format
/// `Invalid pattern '{0}' at offset {1}. {2}` is produced here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegexError {
    pub message: String,
    pub offset: usize,
}

fn make_exception(pattern: &str, pos: usize, detail: &str) -> RegexError {
    RegexError {
        message: format!("Invalid pattern '{}' at offset {}. {}", pattern, pos, detail),
        offset: pos,
    }
}

/// the outcome of a successful parse
pub struct ParseResult {
    pub root: Node,
    /// the number of dense capture slots, including slot 0
    pub capsize: i32,
    /// group name -> raw capture slot number (as parsed, before densification)
    pub capnames: HashMap<String, i32>,
    /// group names in order of first distinct appearance
    pub capnames_list: Vec<String>,
    /// sorted raw capture slot numbers when the set is sparse, else None
    pub capnumlist: Option<Vec<i32>>,
    /// raw slot -> dense index mapping when sparse, else None
    pub sparse: Option<Vec<i32>>,
    pub has_ignore_case_backreference: bool,
}

struct Frame {
    group: Node,
    alternation: Node,
    concatenation: Node,
}

pub struct Parser {
    pattern: Vec<char>,
    _pos: usize,
    _options: i32,
    _options_stack: Vec<i32>,
    group: Option<Node>,
    alternation: Option<Node>,
    concatenation: Option<Node>,
    stack: Vec<Frame>,
    unit: Option<Node>,
    caps: std::collections::HashSet<i32>,
    capcount: i32,
    captop: i32,
    capnames: HashMap<String, i32>,
    capnamelist: Vec<String>,
    capnumlist: Option<Vec<i32>>,
    autocap: i32,
    ignore_next_paren: bool,
    has_ignore_case_backreference: bool,
}

pub fn parse(pattern: &str) -> Result<ParseResult, RegexError> {
    let mut parser = Parser::new(pattern.chars().collect());
    parser.count_captures()?;
    parser.reset(OPT_NONE);
    let root = parser.scan_regex()?;

    let sparse = if parser.capcount < parser.captop {
        let list = parser.capnumlist.clone().unwrap();
        let mut map = vec![0i32; parser.captop as usize];
        for (i, &slot) in list.iter().enumerate() {
            map[slot as usize] = i as i32;
        }
        Some(map)
    } else {
        None
    };

    Ok(ParseResult {
        root,
        capsize: parser.dense_capsize(),
        capnames: parser.capnames,
        capnames_list: parser.capnamelist,
        capnumlist: parser.capnumlist,
        sparse,
        has_ignore_case_backreference: parser.has_ignore_case_backreference,
    })
}

impl Parser {
    fn new(pattern: Vec<char>) -> Self {
        Parser {
            pattern,
            _pos: 0,
            _options: OPT_NONE,
            _options_stack: Vec::new(),
            group: None,
            alternation: None,
            concatenation: None,
            stack: Vec::new(),
            unit: None,
            caps: Default::default(),
            capcount: 0,
            captop: 0,
            capnames: Default::default(),
            capnamelist: Vec::new(),
            capnumlist: None,
            autocap: 0,
            ignore_next_paren: false,
            has_ignore_case_backreference: false,
        }
    }

    fn reset(&mut self, options: i32) {
        self._pos = 0;
        self.autocap = 1;
        self.ignore_next_paren = false;
        self._options_stack.clear();
        self._options = options;
        self.stack.clear();
    }

    fn make_exception(&self, message: &str) -> RegexError {
        make_exception(&String::from_iter(self.pattern.iter()), self._pos, message)
    }

    fn is_capture_slot(&self, i: i32) -> bool {
        self.caps.contains(&i)
    }

    fn note_capture_slot(&mut self, i: i32) {
        if self.caps.insert(i) {
            self.capcount += 1;
            if self.captop <= i {
                self.captop = if i == i32::MAX { i } else { i + 1 };
            }
        }
    }

    fn note_capture_name(&mut self, name: &str) {
        if !self.capnames.contains_key(name) {
            self.capnames.insert(name.to_owned(), 0);
            self.capnamelist.push(name.to_owned());
        }
    }

    fn dense_capsize(&self) -> i32 {
        if self.capnumlist.is_some() {
            self.capcount
        } else {
            self.captop
        }
    }

    fn scan_decimal_impl(&mut self) -> Result<i32, RegexError> {
        let mut i: i32 = 0;
        const MAX_DIV: i32 = i32::MAX / 10;
        const MAX_MOD: i32 = i32::MAX % 10;

        while self._pos < self.pattern.len() && self.pattern[self._pos].is_ascii_digit() {
            let d = (self.pattern[self._pos] as i32) - ('0' as i32);
            self._pos += 1;

            if i > MAX_DIV || (i == MAX_DIV && d > MAX_MOD) {
                return Err(self
                    .make_exception("Quantifier and capture group numbers must be less than or equal to Int32.MaxValue."));
            }

            i = i * 10 + d;
        }

        Ok(i)
    }

    fn scan_capname(&mut self) -> String {
        let startpos = self._pos;
        while self._pos < self.pattern.len() {
            let ch = self.pattern[self._pos];
            if !unicode::is_boundary_word_char(ch) {
                break;
            }
            self._pos += 1;
        }
        String::from_iter(self.pattern[startpos..self._pos].iter())
    }
}


impl Parser {
    /// `CountCaptures` — pass 1, pre-counts the capture slots.
    fn count_captures(&mut self) -> Result<(), RegexError> {
        self.note_capture_slot(0);
        self._pos = 0;
        self.autocap = 1;

        while self._pos < self.pattern.len() {
            let ch = self.pattern[self._pos];
            self._pos += 1;

            match ch {
                '\\' => {
                    if self._pos < self.pattern.len() {
                        self.scan_backslash(true)?;
                    }
                }
                '#' => {
                    if (self._options & OPT_IGNORE_WS) != 0 {
                        self._pos -= 1;
                        self.scan_blank()?;
                    }
                }
                '[' => {
                    self.scan_char_class(false, true)?;
                }
                ')' => {
                    if let Some(o) = self._options_stack.pop() {
                        self._options = o;
                    }
                }
                '(' => {
                    let mut skip_unignore = false;
                    if self._pos + 1 < self.pattern.len()
                        && self.pattern[self._pos + 1] == '#'
                        && self.pattern[self._pos] == '?'
                    {
                        self._pos -= 1;
                        self.scan_blank()?;
                    } else {
                        self._options_stack.push(self._options);
                        if self._pos < self.pattern.len() && self.pattern[self._pos] == '?' {
                            self._pos += 1;

                            if self._pos + 1 < self.pattern.len()
                                && (self.pattern[self._pos] == '<' || self.pattern[self._pos] == '\'')
                            {
                                self._pos += 1;
                                let ch = self.pattern[self._pos];

                                if ch != '0' && unicode::is_boundary_word_char(ch) {
                                    if (ch as i32 - '1' as i32) >= 0 && (ch as i32 - '1' as i32) <= 9 - 1 {
                                        let n = self.scan_decimal_impl()?;
                                        self.note_capture_slot(n);
                                    } else {
                                        let name = self.scan_capname();
                                        self.note_capture_name(&name);
                                    }
                                }
                            } else {
                                // options group?
                                self.scan_options();

                                if self._pos < self.pattern.len() && self.pattern[self._pos] == ')' {
                                    self._pos += 1;
                                    self._options_stack.pop();
                                } else if self._pos < self.pattern.len() && self.pattern[self._pos] == '(' {
                                    // alternation construct: (?(foo)yes|no)
                                    self.ignore_next_paren = true;

                                    // .NET breaks out of the switch here, skipping the
                                    // "_ignoreNextParen = false" at the end of the case.
                                    skip_unignore = true;
                                }
                            }
                        } else {
                            // Simple unnamed capture group
                            if (self._options & OPT_EXPLICIT) == 0 && !self.ignore_next_paren {
                                let slot = self.autocap;
                                self.autocap += 1;
                                self.note_capture_slot(slot);
                            }
                        }
                    }

                    if !skip_unignore {
                        self.ignore_next_paren = false;
                    }
                }
                _ => {}
            }
        }

        self.assign_name_slots();
        Ok(())
    }

    fn assign_name_slots(&mut self) {
        if !self.capnamelist.is_empty() {
            let names: Vec<String> = self.capnamelist.clone();
            for name in &names {
                while self.is_capture_slot(self.autocap) {
                    self.autocap += 1;
                }

                let slot = self.autocap;
                self.capnames.insert(name.clone(), slot);
                self.note_capture_slot(slot);
                self.autocap += 1;
            }
        }

        // If the caps set has gaps, construct the sorted list of used slots
        if self.capcount < self.captop {
            let mut list: Vec<i32> = self.caps.iter().copied().collect();
            list.sort();
            self.capnumlist = Some(list);
        }
    }

    fn scan_blank(&mut self) -> Result<(), RegexError> {
        loop {
            if (self._options & OPT_IGNORE_WS) != 0 {
                while self._pos < self.pattern.len() && unicode::is_whitespace(self.pattern[self._pos]) {
                    self._pos += 1;
                }
            }

            if (self._options & OPT_IGNORE_WS) != 0
                && self._pos < self.pattern.len()
                && self.pattern[self._pos] == '#'
            {
                let rest = &self.pattern[self._pos..];
                match rest.iter().position(|&c| c == '\n') {
                    Some(p) => {
                        self._pos += p + 1;
                    }
                    None => {
                        self._pos = self.pattern.len();
                    }
                }
                continue;
            }

            if self._pos + 2 < self.pattern.len()
                && self.pattern[self._pos + 2] == '#'
                && self.pattern[self._pos + 1] == '?'
                && self.pattern[self._pos] == '('
            {
                let rest = &self.pattern[self._pos..];
                match rest.iter().position(|&c| c == ')') {
                    Some(p) => {
                        self._pos += p + 1;
                    }
                    None => {
                        self._pos = self.pattern.len();
                        return Err(self.make_exception("Unterminated (?#...) comment."));
                    }
                }
                continue;
            }

            break;
        }

        Ok(())
    }

    fn scan_options(&mut self) {
        let mut off = false;
        while self._pos < self.pattern.len() {
            let ch = self.pattern[self._pos];

            if ch == '-' {
                off = true;
            } else if ch == '+' {
                off = false;
            } else {
                let options = match (ch as u8 | 0x20) as char {
                    'i' => OPT_ICASE,
                    'm' => OPT_MULTILINE,
                    'n' => OPT_EXPLICIT,
                    's' => OPT_SINGLELINE,
                    'x' => OPT_IGNORE_WS,
                    _ => 0,
                };
                if options == 0 {
                    return;
                }

                if off {
                    self._options &= !options;
                } else {
                    self._options |= options;
                }
            }

            self._pos += 1;
        }
    }
}



impl CharClass {
    /// .NET `AddWord`
    pub fn add_word(&mut self, ecma: bool, negate: bool) {
        if ecma {
            let ranges: &[(u16, u16)] = if negate {
                &[(0x0, 0x2F), (0x3A, 0x40), (0x5B, 0x5E), (0x60, 0x60), (0x7B, 0x12F), (0x131, 0xFFFF)]
            } else {
                &[(0x30, 0x39), (0x41, 0x5A), (0x5F, 0x5F), (0x61, 0x7A), (0x130, 0x130)]
            };
            self.add_ranges(ranges.iter().copied());
        } else {
            let entries: &[i16] = if negate {
                &[0, -2, -4, -5, -3, -1, -6, -9, -19, 0]
            } else {
                &[0, 2, 4, 5, 3, 1, 6, 9, 19, 0]
            };
            self.add_categories(entries);
        }
    }

    /// .NET `AddSpace`
    pub fn add_space(&mut self, ecma: bool, negate: bool) {
        if ecma {
            let ranges: &[(u16, u16)] = if negate {
                &[(0x0, 0x8), (0xE, 0x1F), (0x21, 0xFFFF)]
            } else {
                &[(9, 13), (0x20, 0x20)]
            };
            self.add_ranges(ranges.iter().copied());
        } else {
            let entries: &[i16] = if negate { &[-100] } else { &[100] };
            self.add_categories(entries);
        }
    }

    /// .NET `AddDigit` (non-ECMA uses the "Nd" category)
    pub fn add_digit_ranges(&mut self, ecma: bool, negate: bool) {
        if ecma {
            let ranges: &[(u16, u16)] = if negate {
                &[(0x0, 0x2F), (0x3A, 0xFFFF)]
            } else {
                &[(0x30, 0x39)]
            };
            self.add_ranges(ranges.iter().copied());
        }
    }
}




impl Parser {
    /// `AddCategoryFromName` — for `\p{...}` and `\P{...}`.
    pub fn add_category_from_name(
        &mut self,
        class: &mut CharClass,
        name: &str,
        invert: bool,
        case_insensitive: bool,
    ) -> Result<(), RegexError> {
        // InternalRegexIgnoreCase group = any of {LowercaseLetter, TitlecaseLetter, UppercaseLetter}
        const INTERNAL_ICASE: &[i16] = &[0, 2, 3, 1, 0];

        if let Some(entries) = unicode::defined_category(name) {
            if name != "InternalRegexIgnoreCase" {
                let mut effective = entries;
                if case_insensitive && (name == "Ll" || name == "Lu" || name == "Lt") {
                    effective = INTERNAL_ICASE;
                }

                if invert {
                    let negated: Vec<i16> = effective.iter().map(|&e| -e).collect();
                    class.add_categories(&negated);
                } else {
                    class.add_categories(effective);
                }
                return Ok(());
            }
        }

        match unicode::prop_ranges(name) {
            Some(ranges) => {
                let use_ranges = if invert { invert_ranges(&ranges) } else { ranges };
                class.add_ranges(use_ranges);
                Ok(())
            }
            None => Err(RegexError {
                message: format!(
                    "Invalid pattern '{}' at offset {}. Unknown property '{}'.",
                    String::from_iter(self.pattern.iter()),
                    self._pos,
                    name
                ),
                offset: self._pos,
            }),
        }
    }

    /// `ParseProperty` — reads the `{...}` after `\p` or `\P`.
    pub fn parse_property(&mut self) -> Result<String, RegexError> {
        if self._pos + 2 >= self.pattern.len() {
            return Err(self.make_exception("Incomplete \\p{X} character escape."));
        }

        let ch = self.pattern[self._pos];
        self._pos += 1;
        if ch != '{' {
            return Err(self.make_exception("Malformed \\p{X} character escape."));
        }

        let startpos = self._pos;
        while self._pos < self.pattern.len() {
            let ch = self.pattern[self._pos];
            if !(unicode::is_boundary_word_char(ch) || ch == '-') {
                break;
            }
            self._pos += 1;
        }

        let capname = String::from_iter(self.pattern[startpos..self._pos].iter());

        if self._pos == self.pattern.len() {
            return Err(self.make_exception("Incomplete \\p{X} character escape."));
        }

        if self.pattern[self._pos] != '}' {
            self._pos += 1;
            return Err(self.make_exception("Incomplete \\p{X} character escape."));
        }

        self._pos += 1;

        Ok(capname)
    }

    /// `ScanCharClass` — scans a `[...]` set. If `scan_only`, only consumes the
    /// text (pass 1) without building a class.
    pub fn scan_char_class(&mut self, case_insensitive: bool, scan_only: bool) -> Result<Option<CharClass>, RegexError> {
        let mut class = CharClass::new();
        let mut ch_prev: char = '\0';
        let mut in_range = false;
        let mut first_char = true;
        let mut closed = false;

        if self._pos < self.pattern.len() && self.pattern[self._pos] == '^' {
            self._pos += 1;
            class.negate = true;
            if (self._options & OPT_ECMASPEC) != 0
                && self._pos < self.pattern.len()
                && self.pattern[self._pos] == ']'
            {
                first_char = false;
            }
        }

        loop {
            if self._pos >= self.pattern.len() {
                break;
            }

            let is_first_char = first_char;
            first_char = false;
            let mut translated_char = false;
            let mut ch = self.pattern[self._pos];
            self._pos += 1;

            if ch == ']' {
                if !is_first_char {
                    closed = true;
                    break;
                }
            } else if ch == '\\' && self._pos < self.pattern.len() {
                let esc = self.pattern[self._pos];
                match esc {
                    'D' | 'd' => {
                        self._pos += 1;
                        if !scan_only {
                            if in_range {
                                return Err(self.make_exception(&format!(
                                    "Cannot include class \\{} in character range.",
                                    esc
                                )));
                            }
                            if (self._options & OPT_ECMASPEC) != 0 {
                                class.add_digit_ranges(true, esc == 'D');
                            } else {
                                let name = "Nd";
                                let negate = esc == 'D';
                                self.add_category_from_name(&mut class, name, negate, case_insensitive)?;
                            }
                        }
                        continue;
                    }
                    'S' | 's' => {
                        self._pos += 1;
                        if !scan_only {
                            if in_range {
                                return Err(self.make_exception(&format!(
                                    "Cannot include class \\{} in character range.",
                                    esc
                                )));
                            }
                            class.add_space((self._options & OPT_ECMASPEC) != 0, esc == 'S');
                        }
                        continue;
                    }
                    'W' | 'w' => {
                        self._pos += 1;
                        if !scan_only {
                            if in_range {
                                return Err(self.make_exception(&format!(
                                    "Cannot include class \\{} in character range.",
                                    esc
                                )));
                            }
                            class.add_word((self._options & OPT_ECMASPEC) != 0, esc == 'W');
                        }
                        continue;
                    }
                    'p' | 'P' => {
                        self._pos += 1;
                        if !scan_only {
                            if in_range {
                                return Err(self.make_exception(&format!(
                                    "Cannot include class \\{} in character range.",
                                    esc
                                )));
                            }
                            let capname = self.parse_property()?;
                            if esc == 'p' {
                                self.add_category_from_name(&mut class, &capname, false, case_insensitive)?;
                            } else {
                                self.add_category_from_name(&mut class, &capname, true, case_insensitive)?;
                            }
                        } else {
                            let _ = self.parse_property()?;
                        }
                        continue;
                    }
                    '-' => {
                        self._pos += 1;
                        if !scan_only {
                            if in_range {
                                if (esc as u32) < (ch_prev as u32) {
                                    return Err(self.make_exception("[x-y] range in reverse order."));
                                }
                                class.add_range(ch_prev, esc);
                                in_range = false;
                                ch_prev = '\0';
                            } else {
                                class.add_range(esc, esc);
                            }
                        }
                        continue;
                    }
                    _ => {
                        // position is already at the char after the backslash —
                        // scan_char_escape reads it fresh, as .NET's default arm does
                        ch = self.scan_char_escape()?;
                        translated_char = true;
                    }
                }
            }

            if in_range {
                in_range = false;
                if !scan_only {
                    if ch == '[' && !translated_char && !is_first_char {
                        // .NET: we thought we were in a range, but it's a subtraction
                        class.add_char(ch_prev);
                        let sub = self.scan_char_class(case_insensitive, scan_only)?;
                        if let Some(sub) = sub {
                            class.add_subtraction(sub);
                        }
                        if self._pos < self.pattern.len() && self.pattern[self._pos] != ']' {
                            return Err(self.make_exception("A subtraction must be the last element in a character class."));
                        }
                    } else {
                        if (ch as u32) < (ch_prev as u32) {
                            return Err(self.make_exception("[x-y] range in reverse order."));
                        }
                        class.add_range(ch_prev, ch);
                    }
                }
            } else if self._pos + 1 < self.pattern.len()
                && self.pattern[self._pos] == '-'
                && self.pattern[self._pos + 1] != ']'
            {
                // this could be the start of a range
                ch_prev = ch;
                in_range = true;
                self._pos += 1;
            } else if self._pos < self.pattern.len()
                && ch == '-'
                && !translated_char
                && self.pattern[self._pos] == '['
                && !is_first_char
            {
                // subtraction after a range, like [a-z-[b]]
                self._pos += 1;
                let sub = self.scan_char_class(case_insensitive, scan_only)?;
                if !scan_only {
                    if let Some(sub) = sub {
                        class.add_subtraction(sub);
                    }
                    if self._pos < self.pattern.len() && self.pattern[self._pos] != ']' {
                        return Err(self.make_exception("A subtraction must be the last element in a character class."));
                    }
                }
            } else if !scan_only {
                class.add_range(ch, ch);
            }
        }

        if !closed {
            return Err(self.make_exception("Unterminated [] set."));
        }

        if !scan_only && case_insensitive {
            class.add_case_equivalences();
        }

        Ok(if scan_only { None } else { Some(class) })
    }
}




impl Parser {
    fn type_from_code(&self, ch: char) -> i32 {
        match ch {
            'b' => K_BOUNDARY,
            'B' => K_NONBOUNDARY,
            'A' => K_BEGINNING,
            'G' => K_START,
            'Z' => K_ENDZ,
            'z' => K_END,
            _ => K_NOTHING,
        }
    }

    /// the number of characters a property name can have (word chars and '-')
    pub fn scan_backslash(&mut self, scan_only: bool) -> Result<Option<Node>, RegexError> {
        let ch = self.pattern[self._pos];

        match ch {
            'b' | 'B' | 'A' | 'G' | 'Z' | 'z' => {
                self._pos += 1;
                let kind = self.type_from_code(ch);
                Ok(if scan_only { None } else { Some(Node::new(kind, self._options)) })
            }
            'w' => {
                self._pos += 1;
                let set = if (self._options & OPT_ECMASPEC) != 0 {
                    ECMA_WORD_CLASS.clone()
                } else {
                    WORD_CLASS.clone()
                };
                Ok(if scan_only { None } else { Some(Node::set(set, self._options & !OPT_ICASE)) })
            }
            'W' => {
                self._pos += 1;
                let set = if (self._options & OPT_ECMASPEC) != 0 {
                    NOT_ECMA_WORD_CLASS.clone()
                } else {
                    NOT_WORD_CLASS.clone()
                };
                Ok(if scan_only { None } else { Some(Node::set(set, self._options & !OPT_ICASE)) })
            }
            's' => {
                self._pos += 1;
                let set = if (self._options & OPT_ECMASPEC) != 0 {
                    ECMA_SPACE_CLASS.clone()
                } else {
                    SPACE_CLASS.clone()
                };
                Ok(if scan_only { None } else { Some(Node::set(set, self._options & !OPT_ICASE)) })
            }
            'S' => {
                self._pos += 1;
                let set = if (self._options & OPT_ECMASPEC) != 0 {
                    NOT_ECMA_SPACE_CLASS.clone()
                } else {
                    NOT_SPACE_CLASS.clone()
                };
                Ok(if scan_only { None } else { Some(Node::set(set, self._options & !OPT_ICASE)) })
            }
            'd' => {
                self._pos += 1;
                let set = if (self._options & OPT_ECMASPEC) != 0 {
                    ECMA_DIGIT_CLASS.clone()
                } else {
                    DIGIT_CLASS.clone()
                };
                Ok(if scan_only { None } else { Some(Node::set(set, self._options & !OPT_ICASE)) })
            }
            'D' => {
                self._pos += 1;
                let set = if (self._options & OPT_ECMASPEC) != 0 {
                    NOT_ECMA_DIGIT_CLASS.clone()
                } else {
                    NOT_DIGIT_CLASS.clone()
                };
                Ok(if scan_only { None } else { Some(Node::set(set, self._options & !OPT_ICASE)) })
            }
            'p' | 'P' => {
                self._pos += 1;
                if scan_only {
                    let _ = self.parse_property()?;
                    return Ok(None);
                }

                let mut cc = CharClass::new();
                let capname = self.parse_property()?;
                let invert = ch != 'p';
                self.add_category_from_name(&mut cc, &capname, invert, (self._options & OPT_ICASE) != 0)?;
                if (self._options & OPT_ICASE) != 0 {
                    cc.add_case_equivalences();
                }

                Ok(Some(Node::set(cc, self._options & !OPT_ICASE)))
            }
            _ => {
                let result = self.scan_basic_backslash(scan_only)?;
                if let Some(node) = &result {
                    if node.kind == K_REF && (node.options & OPT_ICASE) != 0 {
                        self.has_ignore_case_backreference = true;
                    }
                }
                Ok(result)
            }
        }
    }

    /// `ScanBasicBackslash` — backreferences and character escapes.
    pub fn scan_basic_backslash(&mut self, scan_only: bool) -> Result<Option<Node>, RegexError> {
        let backpos = self._pos;
        let mut ch = self.pattern[self._pos];

        // \k<name>
        if ch == 'k' {
            if self._pos + 1 >= self.pattern.len() {
                return Err(self.make_exception("Malformed \\k<...> named back reference."));
            }

            self._pos += 1;
            let next = self.pattern[self._pos];
            if next != '<' && next != '\'' {
                return Err(self.make_exception("Malformed \\k<...> named back reference."));
            }

            self._pos += 1;
            let close = if next == '\'' { '\'' } else { '>' };

            if self._pos == self.pattern.len() {
                return Err(self.make_exception("Malformed \\k<...> named back reference."));
            }

            let ch = self.pattern[self._pos];
            if ch >= '0' && ch <= '9' {
                    let capnum = self.scan_decimal_impl()?;
                    if self._pos < self.pattern.len() && self.pattern[self._pos] == close {
                        self._pos += 1;
                        if self.is_capture_slot(capnum) {
                            if scan_only {
                                return Ok(None);
                            }
                            return Ok(Some(Node::reference(capnum, self._options)));
                        }
                        return Err(self.make_exception(&format!(
                            "Reference to undefined group number {}.",
                            capnum
                        )));
                    }
                } else if unicode::is_boundary_word_char(ch) {
                    let capname = self.scan_capname();
                    if self._pos < self.pattern.len() && self.pattern[self._pos] == close {
                        self._pos += 1;
                        if let Some(&capnum) = self.capnames.get(&capname) {
                            if scan_only {
                                return Ok(None);
                            }
                            return Ok(Some(Node::reference(capnum, self._options)));
                        }
                        if !scan_only {
                            return Err(self.make_exception(&format!(
                                "Reference to undefined group name '{}'.",
                                capname
                            )));
                        }
                        return Ok(None);
                    }
                }

                return Err(self.make_exception("Malformed \\k<...> named back reference."));
            }

        // angle-bracketed backreference (< or ')
        let angled;
        let close;
        if (ch == '<' || ch == '\'') && self._pos + 1 < self.pattern.len() {
            angled = true;
            close = if ch == '\'' { '\'' } else { '>' };
            self._pos += 1;
            ch = self.pattern[self._pos];
        } else {
            angled = false;
            close = '\0';
        }

        // numeric backreference
        if angled && ch >= '0' && ch <= '9' {
            let capnum = self.scan_decimal_impl()?;

            if self._pos < self.pattern.len() && self.pattern[self._pos] == close {
                self._pos += 1;
                if self.is_capture_slot(capnum) {
                    if scan_only {
                        return Ok(None);
                    }
                    return Ok(Some(Node::reference(capnum, self._options)));
                }
                return Err(self.make_exception(&format!(
                    "Reference to undefined group number {}.",
                    capnum
                )));
            }
            // falls through to character escape (e.g. <\4> can't happen in valid patterns)
            self._pos = backpos;
            let escape = self.scan_char_escape()?;
            if scan_only {
                return Ok(None);
            }
            return Ok(Some(self.create_one_with_case_conversion(escape)));
        } else if !angled && ch >= '1' && ch <= '9' {
            let capnum = self.scan_decimal_impl()?;

            if scan_only {
                return Ok(None);
            }

            if self.is_capture_slot(capnum) {
                return Ok(Some(Node::reference(capnum, self._options)));
            }

            if capnum <= 9 {
                return Err(self.make_exception(&format!(
                    "Reference to undefined group number {}.",
                    capnum
                )));
            }
            // number > 9 not a known slot: char escape with the leading digits
            self._pos = backpos;
            let escape = self.scan_char_escape()?;
            return Ok(Some(self.create_one_with_case_conversion(escape)));
        } else if angled && unicode::is_boundary_word_char(ch) {
            let capname = self.scan_capname();

            if self._pos < self.pattern.len() && self.pattern[self._pos] == close {
                self._pos += 1;
                if let Some(&capnum) = self.capnames.get(&capname) {
                    if scan_only {
                        return Ok(None);
                    }
                    return Ok(Some(Node::reference(capnum, self._options)));
                }
                if !scan_only {
                    return Err(self.make_exception(&format!(
                        "Reference to undefined group name '{}'.",
                        capname
                    )));
                }
                return Ok(None);
            }
            self._pos = backpos;
            let escape = self.scan_char_escape()?;
            if scan_only {
                return Ok(None);
            }
            return Ok(Some(self.create_one_with_case_conversion(escape)));
        }

        // not a backreference: character escape
        self._pos = backpos;
        let escape = self.scan_char_escape()?;
        if scan_only {
            return Ok(None);
        }
        Ok(Some(self.create_one_with_case_conversion(escape)))
    }

    /// `ScanCharEscape` — for `\` inside classes and after backslash.
    pub fn scan_char_escape(&mut self) -> Result<char, RegexError> {
        let ch = self.pattern[self._pos];
        self._pos += 1;

        if ch >= '0' && ch <= '7' {
            self._pos -= 1;
            return Ok(self.scan_octal());
        }

        match ch {
            'x' => self.scan_hex(2),
            'u' => self.scan_hex(4),
            'a' => Ok('\u{0007}'),
            'b' => Ok('\u{0008}'),
            'e' => Ok('\u{001B}'),
            'f' => Ok('\u{000C}'),
            'n' => Ok('\n'),
            'r' => Ok('\r'),
            't' => Ok('\t'),
            'v' => Ok('\u{000B}'),
            'c' => self.scan_control(),
            _ => {
                let ecma = (self._options & OPT_ECMASPEC) != 0;
                if !ecma && unicode::is_boundary_word_char(ch) {
                    return Err(self.make_exception(&format!("Unrecognized escape sequence \\{}.", ch)));
                }
                Ok(ch)
            }
        }
    }

    /// `ScanOctal` — up to three octal digits, & 0xFF.
    pub fn scan_octal(&mut self) -> char {
        let mut c = 3.min(self.pattern.len() - self._pos);
        let mut value: u32 = 0;

        while c > 0 && self._pos < self.pattern.len() {
            let d = self.pattern[self._pos] as u32;
            if d >= '8' as u32 {
                break;
            }
            let digit = d - '0' as u32;
            self._pos += 1;
            c -= 1;
            value = value * 8 + digit;
            if (self._options & OPT_ECMASPEC) != 0 && value >= 0x20 {
                break;
            }
        }

        char::from_u32(value & 0xFF).unwrap_or('\u{FFFD}')
    }

    /// `ScanHex` — exactly `c` hex digits; throws if any are invalid.
    pub fn scan_hex(&mut self, c: usize) -> Result<char, RegexError> {
        let mut value: u32 = 0;
        let mut remaining = c;

        if self._pos + c <= self.pattern.len() {
            while remaining > 0 {
                let ch = self.pattern[self._pos];
                self._pos += 1;
                let digit = hex_digit(ch);
                if digit == 0xFF {
                    break;
                }
                value = value * 0x10 + digit;
                remaining -= 1;
            }
        }

        if remaining != 0 {
            return Err(self.make_exception("Insufficient or invalid hexadecimal digits."));
        }

        Ok(char::from_u32(value).unwrap_or('\u{FFFD}'))
    }

    /// `ScanControl` — `\cX` control characters.
    pub fn scan_control(&mut self) -> Result<char, RegexError> {
        if self._pos == self.pattern.len() {
            return Err(self.make_exception("Missing control character."));
        }

        let mut ch = self.pattern[self._pos];
        self._pos += 1;

        if (ch as u32).wrapping_sub('a' as u32) <= ('z' as u32 - 'a' as u32) {
            ch = ((ch as u8) - (b'a' - b'A')) as char;
        }

        let c = (ch as u32).wrapping_sub('@' as u32);
        if c < 0x20 {
            return Ok(char::from_u32(c).unwrap_or('\u{FFFD}'));
        }

        Err(self.make_exception("Unrecognized control character."))
    }

    /// `CreateOneWithCaseConversion` — case conversion of a literal char.
    pub fn create_one_with_case_conversion(&mut self, ch: char) -> Node {
        if (self._options & OPT_ICASE) != 0 {
            if unicode::case_equivalents(ch).len() <= 1 {
                return Node::one(ch, self._options & !OPT_ICASE);
            }

            let mut class = CharClass::new();
            for eq in unicode::case_equivalents(ch) {
                class.add_char(eq);
            }
            return Node::set(class, self._options & !OPT_ICASE);
        }

        Node::one(ch, self._options)
    }
}

fn hex_digit(ch: char) -> u32 {
    match ch {
        '0'..='9' => ch as u32 - '0' as u32,
        'a'..='f' => ch as u32 - 'a' as u32 + 10,
        'A'..='F' => ch as u32 - 'A' as u32 + 10,
        _ => 0xFF,
    }
}

pub static WORD_CLASS: std::sync::LazyLock<CharClass> = std::sync::LazyLock::new(|| {
    let mut c = CharClass::new();
    c.add_categories(&[0, 2, 4, 5, 3, 1, 6, 9, 19, 0]);
    c
});
pub static NOT_WORD_CLASS: std::sync::LazyLock<CharClass> = std::sync::LazyLock::new(|| {
    let mut c = CharClass::new();
    c.add_categories(&[0, -2, -4, -5, -3, -1, -6, -9, -19, 0]);
    c
});
pub static SPACE_CLASS: std::sync::LazyLock<CharClass> = std::sync::LazyLock::new(|| {
    let mut c = CharClass::new();
    c.add_categories(&[100]);
    c
});
pub static NOT_SPACE_CLASS: std::sync::LazyLock<CharClass> = std::sync::LazyLock::new(|| {
    let mut c = CharClass::new();
    c.add_categories(&[-100]);
    c
});
pub static DIGIT_CLASS: std::sync::LazyLock<CharClass> = std::sync::LazyLock::new(|| {
    let mut c = CharClass::new();
    c.add_categories(&[9]);
    c
});
pub static NOT_DIGIT_CLASS: std::sync::LazyLock<CharClass> = std::sync::LazyLock::new(|| {
    let mut c = CharClass::new();
    c.add_categories(&[-9]);
    c
});
pub static ECMA_WORD_CLASS: std::sync::LazyLock<CharClass> = std::sync::LazyLock::new(|| {
    let mut c = CharClass::new();
    c.add_ranges([(0x30, 0x39), (0x41, 0x5A), (0x5F, 0x5F), (0x61, 0x7A), (0x130, 0x130)]);
    c
});
pub static NOT_ECMA_WORD_CLASS: std::sync::LazyLock<CharClass> = std::sync::LazyLock::new(|| {
    let mut c = CharClass::new();
    c.add_ranges([(0x0, 0x2F), (0x3A, 0x40), (0x5B, 0x5E), (0x60, 0x60), (0x7B, 0x12F), (0x131, 0xFFFF)]);
    c
});
pub static ECMA_SPACE_CLASS: std::sync::LazyLock<CharClass> = std::sync::LazyLock::new(|| {
    let mut c = CharClass::new();
    c.add_ranges([(9, 13), (0x20, 0x20)]);
    c
});
pub static NOT_ECMA_SPACE_CLASS: std::sync::LazyLock<CharClass> = std::sync::LazyLock::new(|| {
    let mut c = CharClass::new();
    c.add_ranges([(0x0, 0x8), (0xE, 0x1F), (0x21, 0xFFFF)]);
    c
});
pub static ECMA_DIGIT_CLASS: std::sync::LazyLock<CharClass> = std::sync::LazyLock::new(|| {
    let mut c = CharClass::new();
    c.add_ranges([(0x30, 0x39)]);
    c
});
pub static NOT_ECMA_DIGIT_CLASS: std::sync::LazyLock<CharClass> = std::sync::LazyLock::new(|| {
    let mut c = CharClass::new();
    c.add_ranges([(0x0, 0x2F), (0x3A, 0xFFFF)]);
    c
});




impl Parser {
    fn start_group(&mut self, open_group: Node) {
        self.group = Some(open_group);
        self.alternation = Some(Node::new(K_ALTERNATE, self._options));
        self.concatenation = Some(Node::new(K_CONCATENATE, self._options));
    }

    fn push_group(&mut self) {
        let group = self.group.take().expect("group");
        let alternation = self.alternation.take().expect("alternation");
        let concatenation = self.concatenation.take().expect("concatenation");
        self.stack.push(Frame {
            group,
            alternation,
            concatenation,
        });
    }

    fn pop_group(&mut self) -> Result<(), RegexError> {
        let mut frame = self.stack.pop().expect("stack");

        if frame.group.kind == K_EXPRCOND && frame.group.child_count() == 0 {
            if self.unit.is_none() {
                return Err(self.make_exception("Illegal conditional (?(...)) expression."));
            }
            let unit = self.unit.take().expect("unit");
            frame.group.add_child(unit);
        }

        self.group = Some(frame.group);
        self.alternation = Some(frame.alternation);
        self.concatenation = Some(frame.concatenation);
        Ok(())
    }

    fn add_alternate(&mut self) {
        let concat = self.concatenation.take().expect("concatenation").reverse_concatenation_if_rtl();

        let group_kind = self.group.as_ref().map(|g| g.kind);
        if group_kind == Some(K_EXPRCOND) || group_kind == Some(K_BACKCOND) {
            self.group.as_mut().expect("group").add_child(concat);
        } else {
            self.alternation.as_mut().expect("alternation").add_child(concat);
        }

        self.concatenation = Some(Node::new(K_CONCATENATE, self._options));
    }

    fn add_group(&mut self) -> Result<(), RegexError> {
        let concat = self.concatenation.take().expect("concatenation").reverse_concatenation_if_rtl();

        let kind = self.group.as_ref().map(|g| g.kind);
        if kind == Some(K_EXPRCOND) || kind == Some(K_BACKCOND) {
            let group = self.group.as_mut().expect("group");
            group.add_child(concat);

            let limit = if group.kind == K_BACKCOND { 2 } else { 3 };
            if group.child_count() > limit {
                return Err(self.make_exception("Too many | in (?()|)."));
            }
        } else {
            let mut alt = self.alternation.take().expect("alternation");
            alt.add_child(concat);
            let mut group = self.group.take().expect("group");
            group.add_child(alt);
            self.group = Some(group);
        }

        self.unit = self.group.take();
        Ok(())
    }

    fn add_to_concatenate(&mut self, pos: usize, cch: usize) {
        match cch {
            0 => {}
            1 => {
                let ch = self.pattern[pos];
                let node = self.create_one_with_case_conversion(ch);
                self.concatenation.as_mut().expect("concatenation").add_child(node);
            }
            _ => {
                let text = String::from_iter(self.pattern[pos..pos + cch].iter());
                if (self._options & OPT_ICASE) == 0
                    || !unicode::participates_in_case_conversion(&self.pattern[pos..pos + cch])
                {
                    self.concatenation.as_mut().expect("concatenation").add_child(Node::multi(&text, self._options & !OPT_ICASE));
                } else {
                    let chars: Vec<char> = self.pattern[pos..pos + cch].to_vec();
                    for ch in chars {
                        let node = self.create_one_with_case_conversion(ch);
                        self.concatenation.as_mut().expect("concatenation").add_child(node);
                    }
                }
            }
        }
    }

    fn is_quantifier(ch: char) -> bool {
        ch <= '{' && matches!(ch, '*' | '+' | '?' | '{')
    }

    fn is_special(ch: char) -> bool {
        ch <= '|' && matches!(ch, '*' | '+' | '?' | '{' | '$' | '.' | '(' | ')' | '[' | '\\' | '^' | '|')
    }

    fn is_special_or_space(ch: char) -> bool {
        ch <= '|' && matches!(ch, '*' | '+' | '?' | '{' | '$' | '.' | '(' | ')' | '[' | '\\' | '^' | '|' | '#')
            || unicode::is_whitespace(ch)
    }

    /// `IsTrueQuantifier` — is the current char a quantifier?
    fn is_true_quantifier(&self) -> bool {
        if self._pos >= self.pattern.len() {
            return false;
        }

        let startpos = self._pos;
        let ch = self.pattern[startpos];
        if ch != '{' {
            return Self::is_quantifier(ch);
        }

        let mut pos = startpos;
        let mut n_chars = (self.pattern.len() - self._pos) as i32;
        let mut ch = ch;

        // first digit run (decrement before read, like the .NET loop)
        loop {
            n_chars -= 1;
            if n_chars <= 0 {
                break;
            }
            pos += 1;
            ch = self.pattern[pos];
            if (ch as u32).wrapping_sub('0' as u32) > 9 {
                break;
            }
        }

        if n_chars == 0 || pos - startpos == 1 {
            return false;
        }

        if ch == '}' {
            return true;
        }

        if ch != ',' {
            return false;
        }

        // second digit run
        loop {
            n_chars -= 1;
            if n_chars <= 0 {
                break;
            }
            pos += 1;
            ch = self.pattern[pos];
            if (ch as u32).wrapping_sub('0' as u32) > 9 {
                break;
            }
        }

        n_chars > 0 && ch == '}'
    }
}




impl Parser {
    /// the entry point. Returns the root Capture node.
    pub fn scan_regex(&mut self) -> Result<Node, RegexError> {
        let mut is_quantifier = false;

        // The main Capture object strips IgnoreCase; the rest of the nodes keep it.
        let mut root_capture = Node::new(K_CAPTURE, self._options & !OPT_ICASE);
        root_capture.n = -1;
        self.start_group(root_capture);

        loop {
            if self._pos >= self.pattern.len() {
                break;
            }

            let mut was_prev_quantifier = is_quantifier;
            is_quantifier = false;

            self.scan_blank()?;

            let startpos = self._pos;

            if (self._options & OPT_IGNORE_WS) != 0 {
                while self._pos < self.pattern.len() {
                    let ch = self.pattern[self._pos];
                    if !Self::is_special_or_space(ch) || (ch == '{' && !self.is_true_quantifier()) {
                        self._pos += 1;
                    } else {
                        break;
                    }
                }
            } else {
                while self._pos < self.pattern.len() {
                    let ch = self.pattern[self._pos];
                    if !Self::is_special(ch) || (ch == '{' && !self.is_true_quantifier()) {
                        self._pos += 1;
                    } else {
                        break;
                    }
                }
            }

            let endpos = self._pos;

            self.scan_blank()?;

            let ch;
            if self._pos == self.pattern.len() {
                ch = '!'; // nonspecial, end of pattern
            } else if Self::is_special(self.pattern[self._pos]) {
                let ch0 = self.pattern[self._pos];
                is_quantifier = Self::is_quantifier(ch0);
                self._pos += 1;
                ch = ch0;
            } else {
                ch = ' '; // nonspecial, ordinary char
            }

            if startpos < endpos {
                let cch_unquantified = endpos - startpos - if is_quantifier { 1 } else { 0 };

                was_prev_quantifier = false;

                if cch_unquantified > 0 {
                    self.add_to_concatenate(startpos, cch_unquantified);
                }

                if is_quantifier {
                    self.unit = Some(self.create_one_with_case_conversion(self.pattern[endpos - 1]));
                }
            }

            match ch {
                '!' => break,
                ' ' => continue,
                '[' => {
                    let case_insensitive = (self._options & OPT_ICASE) != 0;
                    let set = self
                        .scan_char_class(case_insensitive, false)?
                        .expect("class");
                    self.unit = Some(Node::set(set, self._options & !OPT_ICASE));
                }
                '(' => {
                    self._options_stack.push(self._options);
                    match self.scan_group_open()? {
                        Some(grouper) => {
                            self.push_group();
                            self.start_group(grouper);
                        }
                        None => {
                            self._options_stack.pop();
                        }
                    }
                    continue;
                }
                '|' => {
                    self.add_alternate();
                    continue;
                }
                ')' => {
                    if self.stack.is_empty() {
                        return Err(self.make_exception("Too many )'s."));
                    }

                    self.add_group()?;
                    self.pop_group()?;
                    self._options = self._options_stack.pop().expect("options stack");

                    if self.unit.is_none() {
                        continue;
                    }
                }
                '\\' => {
                    if self._pos == self.pattern.len() {
                        return Err(self.make_exception("Illegal \\ at end of pattern."));
                    }

                    self.unit = self.scan_backslash(false)?;
                }
                '^' => {
                    let kind = if (self._options & OPT_MULTILINE) != 0 {
                        K_BOL
                    } else {
                        K_BEGINNING
                    };
                    self.unit = Some(Node::new(kind, self._options));
                }
                '$' => {
                    let kind = if (self._options & OPT_MULTILINE) != 0 {
                        K_EOL
                    } else {
                        K_ENDZ
                    };
                    self.unit = Some(Node::new(kind, self._options));
                }
                '.' => {
                    self.unit = Some(if (self._options & OPT_SINGLELINE) != 0 {
                        let mut cc = crate::native::regex::charclass::CharClass::new();
                        cc.add_range('\0', '\u{FFFF}');
                        Node::set(cc, self._options & !OPT_ICASE)
                    } else {
                        Node::notone('\n', self._options & !OPT_ICASE)
                    });
                }
                '{' | '*' | '+' | '?' => {
                    if self.unit.is_none() {
                        return Err(if was_prev_quantifier {
                            self.make_exception(&format!("Nested quantifier '{}'.", ch))
                        } else {
                            self.make_exception(&format!("Quantifier '{}' following nothing.", ch))
                        });
                    }
                    self._pos -= 1;
                }
                _ => {}
            }

            self.scan_blank()?;

            if self._pos == self.pattern.len() {
                if let Some(unit) = self.unit.take() {
                    self.concatenation.as_mut().expect("concatenation").add_child(unit);
                }
                continue;
            }

            is_quantifier = self.is_true_quantifier();
            if !is_quantifier {
                if let Some(unit) = self.unit.take() {
                    self.concatenation.as_mut().expect("concatenation").add_child(unit);
                }
                continue;
            }

            let ch = self.pattern[self._pos];
            self._pos += 1;

            while self.unit.is_some() {
                let mut min = 0;
                let mut max = 0;

                match ch {
                    '*' => {
                        max = i32::MAX;
                    }
                    '+' => {
                        min = 1;
                        max = i32::MAX;
                    }
                    '?' => {
                        max = 1;
                    }
                    '{' => {
                        let startpos2 = self._pos;
                        max = self.scan_decimal_impl()?;
                        min = max;
                        if startpos2 < self._pos && self._pos < self.pattern.len() && self.pattern[self._pos] == ',' {
                            self._pos += 1;
                            max = if self._pos == self.pattern.len() || self.pattern[self._pos] == '}' {
                                i32::MAX
                            } else {
                                self.scan_decimal_impl()?
                            };
                        }

                        if startpos2 == self._pos || self._pos == self.pattern.len() || self.pattern[self._pos] != '}' {
                            // not a valid {x,y} — treat as literal text
                            self.concatenation.as_mut().expect("concatenation").add_child(self.unit.take().expect("unit"));
                            self._pos = startpos2 - 1;
                            break;
                        }

                        self._pos += 1;
                    }
                    _ => {}
                }

                self.scan_blank()?;

                let mut lazy = false;
                if self._pos < self.pattern.len() && self.pattern[self._pos] == '?' {
                    self._pos += 1;
                    lazy = true;
                }

                if min > max {
                    return Err(self.make_exception("Illegal {x,y} with x > y."));
                }

                let unit = self.unit.take().expect("unit");
                self.concatenation
                    .as_mut()
                    .expect("concatenation")
                    .add_child(unit.make_quantifier(lazy, min, max));
            }
        }

        if !self.stack.is_empty() {
            return Err(self.make_exception("Not enough )'s."));
        }

        self.add_group()?;

        let root = self.unit.take().expect("root");
        Ok(root)
    }

    /// `ScanGroupOpen` — parse a `(...)` opener. Returns None for option groups.
    pub fn scan_group_open(&mut self) -> Result<Option<Node>, RegexError> {
        // "(" followed by nothing, "("x where x != '?', or "(?)"
        if self._pos == self.pattern.len()
            || self.pattern[self._pos] != '?'
            || (self._pos + 1 < self.pattern.len() && self.pattern[self._pos + 1] == ')')
        {
            if (self._options & OPT_EXPLICIT) != 0 || self.ignore_next_paren {
                self.ignore_next_paren = false;
                return Ok(Some(Node::new(K_GROUP, self._options)));
            }

            let slot = self.autocap;
            self.autocap += 1;
            return Ok(Some(cap_node(slot, self._options)));
        }

        self._pos += 1;

        loop {
            if self._pos == self.pattern.len() {
                break;
            }

            let ch = self.pattern[self._pos];
            self._pos += 1;

            let node_type: i32;

            match ch {
                ':' => {
                    node_type = K_GROUP;
                }
                '=' => {
                    self._options &= !OPT_RTL;
                    node_type = K_POSLOOK;
                }
                '!' => {
                    self._options &= !OPT_RTL;
                    node_type = K_NEGLOOK;
                }
                '>' => {
                    node_type = K_ATOMIC;
                }
                '\'' => {
                    return self.scan_group_open_named(Some('\''));
                }
                '<' => {
                    return self.scan_group_open_named(None);
                }
                '(' => {
                    let paren_pos = self._pos;
                    if self._pos < self.pattern.len() {
                        let ch = self.pattern[self._pos];

                        if ch >= '0' && ch <= '9' {
                            let capnum = self.scan_decimal_impl()?;
                            if self._pos < self.pattern.len() && self.pattern[self._pos] == ')' {
                                self._pos += 1;
                                if self.is_capture_slot(capnum) {
                                    return Ok(Some(backcond_node(capnum, self._options)));
                                }

                                return Err(self.make_exception(&format!(
                                    "Conditional alternation refers to an undefined group number {}.",
                                    capnum
                                )));
                            }

                            return Err(self.make_exception(&format!(
                                "Conditional alternation is missing a closing parenthesis after the group number {}.",
                                capnum
                            )));
                        }

                        if unicode::is_boundary_word_char(ch) {
                            let capname = self.scan_capname();
                            if let Some(&capnum) = self.capnames.get(&capname) {
                                if self._pos < self.pattern.len() && self.pattern[self._pos] == ')' {
                                    self._pos += 1;
                                    return Ok(Some(backcond_node(capnum, self._options)));
                                }
                            }
                        }
                    }

                    // not a back reference — expression conditional
                    node_type = K_EXPRCOND;
                    self._pos = paren_pos - 1;
                    self.ignore_next_paren = true;

                    if self._pos + 2 < self.pattern.len() && self.pattern[self._pos + 1] == '?' {
                        if self.pattern[self._pos + 2] == '#' {
                            return Err(self.make_exception("Alternation conditions cannot be comments."));
                        }

                        if self.pattern[self._pos + 2] == '\''
                            || (self._pos + 3 < self.pattern.len()
                                && self.pattern[self._pos + 2] == '<'
                                && self.pattern[self._pos + 3] != '!'
                                && self.pattern[self._pos + 3] != '=')
                        {
                            return Err(self.make_exception(
                                "Alternation conditions do not capture and cannot be named.",
                            ));
                        }
                    }
                    // fall through to the shared return at the end of the loop, as the .NET switch does
                }
                _ => {
                    self._pos -= 1;

                    node_type = K_GROUP;
                    if self.group.as_ref().map(|g| g.kind) != Some(K_EXPRCOND) {
                        self.scan_options();
                    }

                    if self._pos == self.pattern.len() {
                        return self.invalid_grouping_construct();
                    }

                    let ch = self.pattern[self._pos];
                    self._pos += 1;
                    if ch == ')' {
                        return Ok(None);
                    }

                    if ch != ':' {
                        return self.invalid_grouping_construct();
                    }
                    // fall through to the shared return at the end of the loop, as the .NET switch does
                }
            }

            return Ok(Some(Node::new(node_type, self._options)));
        }

        Err(self.make_exception("Unrecognized grouping construct."))
    }

    /// handles the `(?<...> ... )` family and `(?'...' ... )`.
    fn scan_group_open_named(&mut self, single_quote: Option<char>) -> Result<Option<Node>, RegexError> {
        let close = if single_quote.is_some() { '\'' } else { '>' };

        if self._pos == self.pattern.len() {
            return self.invalid_grouping_construct();
        }

        let ch = self.pattern[self._pos];
        self._pos += 1;

        match ch {
            '=' => {
                if single_quote.is_some() {
                    return self.invalid_grouping_construct();
                }
                self._options |= OPT_RTL;
                return Ok(Some(Node::new(K_POSLOOK, self._options)));
            }
            '!' => {
                if single_quote.is_some() {
                    return self.invalid_grouping_construct();
                }
                self._options |= OPT_RTL;
                return Ok(Some(Node::new(K_NEGLOOK, self._options)));
            }
            _ => {}
        }

        // not lookbehind: a named group, e.g. (?<name>...)
        self._pos -= 1;
        let mut capnum = -1;
        let mut uncapnum = -1;
        let mut proceed = false;

        let ch = self.pattern[self._pos];

        if (ch as u32).wrapping_sub('0' as u32) <= 9 {
            capnum = self.scan_decimal_impl()?;

            if !self.is_capture_slot(capnum) {
                capnum = -1;
            }

            if self._pos < self.pattern.len() && !(self.pattern[self._pos] == close || self.pattern[self._pos] == '-') {
                return Err(self.make_exception("Invalid group name: Group names must begin with a word character."));
            }

            if capnum == 0 {
                return Err(self.make_exception("Capture number cannot be zero."));
            }
        } else if unicode::is_boundary_word_char(ch) {
            let capname = self.scan_capname();

            if let Some(&n) = self.capnames.get(&capname) {
                capnum = n;
            }

            if self._pos < self.pattern.len() && !(self.pattern[self._pos] == close || self.pattern[self._pos] == '-') {
                return Err(self.make_exception("Invalid group name: Group names must begin with a word character."));
            }
        } else if ch == '-' {
            proceed = true;
        } else {
            return Err(self.make_exception("Invalid group name: Group names must begin with a word character."));
        }

        if (capnum != -1 || proceed) && self._pos + 1 < self.pattern.len() && self.pattern[self._pos] == '-' {
            self._pos += 1;
            let ch = self.pattern[self._pos];

            if (ch as u32).wrapping_sub('0' as u32) <= 9 {
                uncapnum = self.scan_decimal_impl()?;

                if !self.is_capture_slot(uncapnum) {
                    return Err(self.make_exception(&format!(
                        "Reference to undefined group number {}.",
                        uncapnum
                    )));
                }

                if self._pos < self.pattern.len() && self.pattern[self._pos] != close {
                    return Err(self.make_exception("Invalid group name: Group names must begin with a word character."));
                }
            } else if unicode::is_boundary_word_char(ch) {
                let uncapname = self.scan_capname();

                match self.capnames.get(&uncapname) {
                    Some(&n) => {
                        uncapnum = n;
                    }
                    None => {
                        return Err(self.make_exception(&format!(
                            "Reference to undefined group name '{}'.",
                            uncapname
                        )));
                    }
                }

                if self._pos < self.pattern.len() && self.pattern[self._pos] != close {
                    return Err(self.make_exception("Invalid group name: Group names must begin with a word character."));
                }
            } else {
                return Err(self.make_exception("Invalid group name: Group names must begin with a word character."));
            }
        }

        if (capnum != -1 || uncapnum != -1) && self._pos < self.pattern.len() && self.pattern[self._pos] == close {
            self._pos += 1;
            return Ok(Some(balancing_node(capnum, uncapnum, self._options)));
        }

        self.invalid_grouping_construct()
    }

    fn invalid_grouping_construct(&self) -> Result<Option<Node>, RegexError> {
        Err(self.make_exception("Unrecognized grouping construct."))
    }
}

fn cap_node(slot: i32, options: i32) -> Node {
    let mut n = Node::new(K_CAPTURE, options);
    n.m = slot;
    n.n = -1;
    n
}

fn backcond_node(capnum: i32, options: i32) -> Node {
    let mut n = Node::new(K_BACKCOND, options);
    n.m = capnum;
    n
}

fn balancing_node(capnum: i32, uncapnum: i32, options: i32) -> Node {
    let mut n = Node::new(K_CAPTURE, options);
    n.m = capnum;
    n.n = uncapnum;
    n
}

