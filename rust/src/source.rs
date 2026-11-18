use std::collections::HashSet;
use std::path::Path;
#[derive(Debug, Clone)]
pub struct Source {
    pub path: String,
    pub code: Vec<char>,
    pub ignored_new_lines: HashSet<usize>,
    /// Char offset of every line start; index 0 is line 1. Built once at
    /// construction (after CRLF normalisation) so `get_line` / `get_column`
    /// are O(log lines) instead of O(offset).
    line_starts: Vec<usize>,
}
fn line_starts_of(code: &[char]) -> Vec<usize> {
    let mut starts = vec![0];
    for (i, ch) in code.iter().enumerate() {
        if *ch == '\n' {
            starts.push(i + 1);
        }
    }
    starts
}
impl Source {
    pub fn new(path: impl Into<String>, code: &str) -> Self {

        let code = code.strip_prefix('\u{feff}').unwrap_or(code);
        let normalized = code.replace("\r\n", "\n").replace('\r', "\n");
        let code: Vec<char> = normalized.chars().collect();
        Self {
            path: path.into(),
            line_starts: line_starts_of(&code),
            code,
            ignored_new_lines: HashSet::new(),
        }
    }
    pub fn name(&self) -> String {
        Path::new(&self.path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default()
    }
    /// Span lookup must stay total: a crafted `.unbc` can carry spans that
    /// were checked against the embedded source but not against this exact
    /// `Source` instance, and a miss must never panic the VM.
    pub fn span(&self, start: usize, length: usize) -> String {
        let start = start.min(self.code.len());
        let end = start.saturating_add(length).min(self.code.len());
        self.code[start..end].iter().collect()
    }
    pub fn get_line(&self, position: usize) -> usize {
        let position = position.min(self.code.len());
        match self.line_starts.binary_search(&position) {
            Ok(i) => i + 1,
            Err(i) => i,
        }
    }
    pub fn get_column(&self, position: usize) -> usize {
        let position = position.min(self.code.len());
        let line = self.get_line(position);
        position - self.line_starts[line - 1] + 1
    }
    pub fn get_line_start(&self, position: usize) -> usize {
        let position = position.min(self.code.len());
        self.line_starts[self.get_line(position) - 1]
    }
    pub fn get_line_text(&self, position: usize) -> String {
        let mut start = self.get_line_start(position);
        let mut out = String::new();
        loop {
            let mut end = start;
            while end < self.code.len() && self.code[end] != '\n' {
                end += 1;
            }
            out.extend(&self.code[start..end]);
            if end >= self.code.len() || !self.ignored_new_lines.contains(&end) {
                break;
            }
            start = end + 1;
            while start < self.code.len() && (self.code[start] == ' ' || self.code[start] == '\t') {
                start += 1;
            }
        }
        out
    }
}
