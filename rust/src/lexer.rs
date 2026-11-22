use crate::error::LangError;
use crate::source::{Source, TAB_WIDTH};
use crate::token::{Token, TokenType};

/// First character of an identifier. Kept identical to the original inline
/// predicate (`ASCII letter` or `_`) so contextual `b"..."` / `type` paths are
/// unaffected.
pub fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

/// Continuation character of an identifier: ASCII letter, digit or `_`.
pub fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Multi-character operators in the exact order the original `match_str`
/// chain checked them: all 3-character compound assignments first, then the
/// 2-character forms. Order is load-bearing (longest match wins).
const MULTI_CHAR_OPERATORS: &[(&str, TokenType, usize)] = &[
    ("??=", TokenType::DoubleQuestionAssign, 3),
    ("<<=", TokenType::LeftShiftAssign, 3),
    (">>=", TokenType::RightShiftAssign, 3),
    ("//=", TokenType::DoubleSlashAssign, 3),
    ("**=", TokenType::DoubleAsteriskAssign, 3),
    ("??", TokenType::DoubleQuestion, 2),
    ("?.", TokenType::QuestionDot, 2),
    ("+=", TokenType::PlusAssign, 2),
    ("-=", TokenType::MinusAssign, 2),
    ("*=", TokenType::AsteriskAssign, 2),
    ("/=", TokenType::SlashAssign, 2),
    ("%=", TokenType::PercentAssign, 2),
    ("&=", TokenType::BAndAssign, 2),
    ("|=", TokenType::BOrAssign, 2),
    ("^=", TokenType::BXorAssign, 2),
    ("==", TokenType::Equal, 2),
    ("!=", TokenType::Unequal, 2),
    ("<=", TokenType::LessOrEqual, 2),
    (">=", TokenType::GreaterOrEqual, 2),
    ("<<", TokenType::LeftShift, 2),
    (">>", TokenType::RightShift, 2),
    ("//", TokenType::DoubleSlash, 2),
    ("**", TokenType::DoubleAsterisk, 2),
    ("->", TokenType::Return, 2),
];

pub fn lex(source: &mut Source) -> Result<Vec<Token>, LangError> {
    let mut lexer = Lexer::new(source);
    lexer.lex()
}
pub struct Lexer<'a> {
    index: usize,
    source: &'a mut Source,
    indents: Vec<usize>,
    line_start: bool,
    group_depth: i32,
    last_underscore: bool,
}
impl<'a> Lexer<'a> {
    pub fn new(source: &'a mut Source) -> Self {
        Self {
            index: 0,
            source,
            indents: vec![0],
            line_start: true,
            group_depth: 0,
            last_underscore: false,
        }
    }
    fn eof(&self) -> bool {
        self.index >= self.source.code.len()
    }
    fn out_of_range(&self, pos: usize) -> bool {
        self.source.code.len() <= pos
    }
    fn peek(&self) -> char {
        if self.eof() {
            '\0'
        } else {
            self.source.code[self.index]
        }
    }
    fn peek_at(&self, offset: usize) -> char {
        let i = self.index + offset;
        if i >= self.source.code.len() {
            '\0'
        } else {
            self.source.code[i]
        }
    }
    fn peek_is(&self, c: char) -> bool {
        !self.eof() && self.source.code[self.index] == c
    }
    fn consume(&mut self, c: char) -> bool {
        if self.eof() || self.source.code[self.index] != c {
            false
        } else {
            self.index += 1;
            true
        }
    }
    fn consume_pred(&mut self, pred: impl Fn(char) -> bool) -> bool {
        if self.eof() || !pred(self.source.code[self.index]) {
            false
        } else {
            self.index += 1;
            true
        }
    }
    fn match_str(&self, text: &str) -> bool {
        let chars: Vec<char> = text.chars().collect();
        if self.out_of_range(self.index + chars.len()) {
            return false;
        }
        self.source.code[self.index..self.index + chars.len()] == chars[..]
    }
    fn tok(&mut self, start: usize, typ: TokenType, length: usize) -> Token {
        self.index += length;
        Token::new(start, length, typ)
    }
    fn error(&self, start: usize, message: &str) -> LangError {
        LangError::new(start, self.index - start, message)
    }
    pub fn lex(&mut self) -> Result<Vec<Token>, LangError> {
        let mut tokens: Vec<Token> = Vec::new();
        while !self.eof() {
            if self.line_start {
                let mut indent = 0;
                let start = self.index;
                loop {
                    if self.consume(' ') {
                        indent += 1;
                    } else if self.consume('\t') {
                        indent += TAB_WIDTH;
                    } else {
                        break;
                    }
                }
                if self.peek_is('\n') {
                    self.index += 1;
                    tokens.push(Token::new(start, 1, TokenType::NewLine));
                    self.line_start = true;
                    continue;
                }
                if self.peek_is('#') {
                    self.line_start = false;
                    continue;
                }
                let current = *self.indents.last().unwrap();
                if indent > current {
                    self.indents.push(indent);
                    tokens.push(Token::new(start, indent, TokenType::Indent));
                } else {
                    while indent < *self.indents.last().unwrap() {
                        self.indents.pop();
                        tokens.push(Token::new(start, 0, TokenType::Dedent));
                    }
                    if indent != *self.indents.last().unwrap() {
                        return Err(self.error(start, "invalid indentation"));
                    }
                }
                self.line_start = false;
            }
            if self.consume('\n') {
                if self.group_depth > 0 {
                    self.source.ignored_new_lines.insert(self.index - 1);
                    self.line_start = false;
                    continue;
                }
                let mut temp = self.index;
                let len = self.source.code.len();
                while temp < len && (self.source.code[temp] == ' ' || self.source.code[temp] == '\t') {
                    temp += 1;
                }
                if temp < len && self.source.code[temp] == '.' {
                    self.source.ignored_new_lines.insert(self.index - 1);
                    self.line_start = false;
                    continue;
                }
                tokens.push(Token::new(self.index - 1, 1, TokenType::NewLine));
                self.line_start = true;
                continue;
            }
            if self.peek_is('#') {
                while !self.eof() && !self.peek_is('\n') {
                    self.index += 1;
                }
                continue;
            }
            if self.peek().is_whitespace() {
                self.index += 1;
                continue;
            }
            let token = match self.peek() {
                '"' | '\'' => self.get_string_token()?,
                '`' => self.get_fstring_token()?,
                // `b"..."` / `b'...'` is a byte-string literal. Check this
                // before the identifier branch so the `b` prefix is kept in
                // the token span and decoded by the parser.
                c if (c == 'b' || c == 'B') && matches!(self.peek_at(1), '"' | '\'') => {
                    self.get_bytes_token()?
                }
                c if c.is_ascii_digit() => self.get_number_token()?,
                c if is_ident_start(c) => self.get_identifier_token()?,
                _ => self.get_operator_token()?,
            };
            tokens.push(token);
        }
        while self.indents.len() > 1 {
            self.indents.pop();
            tokens.push(Token::new(self.index, 0, TokenType::Dedent));
        }
        tokens.push(Token::new(self.index, 0, TokenType::Eof));
        Ok(tokens)
    }
    fn get_string_token(&mut self) -> Result<Token, LangError> {
        let start = self.index;
        let quote = if self.consume('"') {
            '"'
        } else if self.consume('\'') {
            '\''
        } else {
            return Err(self.error(start, "expected opening quote for string"));
        };
        let mut triple = false;
        if self.consume(quote) {
            if self.consume(quote) {
                triple = true;
            } else {
                return Ok(Token::new(start, self.index - start, TokenType::String));
            }
        }
        while self.index < self.source.code.len() {
            if self.peek_is('\\') {
                self.index += 2;
                continue;
            }
            if !triple {
                if self.peek_is(quote) {
                    self.index += 1;
                    return Ok(Token::new(start, self.index - start, TokenType::String));
                }
            } else if !self.out_of_range(self.index + 2)
                && self.peek_is(quote)
                && self.source.code[self.index + 1] == quote
                && self.source.code[self.index + 2] == quote
            {
                self.index += 3;
                return Ok(Token::new(start, self.index - start, TokenType::String));
            }
            self.index += 1;
        }
        let message = if triple {
            format!("expected closing {}", quote.to_string().repeat(3))
        } else {
            format!("expected closing {}", quote)
        };
        Err(self.error(start, &message))
    }
    /// Scans a `b"..."`, `b'...'`, `b"""..."""` or `b'''...'''` literal.
    /// The token keeps the prefix and quotes; escape decoding happens in the
    /// parser so the lexer stays a single, allocation-free scan.
    fn get_bytes_token(&mut self) -> Result<Token, LangError> {
        let start = self.index;
        if !self.consume('b') && !self.consume('B') {
            return Err(self.error(start, "expected 'b' to start a bytes literal"));
        }
        let quote = if self.consume('"') {
            '"'
        } else if self.consume('\'') {
            '\''
        } else {
            return Err(self.error(start, "expected opening quote for bytes literal"));
        };
        let mut triple = false;
        if self.consume(quote) {
            if self.consume(quote) {
                triple = true;
            } else {
                return Ok(Token::new(start, self.index - start, TokenType::Bytes));
            }
        }
        while self.index < self.source.code.len() {
            if self.peek_is('\\') {
                if self.out_of_range(self.index + 1) {
                    return Err(self.error(start, "unterminated escape sequence"));
                }
                self.index += 2;
                continue;
            }
            if !triple {
                if self.peek_is(quote) {
                    self.index += 1;
                    return Ok(Token::new(start, self.index - start, TokenType::Bytes));
                }
            } else if !self.out_of_range(self.index + 2)
                && self.peek_is(quote)
                && self.source.code[self.index + 1] == quote
                && self.source.code[self.index + 2] == quote
            {
                self.index += 3;
                return Ok(Token::new(start, self.index - start, TokenType::Bytes));
            }
            self.index += 1;
        }
        Err(self.error(
            start,
            if triple {
                "expected closing triple quote for bytes literal"
            } else {
                "expected closing quote for bytes literal"
            },
        ))
    }
    fn get_fstring_token(&mut self) -> Result<Token, LangError> {
        let start = self.index;
        if !self.consume('`') {
            return Err(self.error(start, "expected opening backtick"));
        }
        let triple = self.consume('`') && self.consume('`');
        while !self.out_of_range(self.index) {
            if self.peek_is('\\') {
                if self.out_of_range(self.index + 1) {
                    return Err(self.error(start, "unterminated escape sequence"));
                }
                self.index += 2;
                continue;
            }
            if !triple {
                if self.peek_is('`') {
                    self.index += 1;
                    return Ok(Token::new(start, self.index - start, TokenType::FString));
                }
            } else if !self.out_of_range(self.index + 2)
                && self.peek_is('`')
                && self.source.code[self.index + 1] == '`'
                && self.source.code[self.index + 2] == '`'
            {
                self.index += 3;
                return Ok(Token::new(start, self.index - start, TokenType::FString));
            }
            self.index += 1;
        }
        Err(self.error(
            start,
            if triple {
                "expected closing triple backtick"
            } else {
                "expected closing backtick"
            },
        ))
    }
    fn read_digits(&mut self, pred: impl Fn(char) -> bool, start: usize) -> Result<bool, LangError> {
        let mut found = false;
        loop {
            if self.consume_pred(&pred) {
                found = true;
                self.last_underscore = false;
                continue;
            }
            if self.consume('_') {
                if !found {
                    return Ok(false);
                }
                if self.last_underscore {
                    return Err(self.error(start, "consecutive underscores are not allowed"));
                }
                self.last_underscore = true;
                continue;
            }
            break;
        }
        Ok(found)
    }
    fn validate_number(&mut self, start: usize, is_float: bool) -> Result<Token, LangError> {
        if self.last_underscore {
            return Err(self.error(start, "number cannot end with underscore"));
        }
        if self.consume_pred(is_ident_continue) {
            return Err(self.error(start, "invalid number format"));
        }
        Ok(Token::new(
            start,
            self.index - start,
            if is_float { TokenType::Float } else { TokenType::Integer },
        ))
    }
    fn get_number_token(&mut self) -> Result<Token, LangError> {
        let start = self.index;
        let mut is_float = false;
        self.last_underscore = false;
        if self.consume('0') {
            if self.consume('x') || self.consume('X') {
                if !self.read_digits(|c| c.is_ascii_hexdigit(), start)? {
                    return Err(self.error(start, "expected hex digit"));
                }
                return self.validate_number(start, is_float);
            }
            if self.consume('b') || self.consume('B') {
                if !self.read_digits(|c| c == '0' || c == '1', start)? {
                    return Err(self.error(start, "expected binary digit"));
                }
                return self.validate_number(start, is_float);
            }
            if self.consume('o') || self.consume('O') {
                if !self.read_digits(|c| ('0'..='7').contains(&c), start)? {
                    return Err(self.error(start, "expected octal digit"));
                }
                return self.validate_number(start, is_float);
            }
            self.index -= 1;
        }
        if !self.read_digits(|c| c.is_ascii_digit(), start)? {
            return Err(self.error(start, "expected digit"));
        }
        if self.peek_is('.') && self.peek_at(1) != '.' {
            self.index += 1;
            if self.last_underscore {
                return Err(self.error(start, "underscore cannot precede decimal point"));
            }
            is_float = true;
            if self.peek_is('_') {
                return Err(self.error(start, "underscore cannot follow decimal point"));
            }
            let _ = self.read_digits(|c| c.is_ascii_digit(), start);
        }
        if self.consume('e') || self.consume('E') {
            if self.last_underscore {
                return Err(self.error(start, "underscore cannot precede exponent"));
            }
            is_float = true;
            let _ = self.consume('+') || self.consume('-');
            if !self.read_digits(|c| c.is_ascii_digit(), start)? {
                return Err(self.error(start, "expected digit after exponent"));
            }
        }
        self.validate_number(start, is_float)
    }
    fn get_identifier_token(&mut self) -> Result<Token, LangError> {
        let start = self.index;
        if !self.consume_pred(is_ident_start) {
            return Err(self.error(start, "expected identifier"));
        }
        while self.consume_pred(is_ident_continue) {}
        let text: String = self.source.code[start..self.index].iter().collect();
        let typ = match text.as_str() {
            "fn" => TokenType::Func,
            "class" => TokenType::Class,
            "enum" => TokenType::Enum,
            "true" => TokenType::True,
            "false" => TokenType::False,
            "if" => TokenType::If,
            "elif" => TokenType::ElIf,
            "else" => TokenType::Else,
            "match" => TokenType::Match,
            "for" => TokenType::For,
            "while" => TokenType::While,
            "break" => TokenType::Break,
            "skip" => TokenType::Skip,
            "use" => TokenType::Use,
            "using" => TokenType::Using,
            "in" => TokenType::In,
            "is" => TokenType::Is,
            "as" => TokenType::As,
            "and" => TokenType::And,
            "or" => TokenType::Or,
            "xor" => TokenType::Xor,
            "not" => TokenType::Not,
            "go" => TokenType::Go,
            "wait" => TokenType::Wait,
            "try" => TokenType::Try,
            "catch" => TokenType::Catch,
            "finally" => TokenType::Finally,
            "defer" => TokenType::Defer,
            "none" => TokenType::NoneKw,
            "_" => TokenType::Underscore,
            _ => TokenType::Identifier,
        };
        Ok(Token::new(start, text.chars().count(), typ))
    }
    fn get_operator_token(&mut self) -> Result<Token, LangError> {
        let start = self.index;
        match self.peek() {
            '(' => {
                self.group_depth += 1;
                Ok(self.tok(start, TokenType::LParen, 1))
            }
            ')' => {
                self.group_depth -= 1;
                Ok(self.tok(start, TokenType::RParen, 1))
            }
            '[' => {
                self.group_depth += 1;
                Ok(self.tok(start, TokenType::LBrack, 1))
            }
            ']' => {
                self.group_depth -= 1;
                Ok(self.tok(start, TokenType::RBrack, 1))
            }
            '{' => {
                self.group_depth += 1;
                Ok(self.tok(start, TokenType::LBrace, 1))
            }
            '}' => {
                self.group_depth -= 1;
                Ok(self.tok(start, TokenType::RBrace, 1))
            }
            ',' => Ok(self.tok(start, TokenType::Comma, 1)),
            ':' => {
                if self.match_str(":=") {
                    Ok(self.tok(start, TokenType::ColonAssign, 2))
                } else {
                    Ok(self.tok(start, TokenType::Colon, 1))
                }
            }
            _ => self.get_complex_operator_token(),
        }
    }
    fn get_complex_operator_token(&mut self) -> Result<Token, LangError> {
        let start = self.index;
        // Ordered longest-first so `<<=` wins over `<<` and `+=` over `+`.
        // `..` is handled separately below: it uses `peek_at`, not `match_str`,
        // and must keep that end-of-input behaviour.
        for (text, typ, length) in MULTI_CHAR_OPERATORS {
            if self.match_str(text) {
                return Ok(self.tok(start, *typ, *length));
            }
        }
        if self.peek_is('.') && self.peek_at(1) == '.' {
            return Ok(self.tok(start, TokenType::DotDot, 2));
        }
        match self.peek() {
            '=' => Ok(self.tok(start, TokenType::Assign, 1)),
            '+' => Ok(self.tok(start, TokenType::Plus, 1)),
            '-' => Ok(self.tok(start, TokenType::Minus, 1)),
            '*' => Ok(self.tok(start, TokenType::Asterisk, 1)),
            '/' => Ok(self.tok(start, TokenType::Slash, 1)),
            '%' => Ok(self.tok(start, TokenType::Percent, 1)),
            '&' => Ok(self.tok(start, TokenType::BAnd, 1)),
            '|' => Ok(self.tok(start, TokenType::BOr, 1)),
            '^' => Ok(self.tok(start, TokenType::BXor, 1)),
            '~' => Ok(self.tok(start, TokenType::BNot, 1)),
            '!' => Ok(self.tok(start, TokenType::Bang, 1)),
            '<' => Ok(self.tok(start, TokenType::LessThan, 1)),
            '>' => Ok(self.tok(start, TokenType::GreaterThan, 1)),
            '.' => Ok(self.tok(start, TokenType::Dot, 1)),
            '?' => Ok(self.tok(start, TokenType::Question, 1)),
            '@' => Ok(self.tok(start, TokenType::At, 1)),
            _ => Err(self.error(start, "unknown operator")),
        }
    }
}
