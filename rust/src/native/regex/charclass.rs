use crate::native::regex::unicode;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CharClass {

    ranges: Vec<(u16, u16)>,

    pub categories: Vec<i16>,
    pub negate: bool,
    pub subtraction: Option<Box<CharClass>>,
}
impl CharClass {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn add_char(&mut self, c: char) {
        if (c as u32) <= 0xFFFF {
            let c = c as u16;
            self.ranges.push((c, c));
            self.ensure_canonical();
        }

    }
    pub fn add_range(&mut self, first: char, last: char) {
        let a = first as u32;
        let b = last as u32;
        if a > 0xFFFF {
            return;
        }
        self.ranges.push((a as u16, (b.min(0xFFFF)) as u16));
        self.ensure_canonical();
    }
    pub fn add_ranges(&mut self, ranges: impl IntoIterator<Item = (u16, u16)>) {
        self.ranges.extend(ranges);
        self.ensure_canonical();
    }
    pub fn add_categories(&mut self, entries: &[i16]) {
        self.categories.extend_from_slice(entries);
    }
    pub fn add_subtraction(&mut self, other: CharClass) {
        self.subtraction = Some(Box::new(other));
    }

    pub fn add_case_equivalences(&mut self) {
        let snapshot = self.ranges.clone();
        let mut added: Vec<(u16, u16)> = Vec::new();
        for &(first, last) in snapshot.iter() {
            for c in first..=last {
                let Some(ch) = char::from_u32(c as u32) else {
                    continue;
                };
                for eq in unicode::case_equivalents(ch) {
                    let cp = eq as u32;
                    if cp <= 0xFFFF && char::from_u32(eq as u32).is_some() {
                        added.push((eq as u16, eq as u16));
                    }
                }
            }
        }
        self.ranges.extend(added);
        self.ensure_canonical();
    }
    pub fn char_in_class(&self, ch: char) -> bool {

        let mut in_class = self.char_in_class_internal(ch);
        if self.negate {
            in_class = !in_class;
        }
        if in_class {
            if let Some(sub) = &self.subtraction {
                if sub.char_in_class(ch) {
                    in_class = false;
                }
            }
        }
        in_class
    }
    fn char_in_class_internal(&self, ch: char) -> bool {
        let cp = ch as u32;
        if cp <= 0xFFFF && in_ranges(&self.ranges, cp as u16) {
            return true;
        }
        if !self.categories.is_empty() && char_in_categories(&self.categories, ch) {
            return true;
        }
        false
    }

    pub fn ensure_canonical(&mut self) {
        if self.ranges.len() > 1 {
            self.ranges.sort_by_key(|&(a, _)| a);
            let mut merged: Vec<(u16, u16)> = Vec::with_capacity(self.ranges.len());
            let mut cur = self.ranges[0];
            for &(first, last) in self.ranges.iter().skip(1) {
                if first <= cur.1.saturating_add(1) {
                    if last > cur.1 {
                        cur.1 = last;
                    }
                } else {
                    merged.push(cur);
                    cur = (first, last);
                }
            }
            merged.push(cur);
            self.ranges = merged;
        }
        if !self.negate && self.subtraction.is_none() {

            if self.categories.is_empty() {
                if self.ranges.len() == 2 {
                    let [(a0, a1), (b0, b1)] = [self.ranges[0], self.ranges[1]];
                    if a0 == 0 && b1 == 0xFFFF && (a1 as u32) < (b0 as u32).saturating_sub(1) {
                        self.ranges = vec![(a1 + 1, b0 - 1)];
                        self.negate = true;
                    }
                } else if self.ranges.len() == 1 {
                    let (a, b) = self.ranges[0];
                    if a == 0 && b == 0xFFFE {
                        self.ranges = vec![(0xFFFF, 0xFFFF)];
                        self.negate = true;
                    } else if a == 1 && b == 0xFFFF {
                        self.ranges = vec![(0, 0)];
                        self.negate = true;
                    }
                }
            }

            if !self.categories.is_empty()
                && self.ranges.len() == 1
                && self.ranges[0].0 == 0
                && self.ranges[0].1 == 0xFFFF
            {
                self.categories.clear();
            }
        }
    }
    pub fn is_empty(&self) -> bool {
        self.categories.is_empty() && self.ranges.is_empty() && !self.negate && self.subtraction.is_none()
    }
    pub fn is_singleton(&self) -> bool {
        self.subtraction.is_none()
            && self.categories.is_empty()
            && !self.negate
            && self.ranges.len() == 1
            && self.ranges[0].0 == self.ranges[0].1
    }
    pub fn is_singleton_inverse(&self) -> bool {
        self.subtraction.is_none()
            && self.categories.is_empty()
            && self.negate
            && self.ranges.len() == 1
            && self.ranges[0].0 == self.ranges[0].1
    }
    pub fn singleton_char(&self) -> char {
        char::from_u32(self.ranges[0].0 as u32).unwrap_or('\u{FFFD}')
    }
}
fn in_ranges(ranges: &[(u16, u16)], cp: u16) -> bool {
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
            return true;
        }
    }
    false
}
fn char_in_categories(cats: &[i16], ch: char) -> bool {
    let cat = unicode::category_of(ch) as i16;
    let ws = unicode::is_whitespace(ch);
    let entry_hit = |entry: i16| -> bool {

        if entry == unicode::SPACE_CONST {
            ws
        } else if entry == unicode::NOT_SPACE_CONST {
            ws
        } else if entry > 0 {
            cat == entry - 1
        } else {
            cat == -1 - entry
        }
    };
    let mut i = 0usize;
    while i < cats.len() {
        let cur = cats[i];
        if cur == 0 {

            let mut pos = i + 1;
            let positive = cats[pos] > 0;
            let mut result = !positive;
            loop {
                let hit = entry_hit(cats[pos]);
                if positive {
                    result |= hit;
                } else {
                    result &= !hit;
                }
                pos += 1;
                if cats[pos] == 0 {
                    break;
                }
            }
            if result {
                return true;
            }
            i = pos + 1;
        } else if cur > 0 {
            if entry_hit(cur) {
                return true;
            }
            i += 1;
        } else {
            if !entry_hit(cur) {
                return true;
            }
            i += 1;
        }
    }
    false
}
pub fn invert_ranges(ranges: &[(u16, u16)]) -> Vec<(u16, u16)> {
    let mut out: Vec<(u16, u16)> = Vec::new();
    let mut next: u32 = 0;
    for &(a, b) in ranges {
        if next < a as u32 {
            out.push((next as u16, (a - 1) as u16));
        }
        next = u32::from(b) + 1;
        if next > 0xFFFF {
            return out;
        }
    }
    if next <= 0xFFFF {
        out.push((next as u16, 0xFFFF));
    }
    out
}
