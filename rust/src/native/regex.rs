//! a small backtracking regular expression engine.
//!
//! the reference uses `System.Text.RegularExpressions`, the Rust runtime has no
//! regex crate, so the engine here covers the syntax the language uses: the literals,
//! `.`, the character classes, the escapes (`\d`, `\w`, `\s` and their negations), the
//! groups, the alternatives, the quantifiers (`*`, `+`, `?`, `{n,m}`) and the anchors
//! (`^`, `$`).

/// one node of the parsed pattern.
#[derive(Debug, Clone)]
pub enum Node {
    /// a literal character
    Char(char),
    /// `.` — every character but the line break
    Any,
    /// `\d` and `\D`
    Digit(bool),
    /// `\w` and `\W`
    Word(bool),
    /// `\s` and `\S`
    Space(bool),
    /// `[...]` — the items of a character class
    Class {
        negated: bool,
        ranges: Vec<(char, char)>,
    },
    /// a group, `index` is the capture number
    Group {
        index: Option<usize>,
        nodes: Vec<Node>,
    },
    /// `a|b`
    Alt(Vec<Vec<Node>>),
    /// a repetition
    Repeat {
        node: Box<Node>,
        min: usize,
        max: Option<usize>,
        greedy: bool,
    },
    /// `^`
    Start,
    /// `$`
    End,
}

/// the parsed pattern.
#[derive(Debug, Clone)]
pub struct Regex {
    nodes: Vec<Node>,
    /// the number of the capture groups
    pub groups: usize,
}

/// the result of a match.
#[derive(Debug, Clone)]
pub struct Match {
    /// the start and the end of the whole match
    pub start: usize,
    pub end: usize,
    /// the start and the end of every capture group
    pub captures: Vec<Option<(usize, usize)>>,
}

impl Regex {
    /// parses the pattern, the error is the .NET style message.
    pub fn new(pattern: &str) -> Result<Self, String> {
        let chars: Vec<char> = pattern.chars().collect();
        let mut parser = Parser {
            chars: &chars,
            pos: 0,
            groups: 0,
        };

        let nodes = parser.parse_alt()?;
        if parser.pos != chars.len() {
            return Err(format!("Unrecognized pattern at offset {}.", parser.pos));
        }

        Ok(Self {
            nodes,
            groups: parser.groups,
        })
    }

    /// the first match at or after `from`.
    pub fn find_at(&self, text: &[char], from: usize) -> Option<Match> {
        for start in from..=text.len() {
            let mut captures: Vec<Option<(usize, usize)>> = vec![None; self.groups];
            if let Some(end) = match_seq(&self.nodes, text, start, &mut captures) {
                return Some(Match {
                    start,
                    end,
                    captures,
                });
            }
        }

        None
    }

    /// every match of the pattern, the empty matches advance by one character.
    pub fn find_all(&self, text: &[char]) -> Vec<Match> {
        let mut matches = Vec::new();
        let mut pos = 0usize;

        while pos <= text.len() {
            match self.find_at(text, pos) {
                Some(found) => {
                    let empty = found.end == found.start;
                    let end = found.end;
                    matches.push(found);
                    pos = if empty { end + 1 } else { end };
                }
                None => break,
            }
        }

        matches
    }
}

/// the parser of the pattern.
struct Parser<'a> {
    chars: &'a [char],
    pos: usize,
    groups: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    /// `a|b`
    fn parse_alt(&mut self) -> Result<Vec<Node>, String> {
        let mut branches = vec![self.parse_concat()?];

        while self.peek() == Some('|') {
            self.pos += 1;
            branches.push(self.parse_concat()?);
        }

        if branches.len() == 1 {
            return Ok(branches.pop().expect("one branch"));
        }

        Ok(vec![Node::Alt(branches)])
    }

    /// the sequence of the terms.
    fn parse_concat(&mut self) -> Result<Vec<Node>, String> {
        let mut nodes = Vec::new();

        while let Some(c) = self.peek() {
            if c == '|' || c == ')' {
                break;
            }

            let node = self.parse_repeat()?;
            nodes.push(node);
        }

        Ok(nodes)
    }

    /// a term with its quantifier.
    fn parse_repeat(&mut self) -> Result<Node, String> {
        let node = self.parse_atom()?;

        let (min, max) = match self.peek() {
            Some('*') => {
                self.pos += 1;
                (0, None)
            }
            Some('+') => {
                self.pos += 1;
                (1, None)
            }
            Some('?') => {
                self.pos += 1;
                (0, Some(1))
            }
            Some('{') => match self.parse_bounds()? {
                Some(bounds) => bounds,
                None => return Ok(node),
            },
            _ => return Ok(node),
        };

        let greedy = if self.peek() == Some('?') {
            self.pos += 1;
            false
        } else {
            true
        };

        Ok(Node::Repeat {
            node: Box::new(node),
            min,
            max,
            greedy,
        })
    }

    /// the `{n,m}` quantifier, `None` when the braces are a literal.
    fn parse_bounds(&mut self) -> Result<Option<(usize, Option<usize>)>, String> {
        let start = self.pos;
        self.pos += 1;

        let mut min_text = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                min_text.push(c);
                self.pos += 1;
            } else {
                break;
            }
        }

        if min_text.is_empty() {
            self.pos = start;
            return Ok(None);
        }

        let min: usize = min_text.parse().unwrap_or(0);

        match self.peek() {
            Some('}') => {
                self.pos += 1;
                Ok(Some((min, Some(min))))
            }
            Some(',') => {
                self.pos += 1;
                let mut max_text = String::new();
                while let Some(c) = self.peek() {
                    if c.is_ascii_digit() {
                        max_text.push(c);
                        self.pos += 1;
                    } else {
                        break;
                    }
                }

                if self.peek() != Some('}') {
                    self.pos = start;
                    return Ok(None);
                }
                self.pos += 1;

                if max_text.is_empty() {
                    Ok(Some((min, None)))
                } else {
                    Ok(Some((min, Some(max_text.parse().unwrap_or(min)))))
                }
            }
            _ => {
                self.pos = start;
                Ok(None)
            }
        }
    }

    /// a single term.
    fn parse_atom(&mut self) -> Result<Node, String> {
        let Some(c) = self.peek() else {
            return Err("Unexpected end of pattern.".to_string());
        };

        match c {
            '(' => {
                self.pos += 1;

                // the named and the non capturing groups
                let mut index = Some(self.groups);
                if self.peek() == Some('?') {
                    self.pos += 1;
                    match self.peek() {
                        Some(':') => {
                            self.pos += 1;
                            index = None;
                        }
                        Some('<') => {
                            // the named group `(?<name>...)`
                            self.pos += 1;
                            while let Some(c) = self.peek() {
                                self.pos += 1;
                                if c == '>' {
                                    break;
                                }
                            }
                        }
                        _ => return Err("Unrecognized grouping construct.".to_string()),
                    }
                }

                if index.is_some() {
                    self.groups += 1;
                }

                let nodes = self.parse_alt()?;

                if self.peek() != Some(')') {
                    return Err("Unbalanced parenthesis at offset 0.".to_string());
                }
                self.pos += 1;

                Ok(Node::Group { index, nodes })
            }
            '[' => {
                self.pos += 1;
                let negated = self.peek() == Some('^');
                if negated {
                    self.pos += 1;
                }

                let mut ranges: Vec<(char, char)> = Vec::new();

                loop {
                    let Some(c) = self.peek() else {
                        return Err("Unterminated [] set.".to_string());
                    };

                    if c == ']' {
                        self.pos += 1;
                        break;
                    }

                    self.pos += 1;

                    let start = if c == '\\' {
                        let Some(escaped) = self.peek() else {
                            return Err("Trailing backslash.".to_string());
                        };
                        self.pos += 1;
                        match escaped {
                            'd' => {
                                ranges.push(('0', '9'));
                                continue;
                            }
                            'w' => {
                                ranges.push(('a', 'z'));
                                ranges.push(('A', 'Z'));
                                ranges.push(('0', '9'));
                                ranges.push(('_', '_'));
                                continue;
                            }
                            's' => {
                                ranges.push((' ', ' '));
                                ranges.push(('\t', '\t'));
                                ranges.push(('\n', '\n'));
                                ranges.push(('\r', '\r'));
                                continue;
                            }
                            other => other,
                        }
                    } else {
                        c
                    };

                    // a range like `a-z`
                    if self.peek() == Some('-') && self.at(1) != Some(']') && self.at(1).is_some() {
                        self.pos += 1;
                        let Some(end) = self.peek() else {
                            return Err("Unterminated [] set.".to_string());
                        };
                        self.pos += 1;
                        ranges.push((start, end));
                    } else {
                        ranges.push((start, start));
                    }
                }

                Ok(Node::Class { negated, ranges })
            }
            '.' => {
                self.pos += 1;
                Ok(Node::Any)
            }
            '^' => {
                self.pos += 1;
                Ok(Node::Start)
            }
            '$' => {
                self.pos += 1;
                Ok(Node::End)
            }
            '\\' => {
                self.pos += 1;
                let Some(escaped) = self.peek() else {
                    return Err("Trailing backslash.".to_string());
                };
                self.pos += 1;

                Ok(match escaped {
                    'd' => Node::Digit(false),
                    'D' => Node::Digit(true),
                    'w' => Node::Word(false),
                    'W' => Node::Word(true),
                    's' => Node::Space(false),
                    'S' => Node::Space(true),
                    'n' => Node::Char('\n'),
                    'r' => Node::Char('\r'),
                    't' => Node::Char('\t'),
                    other => Node::Char(other),
                })
            }
            other => {
                self.pos += 1;
                Ok(Node::Char(other))
            }
        }
    }
}

/// the capture slots of a match.
type Captures = Vec<Option<(usize, usize)>>;

/// matches the nodes of a sequence at `pos`, the end position of the match.
fn match_seq(
    nodes: &[Node],
    text: &[char],
    pos: usize,
    captures: &mut Captures,
) -> Option<usize> {
    let Some((first, rest)) = nodes.split_first() else {
        return Some(pos);
    };

    match first {
        Node::Repeat {
            node,
            min,
            max,
            greedy,
        } => {
            // the end positions of the repetitions, the greedy order tries the longest
            // repetition first
            let mut ends: Vec<usize> = vec![pos];
            let mut current = pos;
            let mut count = 0usize;

            while max.map(|m| count < m).unwrap_or(true) {
                let mut trial = captures.clone();
                match match_single(node, text, current, &mut trial) {
                    Some(next) if next != current || count < *min => {
                        current = next;
                        count += 1;
                        ends.push(current);
                    }
                    _ => break,
                }
            }

            let order: Vec<usize> = if *greedy {
                (0..ends.len()).rev().collect()
            } else {
                (0..ends.len()).collect()
            };

            for index in order {
                if index < *min {
                    continue;
                }

                let mut trial = captures.clone();
                if let Some(end) = match_seq(rest, text, ends[index], &mut trial) {
                    *captures = trial;
                    return Some(end);
                }
            }

            None
        }
        other => {
            let mut trial = captures.clone();
            let Some(next) = match_single(other, text, pos, &mut trial) else {
                return None;
            };

            let Some(end) = match_seq(rest, text, next, &mut trial) else {
                return None;
            };

            *captures = trial;
            Some(end)
        }
    }
}

/// matches a single node at `pos`.
fn match_single(
    node: &Node,
    text: &[char],
    pos: usize,
    captures: &mut Captures,
) -> Option<usize> {
    match node {
        Node::Char(c) => {
            if text.get(pos) == Some(c) {
                Some(pos + 1)
            } else {
                None
            }
        }
        Node::Any => match text.get(pos) {
            Some(c) if *c != '\n' => Some(pos + 1),
            _ => None,
        },
        Node::Digit(negated) => {
            let hit = matches!(text.get(pos), Some(c) if c.is_ascii_digit());
            if hit != *negated {
                Some(pos + 1)
            } else {
                None
            }
        }
        Node::Word(negated) => {
            let hit = matches!(text.get(pos), Some(c) if c.is_alphanumeric() || *c == '_');
            if hit != *negated {
                Some(pos + 1)
            } else {
                None
            }
        }
        Node::Space(negated) => {
            let hit = matches!(text.get(pos), Some(c) if c.is_whitespace());
            if hit != *negated {
                Some(pos + 1)
            } else {
                None
            }
        }
        Node::Class { negated, ranges } => {
            let hit = matches!(text.get(pos), Some(c) if ranges.iter().any(|(a, b)| *c >= *a && *c <= *b));
            if hit != *negated {
                Some(pos + 1)
            } else {
                None
            }
        }
        Node::Group { index, nodes } => {
            let start = pos;
            let end = match_seq(nodes, text, pos, captures)?;

            if let Some(index) = index {
                if *index < captures.len() {
                    captures[*index] = Some((start, end));
                }
            }

            Some(end)
        }
        Node::Alt(branches) => {
            for branch in branches {
                let mut trial = captures.clone();
                if let Some(end) = match_seq(branch, text, pos, &mut trial) {
                    *captures = trial;
                    return Some(end);
                }
            }
            None
        }
        Node::Repeat { .. } => {
            // a repetition outside a sequence, the greedy match takes the longest one
            let mut ends: Vec<usize> = vec![pos];
            let mut current = pos;

            loop {
                let mut trial = captures.clone();
                match match_single(node, text, current, &mut trial) {
                    Some(next) if next != current => {
                        current = next;
                        ends.push(current);
                    }
                    _ => break,
                }
            }

            ends.last().copied()
        }
        Node::Start => {
            if pos == 0 {
                Some(pos)
            } else {
                None
            }
        }
        Node::End => {
            if pos == text.len() {
                Some(pos)
            } else {
                None
            }
        }
    }
}

/// `Regex.Escape` — the characters that have a meaning in a pattern.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());

    for c in text.chars() {
        if matches!(
            c,
            '\\' | '*' | '+' | '?' | '|' | '{' | '[' | '(' | ')' | '^' | '$' | '.' | '#'
        ) || c.is_whitespace()
        {
            out.push('\\');
        }
        out.push(c);
    }

    out
}
