use std::collections::HashSet;
use std::path::Path;
#[derive(Debug, Clone)]
pub struct Source {
    pub path: String,
    pub code: Vec<char>,
    pub ignored_new_lines: HashSet<usize>,
}
impl Source {
    pub fn new(path: impl Into<String>, code: &str) -> Self {

        let code = code.strip_prefix('\u{feff}').unwrap_or(code);
        let normalized = code.replace("\r\n", "\n").replace('\r', "\n");
        Self {
            path: path.into(),
            code: normalized.chars().collect(),
            ignored_new_lines: HashSet::new(),
        }
    }
    pub fn name(&self) -> String {
        Path::new(&self.path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default()
    }
    pub fn full_name(&self) -> String {
        Path::new(&self.path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default()
    }
    pub fn len(&self) -> usize {
        self.code.len()
    }
    pub fn span(&self, start: usize, length: usize) -> String {
        self.code[start..start + length].iter().collect()
    }
    pub fn get_line(&self, position: usize) -> usize {
        let mut line = 1;
        for i in 0..position.min(self.code.len()) {
            if self.code[i] == '\n' {
                line += 1;
            }
        }
        line
    }
    pub fn get_column(&self, position: usize) -> usize {
        let mut line_start = 0;
        for i in 0..position.min(self.code.len()) {
            if self.code[i] == '\n' {
                line_start = i + 1;
            }
        }
        position - line_start + 1
    }
    pub fn get_line_start(&self, position: usize) -> usize {
        let mut position = position.min(self.code.len());
        while position > 0 && self.code[position - 1] != '\n' {
            position -= 1;
        }
        position
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
