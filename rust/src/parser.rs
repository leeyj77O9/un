use std::rc::Rc;
use crate::ast::{Const, Node, NodeKind};
use crate::error::LangError;
use crate::lexer::lex;
use crate::source::Source;
use crate::token::{Token, TokenType};
pub fn parse(tokens: Vec<Token>, source: Rc<Source>) -> Result<Node, LangError> {
    let mut parser = Parser::new(tokens, source);
    parser.parse()
}
pub struct Parser {
    tokens: Vec<Token>,
    position: usize,
    source: Rc<Source>,
    /// Names declared with `fn name[T]`, used to disambiguate
    /// `id[int](x)` (type arguments) from `d[k](x)` (index then call).
    generic_names: std::collections::HashSet<String>,
}
type Span = (usize, usize);
fn n(node: &Node) -> Span {
    (node.start, node.length)
}
fn t(token: &Token) -> Span {
    (token.start, token.length)
}
fn len(start: Span, end: Span) -> usize {
    end.0 + end.1 - start.0
}
fn check_no_annotations(annotations: &[Node]) -> Result<(), LangError> {
    if annotations.is_empty() {
        return Ok(());
    }
    let first = &annotations[0];
    let last = &annotations[annotations.len() - 1];
    Err(LangError::new(
        first.start,
        last.start + last.length - first.start,
        "annotations are not allowed here",
    ))
}
impl Parser {
    pub fn new(tokens: Vec<Token>, source: Rc<Source>) -> Self {
        Self {
            tokens,
            position: 0,
            source,
            generic_names: std::collections::HashSet::new(),
        }
    }

    fn current(&self) -> Token {
        if self.position < self.tokens.len() {
            self.tokens[self.position].clone()
        } else {
            self.tokens[self.tokens.len() - 1].clone()
        }
    }
    fn is_end_of_statement(&self) -> bool {
        matches!(
            self.current().typ,
            TokenType::NewLine | TokenType::Dedent | TokenType::Eof
        )
    }
    fn previous(&self) -> Token {
        self.tokens[self.position - 1].clone()
    }
    fn next(&mut self) -> Token {
        let token = self.current();
        self.position += 1;
        token
    }
    fn match_(&mut self, typ: TokenType) -> bool {
        if self.current().typ != typ {
            return false;
        }
        self.position += 1;
        true
    }
    fn peek_type(&self, offset: usize) -> Option<TokenType> {
        self.tokens.get(self.position + offset).map(|t| t.typ)
    }
    fn expect(&mut self, typ: TokenType, msg: &str) -> Result<Token, LangError> {
        if self.current().typ != typ {
            let token = self.current();
            return Err(LangError::new(token.start, token.length, msg));
        }
        Ok(self.next())
    }
    fn skip_new_lines(&mut self) {
        while self.match_(TokenType::NewLine) {}
    }
    fn get_text(&self, start: usize, length: usize) -> String {
        self.source.span(start, length)
    }
    pub fn parse(&mut self) -> Result<Node, LangError> {
        let mut statements: Vec<Node> = Vec::new();
        self.skip_new_lines();
        while self.current().typ != TokenType::Eof {
            statements.push(self.parse_statement()?);
            self.skip_new_lines();
        }
        let length = if statements.is_empty() {
            0
        } else {
            let last = &statements[statements.len() - 1];
            last.start + last.length
        };
        Ok(Node::with_children(
            0,
            length,
            NodeKind::Block,
            None,
            statements,
        ))
    }
    fn parse_statement(&mut self) -> Result<Node, LangError> {
        let annotations = self.parse_annotations()?;
        let statement = if self.is_type_alias_start() {
            check_no_annotations(&annotations)?;
            self.parse_type_alias()?
        } else if self.is_accessor_start() {
            check_no_annotations(&annotations)?;
            self.parse_accessor()?
        } else if self.is_assignment_start() {
            check_no_annotations(&annotations)?;
            self.parse_assignment()?
        } else {
            match self.current().typ {
                TokenType::Use => {
                    check_no_annotations(&annotations)?;
                    self.parse_use()?
                }
                TokenType::Using => {
                    check_no_annotations(&annotations)?;
                    self.parse_using()?
                }
                TokenType::Class => self.parse_class(annotations)?,
                TokenType::Enum => self.parse_enum(annotations)?,
                TokenType::Func => self.parse_function(annotations)?,
                TokenType::If => {
                    check_no_annotations(&annotations)?;
                    self.parse_if()?
                }
                TokenType::For => {
                    check_no_annotations(&annotations)?;
                    self.parse_for()?
                }
                TokenType::While => {
                    check_no_annotations(&annotations)?;
                    self.parse_while()?
                }
                TokenType::Try => {
                    check_no_annotations(&annotations)?;
                    self.parse_try()?
                }
                TokenType::Defer => {
                    check_no_annotations(&annotations)?;
                    self.parse_defer()?
                }
                TokenType::Break => {
                    check_no_annotations(&annotations)?;
                    self.parse_break()?
                }
                TokenType::Skip => {
                    check_no_annotations(&annotations)?;
                    self.parse_skip()?
                }
                TokenType::Return => {
                    check_no_annotations(&annotations)?;
                    self.parse_return()?
                }
                _ => {
                    check_no_annotations(&annotations)?;
                    self.parse_expression()?
                }
            }
        };
        if !self.is_end_of_statement() {
            let token = self.current();
            if self.is_statement_start(token.typ) {
                let prev_end = self.tokens[..self.position]
                    .iter()
                    .rev()
                    .find(|t| !matches!(t.typ, TokenType::NewLine | TokenType::Dedent | TokenType::Indent))
                    .map(|t| t.start + t.length);
                if let Some(prev_end) = prev_end {
                    if self.source.get_line(prev_end) == self.source.get_line(token.start) {
                        return Err(LangError::new(
                            token.start,
                            token.length,
                            format!(
                                "expected a newline after the previous statement (got `{}`)",
                                token.typ
                            ),
                        ));
                    }
                }
                return Ok(statement);
            }
            return Err(LangError::new(
                token.start,
                token.length,
                format!("unexpected token {}", token.typ),
            ));
        }
        Ok(statement)
    }
    fn is_statement_start(&self, typ: TokenType) -> bool {
        match typ {
            TokenType::Identifier => true,
            TokenType::Use
            | TokenType::Using
            | TokenType::Class
            | TokenType::Enum
            | TokenType::Func
            | TokenType::If
            | TokenType::For
            | TokenType::While
            | TokenType::Try
            | TokenType::Defer
            | TokenType::Break
            | TokenType::Skip
            | TokenType::Wait
            | TokenType::At
            | TokenType::Return => true,
            _ => false,
        }
    }
    /// `type` stays a contextual keyword: `type(x)` (the builtin) and a
    /// variable named `type` keep parsing exactly as before. Only the
    /// `type <identifier> = ...` shape starts a type-alias declaration.
    fn is_type_alias_start(&self) -> bool {
        self.current().typ == TokenType::Identifier
            && self.get_text(self.current().start, self.current().length) == "type"
            && self.peek_type(1) == Some(TokenType::Identifier)
            && self.peek_type(2) == Some(TokenType::Assign)
    }
    fn parse_type_alias(&mut self) -> Result<Node, LangError> {
        let keyword = self.next();
        let name = self.parse_identifier()?;
        self.expect(TokenType::Assign, "expected '=' after type alias name")?;
        let typ = self.parse_type()?;
        Ok(Node::with_children(
            keyword.start,
            len(t(&keyword), n(&typ)),
            NodeKind::TypeAlias,
            None,
            vec![name, typ],
        ))
    }
    fn parse_annotation(&mut self) -> Result<Node, LangError> {
        let at = self.expect(TokenType::At, "expected '@' to start an annotation")?;
        let expr = self.parse_expression()?;
        Ok(Node::with_children(
            at.start,
            len(t(&at), n(&expr)),
            NodeKind::Annotation,
            None,
            vec![expr],
        ))
    }
    fn parse_annotations(&mut self) -> Result<Vec<Node>, LangError> {
        let mut annotations = Vec::new();
        while self.current().typ == TokenType::At {
            annotations.push(self.parse_annotation()?);
            self.skip_new_lines();
        }
        Ok(annotations)
    }
    fn parse_use_path(&mut self) -> Result<Node, LangError> {
        // Leading dots make an import explicitly relative to the importing
        // file. One `.` is one level (`.` = the file's own directory, `..` =
        // its parent). They are folded into the path text as leading `.`
        // characters so the existing compiler -> VM -> module::import_file
        // pipeline keeps working: the VM splits that text on `.` and hands the
        // resulting (leading-empty) parts to the module resolver, which
        // interprets the empty prefix as the relative depth. Absolute paths
        // (`use a.b`) take the unchanged path below and get no `Const::Str`
        // value, so their behaviour is byte-for-byte identical.
        let mut depth = 0usize;
        loop {
            if self.match_(TokenType::DotDot) {
                depth += 2;
            } else if self.match_(TokenType::Dot) {
                depth += 1;
            } else {
                break;
            }
        }
        let mut segments: Vec<String> = Vec::new();
        let mut path = self.parse_identifier()?;
        segments.push(self.get_text(path.start, path.length));
        while self.match_(TokenType::Dot) {
            if self.current().typ == TokenType::Asterisk {
                let star = self.next();
                let start = path.start;
                let length = len(n(&path), t(&star));
                let inner = if depth > 0 {
                    Self::relative_use_path(path, depth, &segments)
                } else {
                    path
                };
                return Ok(Node::with_children(
                    start,
                    length,
                    NodeKind::Unary,
                    Some(TokenType::Wildcard),
                    vec![inner],
                ));
            }
            let member = self.parse_identifier()?;
            segments.push(self.get_text(member.start, member.length));
            let start = path.start;
            let length = len(n(&path), n(&member));
            path = Node::with_children(start, length, NodeKind::Property, None, vec![path, member]);
        }
        if depth > 0 {
            path = Self::relative_use_path(path, depth, &segments);
        }
        Ok(path)
    }
    /// Stamps the relative depth onto a `use` path node. The compiler reads
    /// `self.text(path_node)` and pushes the result as the `Op::Use` name, so a
    /// `Const::Str` value here is the only channel that reaches the resolver
    /// without touching the compiler or VM. Source spans of the identifier
    /// children stay untouched (typeck/LSP keep reading the plain module name).
    fn relative_use_path(node: Node, depth: usize, segments: &[String]) -> Node {
        let mut text = ".".repeat(depth);
        text.push_str(&segments.join("."));
        node.with_value(Const::Str(text))
    }
    fn parse_use_imports(&mut self) -> Result<Node, LangError> {
        let mut imports: Vec<Node> = Vec::new();
        if self.match_(TokenType::Asterisk) {
            let token = self.previous();
            imports.push(Node::with_children(
                token.start,
                token.length,
                NodeKind::Unary,
                Some(TokenType::Wildcard),
                vec![],
            ));
        } else {
            loop {
                let name = self.parse_identifier()?;
                if self.match_(TokenType::As) {
                    let alias = self.parse_identifier()?;
                    imports.push(Node::with_children(
                        name.start,
                        len(n(&name), n(&alias)),
                        NodeKind::Pair,
                        None,
                        vec![name, alias],
                    ));
                } else {
                    imports.push(name);
                }
                if !self.match_(TokenType::Comma) {
                    break;
                }
            }
        }

        Ok(Node::with_children(
            imports[0].start,
            imports[imports.len() - 1].length,
            NodeKind::Tuple,
            None,
            imports,
        ))
    }
    fn parse_use(&mut self) -> Result<Node, LangError> {
        let keyword = self.expect(TokenType::Use, "expected 'use' keyword")?;
        let path = self.parse_use_path()?;
        let imports = if self.match_(TokenType::LBrace) {
            let imports = self.parse_use_imports()?;
            self.expect(TokenType::RBrace, "expected '}' to close the use member")?;
            Some(imports)
        } else {
            None
        };
        let alias = if self.match_(TokenType::As) {
            Some(self.parse_identifier()?)
        } else {
            None
        };
        let mut children = vec![path];
        if let Some(imports) = imports {
            children.push(imports);
        }
        if let Some(alias) = alias {
            children.push(alias);
        }
        let end = children.last().unwrap();
        let length = len(t(&keyword), n(end));
        Ok(Node::with_children(keyword.start, length, NodeKind::Use, None, children))
    }
    fn parse_using(&mut self) -> Result<Node, LangError> {
        let keyword = self.expect(TokenType::Using, "expected 'using' keyword")?;
        let expr = self.parse_assignment()?;
        if expr.kind != NodeKind::Assign {
            return Err(LangError::new(
                expr.start,
                expr.length,
                "using requires an assignment",
            ));
        }
        Ok(Node::with_children(
            keyword.start,
            len(t(&keyword), n(&expr)),
            NodeKind::Using,
            None,
            vec![expr],
        ))
    }
    /// Parses an optional `[T, U: bound, ...]` type parameter list.
    /// Callers append the result as the last child so positional readers
    /// (`children[0]`, `children[1]`, ...) keep working.
    fn parse_type_params(&mut self) -> Result<Option<Node>, LangError> {
        if self.current().typ != TokenType::LBrack {
            return Ok(None);
        }
        let lbrack = self.expect(TokenType::LBrack, "expected '['")?;
        let mut names = Vec::new();
        loop {
            let mut name = self.parse_identifier()?;
            if self.match_(TokenType::Colon) {
                // A bound may be a union such as `int | float`, so capture the
                // raw source text up to the next top-level comma or `]`.
                let start = self.current().start;
                let mut end = start;
                let mut depth = 0usize;
                loop {
                    let token = self.current();
                    match token.typ {
                        TokenType::LBrack | TokenType::LParen => {
                            depth += 1;
                            end = token.start + token.length;
                            self.next();
                        }
                        TokenType::RBrack | TokenType::RParen => {
                            if depth == 0 {
                                break;
                            }
                            depth -= 1;
                            end = token.start + token.length;
                            self.next();
                        }
                        TokenType::Comma if depth == 0 => break,
                        TokenType::NewLine | TokenType::Dedent | TokenType::Eof => break,
                        _ => {
                            end = token.start + token.length;
                            self.next();
                        }
                    }
                }
                name = name.with_value(Const::Str(self.get_text(start, end - start)));
            }
            names.push(name);
            if !self.match_(TokenType::Comma) {
                break;
            }
        }
        let rbrack = self.expect(TokenType::RBrack, "expected ']' after type parameters")?;
        Ok(Some(Node::with_children(
            lbrack.start,
            len(t(&lbrack), t(&rbrack)),
            NodeKind::TypeParams,
            None,
            names,
        )))
    }
    fn parse_class(&mut self, annotations: Vec<Node>) -> Result<Node, LangError> {
        let keyword = self.expect(TokenType::Class, "expected 'class' keyword")?;
        let name = self.parse_identifier()?;
        let type_params = self.parse_type_params()?;
        let parameters = if self.current().typ == TokenType::LParen {
            Some(self.parse_parameters()?)
        } else {
            None
        };
        let bases = if self.match_(TokenType::Colon) {
            let mut types = Vec::new();
            loop {
                types.push(self.parse_identifier()?);
                if !self.match_(TokenType::Comma) {
                    break;
                }
            }
            let length = len(n(&types[0]), n(&types[types.len() - 1]));
            let start = types[0].start;
            Some(Node::with_children(start, length, NodeKind::ClassBases, None, types))
        } else {
            None
        };
        self.skip_new_lines();
        let body = if self.current().typ == TokenType::Indent {
            self.parse_block()?
        } else {
            Node::new(self.current().start, 0, NodeKind::Block)
        };
        let length = len(t(&keyword), n(&body));
        let mut children = vec![name];
        if let Some(parameters) = parameters {
            children.push(parameters);
        }
        if let Some(bases) = bases {
            children.push(bases);
        }
        children.push(body);
        if let Some(type_params) = type_params {
            children.push(type_params);
        }
        Ok(Node::with_children(keyword.start, length, NodeKind::Class, None, children)
            .with_annotations(annotations))
    }
    fn parse_enum_member(&mut self) -> Result<Node, LangError> {
        let name = self.parse_identifier()?;
        // `variant: type` declares a data-carrying variant. The type is kept
        // as a `Typed` node so both the checker and the compiler can recognise
        // it without adding a new NodeKind.
        if self.match_(TokenType::Colon) {
            let type_node = self.parse_type()?;
            return Ok(Node::with_children(
                name.start,
                len(n(&name), n(&type_node)),
                NodeKind::Typed,
                None,
                vec![name, type_node],
            ));
        }
        if !self.match_(TokenType::Assign) {
            return Ok(name);
        }
        let value = self.parse_expression()?;
        Ok(Node::with_children(
            name.start,
            len(n(&name), n(&value)),
            NodeKind::Assign,
            Some(TokenType::Assign),
            vec![name, value],
        ))
    }
    fn parse_enum(&mut self, annotations: Vec<Node>) -> Result<Node, LangError> {
        let keyword = self.expect(TokenType::Enum, "expected 'enum' keyword")?;
        let name = self.parse_identifier()?;
        let type_params = self.parse_type_params()?;
        self.expect(TokenType::NewLine, "expected newline after enum name")?;
        self.expect(TokenType::Indent, "expected indented enum members")?;
        let mut members: Vec<Node> = Vec::new();
        while self.current().typ != TokenType::Dedent && self.current().typ != TokenType::Eof {
            self.skip_new_lines();
            if self.current().typ == TokenType::Dedent {
                break;
            }
            members.push(self.parse_enum_member()?);
            while self.match_(TokenType::Comma) {
                self.skip_new_lines();
                if matches!(self.current().typ, TokenType::NewLine | TokenType::Dedent) {
                    break;
                }
                members.push(self.parse_enum_member()?);
            }
            self.skip_new_lines();
        }
        let closer = self.expect(TokenType::Dedent, "expected end of enum block")?;
        let mut children = vec![name];
        children.extend(members);
        if let Some(type_params) = type_params {
            children.push(type_params);
        }
        Ok(
            Node::with_children(
                keyword.start,
                len(t(&keyword), t(&closer)),
                NodeKind::Enum,
                None,
                children,
            )
            .with_annotations(annotations),
        )
    }
    fn parse_if(&mut self) -> Result<Node, LangError> {
        let keyword = self.expect(TokenType::If, "expected 'if' keyword")?;
        let mut cases: Vec<Node> = Vec::new();
        let condition = self.parse_expression()?;
        let then_branch = self.parse_block()?;
        cases.push(Node::with_children(
            keyword.start,
            len(t(&keyword), n(&then_branch)),
            NodeKind::Branch,
            None,
            vec![condition, then_branch],
        ));
        let mut else_node: Option<Node> = None;
        loop {
            if self.match_(TokenType::ElIf) {
                let keyword = self.previous();
                let elif_cond = self.parse_expression()?;
                let elif_body = self.parse_block()?;
                cases.push(Node::with_children(
                    keyword.start,
                    len(t(&keyword), n(&elif_body)),
                    NodeKind::ElIf,
                    None,
                    vec![elif_cond, elif_body],
                ));
            } else if self.match_(TokenType::Else) {
                let keyword = self.previous();
                let else_body = self.parse_block()?;
                else_node = Some(Node::with_children(
                    keyword.start,
                    len(t(&keyword), n(&else_body)),
                    NodeKind::Else,
                    None,
                    vec![else_body],
                ));
                break;
            } else {
                break;
            }
        }
        let mut children = cases;
        if let Some(else_node) = else_node {
            children.push(else_node);
        }
        let length = len(t(&keyword), n(&children[children.len() - 1]));
        Ok(Node::with_children(keyword.start, length, NodeKind::If, None, children))
    }
    fn parse_match_brace_body(&mut self) -> Result<Node, LangError> {
        self.expect(TokenType::LBrace, "expected '{' to start match body")?;
        let mut cases: Vec<Node> = Vec::new();
        self.skip_new_lines();
        while !self.match_(TokenType::RBrace) {
            // `case` is an optional, contextual arm prefix. A binding named
            // `case` (`case -> ...`) still parses as an ordinary pattern.
            if self.current().typ == TokenType::Identifier
                && self.get_text(self.current().start, self.current().length) == "case"
                && self
                    .peek_type(1)
                    .map(|t| self.is_expression_start(t) || t == TokenType::Underscore)
                    .unwrap_or(false)
            {
                self.next();
            }
            let pattern = self.parse_pattern()?;
            // `case <pattern> if <cond> -> <body>`: wrap the pattern in a Guard
            // node so `Case.children[1]` remains the arm body.
            let pattern = if self.match_(TokenType::If) {
                let condition = self.parse_expression()?;
                Node::with_children(
                    pattern.start,
                    len(n(&pattern), n(&condition)),
                    NodeKind::Guard,
                    None,
                    vec![pattern, condition],
                )
            } else {
                pattern
            };
            self.expect(TokenType::Return, "expected '->' after match pattern")?;
            let body = self.parse_ternary()?;
            cases.push(Node::with_children(
                pattern.start,
                len(n(&pattern), n(&body)),
                NodeKind::Case,
                None,
                vec![pattern, body],
            ));

            self.match_(TokenType::Comma);
            self.skip_new_lines();
        }
        if cases.is_empty() {
            return Ok(Node::with_children(0, 0, NodeKind::Block, None, cases));
        }
        let length = len(n(&cases[0]), n(&cases[cases.len() - 1]));
        Ok(Node::with_children(cases[0].start, length, NodeKind::Block, None, cases))
    }
    fn parse_match(&mut self) -> Result<Node, LangError> {
        let keyword = self.expect(TokenType::Match, "expected 'match' keyword")?;
        let expr = self.parse_expression()?;
        let body = self.parse_match_brace_body()?;
        let length = len(t(&keyword), n(&body));
        let mut children = vec![expr];
        children.extend(body.children);
        Ok(Node::with_children(keyword.start, length, NodeKind::Match, None, children))
    }
    fn parse_for(&mut self) -> Result<Node, LangError> {
        let keyword = self.expect(TokenType::For, "expected 'for' keyword")?;

        if !self.has_for_header_in() {
            let iterable_expr = self.parse_expression()?;
            self.skip_new_lines();
            let no_target_body = self.parse_block()?;
            return Ok(Node::with_children(
                keyword.start,
                len(t(&keyword), n(&no_target_body)),
                NodeKind::For,
                None,
                vec![
                    Node::new(iterable_expr.start, 0, NodeKind::Wildcard),
                    iterable_expr,
                    no_target_body,
                ],
            ));
        }
        let target = self.parse_pattern()?;
        self.expect(TokenType::In, "expected 'in' after for target")?;
        let iterable = self.parse_expression()?;
        self.skip_new_lines();
        let body = self.parse_block()?;
        Ok(Node::with_children(
            keyword.start,
            len(t(&keyword), n(&body)),
            NodeKind::For,
            None,
            vec![target, iterable, body],
        ))
    }

    fn has_for_header_in(&self) -> bool {
        let mut depth: i32 = 0;
        for tok in self.tokens[self.position..].iter() {
            match tok.typ {
                TokenType::LParen | TokenType::LBrack | TokenType::LBrace => depth += 1,
                TokenType::RParen | TokenType::RBrack | TokenType::RBrace => depth -= 1,
                TokenType::In if depth == 0 => return true,
                TokenType::NewLine | TokenType::Indent | TokenType::Dedent if depth == 0 => {
                    return false
                }
                _ => {}
            }
        }
        false
    }
    fn parse_while(&mut self) -> Result<Node, LangError> {
        let keyword = self.expect(TokenType::While, "expected 'while' keyword")?;
        let condition = self.parse_expression()?;
        let body = self.parse_block()?;
        Ok(Node::with_children(
            keyword.start,
            len(t(&keyword), n(&body)),
            NodeKind::While,
            None,
            vec![condition, body],
        ))
    }
    fn parse_try(&mut self) -> Result<Node, LangError> {
        let keyword = self.expect(TokenType::Try, "expected 'try' keyword")?;
        let body = self.parse_block()?;
        let mut catches: Vec<Node> = Vec::new();
        let mut finally_node: Option<Node> = None;
        while self.match_(TokenType::Catch) {
            let catch_keyword = self.previous();
            let exception = if self.current().typ == TokenType::Identifier {
                Some(self.parse_identifier()?)
            } else {
                None
            };
            let catch_body = self.parse_block()?;
            let length = len(t(&catch_keyword), n(&catch_body));
            let children = match exception {
                Some(exception) => vec![exception, catch_body],
                None => vec![catch_body],
            };
            catches.push(Node::with_children(
                catch_keyword.start,
                length,
                NodeKind::Catch,
                None,
                children,
            ));
        }
        if self.match_(TokenType::Finally) {
            let finally_keyword = self.previous();
            let finally_body = self.parse_block()?;
            finally_node = Some(Node::with_children(
                finally_keyword.start,
                len(t(&finally_keyword), n(&finally_body)),
                NodeKind::Finally,
                None,
                vec![finally_body],
            ));
        }
        let mut children = vec![body];
        children.extend(catches);
        if let Some(finally_node) = finally_node {
            children.push(finally_node);
        }
        let length = len(t(&keyword), n(&children[children.len() - 1]));
        Ok(Node::with_children(keyword.start, length, NodeKind::Try, None, children))
    }
    fn parse_defer(&mut self) -> Result<Node, LangError> {
        let keyword = self.expect(TokenType::Defer, "expected 'defer' keyword")?;
        self.skip_new_lines();
        let body = if self.current().typ == TokenType::Indent {
            self.parse_block()?
        } else {
            let expr = self.parse_expression()?;
            Node::with_children(expr.start, expr.length, NodeKind::Block, None, vec![expr])
        };
        Ok(Node::with_children(
            keyword.start,
            len(t(&keyword), n(&body)),
            NodeKind::Defer,
            None,
            vec![body],
        ))
    }
    fn parse_break(&mut self) -> Result<Node, LangError> {
        let keyword = self.expect(TokenType::Break, "expected 'break' keyword")?;
        Ok(Node::new(keyword.start, keyword.length, NodeKind::Break))
    }
    fn parse_skip(&mut self) -> Result<Node, LangError> {
        let keyword = self.expect(TokenType::Skip, "expected 'skip' keyword")?;
        Ok(Node::new(keyword.start, keyword.length, NodeKind::Skip))
    }
    fn parse_return(&mut self) -> Result<Node, LangError> {
        let keyword = self.expect(TokenType::Return, "expected '->' keyword")?;
        if self.is_end_of_statement() {
            return Ok(Node::new(keyword.start, keyword.length, NodeKind::Return));
        }
        let expr = self.parse_expression()?;
        Ok(Node::with_children(
            keyword.start,
            len(t(&keyword), n(&expr)),
            NodeKind::Return,
            None,
            vec![expr],
        ))
    }
    fn parse_expression(&mut self) -> Result<Node, LangError> {
        self.parse_expression_assign()
    }
    fn parse_expression_assign(&mut self) -> Result<Node, LangError> {
        let left = self.parse_ternary()?;
        if self.current().typ != TokenType::ColonAssign {
            return Ok(left);
        }
        if !self.is_valid_assignment_target(&left) {
            return Err(LangError::new(
                left.start,
                left.length,
                "invalid assignment target",
            ));
        }
        let op = self.next();
        let right = self.parse_expression_assign()?;
        Ok(Node::with_children(
            left.start,
            right.start + right.length - left.start,
            NodeKind::Assign,
            Some(op.typ),
            vec![left, right],
        ))
    }
    fn parse_ternary(&mut self) -> Result<Node, LangError> {
        let cond = self.parse_binary(0)?;
        if self.current().typ == TokenType::Question && self.is_ternary_question() {
            self.next();
            let true_expr = self.parse_ternary()?;
            self.expect(TokenType::Colon, "expected ':' in ternary expression")?;
            let false_expr = self.parse_ternary()?;
            return Ok(Node::with_children(
                cond.start,
                false_expr.start + false_expr.length - cond.start,
                NodeKind::Ternary,
                None,
                vec![cond, true_expr, false_expr],
            ));
        }
        Ok(cond)
    }
    fn is_ternary_question(&self) -> bool {
        if self.position + 1 >= self.tokens.len() {
            return false;
        }
        if self.tokens[self.position + 1].typ == TokenType::Colon {
            return false;
        }
        if !self.is_expression_start(self.tokens[self.position + 1].typ) {
            return false;
        }
        let mut i = self.position + 1;
        let mut depth = 0;
        while i < self.tokens.len() {
            let typ = self.tokens[i].typ;
            if matches!(typ, TokenType::Eof | TokenType::NewLine | TokenType::Dedent) {
                break;
            }
            if matches!(
                typ,
                TokenType::RParen | TokenType::RBrack | TokenType::RBrace
            ) {
                if depth == 0 {
                    break;
                }
                depth -= 1;
                i += 1;
                continue;
            }
            if matches!(
                typ,
                TokenType::LParen | TokenType::LBrack | TokenType::LBrace
            ) {
                depth += 1;
                i += 1;
                continue;
            }
            if depth == 0 && typ == TokenType::Colon {
                return true;
            }
            i += 1;
        }
        false
    }
    fn parse_comma(&mut self) -> Result<Node, LangError> {
        let mut items = vec![self.parse_expression()?];
        while self.match_(TokenType::Comma) {
            items.push(self.parse_expression()?);
        }
        if items.len() == 1 {
            return Ok(items[0].clone());
        }
        let length = len(n(&items[0]), n(&items[items.len() - 1]));
        Ok(Node::with_children(items[0].start, length, NodeKind::Tuple, None, items))
    }
    fn is_accessor_start(&self) -> bool {
        if self.current().typ != TokenType::Identifier || self.position + 1 >= self.tokens.len() {
            return false;
        }
        if self.tokens[self.position + 1].typ != TokenType::Identifier {
            return false;
        }
        let text = self.source.span(self.current().start, self.current().length);
        text == "get" || text == "set"
    }
    fn parse_accessor(&mut self) -> Result<Node, LangError> {
        let keyword = self.next();
        let kind = self.source.span(keyword.start, keyword.length);
        let name = self.parse_identifier()?;
        let body = if kind == "get" && self.match_(TokenType::Return) {
            let arrow = self.previous();
            let expr = self.parse_expression()?;
            let ret = Node::with_children(
                arrow.start,
                len(t(&arrow), n(&expr)),
                NodeKind::Return,
                None,
                vec![expr],
            );
            Node::with_children(ret.start, ret.length, NodeKind::Block, None, vec![ret])
        } else if kind == "set" && self.match_(TokenType::Assign) {
            let expr = self.parse_expression()?;
            let field = Node::new(name.start, name.length, NodeKind::Identifier)
                .with_value(Const::Str("field".to_string()));
            let assign = Node::with_children(
                field.start,
                len(n(&field), n(&expr)),
                NodeKind::Assign,
                Some(TokenType::Assign),
                vec![field, expr],
            );
            Node::with_children(assign.start, assign.length, NodeKind::Block, None, vec![assign])
        } else {
            self.skip_new_lines();
            self.parse_block()?
        };
        Ok(
            Node::with_children(keyword.start, len(t(&keyword), n(&body)), NodeKind::Accessor, None, vec![name, body])
                .with_value(Const::Str(kind)),
        )
    }
    fn parse_assignment(&mut self) -> Result<Node, LangError> {
        let left = self.parse_assignment_target()?;
        if !self.current().typ.is_assignment_operator() {
            return Ok(left);
        }
        let op = self.next();
        let right = self.parse_comma()?;
        if self.current().typ.is_assignment_operator() {
            let token = self.current();
            return Err(LangError::new(
                token.start,
                token.length,
                "assignment chaining is not supported",
            ));
        }
        Ok(Node::with_children(
            left.start,
            right.start + right.length - left.start,
            NodeKind::Assign,
            Some(op.typ),
            vec![left, right],
        ))
    }
    fn parse_binary(&mut self, parent_precedence: usize) -> Result<Node, LangError> {
        let mut left = self.parse_unary()?;
        loop {
            let op_type = self.current().typ;
            let precedence;
            let op;
            if op_type == TokenType::Is
                && self.position + 1 < self.tokens.len()
                && self.tokens[self.position + 1].typ == TokenType::Not
            {
                precedence = Self::get_binary_precedence(TokenType::IsNot);
                if precedence <= parent_precedence {
                    break;
                }
                let is_tok = self.next();
                let not_tok = self.next();
                op = Token::new(is_tok.start, not_tok.start + not_tok.length - is_tok.start, TokenType::IsNot);
            } else {
                precedence = Self::get_binary_precedence(op_type);
                if precedence <= parent_precedence {
                    break;
                }
                op = self.next();
            }
            let rhs_precedence = if op.typ == TokenType::DoubleAsterisk {
                precedence.saturating_sub(1)
            } else {
                precedence
            };
            let right = self.parse_binary(rhs_precedence)?;
            let start = left.start;
            let length = right.start + right.length - left.start;
            left = Node::with_children(start, length, NodeKind::Binary, Some(op.typ), vec![left, right]);
        }
        Ok(left)
    }
    fn parse_unary(&mut self) -> Result<Node, LangError> {
        if matches!(
            self.current().typ,
            TokenType::Not
                | TokenType::Plus
                | TokenType::Minus
                | TokenType::BNot
                | TokenType::Wait
                | TokenType::Go
        ) {
            let op = self.next();
            let expr = self.parse_unary()?;
            return Ok(Node::with_children(
                op.start,
                expr.start + expr.length - op.start,
                NodeKind::Unary,
                Some(op.typ),
                vec![expr],
            ));
        }
        self.parse_postfix()
    }
    /// At `[` in postfix position, decide whether this is an explicit type
    /// argument list (`id[int](x)`) rather than indexing (`xs[0](x)`).
    fn looks_like_type_args(&self, expr: &Node) -> bool {
        if self.current().typ != TokenType::LBrack || expr.kind != NodeKind::Identifier {
            return false;
        }
        let callee = self.get_text(expr.start, expr.length);
        if !self.generic_names.contains(&callee) {
            return false;
        }
        let mut depth = 0usize;
        let mut i = self.position;
        while i < self.tokens.len() {
            match self.tokens[i].typ {
                TokenType::LBrack => depth += 1,
                TokenType::RBrack => {
                    depth -= 1;
                    if depth == 0 {
                        return i + 1 < self.tokens.len()
                            && self.tokens[i + 1].typ == TokenType::LParen;
                    }
                }
                TokenType::Identifier | TokenType::Comma => {}
                _ => return false,
            }
            i += 1;
        }
        false
    }

    fn parse_type_arg_list(&mut self) -> Result<Node, LangError> {
        let lbrack = self.expect(TokenType::LBrack, "expected '[' before type arguments")?;
        let mut names = Vec::new();
        loop {
            names.push(self.parse_identifier()?);
            if !self.match_(TokenType::Comma) {
                break;
            }
        }
        let rbrack = self.expect(TokenType::RBrack, "expected ']' after type arguments")?;
        Ok(Node::with_children(
            lbrack.start,
            len(t(&lbrack), t(&rbrack)),
            NodeKind::TypeParams,
            None,
            names,
        ))
    }

    fn parse_postfix(&mut self) -> Result<Node, LangError> {
        let mut expr = self.parse_primary()?;
        let mut type_args: Option<Node> = None;
        loop {
            if self.current().typ == TokenType::Question && self.is_ternary_question() {
                break;
            }
            match self.current().typ {
                TokenType::LParen => {
                    expr = self.parse_call(expr, type_args.take())?;
                    continue;
                }
                TokenType::Dot => {
                    self.next();
                    let member = self.parse_identifier()?;
                    let start = expr.start;
                    let length = len(n(&expr), n(&member));
                    expr = Node::with_children(start, length, NodeKind::Property, None, vec![expr, member]);
                    continue;
                }
                TokenType::FString => {
                    let template = self.parse_fstring()?;
                    let start = expr.start;
                    let length = len(n(&expr), n(&template));
                    expr = Node::with_children(start, length, NodeKind::Tagged, None, vec![expr, template]);
                    continue;
                }
                TokenType::QuestionDot => {
                    self.next();
                    let member = self.parse_identifier()?;
                    let start = expr.start;
                    let length = len(n(&expr), n(&member));
                    expr = Node::with_children(
                        start,
                        length,
                        NodeKind::Property,
                        Some(TokenType::QuestionDot),
                        vec![expr, member],
                    );
                    continue;
                }
                TokenType::LBrack => {
                    if self.looks_like_type_args(&expr) {
                        type_args = Some(self.parse_type_arg_list()?);
                        continue;
                    }
                    expr = self.parse_index_or_slice(expr)?;
                    continue;
                }
                TokenType::Question => {
                    let q = self.next();
                    let start = expr.start;
                    let length = q.start + q.length - expr.start;
                    expr = Node::with_children(start, length, NodeKind::Unary, Some(TokenType::Question), vec![expr]);
                    continue;
                }
                _ => break,
            }
        }
        Ok(expr)
    }
    fn parse_argument(&mut self) -> Result<Node, LangError> {
        if self.match_(TokenType::Asterisk) {
            let expr = self.parse_expression()?;
            return Ok(Node::with_children(
                expr.start,
                expr.length,
                NodeKind::Spread,
                None,
                vec![expr],
            ));
        }
        if self.match_(TokenType::DoubleAsterisk) {
            let expr = self.parse_expression()?;
            return Ok(Node::with_children(
                expr.start,
                expr.length,
                NodeKind::KwSpread,
                None,
                vec![expr],
            ));
        }
        let expr = self.parse_expression()?;
        if expr.kind == NodeKind::Identifier && self.match_(TokenType::Assign) {
            let value = self.parse_expression()?;
            return Ok(Node::with_children(
                expr.start,
                len(n(&expr), n(&value)),
                NodeKind::Pair,
                None,
                vec![expr, value],
            ));
        }
        Ok(expr)
    }
    fn parse_arguments(&mut self) -> Result<Vec<Node>, LangError> {
        let mut args = Vec::new();
        if self.current().typ == TokenType::RParen {
            return Ok(args);
        }
        loop {
            args.push(self.parse_argument()?);
            if !self.match_(TokenType::Comma) {
                break;
            }
            if self.current().typ == TokenType::RParen {
                break;
            }
        }
        Ok(args)
    }
    fn parse_call(&mut self, expr: Node, type_args: Option<Node>) -> Result<Node, LangError> {
        let opener = self.expect(TokenType::LParen, "expected '(' before arguments")?;
        let args = self.parse_arguments()?;
        let closer = self.expect(TokenType::RParen, "expected ')' after arguments")?;
        let length = len(t(&opener), t(&closer));
        let tuple = Node::with_children(opener.start, length, NodeKind::Tuple, None, args);
        let start = expr.start;
        let length = len(n(&expr), t(&closer));
        let mut children = vec![expr, tuple];
        if let Some(type_args) = type_args {
            children.push(type_args);
        }
        Ok(Node::with_children(start, length, NodeKind::Call, None, children))
    }
    fn parse_index_or_slice(&mut self, expr: Node) -> Result<Node, LangError> {
        let lbrack = self.expect(TokenType::LBrack, "expected '[' before index or slice")?;
        let start = if self.current().typ != TokenType::Colon && self.current().typ != TokenType::RBrack {
            Some(self.parse_expression()?)
        } else {
            None
        };
        if self.match_(TokenType::Colon) {
            let end = if self.current().typ != TokenType::Colon && self.current().typ != TokenType::RBrack {
                Some(self.parse_expression()?)
            } else {
                None
            };
            let step = if self.match_(TokenType::Colon) {
                if self.current().typ != TokenType::RBrack {
                    Some(self.parse_expression()?)
                } else {
                    None
                }
            } else {
                None
            };
            let rbrack = self.expect(TokenType::RBrack, "expected ']' to close slice")?;

            let null = Node::new(lbrack.start, 0, NodeKind::Null);
            let mut children = vec![expr];
            children.push(start.unwrap_or(null));
            children.push(end.unwrap_or(Node::new(lbrack.start, 0, NodeKind::Null)));
            children.push(step.unwrap_or(Node::new(lbrack.start, 0, NodeKind::Null)));
            let length = len(t(&lbrack), t(&rbrack));
            Ok(Node::with_children(lbrack.start, length, NodeKind::Slice, None, children))
        } else {
            let rbrack = self.expect(TokenType::RBrack, "expected ']' after index")?;
            let start = start.ok_or_else(|| {
                LangError::new(
                    lbrack.start,
                    1,
                    "expected an index expression inside '['",
                )
            })?;
            let children = vec![expr, start];
            let length = len(t(&lbrack), t(&rbrack));
            Ok(Node::with_children(lbrack.start, length, NodeKind::Index, None, children))
        }
    }
    fn parse_primary(&mut self) -> Result<Node, LangError> {
        let token = self.current();
        match token.typ {
            TokenType::Identifier => self.parse_identifier(),
            TokenType::Integer => self.parse_literal(NodeKind::Integer),
            TokenType::Float => self.parse_literal(NodeKind::Float),
            TokenType::String => self.parse_literal(NodeKind::String),
            TokenType::Bytes => self.parse_bytes_literal(),
            TokenType::FString => self.parse_fstring(),
            TokenType::True | TokenType::False => self.parse_literal(NodeKind::Boolean),
            TokenType::NoneKw => self.parse_literal(NodeKind::None),
            TokenType::LParen => self.parse_tuple(),
            TokenType::LBrace => self.parse_dict_or_set(),
            TokenType::LBrack => self.parse_list(),
            TokenType::Func => self.parse_lambda(),
            TokenType::Match => self.parse_match(),
            TokenType::Colon => Err(LangError::new(
                token.start,
                token.length,
                "expected an expression; ':' is only valid as a type annotation or match-pattern label",
            )),
            _ => Err(LangError::new(
                token.start,
                token.length,
                format!("unexpected primary token {}", token.typ),
            )),
        }
    }
    fn parse_identifier(&mut self) -> Result<Node, LangError> {
        let token = self.expect(TokenType::Identifier, "expected identifier")?;
        Ok(
            Node::new(token.start, token.length, NodeKind::Identifier)
                .with_value(Const::Str(self.get_text(token.start, token.length))),
        )
    }
    fn parse_fstring(&mut self) -> Result<Node, LangError> {
        let token = self.expect(TokenType::FString, "expected '`' to start an f-string")?;
        let raw = self.get_text(token.start, token.length);
        let quote_length = if raw.starts_with("```") { 3 } else { 1 };
        let chars: Vec<char> = raw.chars().collect();
        let text: Vec<char> = chars[quote_length..chars.len() - quote_length].to_vec();
        let mut children: Vec<Node> = Vec::new();
        let text_start = token.start + quote_length;
        let mut start = 0usize;
        let mut pos = 0usize;
        while pos < text.len() {
            if text[pos] == '\\' {
                pos += 2;
                continue;
            }
            if text[pos] != '{' {
                pos += 1;
                continue;
            }
            if pos > start {
                let piece: String = text[start..pos].iter().collect();
                children.push(
                    Node::new(text_start + start, pos - start, NodeKind::String)
                        .with_value(Const::Str(piece)),
                );
            }
            let expr_start = pos + 1;
            let mut depth = 1;
            pos += 1;
            while pos < text.len() && depth > 0 {
                if text[pos] == '\\' {
                    pos += 2;
                    continue;
                }
                if text[pos] == '{' {
                    depth += 1;
                } else if text[pos] == '}' {
                    depth -= 1;
                }
                pos += 1;
            }
            if depth != 0 {
                return Err(LangError::new(
                    text_start + expr_start - 1,
                    text.len() - expr_start + 1,
                    "unclosed interpolation",
                ));
            }
            let expr_text: String = text[expr_start..pos - 1].iter().collect();
            children.push(self.parse_interpolation(&expr_text, text_start + expr_start)?);
            start = pos;
        }
        if start < text.len() {
            let piece: String = text[start..].iter().collect();
            children.push(
                Node::new(text_start + start, text.len() - start, NodeKind::String)
                    .with_value(Const::Str(piece)),
            );
        }
        Ok(Node::with_children(token.start, token.length, NodeKind::FString, None, children))
    }
    fn parse_interpolation(&mut self, text: &str, start: usize) -> Result<Node, LangError> {

        let mut sub = Source::new(self.source.path.clone(), text);
        let mut tokens = match lex(&mut sub) {
            Ok(tokens) => tokens,
            Err(error) => {
                return Err(
                    LangError::new(start + error.start, error.length, error.message)
                        .with_header(error.header),
                );
            }
        };
        for token in tokens.iter_mut() {
            token.start += start;
        }
        let mut parser = Parser::new(tokens, self.source.clone());
        parser.parse_expression()
    }
    /// A byte literal is kept as a `Bytes` node whose children are `Integer`
    /// nodes (one byte each). `Const` has no byte variant and adding one would
    /// break non-owned exhaustive matches, so bytes are carried as children —
    /// a lossless representation for raw and non-UTF-8 data.
    fn parse_bytes_literal(&mut self) -> Result<Node, LangError> {
        let token = self.next();
        let text = self.get_text(token.start, token.length);
        let bytes = decode_bytes_literal(&text, &token)?;
        let children = bytes
            .into_iter()
            .map(|b| Node::new(token.start, 1, NodeKind::Integer).with_value(Const::Int(b as i64)))
            .collect();
        Ok(Node::with_children(
            token.start,
            token.length,
            NodeKind::Bytes,
            None,
            children,
        ))
    }
    fn parse_literal(&mut self, kind: NodeKind) -> Result<Node, LangError> {
        let token = self.next();
        let text = self.get_text(token.start, token.length);
        let value = match kind {
            NodeKind::Integer => Const::Int(parse_integer(&text, &token)?),
            NodeKind::Float => Const::Float(parse_float(&text, &token)?),
            NodeKind::String => Const::Str(unescape(&remove_quotes(&text))),
            NodeKind::Boolean => Const::Bool(text == "true"),
            NodeKind::None => return Ok(Node::new(token.start, token.length, kind)),
            _ => {
                return Err(LangError::new(
                    token.start,
                    token.length,
                    format!("invalid literal {kind:?}"),
                ))
            }
        };
        Ok(Node::new(token.start, token.length, kind).with_value(value))
    }
    fn parse_expression_list(&mut self, end: TokenType) -> Result<Vec<Node>, LangError> {
        let mut items = Vec::new();
        if self.current().typ == end {
            return Ok(items);
        }
        loop {
            if self.current().typ == end {
                break;
            }
            items.push(self.parse_expression()?);
            if !self.match_(TokenType::Comma) {
                break;
            }
        }
        Ok(items)
    }
    fn parse_tuple_items(&mut self) -> Result<Vec<Node>, LangError> {
        let mut items = Vec::new();
        while self.current().typ != TokenType::RParen {
            let mut expr = self.parse_expression()?;
            if expr.kind == NodeKind::Identifier && self.match_(TokenType::Assign) {
                let value = self.parse_expression()?;
                let start = expr.start;
                let length = len(n(&expr), n(&value));
                expr = Node::with_children(start, length, NodeKind::Pair, None, vec![expr, value]);
            }
            items.push(expr);
            if !self.match_(TokenType::Comma) {
                break;
            }
        }
        Ok(items)
    }
    fn parse_tuple(&mut self) -> Result<Node, LangError> {
        let opener = self.expect(TokenType::LParen, "expected '(' before tuple items")?;
        let values = self.parse_tuple_items()?;
        let closer = self.expect(TokenType::RParen, "expected ')' after tuple items")?;
        let length = len(t(&opener), t(&closer));
        if values.is_empty() {
            return Ok(Node::with_children(opener.start, length, NodeKind::Tuple, None, values));
        }
        if values.len() == 1 && values[0].kind != NodeKind::Pair {
            return Ok(values[0].clone());
        }
        Ok(Node::with_children(opener.start, length, NodeKind::Tuple, None, values))
    }
    fn parse_dict_or_set(&mut self) -> Result<Node, LangError> {
        let opener = self.expect(TokenType::LBrace, "expected '{'")?;
        let mut values: Vec<Node> = Vec::new();
        if self.current().typ == TokenType::RBrace {
            let closer = self.next();
            return Ok(Node::with_children(
                opener.start,
                len(t(&opener), t(&closer)),
                NodeKind::Dict,
                None,
                values,
            ));
        }
        let first_key = self.parse_expression()?;
        let kind = if self.match_(TokenType::Colon) {
            NodeKind::Dict
        } else {
            NodeKind::Set
        };
        if kind == NodeKind::Set {
            values.push(first_key);
        } else {
            let value = self.parse_expression()?;
            values.push(Node::with_children(
                first_key.start,
                len(n(&first_key), n(&value)),
                NodeKind::Pair,
                None,
                vec![first_key, value],
            ));
        }
        loop {
            if self.current().typ == TokenType::RBrace {
                break;
            }
            self.expect(TokenType::Comma, "expected ',' between elements")?;
            if self.current().typ == TokenType::RBrace {
                break;
            }
            let key = self.parse_expression()?;
            if kind == NodeKind::Set {
                if self.current().typ == TokenType::Colon {
                    let token = self.current();
                    return Err(LangError::new(
                        token.start,
                        token.length,
                        "set cannot contain key-value pairs",
                    ));
                }
                values.push(key);
            } else {
                self.expect(TokenType::Colon, "expected ':' after dictionary key")?;
                let value = self.parse_expression()?;
                values.push(Node::with_children(
                    key.start,
                    len(n(&key), n(&value)),
                    NodeKind::Pair,
                    None,
                    vec![key, value],
                ));
            }
        }
        let closer = self.expect(TokenType::RBrace, "expected '}' to close collection")?;
        Ok(Node::with_children(
            opener.start,
            len(t(&opener), t(&closer)),
            kind,
            None,
            values,
        ))
    }
    fn parse_list(&mut self) -> Result<Node, LangError> {
        let opener = self.expect(TokenType::LBrack, "expected '[' to open list")?;
        let values = self.parse_expression_list(TokenType::RBrack)?;
        let closer = self.expect(TokenType::RBrack, "expected ']' to close list")?;
        Ok(Node::with_children(
            opener.start,
            len(t(&opener), t(&closer)),
            NodeKind::List,
            None,
            values,
        ))
    }
    fn parse_function(&mut self, annotations: Vec<Node>) -> Result<Node, LangError> {
        let func = self.expect(TokenType::Func, "expected 'fn' keyword")?;
        let first = self.parse_identifier()?;

        let (kind, mut children, name) = if self.match_(TokenType::Dot) {
            (NodeKind::Extension, vec![first], self.parse_identifier()?)
        } else {
            (NodeKind::Function, Vec::new(), first)
        };
        // Optional type parameter list: `fn id[T](x: T) -> T`. Stored as a
        // trailing child so existing positional readers stay unchanged.
        let type_params = self.parse_type_params()?;
        if type_params.is_some() {
            self.generic_names.insert(self.get_text(name.start, name.length));
        }
        let parameters = self.parse_parameters()?;
        self.skip_new_lines();
        let ret_type = if self.match_(TokenType::Return) {
            Some(self.parse_type()?)
        } else {
            None
        };
        self.skip_new_lines();
        let body = self.parse_block()?;
        let length = len(t(&func), n(&body));
        children.push(name);
        children.push(parameters);
        children.push(body);
        if let Some(ret_type) = ret_type {
            children.push(ret_type);
        }
        if let Some(type_params) = type_params {
            children.push(type_params);
        }
        Ok(
            Node::with_children(func.start, length, kind, None, children)
                .with_annotations(annotations),
        )
    }
    fn parse_lambda(&mut self) -> Result<Node, LangError> {
        let func = self.expect(TokenType::Func, "expected 'fn' keyword")?;
        let parameters = self.parse_parameters()?;
        self.expect(TokenType::Return, "expected '->' after lambda parameters")?;
        let expr = self.parse_expression()?;
        let ret = Node::with_children(expr.start, expr.length, NodeKind::Return, None, vec![expr]);
        let body = Node::with_children(ret.start, ret.length, NodeKind::Block, None, vec![ret]);
        Ok(Node::with_children(
            func.start,
            len(t(&func), n(&body)),
            NodeKind::Lambda,
            None,
            vec![parameters, body],
        ))
    }
    fn parse_parameters(&mut self) -> Result<Node, LangError> {
        let opener = self.expect(TokenType::LParen, "expected '(' before parameters")?;
        let mut parameters = Vec::new();
        if self.current().typ != TokenType::RParen {
            loop {
                parameters.push(self.parse_parameter()?);
                if self.match_(TokenType::Comma) {
                    continue;
                }
                break;
            }
        }
        let closer = self.expect(TokenType::RParen, "expected ')' after parameters")?;
        Ok(Node::with_children(
            opener.start,
            len(t(&opener), t(&closer)),
            NodeKind::Tuple,
            None,
            parameters,
        ))
    }
    fn parse_parameter(&mut self) -> Result<Node, LangError> {
        let star = if matches!(
            self.current().typ,
            TokenType::Asterisk | TokenType::DoubleAsterisk
        ) {
            Some(self.next())
        } else {
            None
        };
        let name = self.parse_identifier()?;
        let mut value = name;
        if let Some(star) = star {
            let start = star.start;
            let length = len(t(&star), n(&value));
            value = Node::with_children(start, length, NodeKind::Unary, Some(star.typ), vec![value]);
        }
        if self.match_(TokenType::Colon) {
            let type_node = self.parse_type()?;
            let start = value.start;
            let length = type_node.start + type_node.length - value.start;
            value = Node::with_children(start, length, NodeKind::Typed, None, vec![value, type_node]);
        }
        if self.match_(TokenType::Assign) {
            let expr = self.parse_expression()?;
            let start = value.start;
            let length = expr.start + expr.length - value.start;
            value = Node::with_children(
                start,
                length,
                NodeKind::Assign,
                Some(TokenType::Assign),
                vec![value, expr],
            );
        }
        Ok(Node::with_children(value.start, value.length, NodeKind::Parameter, None, vec![value]))
    }
    fn parse_type(&mut self) -> Result<Node, LangError> {
        // Function type annotation: `func(int, int) -> int`. `func` is a
        // contextual keyword (an ordinary `Identifier` token), so detect it
        // here alongside the `(` that disambiguates it from a type alias
        // named `func`. The whole annotation is folded into one Identifier
        // node whose value is the exact source text; typeck's text-based
        // `parse_type_text` then sees the same shape as any other type.
        // Consuming the inner `->` here is important: otherwise it would be
        // mistaken for the enclosing `fn ... -> ...` return arrow.
        let is_func_type = self.current().typ == TokenType::Identifier
            && self.get_text(self.current().start, self.current().length) == "func"
            && self.peek_type(1) == Some(TokenType::LParen);
        let mut typ = if is_func_type {
            let func = self.next();
            self.expect(TokenType::LParen, "expected '(' after 'func' in function type")?;
            if self.current().typ != TokenType::RParen {
                self.parse_type()?;
                while self.match_(TokenType::Comma) {
                    self.parse_type()?;
                }
            }
            self.expect(TokenType::RParen, "expected ')' after function type parameters")?;
            self.expect(TokenType::Return, "expected '->' after function type parameters")?;
            let ret = self.parse_type()?;
            let length = ret.start + ret.length - func.start;
            Node::new(func.start, length, NodeKind::Identifier)
                .with_value(Const::Str(self.get_text(func.start, length)))
        } else if self.match_(TokenType::NoneKw) {
            let tok = self.previous();
            Node::new(tok.start, tok.length, NodeKind::Identifier)
                .with_value(Const::Str("none".to_string()))
        } else {
            self.parse_identifier()?
        };
        if self.match_(TokenType::LBrack) {
            let inner_start = typ.start;
            if self.current().typ != TokenType::RBrack {
                self.parse_type()?;
                while self.match_(TokenType::Comma) {
                    self.parse_type()?;
                }
            }
            let rbrack = self.expect(TokenType::RBrack, "expected ']' after type parameters")?;
            let length = len(n(&typ), t(&rbrack));
            typ = Node::new(inner_start, length, NodeKind::Identifier)
                .with_value(Const::Str(self.get_text(inner_start, length)));
        }
        while self.match_(TokenType::BOr) {
            let right = self.parse_type()?;
            let length = len(n(&typ), n(&right));
            let combined_start = typ.start;
            typ = Node::new(combined_start, length, NodeKind::Identifier)
                .with_value(Const::Str(self.get_text(combined_start, length)));
        }
        Ok(typ)
    }
    fn parse_block(&mut self) -> Result<Node, LangError> {
        self.skip_new_lines();
        self.expect(TokenType::Indent, "expected indented block")?;
        let mut statements: Vec<Node> = Vec::new();
        while self.current().typ != TokenType::Dedent && self.current().typ != TokenType::Eof {
            self.skip_new_lines();
            if self.current().typ == TokenType::Dedent {
                break;
            }
            statements.push(self.parse_statement()?);
            self.skip_new_lines();
        }
        self.expect(TokenType::Dedent, "expected end of block")?;
        if statements.is_empty() {
            return Ok(Node::new(self.current().start, 0, NodeKind::Block));
        }
        let length = len(n(&statements[0]), n(&statements[statements.len() - 1]));
        Ok(Node::with_children(
            statements[0].start,
            length,
            NodeKind::Block,
            None,
            statements,
        ))
    }
    // ---------------------------------------------------------------------
    // Assignment targets keep the original grammar. `parse_assignment` used
    // to call `parse_pattern`; routing it through the extended pattern parser
    // would change how ordinary call statements (`foo(a + b)`) are read, so
    // the old behaviour is preserved verbatim here.
    // ---------------------------------------------------------------------
    fn parse_assignment_target(&mut self) -> Result<Node, LangError> {
        let first = self.parse_primary_assignment_target()?;
        if !self.match_(TokenType::Comma) {
            return Ok(first);
        }
        let mut items = vec![first];
        loop {
            items.push(self.parse_primary_assignment_target()?);
            if !self.match_(TokenType::Comma) {
                break;
            }
        }
        let length = len(n(&items[0]), n(&items[items.len() - 1]));
        Ok(Node::with_children(items[0].start, length, NodeKind::Tuple, None, items))
    }
    fn parse_primary_assignment_target(&mut self) -> Result<Node, LangError> {
        if self.match_(TokenType::Underscore) {
            let token = self.previous();
            return Ok(Node::new(token.start, token.length, NodeKind::Wildcard));
        }
        let expr = self.parse_postfix()?;
        if self.match_(TokenType::Colon) {
            let type_node = self.parse_type()?;
            return Ok(Node::with_children(
                expr.start,
                len(n(&expr), n(&type_node)),
                NodeKind::Typed,
                None,
                vec![expr, type_node],
            ));
        }
        Ok(expr)
    }

    // ---------------------------------------------------------------------
    // Full pattern grammar (match arms / for targets):
    //   pattern     := or_pattern (',' or_pattern)*
    //   or_pattern  := primary ('|' primary)*
    //   primary     := '_' | tuple | list | constructor | atom
    //   constructor := path ('(' field (',' field)* ')')?  (':'|'is' type)?
    // ---------------------------------------------------------------------
    fn parse_pattern(&mut self) -> Result<Node, LangError> {
        let first = self.parse_or_pattern()?;
        if !self.match_(TokenType::Comma) {
            return Ok(first);
        }
        let mut items = vec![first];
        loop {
            items.push(self.parse_or_pattern()?);
            if !self.match_(TokenType::Comma) {
                break;
            }
        }
        let length = len(n(&items[0]), n(&items[items.len() - 1]));
        Ok(Node::with_children(items[0].start, length, NodeKind::Tuple, None, items))
    }
    fn parse_or_pattern(&mut self) -> Result<Node, LangError> {
        let first = self.parse_primary_pattern()?;
        if !self.match_(TokenType::BOr) {
            return Ok(first);
        }
        let mut items = vec![first];
        loop {
            items.push(self.parse_primary_pattern()?);
            if !self.match_(TokenType::BOr) {
                break;
            }
        }
        let length = len(n(&items[0]), n(&items[items.len() - 1]));
        Ok(Node::with_children(items[0].start, length, NodeKind::OrPattern, None, items))
    }
    fn parse_primary_pattern(&mut self) -> Result<Node, LangError> {
        if self.match_(TokenType::Underscore) {
            let token = self.previous();
            return Ok(Node::new(token.start, token.length, NodeKind::Wildcard));
        }
        let node = match self.current().typ {
            TokenType::LParen => self.parse_tuple_pattern()?,
            TokenType::LBrack => self.parse_list_pattern()?,
            TokenType::Identifier => self.parse_constructor_pattern()?,
            _ => self.parse_postfix()?,
        };
        self.finish_typed_pattern(node)
    }
    /// Both `x: T` and `x is T` narrow the binding to `T`. Keep the existing
    /// `Typed` node shape so downstream type/narrowing readers are unchanged.
    fn finish_typed_pattern(&mut self, node: Node) -> Result<Node, LangError> {
        let type_node = if self.match_(TokenType::Colon) {
            Some(self.parse_type()?)
        } else if self.match_(TokenType::Is) {
            Some(self.parse_type()?)
        } else {
            None
        };
        match type_node {
            Some(type_node) => Ok(Node::with_children(
                node.start,
                len(n(&node), n(&type_node)),
                NodeKind::Typed,
                None,
                vec![node, type_node],
            )),
            None => Ok(node),
        }
    }
    fn parse_tuple_pattern(&mut self) -> Result<Node, LangError> {
        let opener = self.expect(TokenType::LParen, "expected '(' to start tuple pattern")?;
        let mut items = Vec::new();
        let mut trailing_comma = false;
        while self.current().typ != TokenType::RParen && self.current().typ != TokenType::Eof {
            items.push(self.parse_pattern_arg()?);
            if self.match_(TokenType::Comma) {
                trailing_comma = true;
                continue;
            }
            trailing_comma = false;
            break;
        }
        let closer = self.expect(TokenType::RParen, "expected ')' after tuple pattern")?;
        let length = len(t(&opener), t(&closer));
        if items.len() == 1 && !trailing_comma {
            return Ok(items.remove(0));
        }
        Ok(Node::with_children(opener.start, length, NodeKind::Tuple, None, items))
    }
    fn parse_list_pattern(&mut self) -> Result<Node, LangError> {
        let opener = self.expect(TokenType::LBrack, "expected '[' to start list pattern")?;
        let mut items = Vec::new();
        while self.current().typ != TokenType::RBrack && self.current().typ != TokenType::Eof {
            items.push(self.parse_pattern_arg()?);
            if !self.match_(TokenType::Comma) {
                break;
            }
        }
        let closer = self.expect(TokenType::RBrack, "expected ']' after list pattern")?;
        Ok(Node::with_children(
            opener.start,
            len(t(&opener), t(&closer)),
            NodeKind::List,
            None,
            items,
        ))
    }
    /// Parses one pattern slot, falling back to a full expression when the
    /// slot is an ordinary expression (e.g. `case foo(a + b)`), so the new
    /// pattern grammar never rejects previously-parseable argument lists.
    fn parse_pattern_arg(&mut self) -> Result<Node, LangError> {
        let saved = self.position;
        match self.parse_or_pattern() {
            Ok(node) if matches!(self.current().typ, TokenType::Comma | TokenType::RParen | TokenType::RBrack) => {
                return Ok(node)
            }
            _ => self.position = saved,
        }
        self.parse_expression()
    }
    /// `Name(...)`, `Enum.Variant(...)` and plain paths. The produced AST is
    /// identical to the old shape for `Enum.Variant(x)` so existing consumers
    /// keep working; extra arguments and nested patterns are simply carried
    /// in the trailing `Tuple` child.
    fn parse_constructor_pattern(&mut self) -> Result<Node, LangError> {
        let mut callee = self.parse_identifier()?;
        while self.match_(TokenType::Dot) {
            let member = self.parse_identifier()?;
            let start = callee.start;
            callee = Node::with_children(
                start,
                len(n(&callee), n(&member)),
                NodeKind::Property,
                None,
                vec![callee, member],
            );
        }
        if self.current().typ != TokenType::LParen {
            return Ok(callee);
        }
        let opener = self.expect(TokenType::LParen, "expected '(' after pattern constructor")?;
        let mut args = Vec::new();
        if self.current().typ != TokenType::RParen {
            loop {
                if self.current().typ == TokenType::Identifier
                    && matches!(self.peek_type(1), Some(TokenType::Colon) | Some(TokenType::Assign))
                {
                    let name = self.parse_identifier()?;
                    let op = self.next().typ;
                    let value = self.parse_pattern_arg()?;
                    args.push(Node::with_children(
                        name.start,
                        len(n(&name), n(&value)),
                        NodeKind::Pair,
                        Some(op),
                        vec![name, value],
                    ));
                } else {
                    args.push(self.parse_pattern_arg()?);
                }
                if !self.match_(TokenType::Comma) {
                    break;
                }
                if self.current().typ == TokenType::RParen {
                    break;
                }
            }
        }
        let closer = self.expect(TokenType::RParen, "expected ')' after pattern arguments")?;
        let tuple = Node::with_children(
            opener.start,
            len(t(&opener), t(&closer)),
            NodeKind::Tuple,
            None,
            args,
        );
        Ok(Node::with_children(
            callee.start,
            len(n(&callee), t(&closer)),
            NodeKind::Call,
            None,
            vec![callee, tuple],
        ))
    }
    fn is_expression_start(&self, typ: TokenType) -> bool {
        matches!(
            typ,
            TokenType::Identifier
                | TokenType::Integer
                | TokenType::Float
                | TokenType::String
                | TokenType::Bytes
                | TokenType::FString
                | TokenType::True
                | TokenType::False
                | TokenType::NoneKw
                | TokenType::LParen
                | TokenType::LBrack
                | TokenType::LBrace
                | TokenType::Func
                | TokenType::Match
                | TokenType::Not
                | TokenType::Plus
                | TokenType::Minus
                | TokenType::BNot
                | TokenType::Wait
                | TokenType::Go
                | TokenType::Asterisk
        )
    }
    fn is_assignment_start(&self) -> bool {
        matches!(
            self.current().typ,
            TokenType::Identifier | TokenType::LParen | TokenType::Underscore
        )
    }
    fn is_valid_assignment_target(&self, node: &Node) -> bool {
        match node.kind {
            NodeKind::Identifier => true,
            NodeKind::Property => true,
            NodeKind::Index => true,
            NodeKind::Slice => true,
            NodeKind::Wildcard => true,
            NodeKind::Typed => {
                node.children.is_empty()
                    || self.is_valid_assignment_target(&node.children[0])
            }
            NodeKind::Tuple => node.children.iter().all(|child| self.is_valid_assignment_target(child)),
            _ => false,
        }
    }
    fn get_binary_precedence(typ: TokenType) -> usize {
        match typ {
            TokenType::Or => 1,
            TokenType::Xor => 2,
            TokenType::And => 3,
            TokenType::Equal => 4,
            TokenType::Unequal => 4,
            TokenType::LessThan => 5,
            TokenType::LessOrEqual => 5,
            TokenType::GreaterThan => 5,
            TokenType::GreaterOrEqual => 5,
            TokenType::Is => 5,
            TokenType::IsNot => 5,
            TokenType::In => 5,
            TokenType::BOr => 7,
            TokenType::BXor => 8,
            TokenType::BAnd => 9,
            TokenType::DotDot => 6,
            TokenType::LeftShift => 10,
            TokenType::RightShift => 10,
            TokenType::Plus => 11,
            TokenType::Minus => 11,
            TokenType::Asterisk => 12,
            TokenType::Slash => 12,
            TokenType::DoubleSlash => 12,
            TokenType::Percent => 12,
            TokenType::DoubleAsterisk => 13,
            TokenType::DoubleQuestion => 1,
            _ => 0,
        }
    }
}
fn remove_quotes(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() >= 6
        && ((chars[0] == '"' && chars[1] == '"' && chars[2] == '"' && chars[chars.len() - 1] == '"' && chars[chars.len() - 2] == '"' && chars[chars.len() - 3] == '"')
            || (chars[0] == '\'' && chars[1] == '\'' && chars[2] == '\'' && chars[chars.len() - 1] == '\'' && chars[chars.len() - 2] == '\'' && chars[chars.len() - 3] == '\''))
    {
        return chars[3..chars.len() - 3].iter().collect();
    }
    chars[1..chars.len() - 1].iter().collect()
}
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('\'') => out.push('\''),
            Some(c) => {
                out.push('\\');
                out.push(c);
            }
            None => out.push('\\'),
        }
    }
    out
}
fn parse_integer(text: &str, token: &Token) -> Result<i64, LangError> {
    let too_large = || LangError::new(token.start, token.length, "integer literal is too large");
    let t = text.replace('_', "");
    if t.starts_with("0x") || t.starts_with("0X") {
        return i64::from_str_radix(t[2..].trim(), 16).map_err(|_| too_large());
    }
    if t.starts_with("0b") || t.starts_with("0B") {
        return i64::from_str_radix(&t[2..], 2).map_err(|_| too_large());
    }
    if t.starts_with("0o") || t.starts_with("0O") {
        return i64::from_str_radix(&t[2..], 8).map_err(|_| too_large());
    }
    t.trim().parse::<i64>().map_err(|_| too_large())
}
fn parse_float(text: &str, token: &Token) -> Result<f64, LangError> {
    text.replace('_', "")
        .parse::<f64>()
        .map_err(|_| LangError::new(token.start, token.length, "invalid floating-point literal"))
}
fn hex_digit(c: char) -> Option<u8> {
    match c {
        '0'..='9' => Some(c as u8 - b'0'),
        'a'..='f' => Some(c as u8 - b'a' + 10),
        'A'..='F' => Some(c as u8 - b'A' + 10),
        _ => None,
    }
}
/// Decodes the `b` prefix, quotes and escapes of a byte-string literal.
/// Raw non-ASCII characters are encoded as UTF-8; `\xHH` writes a raw byte.
fn decode_bytes_literal(text: &str, token: &Token) -> Result<Vec<u8>, LangError> {
    let error = || LangError::new(token.start, token.length, "invalid bytes literal");
    let chars: Vec<char> = text.chars().collect();
    if chars.len() < 3 {
        return Err(error());
    }
    let quote = chars[1];
    if quote != '"' && quote != '\'' {
        return Err(error());
    }
    let quote_len = if chars.len() >= 4 && chars[2] == quote && chars[3] == quote {
        3
    } else {
        1
    };
    let body_start = 1 + quote_len;
    let body_end = chars.len().checked_sub(quote_len).ok_or_else(error)?;
    if body_start > body_end {
        return Err(error());
    }
    let mut out = Vec::with_capacity(body_end - body_start);
    let mut i = body_start;
    while i < body_end {
        let c = chars[i];
        if c != '\\' {
            let mut buf = [0u8; 4];
            out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            i += 1;
            continue;
        }
        i += 1;
        if i >= body_end {
            return Err(error());
        }
        match chars[i] {
            'n' => out.push(b'\n'),
            'r' => out.push(b'\r'),
            't' => out.push(b'\t'),
            '0' => out.push(0),
            '\\' => out.push(b'\\'),
            '"' => out.push(b'"'),
            '\'' => out.push(b'\''),
            'x' => {
                if i + 2 >= body_end {
                    return Err(error());
                }
                let hi = hex_digit(chars[i + 1]).ok_or_else(error)?;
                let lo = hex_digit(chars[i + 2]).ok_or_else(error)?;
                out.push((hi << 4) | lo);
                i += 2;
            }
            other => {
                out.push(b'\\');
                let mut buf = [0u8; 4];
                out.extend_from_slice(other.encode_utf8(&mut buf).as_bytes());
            }
        }
        i += 1;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_text(text: &str) -> Node {
        let mut source = Source::new("test.un", text);
        let tokens = lex(&mut source).expect("lex");
        parse(tokens, Rc::new(source)).expect("parse")
    }

    /// Collect the exact source text of every type annotation node that
    /// mentions a function type. Function types are ordinary Identifier
    /// nodes carrying the whole annotation as a `Const::Str` value.
    fn func_type_texts(node: &Node, out: &mut Vec<String>) {
        if let Some(Const::Str(text)) = &node.value {
            if text.contains("func(") {
                out.push(text.clone());
            }
        }
        for child in &node.children {
            func_type_texts(child, out);
        }
    }

    #[test]
    fn parses_func_type_parameter_and_return_arrow() {
        let root = parse_text("fn apply(f: func(int) -> int, x: int) -> int\n    -> f(x)\n");
        let mut texts = Vec::new();
        func_type_texts(&root, &mut texts);
        assert_eq!(texts, vec!["func(int) -> int".to_string()]);
    }

    #[test]
    fn parses_zero_argument_func_type() {
        let root = parse_text("fn apply(f: func() -> int) -> int\n    -> f()\n");
        let mut texts = Vec::new();
        func_type_texts(&root, &mut texts);
        assert_eq!(texts, vec!["func() -> int".to_string()]);
    }

    #[test]
    fn parses_func_type_as_generic_argument_and_union() {
        let root = parse_text(
            "fn a(xs: list[func(int) -> int]) -> int\n    -> 0\n\
             fn b(f: func(int) -> int | str) -> int\n    -> 0\n",
        );
        let mut texts = Vec::new();
        func_type_texts(&root, &mut texts);
        assert_eq!(
            texts,
            vec![
                "list[func(int) -> int]".to_string(),
                "func(int) -> int | str".to_string(),
            ]
        );
    }

    #[test]
    fn parses_nested_func_type() {
        let root = parse_text(
            "fn apply(f: func(func(int) -> int) -> int) -> int\n    -> f\n",
        );
        let mut texts = Vec::new();
        func_type_texts(&root, &mut texts);
        assert_eq!(texts, vec!["func(func(int) -> int) -> int".to_string()]);
    }

    #[test]
    fn parses_func_type_as_function_return_type() {
        let root = parse_text("fn make() -> func(int) -> int\n    -> f\n");
        let mut texts = Vec::new();
        func_type_texts(&root, &mut texts);
        assert_eq!(texts, vec!["func(int) -> int".to_string()]);
    }
}
