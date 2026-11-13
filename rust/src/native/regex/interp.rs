use crate::native::regex::opts::*;
use crate::native::regex::unicode;
use super::writer::Code;
#[derive(Debug, Clone)]
pub struct MatchData {
    pub capsize: usize,
    pub captures: Vec<Vec<(i32, i32)>>,
    pub capcount: Vec<i32>,
}
pub struct Engine<'a> {
    code: &'a Code,
    input: &'a [char],
    len: usize,
    pos: i64,
    right_to_left: bool,
    stack: Vec<i32>,
    sp: usize, 
    track: Vec<i32>,
    track_pos: usize, 
    crawl: Vec<i32>,
    crawl_pos: usize, 
    matchcount: Vec<i32>,
    matches: Vec<Vec<(i32, i32)>>,
    operator: i32,
    codepos: usize,
    caps: usize,
}
impl<'a> Engine<'a> {
    pub fn new(code: &'a Code, input: &'a [char], caps: usize) -> Self {
        const CAP: usize = 32;
        Engine {
            code,
            input,
            len: input.len(),
            pos: 0,
            right_to_left: false,
            stack: vec![0; CAP],
            sp: CAP,
            track: vec![0; CAP],
            track_pos: CAP,
            crawl: vec![0; 8],
            crawl_pos: 8,
            matchcount: vec![0; caps],
            matches: vec![Vec::new(); caps],
            operator: 0,
            codepos: 0,
            caps,
        }
    }
    fn ensure_storage(&mut self) {
        if self.sp < 4 {
            let old_len = self.stack.len();
            self.stack.resize(old_len * 2, 0);
            self.stack.copy_within(0..old_len, old_len);
            self.sp += old_len;
        }
        if self.track_pos < 4 {
            let old_len = self.track.len();
            self.track.resize(old_len * 2, 0);
            self.track.copy_within(0..old_len, old_len);
            self.track_pos += old_len;
        }
    }
    #[inline]
    fn operand(&self, i: usize) -> i32 {
        self.code.codes[self.codepos + i + 1]
    }
    fn advance(&mut self, i: usize) {
        self.codepos += i + 1;
        self.set_operator(self.code.codes[self.codepos]);
        if std::env::var_os("REGEX_DEBUG").is_some() {
            eprintln!("advance op={} pos={} codepos={} track={} stack={}", self.operator, self.pos, self.codepos, self.track_pos, self.sp);
        }
    }
    fn goto(&mut self, newpos: usize) {
        self.ensure_storage();
        self.codepos = newpos;
        self.set_operator(self.code.codes[newpos]);
    }
    fn set_operator(&mut self, op: i32) {
        self.operator = op & !OP_RTL;
        self.right_to_left = (op & OP_RTL) != 0;
    }
    #[inline]
    fn track_pos(&self) -> i32 {
        (self.track.len() - self.track_pos) as i32
    }
    fn track_to(&mut self, newpos: i32) {
        self.track_pos = self.track.len() - newpos as usize;
    }
    fn track_push(&mut self, i1: i32) {
        self.ensure_storage();
        self.track_pos -= 1;
        self.track[self.track_pos] = i1;
        self.track_pos -= 1;
        self.track[self.track_pos] = self.codepos as i32;
    }
    fn track_push2(&mut self, i1: i32, i2: i32) {
        self.ensure_storage();
        self.track_pos -= 1;
        self.track[self.track_pos] = i1;
        self.track_pos -= 1;
        self.track[self.track_pos] = i2;
        self.track_pos -= 1;
        self.track[self.track_pos] = self.codepos as i32;
    }
    fn track_push3(&mut self, i1: i32, i2: i32, i3: i32) {
        self.ensure_storage();
        self.track_pos -= 1;
        self.track[self.track_pos] = i1;
        self.track_pos -= 1;
        self.track[self.track_pos] = i2;
        self.track_pos -= 1;
        self.track[self.track_pos] = i3;
        self.track_pos -= 1;
        self.track[self.track_pos] = self.codepos as i32;
    }
    #[inline]
    fn track_push_code(&mut self) {
        self.track_pos -= 1;
        self.track[self.track_pos] = self.codepos as i32;
    }
    fn track_push2neg(&mut self, i1: i32, i2: i32) {
        self.ensure_storage();
        self.track_pos -= 1;
        self.track[self.track_pos] = i1;
        self.track_pos -= 1;
        self.track[self.track_pos] = i2;
        self.track_pos -= 1;
        self.track[self.track_pos] = -(self.codepos as i32);
    }
    fn track_push1neg(&mut self, i1: i32) {
        self.ensure_storage();
        self.track_pos -= 1;
        self.track[self.track_pos] = i1;
        self.track_pos -= 1;
        self.track[self.track_pos] = -(self.codepos as i32);
    }
    fn track_pop(&mut self, n: usize) {
        self.track_pos += n;
    }
    #[inline]
    fn track_peek(&self) -> i32 {
        self.track[self.track_pos - 1]
    }
    #[inline]
    fn track_peek_at(&self, i: usize) -> i32 {
        self.track[self.track_pos - i - 1]
    }
    fn stack_push(&mut self, i1: i32) {
        self.ensure_storage();
        self.sp -= 1;
        self.stack[self.sp] = i1;
    }
    fn stack_push2(&mut self, i1: i32, i2: i32) {
        self.stack_push(i1);
        self.stack_push(i2);
    }
    fn stack_pop(&mut self) {
        self.sp += 1;
    }
    fn stack_pop_n(&mut self, n: usize) {
        self.sp += n;
    }
    #[inline]
    fn stack_peek(&self) -> i32 {
        self.stack[self.sp - 1]
    }
    #[inline]
    fn stack_peek_at(&self, i: usize) -> i32 {
        self.stack[self.sp - i - 1]
    }
    fn crawl_push(&mut self, i: i32) {
        self.ensure_crawl();
        self.crawl_pos -= 1;
        self.crawl[self.crawl_pos] = i;
    }
    fn ensure_crawl(&mut self) {
        if self.crawl_pos < 2 {
            let old_len = self.crawl.len();
            self.crawl.resize(old_len * 2, 0);
            self.crawl.copy_within(0..old_len, old_len);
            self.crawl_pos += old_len;
        }
    }
    fn crawl_pop(&mut self) -> i32 {
        let v = self.crawl[self.crawl_pos];
        self.crawl_pos += 1;
        v
    }
    fn crawl_pos(&self) -> i32 {
        (self.crawl.len() - self.crawl_pos) as i32
    }
    fn forward_chars(&self) -> i64 {
        if self.right_to_left {
            self.pos
        } else {
            self.len as i64 - self.pos
        }
    }
    fn bump(&self) -> i64 {
        if self.right_to_left {
            -1
        } else {
            1
        }
    }
    fn forward_char_next(&mut self) -> char {
        if self.right_to_left {
            self.pos -= 1;
            self.input[self.pos as usize]
        } else {
            let c = self.input[self.pos as usize];
            self.pos += 1;
            c
        }
    }
    fn backward_next(&mut self) {
        self.pos += if self.right_to_left { 1 } else { -1 };
    }
    fn match_string(&mut self, s: &[char]) -> bool {
        let c = s.len() as i64;
        let mut pos;
        if !self.right_to_left {
            if (self.len as i64) - self.pos < c {
                return false;
            }
            pos = self.pos + c;
        } else {
            if self.pos < c {
                return false;
            }
            pos = self.pos;
        }
        for i in (0..c).rev() {
            if s[i as usize] != self.input[(pos - 1) as usize] {
                return false;
            }
            pos -= 1;
        }
        if !self.right_to_left {
            pos += c;
        }
        self.pos = pos;
        true
    }
    fn match_ref(&mut self, index: i32, length: i32, case_insensitive: bool) -> bool {
        let length = length as i64;
        let mut pos;
        if !self.right_to_left {
            if (self.len as i64) - self.pos < length {
                return false;
            }
            pos = self.pos + length;
        } else {
            if self.pos < length {
                return false;
            }
            pos = self.pos;
        }
        let mut cmpos = index as i64 + length;
        let mut c = length;
        if !case_insensitive {
            while c != 0 {
                c -= 1;
                cmpos -= 1;
                pos -= 1;
                if self.input[cmpos as usize] != self.input[pos as usize] {
                    return false;
                }
            }
        } else {
            while c != 0 {
                c -= 1;
                cmpos -= 1;
                pos -= 1;
                let back = self.input[cmpos as usize];
                let fwd = self.input[pos as usize];
                if !ref_char_matches(back, fwd) {
                    return false;
                }
            }
        }
        if !self.right_to_left {
            pos += length;
        }
        self.pos = pos;
        true
    }
    fn is_matched(&self, cap: i32) -> bool {
        let cap = cap as usize;
        if self.matchcount.get(cap).copied().unwrap_or(0) == 0 {
            return false;
        }
        self.matches[cap].last().map(|p| p.1).unwrap_or(-2) != -2
    }
    fn match_index(&self, cap: i32) -> i32 {
        self.matches[cap as usize].last().map(|p| p.0).unwrap_or(0)
    }
    fn match_length(&self, cap: i32) -> i32 {
        self.matches[cap as usize].last().map(|p| p.1).unwrap_or(0)
    }
    fn capture(&mut self, capnum: i32, start: i64, end: i64) {
        let (mut start, mut end) = (start, end);
        if end < start {
            std::mem::swap(&mut start, &mut end);
        }
        self.crawl_push(capnum);
        self.matches[capnum as usize].push((start as i32, (end - start) as i32));
        self.matchcount[capnum as usize] += 1;
    }
    fn transfer_capture(&mut self, capnum: i32, uncapnum: i32, start: i64, end: i64) {
        let (mut start, mut end) = (start, end);
        if end < start {
            std::mem::swap(&mut start, &mut end);
        }
        let index = self.match_index(uncapnum) as i64;
        let end2 = index + self.match_length(uncapnum) as i64;
        if start >= end2 {
            end = start;
            start = end2;
        } else if end <= index {
            start = index;
        } else {
            if end > end2 {
                end = end2;
            }
            if index > start {
                start = index;
            }
        }
        self.crawl_push(uncapnum);
        self.balance_match(uncapnum);
        if capnum != -1 {
            self.crawl_push(capnum);
            self.matches[capnum as usize].push((start as i32, (end - start) as i32));
            self.matchcount[capnum as usize] += 1;
        }
    }
    fn balance_match(&mut self, capnum: i32) {
        if let Some(last) = self.matches[capnum as usize].last_mut() {
            last.1 = -2;
        }
    }
    fn uncapture(&mut self) {
        let capnum = self.crawl_pop();
        let cap = capnum as usize;
        self.matches[cap].pop();
        self.matchcount[cap] -= 1;
    }
    fn boundary(&self) -> bool {
        let pos = self.pos as usize;
        let prev = if pos > 0 { Some(self.input[pos - 1]) } else { None };
        let cur = if pos < self.len { Some(self.input[pos]) } else { None };
        let prev_word = prev.map(unicode::is_boundary_word_char).unwrap_or(false);
        let cur_word = cur.map(unicode::is_boundary_word_char).unwrap_or(false);
        prev_word != cur_word
    }
    fn ecmaboundary(&self) -> bool {
        let pos = self.pos as usize;
        let prev = if pos > 0 { Some(self.input[pos - 1]) } else { None };
        let cur = if pos < self.len { Some(self.input[pos]) } else { None };
        let prev_word = prev.map(unicode::is_ecma_word_char).unwrap_or(false);
        let cur_word = cur.map(unicode::is_ecma_word_char).unwrap_or(false);
        prev_word != cur_word
    }
    fn backtrack(&mut self) {
        let mut newpos = self.track[self.track_pos];
        self.track_pos += 1;
        let mut back = OP_BACKTRACKING;
        if newpos < 0 {
            newpos = -newpos;
            back = OP_BACKTRACKING_SECOND;
        }
        self.ensure_storage();
        self.set_operator(self.code.codes[newpos as usize] | back);
        self.codepos = newpos as usize;
    }
    fn track_push_i_pos(&mut self) {
        let p = self.pos as i32;
        self.track_push(p);
    }
    fn track_pushi3(&mut self, i1: i32, i2: i32, i3: i32) {
        self.track_push3(i1, i2, i3);
    }
    fn undo_char_next(&mut self) {
        if self.right_to_left {
            self.pos += 1;
        } else {
            self.pos -= 1;
        }
    }
    fn loop_notone(&mut self, ch: i32, max: i32, keep: bool) {
        let len = (max as i64).min(self.forward_chars());
        let target = char::from_u32(ch as u32).unwrap_or('\u{FFFD}');
        if !self.right_to_left {
            let mut found: Option<i64> = None;
            let end = self.pos as usize + len as usize;
            for (offset, &c) in self.input[self.pos as usize..end].iter().enumerate() {
                if c == target {
                    found = Some(offset as i64);
                    break;
                }
            }
            let i: i64 = match found {
                Some(x) => {
                    self.pos += x;
                    len - x
                }
                None => {
                    self.pos += len;
                    0
                }
            };
            if len > i && keep {
                self.track_push2((len - i - 1) as i32, self.pos as i32 - 1);
            }
        } else {
            let mut i: i64 = len;
            while i > 0 {
                if self.forward_char_next() == target {
                    self.backward_next();
                    break;
                }
                i -= 1;
            }
            if len > i && keep {
                self.track_push2((len - i - 1) as i32, self.pos as i32 + 1);
            }
        }
    }
    fn loop_set(&mut self, set_index: i32, max: i32, keep: bool) {
        let len = (max as i64).min(self.forward_chars());
        let index = set_index as usize;
        let mut i: i64 = len;
        while i > 0 {
            let ch = self.forward_char_next();
            if !self.code.sets[index].char_in_class(ch) {
                self.undo_char_next();
                break;
            }
            i -= 1;
        }
        if len > i && keep {
            self.track_push2((len - i - 1) as i32, self.pos as i32 - self.bump() as i32);
        }
    }

    pub fn finish(self) -> MatchData {
        MatchData {
            capsize: self.caps,
            captures: self.matches,
            capcount: self.matchcount,
        }
    }
    pub fn run(&mut self, startpos: i64) -> bool {
        self.pos = startpos;
        if self.code.codes.is_empty() {
            return false;
        }
        self.set_operator(self.code.codes[0]);
        self.codepos = 0;
        let mut advance: i32 = -1;
        loop {
            if advance >= 0 {
                self.advance(advance as usize);
                advance = -1;
            }
            match self.operator {
                OP_STOP => {
                    return self.matchcount[0] > 0;
                }
                OP_NOTHING => {
                    self.backtrack();
                    continue;
                }
                OP_GOTO => {
                    self.goto(self.operand(0) as usize);
                    continue;
                }
                OP_TESTREF => {
                    if !self.is_matched(self.operand(0)) {
                        self.backtrack();
                        continue;
                    }
                    advance = 1;
                    continue;
                }
                OP_LAZYBRANCH => {
                    self.track_push_i_pos();
                    advance = 1;
                    continue;
                }
                OP_LAZYBRANCH_BACK => {
                    self.track_pop(1);
                    self.pos = self.track_peek() as i64;
                    self.goto(self.operand(0) as usize);
                    continue;
                }
                OP_SETMARK => {
                    self.stack_push(self.pos as i32);
                    self.track_push_code();
                    advance = 0;
                    continue;
                }
                OP_SETMARK_BACK => {
                    self.stack_pop();
                    self.backtrack();
                    continue;
                }
                OP_NULLMARK => {
                    self.stack_push(-1);
                    self.track_push_code();
                    advance = 0;
                    continue;
                }
                OP_NULLMARK_BACK => {
                    self.stack_pop();
                    self.backtrack();
                    continue;
                }
                OP_GETMARK => {
                    self.stack_pop();
                    let p = self.stack_peek();
                    self.track_push(p);
                    self.pos = p as i64;
                    advance = 0;
                    continue;
                }
                OP_GETMARK_BACK => {
                    self.track_pop(1);
                    let p = self.track_peek();
                    self.stack_push(p);
                    self.backtrack();
                    continue;
                }
                OP_CAPTUREMARK => {
                    if self.operand(1) != -1 && !self.is_matched(self.operand(1)) {
                        self.backtrack();
                        continue;
                    }
                    self.stack_pop();
                    let start = self.stack_peek();
                    if self.operand(1) != -1 {
                        self.transfer_capture(self.operand(0), self.operand(1), start as i64, self.pos);
                    } else {
                        self.capture(self.operand(0), start as i64, self.pos);
                    }
                    let p = self.stack_peek();
                    self.track_push(p);
                    advance = 2;
                    continue;
                }
                OP_CAPTUREMARK_BACK => {
                    self.track_pop(1);
                    let p = self.track_peek();
                    self.stack_push(p);
                    self.uncapture();
                    if self.operand(0) != -1 && self.operand(1) != -1 {
                        self.uncapture();
                    }
                    self.backtrack();
                    continue;
                }
                OP_BRANCHMARK => {
                    self.stack_pop();
                    let mark = self.stack_peek();
                    if self.pos != mark as i64 {
                        self.track_push2(mark, self.pos as i32);
                        self.stack_push(self.pos as i32);
                        self.goto(self.operand(0) as usize);
                        continue;
                    }
                    let mark = self.stack_peek();
                    self.track_push1neg(mark);
                    advance = 1;
                    continue;
                }
                OP_BRANCHMARK_BACK => {
                    self.track_pop(2);
                    self.stack_pop();
                    self.pos = self.track_peek_at(1) as i64;
                    let p = self.track_peek();
                    self.track_push1neg(p);
                    advance = 1;
                    continue;
                }
                OP_BRANCHMARK_BACK2 => {
                    self.track_pop(1);
                    let p = self.track_peek();
                    self.stack_push(p);
                    self.backtrack();
                    continue;
                }
                OP_LAZYBRANCHMARK => {
                    self.stack_pop();
                    let old = self.stack_peek();
                    if self.pos != old as i64 {
                        if old != -1 {
                            self.track_push2(old, self.pos as i32);
                        } else {
                            self.track_push2(self.pos as i32, self.pos as i32);
                        }
                    } else {
                        self.stack_push(old);
                        let p = self.stack_peek();
                        self.track_push1neg(p);
                    }
                    advance = 1;
                    continue;
                }
                OP_LAZYBRANCHMARK_BACK => {
                    self.track_pop(2);
                    let p1 = self.track_peek_at(1);
                    let p0 = self.track_peek();
                    self.track_push1neg(p0);
                    self.stack_push(p1);
                    self.pos = p1 as i64;
                    self.goto(self.operand(0) as usize);
                    continue;
                }
                OP_LAZYBRANCHMARK_BACK2 => {
                    self.stack_pop();
                    self.track_pop(1);
                    let p = self.track_peek();
                    self.stack_push(p);
                    self.backtrack();
                    continue;
                }
                OP_SETCOUNT => {
                    self.stack_push2(self.pos as i32, self.operand(0));
                    self.track_push_code();
                    advance = 1;
                    continue;
                }
                OP_NULLCOUNT => {
                    self.stack_push2(-1, self.operand(0));
                    self.track_push_code();
                    advance = 1;
                    continue;
                }
                OP_SETCOUNT_BACK | OP_NULLCOUNT_BACK => {
                    self.stack_pop_n(2);
                    self.backtrack();
                    continue;
                }
                OP_SETJUMP => {
                    self.stack_push(self.track_pos());
                    self.stack_push(self.crawl_pos());
                    self.track_push_code();
                    advance = 0;
                    continue;
                }
                OP_SETJUMP_BACK => {
                    self.stack_pop_n(2);
                    self.backtrack();
                    continue;
                }
                OP_BACKJUMP => {
                    self.stack_pop_n(2);
                    let tp = self.stack_peek();
                    self.track_to(tp);
                    let cp = self.stack_peek_at(1);
                    while self.crawl_pos() != cp {
                        self.uncapture();
                    }
                    self.backtrack();
                    continue;
                }
                OP_FOREJUMP => {
                    self.stack_pop_n(2);
                    let tp = self.stack_peek();
                    self.track_to(tp);
                    let cp = self.stack_peek_at(1);
                    self.track_push(cp);
                    advance = 0;
                    continue;
                }
                OP_FOREJUMP_BACK => {
                    self.track_pop(1);
                    let cp = self.track_peek();
                    while self.crawl_pos() != cp {
                        self.uncapture();
                    }
                    self.backtrack();
                    continue;
                }
                OP_BOL => {
                    let m1 = self.pos - 1;
                    if (m1 as usize) < self.len && self.input[m1 as usize] != '\n' {
                        self.backtrack();
                        continue;
                    }
                    advance = 0;
                    continue;
                }
                OP_EOL => {
                    if (self.pos as usize) < self.len && self.input[self.pos as usize] != '\n' {
                        self.backtrack();
                        continue;
                    }
                    advance = 0;
                    continue;
                }
                OP_BOUNDARY => {
                    if !self.boundary() {
                        self.backtrack();
                        continue;
                    }
                    advance = 0;
                    continue;
                }
                OP_NONBOUNDARY => {
                    if self.boundary() {
                        self.backtrack();
                        continue;
                    }
                    advance = 0;
                    continue;
                }
                OP_ECMABOUNDARY => {
                    if !self.ecmaboundary() {
                        self.backtrack();
                        continue;
                    }
                    advance = 0;
                    continue;
                }
                OP_NONECMABOUNDARY => {
                    if self.ecmaboundary() {
                        self.backtrack();
                        continue;
                    }
                    advance = 0;
                    continue;
                }
                OP_BEGINNING => {
                    if self.pos > 0 {
                        self.backtrack();
                        continue;
                    }
                    advance = 0;
                    continue;
                }
                OP_START => {
                    if self.pos != 0 {
                        self.backtrack();
                        continue;
                    }
                    advance = 0;
                    continue;
                }
                OP_ENDZ => {
                    let pos = self.pos;
                    let len = self.len as i64;
                    if pos < len - 1 || (pos < len && self.input[pos as usize] != '\n') {
                        self.backtrack();
                        continue;
                    }
                    advance = 0;
                    continue;
                }
                OP_END => {
                    if (self.pos as usize) < self.len {
                        self.backtrack();
                        continue;
                    }
                    advance = 0;
                    continue;
                }
                OP_ONE => {
                    let ch = self.operand(0);
                    let ok = self.forward_chars() >= 1 && (self.forward_char_next() as u32) == (ch as u32);
                    if !ok {
                        self.undo_char_next();
                        self.backtrack();
                        continue;
                    }
                    advance = 1;
                    continue;
                }
                OP_NOTONE => {
                    let ch = self.operand(0);
                    let ok = self.forward_chars() >= 1 && (self.forward_char_next() as u32) != (ch as u32);
                    if !ok {
                        self.undo_char_next();
                        self.backtrack();
                        continue;
                    }
                    advance = 1;
                    continue;
                }
                OP_SET => {
                    if self.forward_chars() < 1 {
                        self.backtrack();
                        continue;
                    }
                    let index = self.operand(0);
                    let ch = self.forward_char_next();
                    if !self.code.sets[index as usize].char_in_class(ch) {
                        self.undo_char_next();
                        self.backtrack();
                        continue;
                    }
                    advance = 1;
                    continue;
                }
                OP_MULTI => {
                    let index = self.operand(0);
                    let s: Vec<char> = self.code.strings[index as usize].clone();
                    if !self.match_string(&s) {
                        self.backtrack();
                        continue;
                    }
                    advance = 1;
                    continue;
                }
                OP_BACKREF | OP_BACKREF_CI => {
                    let capnum = self.operand(0);
                    if self.is_matched(capnum) {
                        let ci = (self.operator & OP_CASEINS) != 0;
                        let ok = self.match_ref(self.match_index(capnum), self.match_length(capnum), ci);
                        if !ok {
                            self.backtrack();
                            continue;
                        }
                    } else {

                        self.backtrack();
                        continue;
                    }
                    advance = 1;
                    continue;
                }
                OP_ONEREP => {
                    let c = self.operand(1);
                    if self.forward_chars() < c as i64 {
                        self.backtrack();
                        continue;
                    }
                    let ch = self.operand(0);
                    let mut ok = true;
                    for _ in 0..c {
                        if (self.forward_char_next() as u32) != (ch as u32) {
                            ok = false;
                            break;
                        }
                    }
                    if !ok {
                        self.backtrack();
                        continue;
                    }
                    advance = 2;
                    continue;
                }
                OP_NOTONEREP => {
                    let c = self.operand(1);
                    if self.forward_chars() < c as i64 {
                        self.backtrack();
                        continue;
                    }
                    let ch = self.operand(0);
                    let mut ok = true;
                    for _ in 0..c {
                        if (self.forward_char_next() as u32) == (ch as u32) {
                            ok = false;
                            break;
                        }
                    }
                    if !ok {
                        self.backtrack();
                        continue;
                    }
                    advance = 2;
                    continue;
                }
                OP_SETREP => {
                    let c = self.operand(1);
                    if self.forward_chars() < c as i64 {
                        self.backtrack();
                        continue;
                    }
                    let index = self.operand(0);
                    let set = &self.code.sets[index as usize];
                    let mut ok = true;
                    for _ in 0..c {
                        let ch = self.forward_char_next();
                        if !set.char_in_class(ch) {
                            ok = false;
                            break;
                        }
                    }
                    if !ok {
                        self.backtrack();
                        continue;
                    }
                    advance = 2;
                    continue;
                }
                OP_ONELLOOP | OP_ONELLOOP_ATOMIC => {
                    let keep = self.operator == OP_ONELLOOP;
                    let len = (self.operand(1) as i64).min(self.forward_chars());
                    let ch = self.operand(0);
                    let mut i = len;
                    while i > 0 {
                        if (self.forward_char_next() as u32) != (ch as u32) {
                            self.undo_char_next();
                            break;
                        }
                        i -= 1;
                    }
                    if len > i && keep {
                        self.track_push2((len - i - 1) as i32, self.pos as i32 - self.bump() as i32);
                    }
                    advance = 2;
                    continue;
                }
                OP_NOTONELLOOP | OP_NOTONELLOOP_ATOMIC => {
                    let keep = self.operator == OP_NOTONELLOOP;
                    self.loop_notone(self.operand(0), self.operand(1), keep);
                    advance = 2;
                    continue;
                }
                OP_SETLOOP | OP_SETLOOP_ATOMIC => {
                    let keep = self.operator == OP_SETLOOP;
                    self.loop_set(self.operand(0), self.operand(1), keep);
                    advance = 2;
                    continue;
                }
                OP_ONELLOOP_BACK
                | OP_NOTONELLOOP_BACK
                | OP_SETLOOP_BACK => {
                    self.track_pop(2);
                    let i = self.track_peek();
                    let p = self.track_peek_at(1);
                    self.pos = p as i64;
                    if i > 0 {
                        self.track_push2(i - 1, p - self.bump() as i32);
                    }
                    advance = 2;
                    continue;
                }
                OP_ONELAZY | OP_NOTONELAZY | OP_SETLAZY => {
                    let c = (self.operand(1) as i64).min(self.forward_chars());
                    if c > 0 {
                        self.track_push2(c as i32 - 1, self.pos as i32);
                    }
                    advance = 2;
                    continue;
                }
                OP_ONELAZY_BACK => {
                    self.track_pop(2);
                    let p = self.track_peek_at(1);
                    self.pos = p as i64;
                    let ch = self.operand(0);
                    if (self.forward_char_next() as u32) != (ch as u32) {
                        self.backtrack();
                        continue;
                    }
                    let i = self.track_peek();
                    if i > 0 {
                        self.track_push2(i - 1, p + self.bump() as i32);
                    }
                    advance = 2;
                    continue;
                }
                OP_NOTONELAZY_BACK => {
                    self.track_pop(2);
                    let p = self.track_peek_at(1);
                    self.pos = p as i64;
                    let ch = self.operand(0);
                    if (self.forward_char_next() as u32) == (ch as u32) {
                        self.backtrack();
                        continue;
                    }
                    let i = self.track_peek();
                    if i > 0 {
                        self.track_push2(i - 1, p + self.bump() as i32);
                    }
                    advance = 2;
                    continue;
                }
                OP_SETLAZY_BACK => {
                    self.track_pop(2);
                    let p = self.track_peek_at(1);
                    self.pos = p as i64;
                    let index = self.operand(0);
                    let ch = self.forward_char_next();
                    if !self.code.sets[index as usize].char_in_class(ch) {
                        self.backtrack();
                        continue;
                    }
                    let i = self.track_peek();
                    if i > 0 {
                        self.track_push2(i - 1, p + self.bump() as i32);
                    }
                    advance = 2;
                    continue;
                }
                OP_BRANCHCOUNT => {
                    self.stack_pop_n(2);
                    let mark = self.stack_peek();
                    let count = self.stack_peek_at(1);
                    let matched = self.pos - mark as i64;
                    if (count >= self.operand(1)) || (matched == 0 && count >= 0) {
                        self.track_push2neg(mark, count);
                        advance = 2;
                        continue;
                    }
                    self.track_push(mark);
                    self.stack_push2(self.pos as i32, count + 1);
                    self.goto(self.operand(0) as usize);
                    continue;
                }
                OP_BRANCHCOUNT_BACK => {
                    self.track_pop(1);
                    self.stack_pop_n(2);
                    if self.stack_peek_at(1) > 0 {
                        self.pos = self.stack_peek() as i64;
                        let p = self.track_peek();
                        self.track_push2neg(p, self.stack_peek_at(1) - 1);
                        advance = 2;
                        continue;
                    }
                    let p = self.track_peek();
                    let c = self.stack_peek_at(1) - 1;
                    self.stack_push2(p, c);
                    self.backtrack();
                    continue;
                }
                OP_BRANCHCOUNT_BACK2 => {
                    self.track_pop(2);
                    let p = self.track_peek();
                    let p1 = self.track_peek_at(1);
                    self.stack_push2(p, p1);
                    self.backtrack();
                    continue;
                }
                OP_LAZYBRANCHCOUNT => {
                    self.stack_pop_n(2);
                    let mark = self.stack_peek();
                    let count = self.stack_peek_at(1);
                    if count < 0 {
                        self.track_push1neg(mark);
                        self.stack_push2(self.pos as i32, count + 1);
                        self.goto(self.operand(0) as usize);
                        continue;
                    }
                    self.track_pushi3(mark, count, self.pos as i32);
                    advance = 2;
                    continue;
                }
                OP_LAZYBRANCHCOUNT_BACK => {
                    self.track_pop(3);
                    let mark = self.track_peek();
                    let pos = self.track_peek_at(2);
                    if self.track_peek_at(1) < self.operand(1) && (pos as i64) != (mark as i64) {
                        self.pos = pos as i64;
                        let c = self.track_peek_at(1) + 1;
                        self.stack_push2(pos, c);
                        self.track_push1neg(mark);
                        self.goto(self.operand(0) as usize);
                        continue;
                    }
                    let m = self.track_peek();
                    let c = self.track_peek_at(1);
                    self.stack_push2(m, c);
                    self.backtrack();
                    continue;
                }
                OP_LAZYBRANCHCOUNT_BACK2 => {
                    self.track_pop(1);
                    self.stack_pop_n(2);
                    let p = self.track_peek();
                    let c = self.stack_peek_at(1) - 1;
                    self.stack_push2(p, c);
                    self.backtrack();
                    continue;
                }
                OP_UPDATEBUMPALONG => {
                    advance = 0;
                    continue;
                }
                _ => {
                    self.backtrack();
                    continue;
                }
            }
        }
    }
}
fn ref_char_matches(back: char, fwd: char) -> bool {
    if back == fwd {
        return true;
    }
    unicode::case_equivalents(back).contains(&fwd)
}
