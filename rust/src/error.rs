use crate::source::Source;
use std::rc::Rc;
pub fn nl() -> &'static str {
    if cfg!(windows) {
        "\r\n"
    } else {
        "\n"
    }
}
#[derive(Debug, Clone)]
pub struct Panic {
    pub name: String,
    pub message: String,
}
impl Panic {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            name: "panic".to_string(),
            message: message.into(),
        }
    }

    pub fn named(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            message: message.into(),
        }
    }
    pub fn display(&self) -> String {
        if self.name == "panic" {
            format!("{}panic : {}", nl(), self.message)
        } else {
            format!("{}: {}", self.name, self.message)
        }
    }
}
#[derive(Debug, Clone)]
pub struct LangError {
    pub start: usize,
    pub length: usize,
    pub message: String,
    pub header: String,
    pub source: Option<Rc<Source>>,
}
impl LangError {
    pub fn new(start: usize, length: usize, message: impl Into<String>) -> Self {
        Self {
            start,
            length,
            message: message.into(),
            header: "error".to_string(),
            source: None,
        }
    }
    pub fn with_source(mut self, source: Rc<Source>) -> Self {
        self.source = Some(source);
        self
    }
    pub fn with_header(mut self, header: impl Into<String>) -> Self {
        self.header = header.into();
        self
    }

    pub fn display(&self, source: &Source) -> String {
        let source: &Source = self.source.as_ref().map(|s| s.as_ref()).unwrap_or(source);
        let line_text = source.get_line_text(self.start);
        let line_start = source.get_line_start(self.start);

        let column = self.start as i64 - line_start as i64;
        let indent = line_text
            .chars()
            .take_while(|c| c.is_whitespace())
            .count();
        let display: String = line_text.chars().skip(indent).collect();
        let column = (column - indent as i64).max(0) as usize;
        let caret_len = self.length.max(1);

        format!(
            "{0}<{1}>, line [{2}], column [{3}]{0}    {4}{0}    {5}{6}{0}{7}: {8}",
            nl(),
            source.name(),
            source.get_line(self.start),
            source.get_column(self.start),
            display,
            " ".repeat(column),
            "^".repeat(caret_len),
            self.header,
            self.message
        )
    }
}
