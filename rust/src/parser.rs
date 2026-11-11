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
}

/// `(start, length)` pair, mirroring the reference `ISpan`
type Span = (usize, usize);

fn n(node: &Node) -> Span {
    (node.start, node.length)
}

fn t(token: &Token) -> Span {
    (token.start, token.length)
}

/// mirrors `GetLength(ISpan start, ISpan end)`
fn len(start: Span, end: Span) -> usize {
    end.0 + end.1 - start.0
}

/// mirrors `RequireNoAnnotations`, turned into a check so the caller owns the parser method
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
        }
    }

    /// `Token` carries a `String` message, so it is not `Copy`; the reference reads
    /// tokens by value, so clones keep the same semantics here
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

        let statement = if self.is_accessor_start() {
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

        if !self.is_end_of_statement() && !self.is_statement_start(self.current().typ) {
            let token = self.current();
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
        let mut path = self.parse_identifier()?;

        while self.match_(TokenType::Dot) {
            if self.current().typ == TokenType::Asterisk {
                let star = self.next();
                return Ok(Node::with_children(
                    path.start,
                    len(n(&path), t(&star)),
                    NodeKind::Unary,
                    Some(TokenType::Wildcard),
                    vec![path],
                ));
            }

            let member = self.parse_identifier()?;
            let start = path.start;
            let length = len(n(&path), n(&member));

            path = Node::with_children(start, length, NodeKind::Property, None, vec![path, member]);
        }

        Ok(path)
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

        // the reference uses `imports[^1].Length`, not the full span; reproduced verbatim
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

    fn parse_class(&mut self, annotations: Vec<Node>) -> Result<Node, LangError> {
        let keyword = self.expect(TokenType::Class, "expected 'class' keyword")?;
        let name = self.parse_identifier()?;

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

        Ok(Node::with_children(keyword.start, length, NodeKind::Class, None, children)
            .with_annotations(annotations))
    }

    fn parse_enum_member(&mut self) -> Result<Node, LangError> {
        let name = self.parse_identifier()?;

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
            let pattern = self.parse_pattern()?;
            self.expect(TokenType::Return, "expected '->' after match pattern")?;
            let body = self.parse_ternary()?;

            cases.push(Node::with_children(
                pattern.start,
                len(n(&pattern), n(&body)),
                NodeKind::Case,
                None,
                vec![pattern, body],
            ));

            // trailing comma is optional
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

        // `for <iterable>` with no binding target: the header has no `In` at bracket
        // depth 0, so the iterable is parsed as an expression and the target slot is a
        // zero-length `Wildcard` at the iterable's start. The body is unchanged.
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

    /// `HasForHeaderIn` looks ahead over the header tokens without consuming them,
    /// counting bracket depth. `In` at depth 0 is the binding form; a newline, indent
    /// or dedent at depth 0 (or the end of the token list) is the iterable-only form.
    /// `for x in f(a, b) : ...` keeps depth > 0, so the `In` inside brackets never
    /// makes the header look like a binding form.
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
        let left = self.parse_pattern()?;

        if !self.is_assignment_operator(self.current().typ) {
            return Ok(left);
        }

        let op = self.next();
        let right = self.parse_comma()?;

        if self.is_assignment_operator(self.current().typ) {
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

            let right = self.parse_binary(precedence)?;
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

    fn parse_postfix(&mut self) -> Result<Node, LangError> {
        let mut expr = self.parse_primary()?;

        loop {
            if self.current().typ == TokenType::Question && self.is_ternary_question() {
                break;
            }

            match self.current().typ {
                TokenType::LParen => {
                    expr = self.parse_call(expr)?;
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

    fn parse_call(&mut self, expr: Node) -> Result<Node, LangError> {
        let opener = self.expect(TokenType::LParen, "expected '(' before arguments")?;
        let args = self.parse_arguments()?;
        let closer = self.expect(TokenType::RParen, "expected ')' after arguments")?;

        let length = len(t(&opener), t(&closer));
        let tuple = Node::with_children(opener.start, length, NodeKind::Tuple, None, args);

        let length = len(n(&expr), t(&closer));

        Ok(Node::with_children(expr.start, length, NodeKind::Call, None, vec![expr, tuple]))
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

            // the reference stores `null` children for omitted bounds
            let null = Node::new(lbrack.start, 0, NodeKind::Null);

            let mut children = vec![expr];
            children.push(start.unwrap_or(null));
            children.push(end.unwrap_or(Node::new(lbrack.start, 0, NodeKind::Null)));
            children.push(step.unwrap_or(Node::new(lbrack.start, 0, NodeKind::Null)));

            let length = len(t(&lbrack), t(&rbrack));

            Ok(Node::with_children(lbrack.start, length, NodeKind::Slice, None, children))
        } else {
            let rbrack = self.expect(TokenType::RBrack, "expected ']' after index")?;

            let start = start.unwrap();
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
            TokenType::FString => self.parse_fstring(),
            TokenType::True | TokenType::False => self.parse_literal(NodeKind::Boolean),
            TokenType::NoneKw => self.parse_literal(NodeKind::None),
            TokenType::LParen => self.parse_tuple(),
            TokenType::LBrace => self.parse_dict_or_set(),
            TokenType::LBrack => self.parse_list(),
            TokenType::Func => self.parse_lambda(),
            TokenType::Match => self.parse_match(),

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
        // the reference lexes the sub-source but parses it with the outer source,
        // so `GetText` and error positions keep referring to the original file
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

        // `fn type.name(...)` is an extension, `fn name(...)` is a function
        let (kind, mut children, name) = if self.match_(TokenType::Dot) {
            (NodeKind::Extension, vec![first], self.parse_identifier()?)
        } else {
            (NodeKind::Function, Vec::new(), first)
        };

        let parameters = self.parse_parameters()?;
        self.skip_new_lines();
        let body = self.parse_block()?;

        let length = len(t(&func), n(&body));

        children.push(name);
        children.push(parameters);
        children.push(body);

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
        let mut typ = self.parse_identifier()?;

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

    fn parse_pattern(&mut self) -> Result<Node, LangError> {
        let first = self.parse_primary_pattern()?;

        if !self.match_(TokenType::Comma) {
            return Ok(first);
        }

        let mut items = vec![first];

        loop {
            items.push(self.parse_primary_pattern()?);

            if !self.match_(TokenType::Comma) {
                break;
            }
        }

        let length = len(n(&items[0]), n(&items[items.len() - 1]));

        Ok(Node::with_children(items[0].start, length, NodeKind::Tuple, None, items))
    }

    fn parse_primary_pattern(&mut self) -> Result<Node, LangError> {
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

    fn is_expression_start(&self, typ: TokenType) -> bool {
        matches!(
            typ,
            TokenType::Identifier
                | TokenType::Integer
                | TokenType::Float
                | TokenType::String
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

    fn is_assignment_operator(&self, typ: TokenType) -> bool {
        matches!(
            typ,
            TokenType::Assign
                | TokenType::ColonAssign
                | TokenType::PlusAssign
                | TokenType::MinusAssign
                | TokenType::AsteriskAssign
                | TokenType::SlashAssign
                | TokenType::DoubleSlashAssign
                | TokenType::DoubleAsteriskAssign
                | TokenType::PercentAssign
                | TokenType::BAndAssign
                | TokenType::BOrAssign
                | TokenType::BXorAssign
                | TokenType::LeftShiftAssign
                | TokenType::RightShiftAssign
                | TokenType::DoubleQuestionAssign
        )
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
            Some(c) => out.push(c),
            None => break,
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
