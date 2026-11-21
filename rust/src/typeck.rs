use crate::ast::{Const, Node, NodeKind};
use crate::error::LangError;
use crate::source::Source;
use crate::token::TokenType;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

const MAX_ERRORS: usize = 50;
const MAX_ALIAS_DEPTH: usize = 8;


#[derive(Clone, PartialEq)]
pub enum Ty {
    Int,
    Float,
    Bool,
    Str,
    None_,
    List(Rc<Ty>),
    Dict(Rc<Ty>, Rc<Ty>),
    Set(Rc<Ty>),
    Tuple(Rc<[Ty]>),
    Union(Rc<[Ty]>),
    Any,
    Class(Rc<str>),
    Func {
        params: Rc<[Ty]>,
        names: Rc<[String]>,
        ret: Rc<Ty>,
        /// Minimum number of arguments (trailing params may have defaults).
        required: usize,
    },
    Unknown,
}

impl Ty {
    fn list(elem: Ty) -> Ty {
        Ty::List(Rc::new(elem))
    }
    fn dict(k: Ty, v: Ty) -> Ty {
        Ty::Dict(Rc::new(k), Rc::new(v))
    }
    fn tuple(items: Vec<Ty>) -> Ty {
        Ty::Tuple(items.into())
    }
    pub fn union(members: Vec<Ty>) -> Ty {
        let mut flat: Vec<Ty> = Vec::new();
        for member in members {
            match member {
                Ty::Union(inner) => flat.extend(inner.iter().cloned()),
                other => flat.push(other),
            }
        }
        if flat.iter().any(|t| matches!(t, Ty::Any)) {
            return Ty::Any;
        }
        let mut uniq: Vec<Ty> = Vec::new();
        for t in flat {
            if !uniq.contains(&t) {
                uniq.push(t);
            }
        }
        match uniq.len() {
            0 => Ty::None_,
            1 => uniq.pop().unwrap(),
            _ => {
                uniq.sort_by_key(type_sort_key);
                Ty::Union(uniq.into())
            }
        }
    }
    pub fn display(&self) -> String {
        match self {
            Ty::Int => "int".to_string(),
            Ty::Float => "float".to_string(),
            Ty::Bool => "bool".to_string(),
            Ty::Str => "str".to_string(),
            Ty::None_ => "none".to_string(),
            Ty::List(item) => format!("list[{}]", item.display()),
            Ty::Dict(k, v) => format!("dict[{}, {}]", k.display(), v.display()),
            Ty::Set(item) => format!("set[{}]", item.display()),
            Ty::Tuple(items) => {
                let inner = items
                    .iter()
                    .map(|t| t.display())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("tuple[{inner}]")
            }
            Ty::Union(members) => members
                .iter()
                .map(|t| t.display())
                .collect::<Vec<_>>()
                .join(" | "),
            Ty::Any => "any".to_string(),
            Ty::Class(name) => name.to_string(),
            Ty::Func { params, ret, .. } => {
                let inner = params
                    .iter()
                    .map(|t| t.display())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("fn({inner}) -> {}", ret.display())
            }
            Ty::Unknown => "<unknown>".to_string(),
        }
    }
    fn is_numeric(&self) -> bool {
        matches!(self, Ty::Int | Ty::Float | Ty::Any)
    }
    fn is_integer_ish(&self) -> bool {
        matches!(self, Ty::Int | Ty::Any)
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display())
    }
}

impl fmt::Debug for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display())
    }
}

fn type_sort_key(t: &Ty) -> String {
    t.display()
}


#[derive(Clone, Debug)]
pub struct FnSig {
    pub name: String,
    pub params: Vec<Ty>,
    pub param_names: Vec<String>,
    pub ret: Ty,
    pub required: usize,
    pub is_method: bool,
    pub receiver: usize,
    pub variadic: bool,
}

#[derive(Clone)]
pub struct FnCheck {
    pub sig: FnSig,
    pub locals: Vec<(String, Ty)>,
    pub local_types: HashMap<String, Ty>,
}

#[derive(Default)]
pub struct CheckedProgram {
    pub sigs: HashMap<String, FnSig>,
    pub classes: HashMap<String, ClassInfo>,
    pub aliases: HashMap<String, Ty>,
    pub functions: Vec<FnCheck>,
    pub fn_by_span: HashMap<usize, usize>,
}

#[derive(Clone, Default)]
pub struct ClassInfo {
    pub fields: HashMap<String, Ty>,
    pub methods: HashMap<String, Ty>,
    pub bases: Vec<String>,
    pub has_getattr: bool,
    pub setters: Vec<String>,
    pub ctor_params: Vec<(String, Ty)>,
    pub ctor_required: usize,
    /// Declared field order (positional ctor argument order).
    pub field_order: Vec<String>,
}




#[derive(Clone)]
struct DeclFn<'a> {
    name: String,
    node: &'a Node,
    is_method: bool,
    class_name: Option<String>,
}

pub struct Checker<'a> {
    source: &'a Source,
    root: &'a Node,
    errors: Vec<LangError>,
    aliases: HashMap<String, Ty>,
    functions: Vec<DeclFn<'a>>,
    classes: HashMap<String, ClassInfo>,
    class_order: Vec<String>,
    globals: HashMap<String, Ty>,
    import_names: Vec<String>,
    enums: Vec<String>,
    current_ret: Option<Ty>,
    fn_sigs: HashMap<String, FnSig>,
    sigs_by_span: HashMap<usize, FnSig>,
    silence: u32,
    checked_functions: Vec<FnCheck>,
    checked_by_span: HashMap<usize, usize>,
}

impl<'a> Checker<'a> {
    fn new(source: &'a Source, root: &'a Node) -> Self {
        Self {
            source,
            root,
            errors: Vec::new(),
            aliases: HashMap::new(),
            functions: Vec::new(),
            classes: HashMap::new(),
            class_order: Vec::new(),
            globals: HashMap::new(),
            import_names: Vec::new(),
            enums: Vec::new(),
            current_ret: None,
            fn_sigs: HashMap::new(),
            sigs_by_span: HashMap::new(),
            silence: 0,
            checked_functions: Vec::new(),
            checked_by_span: HashMap::new(),
        }
    }

    fn span(&self, node: &Node) -> (usize, usize) {
        (node.start, node.length)
    }

    fn error(&mut self, node: &Node, message: impl Into<String>) {
        if self.errors.len() >= MAX_ERRORS || self.silence > 0 {
            return;
        }
        let (start, length) = self.span(node);
        self.errors
            .push(LangError::new(start, length, message).with_header("compile error"));
    }

    fn error_span(&mut self, start: usize, length: usize, message: impl Into<String>) {
        if self.errors.len() >= MAX_ERRORS || self.silence > 0 {
            return;
        }
        self.errors
            .push(LangError::new(start, length, message).with_header("compile error"));
    }

    fn text_of(&self, node: &Node) -> String {
        self.source.span(node.start, node.length)
    }

    pub fn run(mut self) -> Result<CheckedProgram, Vec<LangError>> {
        self.collect_block(self.root);
        let checked = self.check_all();
        if self.errors.is_empty() {
            Ok(checked)
        } else {
            Err(self.errors)
        }
    }
}



impl<'a> Checker<'a> {
    fn try_type_text(&self, text: &str) -> Option<Ty> {
        self.try_union_text(text.trim(), 0)
    }

    fn try_union_text(&self, text: &str, depth: usize) -> Option<Ty> {
        if depth > MAX_ALIAS_DEPTH {
            return None;
        }
        let alts = split_union(text);
        if alts.iter().any(|a| a.trim().is_empty()) {
            return None;
        }
        let mut members = Vec::with_capacity(alts.len());
        for alt in alts {
            members.push(self.try_atom_text(alt.trim(), depth)?);
        }
        Some(Ty::union(members))
    }

    fn try_atom_text(&self, text: &str, depth: usize) -> Option<Ty> {
        if let Some(open) = find_top_bracket(text) {
            let head = text[..open].trim();
            if !matches!(head, "list" | "set" | "dict" | "tuple") {
                return None;
            }
            let close = text.rfind(']')?;
            let parts = split_top_level(&text[open + 1..close], ',');
            let args: Vec<Ty> = parts
                .iter()
                .map(|p| self.try_union_text(p.trim(), depth + 1))
                .collect::<Option<Vec<_>>>()?;
            return match head {
                "list" if args.len() == 1 => Some(Ty::list(args.into_iter().next().unwrap())),
                "set" if args.len() == 1 => Some(Ty::Set(Rc::new(args.into_iter().next().unwrap()))),
                "dict" if args.len() == 2 => {
                    let mut it = args.into_iter();
                    Some(Ty::dict(it.next().unwrap(), it.next().unwrap()))
                }
                "tuple" => Some(Ty::Tuple(args.into())),
                _ => None,
            };
        }
        let name = text.trim();
        match name {
            "int" => Some(Ty::Int),
            "float" => Some(Ty::Float),
            "bool" => Some(Ty::Bool),
            "str" => Some(Ty::Str),
            "none" => Some(Ty::None_),
            "any" => Some(Ty::Any),
            _ => {
                if let Some(alias) = self.aliases.get(name) {
                    return Some(alias.clone());
                }
                if self.classes.contains_key(name) {
                    return Some(Ty::Class(Rc::from(name)));
                }
                None
            }
        }
    }

    fn parse_type_text(&mut self, text: &str, span: (usize, usize), depth: usize) -> Ty {
        if depth > MAX_ALIAS_DEPTH {
            self.error_span(span.0, span.1, "type alias nesting too deep (cycle?)");
            return Ty::Unknown;
        }
        let alts = split_union(text);
        if alts.iter().any(|a| a.is_empty()) {
            self.error_span(span.0, span.1, "empty alternative in type union");
            return Ty::Unknown;
        }
        let mut members = Vec::with_capacity(alts.len());
        for alt in alts {
            members.push(self.parse_type_atom(alt.trim(), span, depth));
        }
        Ty::union(members)
    }

    fn parse_type_atom(&mut self, text: &str, span: (usize, usize), depth: usize) -> Ty {
        if let Some(open) = find_top_bracket(text) {
            let head = text[..open].trim();
            let Some(close) = text.rfind(']') else {
                self.error_span(span.0, span.1, "unbalanced `[` in type annotation");
                return Ty::Unknown;
            };
            let inner = &text[open + 1..close];
            let parts = split_top_level(inner, ',');
            let args: Vec<Ty> = parts
                .iter()
                .map(|part| self.parse_type_text(part.trim(), span, depth + 1))
                .collect();
            match head {
                "list" | "set" => {
                    if args.len() != 1 {
                        self.error_span(span.0, span.1, format!("'{head}' takes exactly one type argument"));
                        return Ty::Unknown;
                    }
                    let item = args.into_iter().next().unwrap();
                    if head == "list" {
                        return Ty::list(item);
                    }
                    return Ty::Set(Rc::new(item));
                }
                "dict" => {
                    if args.len() != 2 {
                        self.error_span(span.0, span.1, "'dict' takes exactly two type arguments");
                        return Ty::Unknown;
                    }
                    let mut it = args.into_iter();
                    let k = it.next().unwrap();
                    let v = it.next().unwrap();
                    return Ty::dict(k, v);
                }
                "tuple" => {
                    if args.len() == 1 {
                        return Ty::Tuple(Rc::from(vec![args[0].clone()]));
                    }
                    return Ty::tuple(args);
                }
                _ => {
                    self.error_span(span.0, span.1, format!("unknown generic type '{head}'"));
                    return Ty::Unknown;
                }
            }
        }
        self.resolve_type_atom(text, span, depth)
    }

    fn resolve_type_atom(&mut self, name: &str, span: (usize, usize), depth: usize) -> Ty {
        match name {
            "int" => Ty::Int,
            "float" => Ty::Float,
            "bool" => Ty::Bool,
            "str" => Ty::Str,
            "none" => Ty::None_,
            "any" => Ty::Any,
            n if is_fixed_width_alias(n) => {
                if self.import_names.iter().any(|i| i == "types") {
                    match n {
                        "f32" | "f64" => Ty::Float,
                        _ => Ty::Int,
                    }
                } else {
                    if depth == 0 {
                        self.error_span(
                            span.0,
                            span.1,
                            format!("unknown type '{n}' (fixed-width aliases require 'use types')"),
                        );
                    }
                    Ty::Unknown
                }
            }
            _ => {
                if let Some(alias) = self.aliases.get(name) {
                    return alias.clone();
                }
                if self.classes.contains_key(name) {
                    return Ty::Class(Rc::from(name));
                }
                if self.enums.iter().any(|n| n == name) || self.import_names.iter().any(|n| n == name) {
                    return Ty::Any;
                }
                if depth == 0 {
                    self.error_span(span.0, span.1, format!("unknown type '{name}'"));
                }
                Ty::Unknown
            }
        }
    }

    fn parse_annotation_node(&mut self, node: &Node) -> Ty {
        self.parse_type_text(&self.text_of(node), self.span(node), 0)
    }
}

fn is_fixed_width_alias(name: &str) -> bool {
    matches!(
        name,
        "u8" | "u16" | "u32" | "u64" | "i8" | "i16" | "i32" | "i64" | "f32" | "f64"
    )
}

fn fixed_width_bounds(name: &str) -> Option<(&'static str, (i128, i128))> {
    Some(match name {
        "u8" => ("u8", (0, 255)),
        "u16" => ("u16", (0, u16::MAX as i128)),
        "u32" => ("u32", (0, u32::MAX as i128)),
        "u64" => ("u64", (0, i64::MAX as i128)),
        "i8" => ("i8", (i8::MIN as i128, i8::MAX as i128)),
        "i16" => ("i16", (i16::MIN as i128, i16::MAX as i128)),
        "i32" => ("i32", (i32::MIN as i128, i32::MAX as i128)),
        "i64" => ("i64", (i64::MIN as i128, i64::MAX as i128)),
        _ => return None,
    })
}

fn const_int_value(node: &Node, source: &Source) -> Option<i128> {
    match node.kind {
        NodeKind::Integer => {
            if let Some(crate::ast::Const::Int(v)) = &node.value {
                return Some(*v as i128);
            }
            let text = source.span(node.start, node.length).replace('_', "");
            let (unsigned, negative) = match text.chars().next() {
                Some('-') => (text[1..].to_string(), true),
                _ => (text, false),
            };
            let value = if let Some(hex) = unsigned.strip_prefix("0x").or_else(|| unsigned.strip_prefix("0X")) {
                i128::from_str_radix(hex, 16).ok()?
            } else if let Some(bin) = unsigned.strip_prefix("0b").or_else(|| unsigned.strip_prefix("0B")) {
                i128::from_str_radix(bin, 2).ok()?
            } else if let Some(oct) = unsigned.strip_prefix("0o").or_else(|| unsigned.strip_prefix("0O")) {
                i128::from_str_radix(oct, 8).ok()?
            } else {
                unsigned.parse::<i128>().ok()?
            };
            Some(if negative { -value } else { value })
        }
        NodeKind::Unary => match node.op? {
            TokenType::Minus => const_int_value(node.children.first()?, source).map(|v| -v),
            TokenType::Positive => const_int_value(node.children.first()?, source),
            TokenType::BNot => const_int_value(node.children.first()?, source).map(|v| !v),
            _ => None,
        },
        NodeKind::Binary => {
            let lhs = const_int_value(node.children.first()?, source)?;
            let rhs = const_int_value(node.children.get(1)?, source)?;
            match node.op? {
                TokenType::Plus => Some(lhs + rhs),
                TokenType::Minus => Some(lhs - rhs),
                TokenType::Asterisk => Some(lhs * rhs),
                TokenType::LeftShift => lhs.checked_shl(u32::try_from(rhs).ok()?),
                TokenType::RightShift => lhs.checked_shr(u32::try_from(rhs).ok()?),
                TokenType::BAnd => Some(lhs & rhs),
                TokenType::BOr => Some(lhs | rhs),
                TokenType::BXor => Some(lhs ^ rhs),
                TokenType::DoubleSlash => {
                    if rhs == 0 {
                        return None;
                    }
                    let (mut q, r) = (lhs / rhs, lhs % rhs);
                    if r != 0 && ((lhs < 0) != (rhs < 0)) {
                        q -= 1;
                    }
                    Some(q)
                }
                TokenType::Percent => {
                    if rhs == 0 {
                        return None;
                    }
                    let div = lhs / rhs;
                    let mut q = div;
                    let r = lhs % rhs;
                    if r != 0 && ((lhs < 0) != (rhs < 0)) {
                        q -= 1;
                    }
                    Some(lhs - rhs * q)
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn split_union(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'[' => depth += 1,
            b']' => depth -= 1,
            b'|' if depth == 0 => {
                out.push(text[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    out.push(text[start..].trim());
    out
}

fn find_top_bracket(text: &str) -> Option<usize> {
    let mut depth = 0i32;
    for (index, ch) in text.char_indices() {
        match ch {
            '[' if depth == 0 => return Some(index),
            '[' => depth += 1,
            ']' => depth -= 1,
            _ => {}
        }
    }
    None
}

fn split_top_level(s: &str, sep: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (index, ch) in s.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => depth -= 1,
            c if c == sep && depth == 0 => {
                parts.push(s[start..index].trim());
                start = index + c.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(s[start..].trim());
    parts
}


impl<'a> Checker<'a> {
    fn collect_block(&mut self, node: &'a Node) {
        for child in &node.children {
            self.collect_stmt(child);
        }
    }

    fn collect_stmt(&mut self, node: &'a Node) {
        match node.kind {
            NodeKind::Function | NodeKind::Extension => {
                self.collect_function_decl(node);
                if let Some(body) = body_block(node) {
                    self.collect_nested_block(body);
                }
            }
            NodeKind::Lambda => {
                if let Some(body) = body_block(node) {
                    self.collect_nested_block(body);
                }
            }
            NodeKind::Class => self.collect_class(node),
            NodeKind::Enum => {
                if let Some(name) = first_identifier_text(node, self.source) {
                    if !self.enums.iter().any(|n| n == &name) {
                        self.enums.push(name);
                    }
                }
            }
            NodeKind::Use => self.collect_use(node),
            NodeKind::Assign | NodeKind::Typed => {
                self.collect_module_var(node);
            }
            NodeKind::Block
            | NodeKind::If
            | NodeKind::ElIf
            | NodeKind::Else
            | NodeKind::While
            | NodeKind::For
            | NodeKind::Match
            | NodeKind::Case
            | NodeKind::Try
            | NodeKind::Catch
            | NodeKind::Finally
            | NodeKind::Using
            | NodeKind::Defer => {
                for child in &node.children {
                    self.collect_stmt(child);
                }
            }
            _ => {}
        }
    }

    fn collect_nested_block(&mut self, node: &'a Node) {
        for child in &node.children {
            self.collect_nested_stmt(child);
        }
    }

    fn collect_nested_stmt(&mut self, node: &'a Node) {
        match node.kind {
            NodeKind::Function | NodeKind::Extension => {
                self.collect_function_decl(node);
                if let Some(body) = body_block(node) {
                    self.collect_nested_block(body);
                }
            }
            NodeKind::Lambda => {
                if let Some(body) = body_block(node) {
                    self.collect_nested_block(body);
                }
            }
            NodeKind::Class => self.collect_class(node),
            NodeKind::Enum => {
                if let Some(name) = first_identifier_text(node, self.source) {
                    if !self.enums.iter().any(|n| n == &name) {
                        self.enums.push(name);
                    }
                }
            }
            NodeKind::Use => {}
            NodeKind::Assign | NodeKind::Typed => {}
            _ => {
                for child in &node.children {
                    self.collect_nested_stmt(child);
                }
            }
        }
    }

    fn collect_function_decl(&mut self, node: &'a Node) {
        let (name, is_method, class_name) = function_decl_name(node, self.source);
        self.functions.push(DeclFn {
            name,
            node,
            is_method,
            class_name,
        });
    }

    fn collect_class(&mut self, node: &'a Node) {
        let name = first_identifier_text(node, self.source).unwrap_or_default();
        let class_tag = name.clone();
        let mut info = ClassInfo::default();
        if let Some(bases_node) = node.children.iter().find(|c| c.kind == NodeKind::ClassBases) {
            for base in &bases_node.children {
                info.bases.push(self.text_of(base));
            }
        }
        // Base-class constructor parameters are part of the derived class's
        // constructor (the VM passes base args first). Collect them up front.
        let inherited: Vec<(String, Ty)> = info.bases.iter()
            .filter_map(|base| self.classes.get(base))
            .flat_map(|base| base.ctor_params.iter().cloned())
            .collect();
        let inherited_required: usize = info.bases.iter()
            .filter_map(|base| self.classes.get(base))
            .map(|base| base.ctor_required)
            .sum();
        info.ctor_params.extend(inherited.iter().cloned());
        info.ctor_required += inherited_required;
        if let Some(params) = node.children.iter().find(|c| c.kind == NodeKind::Tuple) {
            for param in &params.children {
                let pname = param_name_text(param, self.source).unwrap_or_default();
                if pname.is_empty() {
                    continue;
                }
                let ty = param_type_node(param)
                    .map(|t| self.parse_annotation_node(t))
                    .unwrap_or(Ty::Unknown);
                merge_field(&mut info.fields, pname.clone(), ty.clone());
                info.field_order.push(pname.clone());
                let has_default = param
                    .children
                    .first()
                    .map(|p| p.kind == NodeKind::Assign)
                    .unwrap_or(false)
                    || matches!(param.children.first().map(|p| p.kind), Some(NodeKind::Assign));
                let _ = has_default;
                let essential = !matches!(param.children.first().map(|p| p.kind), Some(NodeKind::Assign));
                if essential {
                    info.ctor_required += 1;
                }
                info.ctor_params.push((pname, ty));
            }
        }
        if let Some(body) = block_of_class(node) {
            for child in &body.children {
                match child.kind {
                    NodeKind::Typed => {
                        if let Some((name, ty)) = bare_typed_member(child, self) {
                            if !info.field_order.contains(&name) { info.field_order.push(name.clone()); }
                            merge_field(&mut info.fields, name, ty);
                        }
                    }
                    NodeKind::Assign => {
                        if let Some((name, ty)) = member_field(child, self) {
                            if !info.field_order.contains(&name) { info.field_order.push(name.clone()); }
                            merge_field(&mut info.fields, name, ty);
                        }
                    }
                    _ => {}
                }
            }
            for child in &body.children {
                if child.kind == NodeKind::Function {
                    scan_self_assignments(child, &mut info.fields, self.source, &mut info.field_order);
                }
            }
            for child in &body.children {
                if child.kind == NodeKind::Function {
                    let name = first_identifier_text(child, self.source).unwrap_or_default();
                    if name == "__getattr__" {
                        info.has_getattr = true;
                    }
                    self.functions.push(DeclFn {
                        name,
                        node: child,
                        is_method: true,
                        class_name: Some(class_tag.clone()),
                    });
                } else if child.kind == NodeKind::Accessor {
                    let name = child
                        .children
                        .first()
                        .map(|c| self.text_of(c))
                        .unwrap_or_default();
                    if child.value.as_ref() == Some(&Const::Str("set".to_string())) {
                        info.setters.push(name.clone());
                    }
                    self.functions.push(DeclFn {
                        name,
                        node: child,
                        is_method: true,
                        class_name: Some(class_tag.clone()),
                    });
                }
            }
        }
        let mut merged: HashMap<String, Ty> = HashMap::new();
        for base in &info.bases {
            if let Some(base_info) = self.classes.get(base) {
                for (k, v) in &base_info.fields {
                    merged.insert(k.clone(), refine_unknown(v.clone()));
                }
            }
        }
        for (k, v) in info.fields {
            merged.insert(k, v);
        }
        info.fields = merged;
        self.classes.insert(name.clone(), info);
        self.class_order.push(name);
        if let Some(body) = block_of_class(node) {
            for child in &body.children {
                if child.kind == NodeKind::Function {
                    if let Some(body) = body_block(child) {
                        self.collect_block(body);
                    }
                }
            }
        }
    }

    fn collect_use(&mut self, node: &'a Node) {
        for name in use_bound_names(node, self.source) {
            if !self.import_names.iter().any(|n| n == &name) {
                self.import_names.push(name);
            }
        }
    }

    fn collect_module_var(&mut self, node: &'a Node) {
        if node.kind == NodeKind::Assign {
            if let Some(target) = node.children.first() {
                if target.kind == NodeKind::Identifier {
                    let value = &node.children[1];
                    let text = self.text_of(value);
                    if let Some(ty) = self.try_type_text(&text) {
                        let name = self.text_of(target);
                        self.aliases.insert(name, ty);
                        return;
                    }
                }
            }
        }
        for name in assigned_names(node, self.source) {
            if !self.globals.contains_key(&name) {
                self.globals.insert(name, Ty::Unknown);
            }
        }
    }
}

fn function_decl_name(node: &Node, source: &Source) -> (String, bool, Option<String>) {
    if node.kind == NodeKind::Extension {
        let class_name = node
            .children
            .first()
            .map(|c| source.span(c.start, c.length))
            .unwrap_or_default();
        let name = node
            .children
            .get(1)
            .map(|c| source.span(c.start, c.length))
            .unwrap_or_default();
        (name, true, Some(class_name))
    } else {
        (
            first_identifier_text(node, source).unwrap_or_default(),
            false,
            None,
        )
    }
}

fn first_identifier_text(node: &Node, source: &Source) -> Option<String> {
    let name = node.children.iter().find(|c| c.kind == NodeKind::Identifier)?;
    Some(source.span(name.start, name.length))
}

fn body_block<'a>(node: &'a Node) -> Option<&'a Node> {
    node.children.iter().find(|c| c.kind == NodeKind::Block)
}

fn block_of_class<'a>(node: &'a Node) -> Option<&'a Node> {
    node.children.iter().find(|c| c.kind == NodeKind::Block)
}

fn param_name_text(param: &Node, source: &Source) -> Option<String> {
    let inner = param.children.first()?;
    match inner.kind {
        NodeKind::Identifier => Some(source.span(inner.start, inner.length)),
        NodeKind::Typed | NodeKind::Assign => {
            let name = inner.children.first()?;
            match name.kind {
                NodeKind::Typed => {
                    let n = name.children.first()?;
                    Some(source.span(n.start, n.length))
                }
                _ => Some(source.span(name.start, name.length)),
            }
        }
        NodeKind::Unary => param_name_text(inner, source),
        _ => None,
    }
}

fn param_type_node(param: &Node) -> Option<&Node> {
    let inner = param.children.first()?;
    match inner.kind {
        NodeKind::Typed => inner.children.get(1),
        NodeKind::Assign => {
            let name = inner.children.first()?;
            if name.kind == NodeKind::Typed {
                name.children.get(1)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn bare_typed_member(node: &Node, checker: &mut Checker) -> Option<(String, Ty)> {
    let name = node.children.first()?;
    if name.kind != NodeKind::Identifier {
        return None;
    }
    let ty_node = node.children.get(1)?;
    let text = checker.source.span(ty_node.start, ty_node.length);
    let ty = checker.parse_type_text(&text, (ty_node.start, ty_node.length), 0);
    Some((checker.source.span(name.start, name.length), ty))
}

fn member_field(node: &Node, checker: &mut Checker) -> Option<(String, Ty)> {
    let target = node.children.first()?;
    match target.kind {
        NodeKind::Typed => {
            let inner = target.children.first()?;
            if inner.kind != NodeKind::Identifier {
                return None;
            }
            let ty_node = target.children.get(1)?;
            let text = checker.source.span(ty_node.start, ty_node.length);
            let ty = checker.parse_type_text(&text, (ty_node.start, ty_node.length), 0);
            Some((checker.source.span(inner.start, inner.length), ty))
        }
        NodeKind::Identifier => {
            let text = checker.source.span(target.start, target.length);
            Some((text, Ty::Unknown))
        }
        _ => None,
    }
}

fn merge_field(fields: &mut HashMap<String, Ty>, name: String, ty: Ty) {
    match fields.get(&name) {
        None => {
            fields.insert(name, ty);
        }
        Some(existing) => {
            let refined = refine_unknown(existing.clone());
            if matches!(refined, Ty::Unknown | Ty::Any) {
                fields.insert(name, ty);
            } else {
                fields.insert(name, refined);
            }
        }
    }
}

fn refine_unknown(ty: Ty) -> Ty {
    match ty {
        Ty::Unknown => Ty::Any,
        other => other,
    }
}

fn scan_self_assignments(node: &Node, fields: &mut HashMap<String, Ty>, source: &Source, order: &mut Vec<String>) {
    fn visit(node: &Node, fields: &mut HashMap<String, Ty>, source: &Source, order: &mut Vec<String>) {
        if node.kind == NodeKind::Assign {
            if let Some(target) = node.children.first() {
                if target.kind == NodeKind::Property {
                    if let (Some(receiver), Some(member)) = (target.children.first(), target.children.get(1)) {
                        if receiver.kind == NodeKind::Identifier
                            && member.kind == NodeKind::Identifier
                            && source.span(receiver.start, receiver.length) == "self"
                        {
                            let name = source.span(member.start, member.length);
                            if !order.contains(&name) { order.push(name.to_string()); }
                            fields.entry(name).or_insert(Ty::Unknown);
                        }
                    }
                }
            }
        }
        for child in &node.children {
            visit(child, fields, source, order);
        }
    }
    visit(node, fields, source, order);
}

fn assigned_names(node: &Node, source: &Source) -> Vec<String> {
    let mut out = Vec::new();
    fn visit(node: &Node, source: &Source, out: &mut Vec<String>) {
        match node.kind {
            NodeKind::Assign => {
                if let Some(target) = node.children.first() {
                    match target.kind {
                        NodeKind::Identifier => out.push(source.span(target.start, target.length)),
                        NodeKind::Typed => {
                            if let Some(inner) = target.children.first() {
                                if inner.kind == NodeKind::Identifier {
                                    out.push(source.span(inner.start, inner.length));
                                }
                            }
                        }
                        NodeKind::Tuple => {
                            collect_tuple_names(target, source, out);
                        }
                        _ => {}
                    }
                }
            }
            NodeKind::Block
            | NodeKind::If
            | NodeKind::ElIf
            | NodeKind::Else
            | NodeKind::While
            | NodeKind::For
            | NodeKind::Match
            | NodeKind::Case
            | NodeKind::Try
            | NodeKind::Catch
            | NodeKind::Finally
            | NodeKind::Using
            | NodeKind::Defer => {
                for child in &node.children {
                    visit(child, source, out);
                }
            }
            NodeKind::Function | NodeKind::Lambda | NodeKind::Extension => {}
            _ => {}
        }
    }
    fn collect_tuple_names(node: &Node, source: &Source, out: &mut Vec<String>) {
        for child in &node.children {
            match child.kind {
                NodeKind::Identifier => out.push(source.span(child.start, child.length)),
                NodeKind::Typed => {
                    if let Some(inner) = child.children.first() {
                        if inner.kind == NodeKind::Identifier {
                            out.push(source.span(inner.start, inner.length));
                        }
                    }
                }
                NodeKind::Tuple => collect_tuple_names(child, source, out),
                _ => {}
            }
        }
    }
    visit(node, source, &mut out);
    out
}

fn use_bound_names(node: &Node, source: &Source) -> Vec<String> {
    let children = &node.children;
    if let Some(tuple) = children.iter().find(|c| c.kind == NodeKind::Tuple) {
        return tuple
            .children
            .iter()
            .filter(|c| c.kind == NodeKind::Identifier)
            .map(|c| source.span(c.start, c.length))
            .collect();
    }
    if children.len() >= 2 {
        if let Some(alias) = children.last() {
            if alias.kind == NodeKind::Identifier {
                return vec![source.span(alias.start, alias.length)];
            }
        }
    }
    fn last_segment<'n>(node: &'n Node) -> &'n Node {
        let mut current = node;
        loop {
            if current.kind == NodeKind::Property {
                if let Some(last) = current.children.last() {
                    current = last;
                    continue;
                }
            }
            return current;
        }
    }
    match children.first() {
        Some(path) => {
            let last = last_segment(path);
            vec![source.span(last.start, last.length)]
        }
        None => Vec::new(),
    }
}


#[derive(Clone)]
struct Env {
    scopes: Vec<HashMap<String, Ty>>,
    order: Vec<String>,
    loop_depth: u32,
    in_function: bool,
    terminated: bool,
    fixed_width: HashMap<String, (String, (i128, i128))>,
}

impl Env {
    fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            order: Vec::new(),
            loop_depth: 0,
            in_function: false,
            terminated: false,
            fixed_width: HashMap::new(),
        }
    }

    fn fork(&self) -> Self {
        Self {
            scopes: self.scopes.clone(),
            order: self.order.clone(),
            loop_depth: self.loop_depth,
            in_function: self.in_function,
            terminated: self.terminated,
            fixed_width: self.fixed_width.clone(),
        }
    }

    fn lookup(&self, name: &str) -> Option<&Ty> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }

    fn declare(&mut self, name: &str, ty: Ty) {
        if let Some(scope) = self.scopes.last_mut() {
            if !scope.contains_key(name) {
                self.order.push(name.to_string());
            }
            scope.insert(name.to_string(), ty);
        }
    }

    fn set_existing(&mut self, name: &str, ty: Ty) -> bool {
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), ty);
                return true;
            }
        }
        false
    }

    fn new_names_since(&self, base_scopes: &[HashMap<String, Ty>]) -> Vec<String> {
        let mut out = Vec::new();
        for scope in &self.scopes {
            for key in scope.keys() {
                let existed = base_scopes
                    .iter()
                    .any(|base_scope| base_scope.contains_key(key));
                if !existed && !out.contains(key) {
                    out.push(key.clone());
                }
            }
        }
        out
    }
}

fn merge_arms(base: &Env, arms: &[Env]) -> Env {
    let mut out = base.clone();
    let sets: Vec<Vec<String>> = arms
        .iter()
        .map(|arm| {
            if arm.terminated {
                Vec::new()
            } else {
                arm.new_names_since(&base.scopes)
            }
        })
        .collect();
    if sets.is_empty() {
        return out;
    }
    let mut common: Vec<String> = sets[0].clone();
    for set in &sets[1..] {
        common.retain(|name| set.contains(name));
    }
    for name in common {
        let types: Vec<Ty> = arms
            .iter()
            .filter_map(|arm| arm.lookup(&name).cloned())
            .collect();
        let joined = Ty::union(types);
        if base.lookup(&name).is_none() {
            out.order.push(name.clone());
        }
        out.declare(&name, joined);
    }
    out
}


impl<'a> Checker<'a> {
    fn func_ty_for_span(&self, start: usize) -> Ty {
        match self.sigs_by_span.get(&start) {
            Some(sig) => Ty::Func {
                params: sig.params[sig.receiver..].into(),
                names: sig.param_names[sig.receiver..].into(),
                ret: Rc::new(sig.ret.clone()),
                required: sig.required.saturating_sub(sig.receiver),
            },
            None => Ty::Any,
        }
    }

    fn check_all(&mut self) -> CheckedProgram {
        let decls = std::mem::take(&mut self.functions);
        let mut sigs: HashMap<String, FnSig> = HashMap::new();
        for decl in &decls {
            let sig = self.build_sig(decl);
            self.sigs_by_span.insert(decl.node.start, sig.clone());
            sigs.insert(decl.name.clone(), sig);
        }
        self.fn_sigs = sigs;
        for decl in &decls {
            if let Some(class_name) = &decl.class_name {
                if let Some(sig) = self.sigs_by_span.get(&decl.node.start).cloned() {
                    self.classes
                        .entry(class_name.clone())
                        .or_default()
                        .methods
                        .insert(decl.name.clone(), bind_method_ty(&sig));
                }
            }
        }
        self.silence += 1;
        let mut seed_env = Env::new();
        for name in &self.import_names {
            seed_env.declare(name, Ty::Any);
        }
        self.check_module_block(self.root, &mut seed_env);
        for scope in &seed_env.scopes {
            for (name, ty) in scope {
                self.globals.insert(name.clone(), ty.clone());
            }
        }
        self.silence -= 1;
        let mut env = Env::new();
        for (name, ty) in self.globals.clone() {
            env.declare(&name, ty);
        }
        for name in &self.import_names {
            env.declare(name, Ty::Any);
        }
        self.check_module_block(self.root, &mut env);
        let functions = std::mem::take(&mut self.checked_functions);
        let fn_by_span = std::mem::take(&mut self.checked_by_span);
        CheckedProgram {
            sigs: std::mem::take(&mut self.fn_sigs),
            classes: std::mem::take(&mut self.classes),
            aliases: std::mem::take(&mut self.aliases),
            functions,
            fn_by_span,
        }
    }

    fn check_module_block(&mut self, node: &'a Node, env: &mut Env) {
        for child in &node.children {
            match child.kind {
                NodeKind::Function | NodeKind::Extension => {
                    let (name, is_method, class_name) = function_decl_name(child, self.source);
                    let decl = DeclFn {
                        name: name.clone(),
                        node: child,
                        is_method,
                        class_name,
                    };
                    self.check_function_inline(&decl, env, false);
                    if env.lookup(&name).is_none() {
                        env.declare(&name, self.func_ty_for_span(child.start));
                    }
                }
                NodeKind::Class => self.check_class_defaults(child, env),
                NodeKind::Enum => {}
                NodeKind::Use => {
                    for name in use_bound_names(child, self.source) {
                        env.declare(&name, Ty::Any);
                    }
                }
                _ => self.check_stmt(child, env),
            }
        }
    }

    fn check_function_inline(&mut self, decl: &DeclFn<'a>, captured: &Env, _is_method_capture: bool) {
        if self.silence > 0 {
            return;        }
        let mut env = captured.fork();
        env.in_function = true;
        env.order.clear();
        env.fixed_width.clear();
        let sig = self
            .sigs_by_span
            .get(&decl.node.start)
            .cloned()
            .unwrap_or_else(|| FnSig {
            name: decl.name.clone(),
            params: Vec::new(),
            param_names: Vec::new(),
            ret: Ty::Unknown,
            required: 0,
            is_method: decl.is_method,
            receiver: if decl.is_method { 1 } else { 0 },
            variadic: false,
        });
        for (i, name) in sig.param_names.iter().enumerate() {
            env.declare(name, sig.params[i].clone());
        }
        if decl.node.kind == NodeKind::Accessor {
            env.declare("field", Ty::Any);
            let is_set = decl.node.value.as_ref() == Some(&Const::Str("set".to_string()));
            if is_set {
                env.declare("value", Ty::Any);
            }
        }
        if let Some(body) = body_block(decl.node) {
            let sig_ret = sig.ret.clone();
            let saved = self.current_ret.replace(sig_ret);
            self.check_block_no_terminate_reset(body, &mut env);
            self.current_ret = saved;
        }
        let mut locals = Vec::with_capacity(env.order.len());
        let mut local_types = HashMap::new();
        let mut seen = std::collections::HashSet::new();
        for name in &env.order {
            if !seen.insert(name.clone()) {
                continue;
            }
            if let Some(ty) = env.lookup(name).cloned() {
                locals.push((name.clone(), ty.clone()));
                local_types.insert(name.clone(), ty);
            }
        }
        let index = self.checked_functions.len();
        self.checked_by_span.insert(decl.node.start, index);
        self.checked_functions.push(FnCheck {
            sig,
            locals,
            local_types,
        });
    }

    fn check_class_defaults(&mut self, node: &'a Node, env: &mut Env) {
        let class_name = first_identifier_text(node, self.source).unwrap_or_default();
        let Some(body) = block_of_class(node) else {
            return;
        };
        for child in &body.children {
            match child.kind {
                NodeKind::Assign => {
                    let target = child.children.first().cloned();
                    if let (Some(target), Some(value)) = (target, child.children.get(1)) {
                        let vty = self.check_expr(value, env);
                        let name = match target.kind {
                            NodeKind::Identifier => Some(self.text_of(&target)),
                            NodeKind::Typed => target
                                .children
                                .first()
                                .map(|n| self.text_of(n)),
                            _ => None,
                        };
                        if let Some(name) = name {
                            let field_ty = self
                                .classes
                                .get(&class_name)
                                .and_then(|info| info.fields.get(&name).cloned());
                            match field_ty {
                                Some(Ty::Unknown) | None => {
                                    let refined = if vty == Ty::Unknown { Ty::Any } else { vty };
                                    self.insert_class_field(&class_name, &name, refined);
                                }
                                Some(known) => {
                                    if !assignable(&known, &vty, &self.classes) {
                                        self.error(
                                            value,
                                            format!(
                                                "cannot assign value of type '{}' to field '{}' of type '{}'",
                                                vty, name, known
                                            ),
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                NodeKind::Typed => {}                NodeKind::Function | NodeKind::Accessor => {
                    let decl = DeclFn {
                        name: match child.kind {
                            NodeKind::Function => {
                                first_identifier_text(child, self.source).unwrap_or_default()
                            }
                            _ => child
                                .children
                                .first()
                                .map(|c| self.text_of(c))
                                .unwrap_or_default(),
                        },
                        node: child,
                        is_method: true,
                        class_name: Some(class_name.clone()),
                    };
                    self.check_function_inline(&decl, env, true);
                }
                _ => {
                    self.check_stmt(child, env);
                }
            }
        }
    }

    fn insert_class_field(&mut self, class_name: &str, name: &str, ty: Ty) {
        if let Some(info) = self.classes.get_mut(class_name) {
            info.fields.insert(name.to_string(), ty);
        }
    }

    fn refine_class_field(&mut self, class_name: &str, name: &str, ty: Ty) {
        if let Some(info) = self.classes.get_mut(class_name) {
            match info.fields.get(name) {
                Some(Ty::Unknown) => {
                    info.fields.insert(name.to_string(), ty);
                }
                None => {
                    info.fields.insert(name.to_string(), ty);
                }
                _ => {}
            }
        }
    }

    fn class_field(&self, class_name: &str, name: &str) -> Option<Ty> {
        let mut current = Some(class_name.to_string());
        let mut seen = Vec::new();
        while let Some(name_c) = current {
            if seen.iter().any(|n| n == &name_c) {
                return None;
            }
            seen.push(name_c.clone());
            let info = self.classes.get(&name_c)?;
            if let Some(field) = info.fields.get(name) {
                return Some(refine_unknown(field.clone()));
            }
            current = info.bases.first().cloned();
        }
        None
    }

    fn class_method(&self, class_name: &str, name: &str) -> Option<Ty> {
        let mut current = Some(class_name.to_string());
        let mut seen = Vec::new();
        while let Some(name_c) = current {
            if seen.iter().any(|n| n == &name_c) {
                return None;
            }
            seen.push(name_c.clone());
            let info = self.classes.get(&name_c)?;
            if let Some(method) = info.methods.get(name) {
                return Some(method.clone());
            }
            current = info.bases.first().cloned();
        }
        None
    }

    fn class_has_setter(&self, class_name: &str, name: &str) -> bool {
        let mut current = Some(class_name.to_string());
        let mut seen = Vec::new();
        while let Some(name_c) = current {
            if seen.iter().any(|n| n == &name_c) {
                return false;
            }
            seen.push(name_c.clone());
            let Some(info) = self.classes.get(&name_c) else {
                return false;
            };
            if info.setters.iter().any(|s| s == name) {
                return true;
            }
            current = info.bases.first().cloned();
        }
        false
    }

    fn class_has_getattr(&self, class_name: &str) -> bool {
        let mut current = Some(class_name.to_string());
        let mut seen = Vec::new();
        while let Some(name_c) = current {
            if seen.iter().any(|n| n == &name_c) {
                return false;
            }
            seen.push(name_c.clone());
            let Some(info) = self.classes.get(&name_c) else {
                return false;
            };
            if info.has_getattr {
                return true;
            }
            current = info.bases.first().cloned();
        }
        false
    }

    fn build_sig(&mut self, decl: &DeclFn) -> FnSig {
        let node = decl.node;
        if node.kind == NodeKind::Accessor {
            let is_set = node.value.as_ref() == Some(&Const::Str("set".to_string()));
            let mut params = vec![Ty::Class(Rc::from(decl.class_name.clone().unwrap_or_default()))];
            let mut param_names = vec!["self".to_string()];
            if is_set {
                params.push(Ty::Any);
                param_names.push("value".to_string());
            }
            let ret = if is_set { Ty::None_ } else { Ty::Any };
            return FnSig {
                name: decl.name.clone(),
                params,
                param_names,
                ret,
                required: 1,
                is_method: true,
                receiver: 1,
                variadic: false,
            };
        }
        let mut params: Vec<Ty> = Vec::new();
        let mut param_names: Vec<String> = Vec::new();
        let mut required = 0usize;
        let mut variadic = false;
        let mut receiver = 0usize;
        if decl.is_method {
            let class_name = decl.class_name.clone().unwrap_or_default();
            params.push(Ty::Class(Rc::from(class_name)));
            param_names.push("self".to_string());
            receiver = 1;
        }
        if let Some(params_node) = node.children.iter().find(|c| c.kind == NodeKind::Tuple) {
            for param in &params_node.children {
                let pname = param_name_text(param, self.source).unwrap_or_default();
                let inner = param.children.first();
                let is_variadic = matches!(
                    inner.map(|i| i.kind),
                    Some(NodeKind::Unary)
                );
                let has_default = matches!(inner.map(|i| i.kind), Some(NodeKind::Assign));
                let ty = match param_type_node(param) {
                    Some(t) => {
                        let text = self.text_of(t);
                        let (start, length) = self.span(t);
                        self.parse_type_text(&text, (start, length), 0)
                    }
                    None => {
                        if is_variadic {
                            Ty::Any
                        } else if pname.is_empty() {
                            Ty::Unknown
                        } else {
                            self.error(
                                param,
                                format!(
                                    "parameter '{pname}' of '{}' requires a type annotation",
                                    decl.name
                                ),
                            );
                            Ty::Unknown
                        }
                    }
                };
                if is_variadic {
                    variadic = true;
                } else if !has_default {
                    required += 1;
                }
                params.push(ty);
                param_names.push(pname);
            }
        }
        let ret_child = match node.kind {
            NodeKind::Function | NodeKind::Accessor => node.children.get(3),
            NodeKind::Extension => node.children.get(4),
            _ => None,
        };
        let ret = ret_child
            .filter(|c| c.kind == NodeKind::Identifier)
            .map(|t| {
                let text = self.text_of(t);
                let (start, length) = self.span(t);
                self.parse_type_text(&text, (start, length), 0)
            });
        let ret = match ret {
            Some(Ty::Unknown) => Ty::Unknown,
            Some(ty) => ty,
            None => {
                if decl.name == "__init__" {
                    Ty::None_
                } else {
                    self.error(
                        node,
                        format!("function '{}' requires a return type annotation", decl.name),
                    );
                    Ty::Unknown
                }
            }
        };
        FnSig {
            name: decl.name.clone(),
            params,
            param_names,
            ret,
            required: required + receiver,
            is_method: decl.is_method,
            receiver,
            variadic,
        }
    }
}

fn bind_method_ty(sig: &FnSig) -> Ty {
    Ty::Func {
        params: sig.params[sig.receiver..].into(),
        names: sig.param_names[sig.receiver..].into(),
        ret: Rc::new(sig.ret.clone()),
        required: sig.required.saturating_sub(sig.receiver),
    }
}


impl<'a> Checker<'a> {
    fn check_block_no_terminate_reset(&mut self, node: &'a Node, env: &mut Env) {
        for child in &node.children {
            self.check_stmt(child, env);
        }
    }

    fn check_block(&mut self, node: &'a Node, env: &mut Env) {
        for child in &node.children {
            self.check_stmt(child, env);
        }
    }

    fn check_stmt(&mut self, node: &'a Node, env: &mut Env) {
        match node.kind {
            NodeKind::Block => self.check_block(node, env),
            NodeKind::Assign => self.check_assign(node, env),
            NodeKind::Return => self.check_return(node, env),
            NodeKind::If => self.check_if(node, env),
            NodeKind::While => self.check_while(node, env),
            NodeKind::For => self.check_for(node, env),
            NodeKind::Match => self.check_match(node, env),
            NodeKind::Break | NodeKind::Skip => {
                if env.loop_depth == 0 {
                    let kind = if node.kind == NodeKind::Break {
                        "'break' outside loop"
                    } else {
                        "'skip' outside loop"
                    };
                    self.error(node, kind);
                } else {
                    env.terminated = true;
                }
            }
            NodeKind::Try => self.check_try(node, env),
            NodeKind::Using => {
                if let Some(assign) = node.children.first() {
                    self.check_stmt(assign, env);
                }
            }
            NodeKind::Defer => {
                if let Some(body) = node.children.first() {
                    self.check_stmt(body, env);
                }
            }
            NodeKind::Use => {
                for name in use_bound_names(node, self.source) {
                    env.declare(&name, Ty::Any);
                }
            }
            NodeKind::Function | NodeKind::Extension => {
                let (name, is_method, class_name) = function_decl_name(node, self.source);
                let decl = DeclFn {
                    name: name.clone(),
                    node,
                    is_method,
                    class_name,
                };
                self.check_function_inline(&decl, env, false);
                if env.lookup(&name).is_none() {
                    env.declare(&name, self.func_ty_for_span(node.start));
                }
            }
            NodeKind::Class => {
                self.check_class_defaults(node, env);
                if let Some(name) = first_identifier_text(node, self.source) {
                    if env.lookup(&name).is_none() {
                        env.declare(&name, Ty::Any);
                    }
                }
            }
            NodeKind::Enum => {
                if let Some(name) = first_identifier_text(node, self.source) {
                    if env.lookup(&name).is_none() {
                        env.declare(&name, Ty::Any);
                    }
                }
            }
            NodeKind::Typed => {
                self.error(node, "type annotation is not a statement");
            }
            NodeKind::Branch | NodeKind::ElIf | NodeKind::IfCase => {
                let cond = node.children.first();
                let body = node.children.get(1);
                if let Some(cond) = cond {
                    self.check_condition(cond, env);
                }
                if let Some(body) = body {
                    if body.kind == NodeKind::Block {
                        self.check_block_no_terminate_reset(body, env);
                    } else {
                        self.check_stmt(body, env);
                    }
                }
            }
            NodeKind::Else => {
                for child in &node.children {
                    if child.kind == NodeKind::Block {
                        self.check_block_no_terminate_reset(child, env);
                    } else {
                        self.check_stmt(child, env);
                    }
                }
            }
            _ => {
                self.check_expr(node, env);
            }
        }
    }

    fn check_assign(&mut self, node: &'a Node, env: &mut Env) {
        let Some(target) = node.children.first() else {
            for child in &node.children {
                self.check_expr(child, env);
            }
            return;
        };
        let Some(value) = node.children.get(1) else {
            self.check_expr(target, env);
            return;
        };
        if target.kind == NodeKind::Identifier {
            let value_text = self.text_of(value);
            if self.try_type_text(&value_text).is_some()
                && self.aliases.contains_key(&self.text_of(target))
            {
                return;
            }
        }
        match target.kind {
            NodeKind::Typed => {
                let Some(inner) = target.children.first() else {
                    return;
                };
                let annotation_node = target.children.get(1);
                let annotation_text = annotation_node.map(|t| self.text_of(t));
                let ty = annotation_node
                    .map(|t| self.parse_annotation_node(t))
                    .unwrap_or(Ty::Unknown);
                let annotation_text_str = annotation_text.as_deref();
                let fixed_width = annotation_text_str.and_then(fixed_width_bounds);
                if let Some((fw_name, range)) = fixed_width {
                    if let Some(constant) = const_int_value(value, self.source) {
                        if constant < range.0 || constant > range.1 {
                            self.error(
                                value,
                                format!(
                                    "value {constant} out of range for {fw_name} ({0}..={1})",
                                    range.0, range.1
                                ),
                            );
                        }
                    }
                }
                match inner.kind {
                    NodeKind::Identifier => {
                        let vty = self.check_expr(value, env);
                        self.bind_annotated(inner, ty, vty, env);
                        let name = self.text_of(inner);
                        if let Some((fw_name, range)) = fixed_width {
                            env.fixed_width
                                .insert(name, (fw_name.to_string(), range));
                        } else {
                            env.fixed_width.remove(&self.text_of(inner));
                        }
                    }
                    NodeKind::Tuple => {
                        let vty = self.check_expr(value, env);
                        self.bind_tuple(inner, Some(ty), vty, env, node);
                    }
                    _ => {
                        self.check_expr(value, env);
                    }
                }
            }
            NodeKind::Identifier => {
                let vty = self.check_expr(value, env);
                let name = self.text_of(target);
                if let Some((fw_name, (lo, hi))) = env.fixed_width.get(&name).cloned() {
                    if let Some(constant) = const_int_value(value, self.source) {
                        if constant < lo || constant > hi {
                            self.error(
                                value,
                                format!("value {constant} out of range for {fw_name} ({lo}..={hi})"),
                            );
                        }
                    }
                }
                self.bind_name(&name, vty, env, target);
            }
            NodeKind::Tuple => {
                let vty = self.check_expr(value, env);
                self.bind_tuple(target, None, vty, env, node);
            }
            NodeKind::Property => {
                let vty = self.check_expr(value, env);
                let Some(receiver) = target.children.first() else {
                    return;
                };
                let Some(member) = target.children.get(1) else {
                    return;
                };
                let rty = self.check_expr(receiver, env);
                let member_name = self.text_of(member);
                self.assign_member(receiver, rty, &member_name, vty, node, env);
            }
            NodeKind::Index => {
                let vty = self.check_expr(value, env);
                let Some(obj) = target.children.first() else {
                    return;
                };
                let obj_ty = self.check_expr(obj, env);
                self.assign_index(target, obj_ty, vty, env);
            }
            NodeKind::Slice => {
                let vty = self.check_expr(value, env);
                let Some(obj) = target.children.first() else {
                    return;
                };
                let obj_ty = self.check_expr(obj, env);
                self.assign_index(target, obj_ty, vty, env);
            }
            _ => {
                self.check_expr(target, env);
                self.check_expr(value, env);
            }
        }
    }

    fn bind_annotated(&mut self, target: &Node, ann: Ty, vty: Ty, env: &mut Env) {
        if matches!(ann, Ty::Unknown) {
            env.declare(&self.text_of(target), Ty::Unknown);
            return;
        }
        if !assignable(&ann, &vty, &self.classes) {
            self.error(
                target,
                format!(
                    "cannot assign value of type '{}' to '{}' of type '{}'",
                    vty,
                    self.text_of(target),
                    ann
                ),
            );
        }
        env.declare(&self.text_of(target), ann);
    }

    fn bind_name(&mut self, name: &str, vty: Ty, env: &mut Env, node: &Node) {
        if let Some(current) = env.lookup(name).cloned() {
            if !assignable(&current, &vty, &self.classes) {
                self.error(
                    node,
                    format!(
                        "cannot assign value of type '{}' to '{name}' of type '{}'",
                        vty, current
                    ),
                );
            }
            env.set_existing(name, current);
            return;
        }
        if let Some(current) = self.globals.get(name).cloned() {
            if !assignable(&current, &vty, &self.classes) {
                self.error(
                    node,
                    format!(
                        "cannot assign value of type '{}' to '{name}' of type '{}'",
                        vty, current
                    ),
                );
            }
            env.declare(name, current);
            return;
        }
        env.declare(name, vty);
    }

    fn bind_tuple(&mut self, target: &'a Node, _ann: Option<Ty>, vty: Ty, env: &mut Env, node: &Node) {
        let elem_tys: Vec<Ty> = match &vty {
            Ty::Tuple(items) => items.iter().cloned().collect(),
            Ty::Any => target.children.iter().map(|_| Ty::Any).collect(),
            _ => {
                self.error(
                    node,
                    format!("tuple destructure requires a tuple, got '{}'", vty),
                );
                target.children.iter().map(|_| Ty::Unknown).collect()
            }
        };
        for (index, item) in target.children.iter().enumerate() {
            let elem = elem_tys.get(index).cloned().unwrap_or(Ty::Unknown);
            match item.kind {
                NodeKind::Identifier => {
                    let name = self.text_of(item);
                    self.bind_name(&name, elem, env, item);
                }
                NodeKind::Typed => {
                    let Some(inner) = item.children.first() else {
                        continue;
                    };
                    let ty = item
                        .children
                        .get(1)
                        .map(|t| self.parse_annotation_node(t))
                        .unwrap_or(Ty::Unknown);
                    if !assignable(&ty, &elem, &self.classes) {
                        self.error(
                            item,
                            format!(
                                "tuple element type mismatch: cannot bind '{}' as '{}'",
                                elem, ty
                            ),
                        );
                    }
                    if inner.kind == NodeKind::Identifier {
                        env.declare(&self.text_of(inner), ty);
                    } else if inner.kind == NodeKind::Tuple {
                        self.bind_tuple(inner, None, elem, env, node);
                    }
                }
                NodeKind::Tuple => {
                    self.bind_tuple(item, None, elem, env, node);
                }
                _ => {
                    self.check_expr(item, env);
                }
            }
        }
    }

    fn assign_member(
        &mut self,
        receiver: &Node,
        rty: Ty,
        member: &str,
        vty: Ty,
        node: &Node,
        _env: &mut Env,
    ) {
        match &rty {
            Ty::Class(class) => {
                let class = class.to_string();
                if self.class_field(&class, member).is_some() {
                    let known = self.class_field(&class, member).unwrap();
                    let refined = refine_unknown(known.clone());
                    if !assignable(&refined, &vty, &self.classes) {
                        self.error(
                            node,
                            format!(
                                "cannot assign value of type '{}' to field '{}' of type '{}'",
                                vty, member, refined
                            ),
                        );
                    } else if matches!(known, Ty::Unknown) {
                        self.refine_class_field(&class, member, vty);
                    }
                    return;
                }
                let is_self = receiver.kind == NodeKind::Identifier && self.text_of(receiver) == "self";
                if is_self {
                    self.insert_class_field(&class, member, vty);
                    return;
                }
                if self.class_has_getattr(&class) {
                    return;
                }
                if self.class_has_setter(&class, member) {
                    return;
                }
                if self.class_method(&class, member).is_some() {
                    self.error(
                        node,
                        format!("'{class}' method '{member}' is not assignable"),
                    );
                    return;
                }
                self.error(
                    node,
                    format!("'{class}' object has no attribute '{member}'"),
                );
            }
            Ty::None_ => {
                self.error(node, "cannot assign member of 'none'");
            }
            Ty::Any | Ty::List(_) | Ty::Dict(..) | Ty::Set(_) | Ty::Tuple(_) | Ty::Str | Ty::Int | Ty::Float | Ty::Bool => {}
            other => {
                self.error(
                    node,
                    format!("member assignment on value of type '{}'", other),
                );
            }
        }
    }

    fn assign_index(&mut self, target: &'a Node, obj_ty: Ty, vty: Ty, env: &mut Env) {
        let elem = match &obj_ty {
            Ty::List(elem) => Some(refine_unknown((**elem).clone())),
            Ty::Dict(_, value) => Some((**value).clone()),
            Ty::Str => {
                self.error_span(target.start, target.length, "string assignment does not support item assignment");
                None
            }
            Ty::Tuple(items) => {
                self.error_span(target.start, target.length, "tuple does not support item assignment");
                let _ = items;
                None
            }
            Ty::Any => Some(Ty::Any),
            Ty::Unknown => Some(Ty::Unknown),
            _ => {
                self.error_span(
                    target.start,
                    target.length,
                    format!("value of type '{}' does not support item assignment", obj_ty),
                );
                None
            }
        };
        if let Some(index) = target.children.get(1) {
            let _ = self.check_expr(index, env);
        }
        if let Some(elem) = elem {
            if !assignable(&elem, &vty, &self.classes) {
                self.error_span(
                    target.start,
                    target.length,
                    format!("cannot assign value of type '{}' to index of type '{}'", vty, elem),
                );
            }
        }
    }
}


impl<'a> Checker<'a> {
    fn check_return(&mut self, node: &'a Node, env: &mut Env) {
        let sig_ret = self.current_ret.take();
        match node.children.first() {
            None => {
                if let Some(ret) = sig_ret {
                    self.expect_return_value(ret, None, node);
                }
            }
            Some(expr) => {
                let vty = self.check_expr(expr, env);
                if let Some(ret) = sig_ret {
                    self.expect_return_value(ret, Some(vty), node);
                }
            }
        }
        env.terminated = true;
    }

    fn expect_return_value(&mut self, ret: Ty, value: Option<Ty>, node: &'a Node) {
        match value {
            None => {
                if !matches!(ret, Ty::None_ | Ty::Any | Ty::Unknown) {
                    self.error(
                        node,
                        format!("function must return a value of type '{}'", ret),
                    );
                }
            }
            Some(vty) => {
                if !assignable(&ret, &vty, &self.classes) {
                    self.error(
                        node,
                        format!(
                            "cannot return value of type '{}' from a function returning '{}'",
                            vty, ret
                        ),
                    );
                }
            }
        }
    }

    fn check_condition(&mut self, node: &'a Node, env: &mut Env) {
        let ty = self.check_expr(node, env);
        if !matches!(ty, Ty::Bool | Ty::Any | Ty::Unknown) {
            self.error(
                node,
                format!("condition must be a bool, got '{}'", ty),
            );
        }
    }

    fn check_if(&mut self, node: &'a Node, env: &mut Env) {
        if node.kind == NodeKind::If && has_if_cases(node) {
            self.check_if_branches(node, env);
            return;
        }
        let (cond, body, else_branch) = (
            node.children.first(),
            node.children.get(1),
            node.children.get(2),
        );
        if let Some(cond) = cond {
            self.check_condition(cond, env);
        }
        let base = env.clone();
        let mut arm_envs: Vec<Env> = Vec::new();
        if let Some(body) = body {
            let mut arm = base.fork();
            self.check_block_no_terminate_reset(body, &mut arm);
            arm_envs.push(arm);
        } else {
            arm_envs.push(base.fork());
        }
        if let Some(else_branch) = else_branch {
            let mut arm = base.fork();
            self.check_stmt(else_branch, &mut arm);
            arm_envs.push(arm);
        } else {
            arm_envs.push(base.fork());
        }
        let merged = merge_arms(&base, &arm_envs);
        *env = merged;
    }

    fn check_if_branches(&mut self, node: &'a Node, env: &mut Env) {
        let base = env.clone();
        let mut arm_envs: Vec<Env> = Vec::new();
        for child in &node.children {
            match child.kind {
                NodeKind::Branch | NodeKind::ElIf | NodeKind::IfCase => {
                    let cond = child.children.first();
                    let body = child.children.get(1);
                    if let Some(cond) = cond {
                        self.check_condition(cond, env);
                    }
                    let mut arm = base.fork();
                    if let Some(body) = body {
                        self.check_block_no_terminate_reset(body, &mut arm);
                    }
                    arm_envs.push(arm);
                }
                NodeKind::Else => {
                    let mut arm = base.fork();
                    for body in &child.children {
                        self.check_block_no_terminate_reset(body, &mut arm);
                    }
                    arm_envs.push(arm);
                }
                _ => {
                    let mut arm = base.fork();
                    self.check_stmt(child, &mut arm);
                    arm_envs.push(arm);
                }
            }
        }
        let has_else = node.children.iter().any(|c| c.kind == NodeKind::Else);
        if !has_else {
            arm_envs.push(base.fork());
        }
        let merged = merge_arms(&base, &arm_envs);
        *env = merged;
    }

    fn check_while(&mut self, node: &'a Node, env: &mut Env) {
        let cond = node.children.first();
        if let Some(cond) = cond {
            self.check_condition(cond, env);
        }
        let base = env.clone();
        let mut arm = base.fork();
        arm.loop_depth += 1;
        if let Some(body) = node.children.get(1) {
            self.check_block_no_terminate_reset(body, &mut arm);
        }
        *env = base;
    }

    fn check_for(&mut self, node: &'a Node, env: &mut Env) {
        let target = node.children.first();
        let iter = node.children.get(1);
        let body = node.children.get(2);
        let elem_ty = iter.map(|i| self.check_iter_elem(i, env)).unwrap_or(Ty::Any);
        if let Some(target) = target {
            self.bind_for_target(target, elem_ty, env);
        }
        let base = env.clone();
        let mut arm = base.fork();
        arm.loop_depth += 1;
        if let Some(body) = body {
            self.check_block_no_terminate_reset(body, &mut arm);
        }
        *env = base;
    }

    fn check_iter_elem(&mut self, node: &'a Node, env: &mut Env) -> Ty {
        if let Some(call) = iter_range_call(node, self.source) {
            for arg in &call.children[1].children {
                if arg.kind == NodeKind::Pair || arg.kind == NodeKind::Spread {
                    continue;
                }
                let ty = self.check_expr(arg, env);
                if !matches!(ty, Ty::Int | Ty::Any | Ty::Unknown) {
                    self.error(
                        arg,
                        format!("range bound must be an int, got '{}'", ty),
                    );
                }
            }
            return Ty::Int;
        }
        if node.kind == NodeKind::Binary && node.op == Some(TokenType::DotDot) {
            if let (Some(start), Some(end)) = (node.children.first(), node.children.get(1)) {
                let lty = self.check_expr(start, env);
                let rty = self.check_expr(end, env);
                if lty.is_integer_ish() && rty.is_integer_ish() {
                    return Ty::Int;
                }
                return Ty::Any;
            }
            return Ty::Any;
        }
        match self.check_expr(node, env) {
            Ty::List(elem) | Ty::Set(elem) => refine_unknown((*elem).clone()),
            Ty::Dict(key, _) => (*key).clone(),
            Ty::Tuple(items) => {
                Ty::union(items.iter().cloned().collect())
            }
            Ty::Str => Ty::Str,
            Ty::Any => Ty::Any,
            _ => Ty::Any,
        }
    }

    fn iter_array_sig(&mut self, node: &'a Node, env: &mut Env) -> Option<Ty> {
        let callee = node.children.first()?;
        if callee.kind != NodeKind::Property {
            return None;
        }
        let member = callee.children.get(1)?;
        let head = callee.children.first()?;
        if head.kind != NodeKind::Identifier || member.kind != NodeKind::Identifier {
            return None;
        }
        if self.text_of(head) != "iter" || self.text_of(member) != "array" {
            return None;
        }
        let args_node = node.children.get(1)?;
        if args_node.kind != NodeKind::Tuple {
            return Some(Ty::Any);
        }
        let args: Vec<&'a Node> = args_node.children.iter().collect();
        if args.len() != 2 {
            self.error(callee, format!("iter.array expects exactly 2 arguments, got {}", args.len()));
            return Some(Ty::Any);
        }
        let value_ty = self.check_expr(args[0], env);
        let size_ty = self.check_expr(args[1], env);
        if !matches!(size_ty, Ty::Int | Ty::Any) {
            self.error(args[1], format!("iter.array size must be an int, got '{}'", size_ty));
        }
        Some(match value_ty {
            Ty::Int => Ty::list(Ty::Int),
            Ty::Float => Ty::list(Ty::Float),
            other => {
                self.error(args[0], format!("iter.array value must be an int or float, got '{}'", other));
                Ty::list(Ty::Int)
            }
        })
    }

    fn iter_minmax_sig(&mut self, node: &'a Node, env: &mut Env) -> Option<Ty> {
        let callee = node.children.first()?;
        if callee.kind != NodeKind::Property {
            return None;
        }
        let member = callee.children.get(1)?;
        let head = callee.children.first()?;
        if head.kind != NodeKind::Identifier || member.kind != NodeKind::Identifier {
            return None;
        }
        let member_name = self.text_of(member);
        if self.text_of(head) != "iter" || (member_name != "min" && member_name != "max") {
            return None;
        }
        let args_node = node.children.get(1)?;
        if args_node.kind != NodeKind::Tuple {
            return Some(Ty::Any);
        }
        let args: Vec<&'a Node> = args_node.children.iter().collect();
        if args.len() != 1 {
            return Some(Ty::Any);
        }
        let arg_ty = self.check_expr(args[0], env);
        Some(match arg_ty {
            Ty::List(elem) => match elem.as_ref() {
                Ty::Int | Ty::Float => (*elem).clone(),
                _ => Ty::Any,
            },
            Ty::Any => Ty::Any,
            _ => Ty::Any,
        })
    }

    fn bind_for_target(&mut self, target: &'a Node, elem_ty: Ty, env: &mut Env) {        match target.kind {
            NodeKind::Identifier => {
                env.declare(&self.text_of(target), elem_ty);
            }
            NodeKind::Tuple => {
                let elems: Vec<Ty> = match &elem_ty {
                    Ty::Tuple(items) => items.iter().cloned().collect(),
                    _ => target.children.iter().map(|_| Ty::Any).collect(),
                };
                for (index, item) in target.children.iter().enumerate() {
                    let elem = elems.get(index).cloned().unwrap_or(Ty::Any);
                    match item.kind {
                        NodeKind::Identifier => {
                            env.declare(&self.text_of(item), elem);
                        }
                        NodeKind::Tuple => self.bind_for_target(item, elem, env),
                        _ => {
                            self.check_expr(item, env);
                        }
                    }
                }
            }
            _ => {
                self.check_expr(target, env);
            }
        }
    }

    fn check_match(&mut self, node: &'a Node, env: &mut Env) {
        let scrutinee = node.children.first();
        let scrut_ty = scrutinee.map(|s| self.check_expr(s, env)).unwrap_or(Ty::Any);
        self.check_match_exhaustive(node, &scrut_ty);
        let base = env.clone();
        let mut arm_envs: Vec<Env> = Vec::new();
        let mut any_arm = false;
        for child in &node.children {
            if child.kind != NodeKind::Case {
                continue;
            }
            any_arm = true;
            let Some(pattern) = child.children.first() else {
                continue;
            };
            let body = child.children.get(1);
            let mut arm = base.fork();
            self.check_pattern(pattern, &scrut_ty, &mut arm);
            if let Some(body) = body {
                if body.kind == NodeKind::Block {
                    self.check_block_no_terminate_reset(body, &mut arm);
                } else {
                    self.check_stmt(body, &mut arm);
                }
            }
            arm_envs.push(arm);
        }
        if !any_arm {
            return;
        }
        let merged = merge_arms(&base, &arm_envs);
        *env = merged;
    }

    fn check_match_exhaustive(&mut self, node: &'a Node, scrut_ty: &Ty) {
        let mut has_wildcard = false;
        let mut has_bool_true = false;
        let mut has_bool_false = false;
        let mut has_none = false;
        for child in &node.children {
            if child.kind != NodeKind::Case {
                continue;
            }
            let Some(pattern) = child.children.first() else {
                continue;
            };
            match pattern.kind {
                NodeKind::Wildcard => has_wildcard = true,
                NodeKind::Boolean => match pattern.value.as_ref() {
                    Some(Const::Bool(true)) => has_bool_true = true,
                    Some(Const::Bool(false)) => has_bool_false = true,
                    _ => {}
                },
                NodeKind::None => has_none = true,
                _ => {}
            }
        }
        if has_wildcard {
            return;
        }
        let scrutinee = node.children.first();
        match scrut_ty {
            Ty::None_ => {
                if !has_none {
                    self.error_match_exhaustive(
                        node,
                        scrutinee,
                        "expected a 'none' arm or a wildcard '_' arm",
                    );
                }
            }
            Ty::Bool => {
                if !(has_bool_true && has_bool_false) {
                    self.error_match_exhaustive(
                        node,
                        scrutinee,
                        "expected 'true' and 'false' arms or a wildcard '_' arm",
                    );
                }
            }
            Ty::Int | Ty::Float | Ty::Str | Ty::Unknown | Ty::Any | Ty::Union(_) | Ty::Tuple(_)
                if !matches!(scrut_ty, Ty::Bool) => {
                self.error_match_exhaustive(
                    node,
                    scrutinee,
                    "expected a wildcard '_' arm; the match arms do not cover every case",
                );
            }
            _ => {}
        }
    }

    fn error_match_exhaustive(&mut self, node: &'a Node, scrut: Option<&'a Node>, message: &str) {
        self.error(
            scrut.unwrap_or(node),
            format!("match exhaustiveness: {message}"),
        );
    }

    fn check_pattern(&mut self, pattern: &'a Node, scrut: &Ty, env: &mut Env) {
        match pattern.kind {
            NodeKind::Wildcard => {
            }
            NodeKind::Identifier => {
                env.declare(&self.text_of(pattern), scrut.clone());
            }
            NodeKind::Tuple => {
                let elems: Vec<Ty> = match scrut {
                    Ty::Tuple(items) => items.iter().cloned().collect(),
                    Ty::Any => pattern.children.iter().map(|_| Ty::Any).collect(),
                    _ => {
                        self.error(pattern, format!("tuple pattern does not match type '{}'", scrut));
                        pattern.children.iter().map(|_| Ty::Unknown).collect()
                    }
                };
                for (index, item) in pattern.children.iter().enumerate() {
                    let elem = elems.get(index).cloned().unwrap_or(Ty::Unknown);
                    self.check_pattern(item, &elem, env);
                }
            }
            _ => {
                let lit = literal_ty(pattern).unwrap_or(Ty::Any);
                if !matches!(lit, Ty::Any) && !matches_pattern_ty(&lit, scrut) {
                    self.error(
                        pattern,
                        format!(
                            "pattern of type '{}' does not match scrutinee of type '{}'",
                            lit, scrut
                        ),
                    );
                }
            }
        }
    }

    fn check_try(&mut self, node: &'a Node, env: &mut Env) {
        let base = env.clone();
        for child in &node.children {
            match child.kind {
                NodeKind::Catch => {
                    let mut arm = base.fork();
                    if let Some(var) = child.children.first() {
                        if var.kind == NodeKind::Identifier {
                            arm.declare(&self.text_of(var), Ty::Any);
                        }
                    }
                    if let Some(body) = child.children.get(1).or_else(|| child.children.last()) {
                        if body.kind == NodeKind::Block {
                            self.check_block_no_terminate_reset(body, &mut arm);
                        } else {
                            self.check_stmt(body, &mut arm);
                        }
                    }
                }
                NodeKind::Finally => {
                    let mut arm = base.fork();
                    for body in &child.children {
                        self.check_stmt(body, &mut arm);
                    }
                }
                NodeKind::Block => {
                    let mut arm = base.fork();
                    self.check_block_no_terminate_reset(child, &mut arm);
                }
                _ => {
                    self.check_stmt(child, env);
                }
            }
        }
        *env = base;
    }
}

fn has_if_cases(node: &Node) -> bool {
    node.children
        .iter()
        .any(|c| matches!(c.kind, NodeKind::Branch | NodeKind::ElIf | NodeKind::IfCase))
}

fn iter_range_call<'a>(node: &'a Node, source: &Source) -> Option<&'a Node> {
    if node.kind != NodeKind::Call {
        return None;
    }
    let callee = node.children.first()?;
    if callee.kind != NodeKind::Property {
        return None;
    }
    let member = callee.children.get(1)?;
    let recv = callee.children.first()?;
    if recv.kind == NodeKind::Identifier && member.kind == NodeKind::Identifier {
        let recv_text = source.span(recv.start, recv.length);
        let member_text = source.span(member.start, member.length);
        if recv_text == "iter" && member_text == "range" {
            return Some(node);
        }
    }
    None
}

fn literal_ty(node: &Node) -> Option<Ty> {
    match node.kind {
        NodeKind::Integer => Some(Ty::Int),
        NodeKind::Float => Some(Ty::Float),
        NodeKind::String | NodeKind::FString => Some(Ty::Str),
        NodeKind::Boolean => Some(Ty::Bool),
        NodeKind::None => Some(Ty::None_),
        _ => None,
    }
}

fn matches_pattern_ty(lit: &Ty, scrut: &Ty) -> bool {
    match (lit, scrut) {
        (_, Ty::Any) | (Ty::Any, _) => true,
        (Ty::Int, Ty::Int) | (Ty::Float, Ty::Float) | (Ty::Bool, Ty::Bool) | (Ty::Str, Ty::Str) | (Ty::None_, Ty::None_) => true,
        (Ty::Int, Ty::Float) | (Ty::Float, Ty::Int) => true,
        (Ty::Union(members), _) => members.iter().any(|m| matches_pattern_ty(m, scrut)),
        (_, Ty::Union(members)) => members.iter().any(|m| matches_pattern_ty(lit, m)),
        _ => false,
    }
}



impl<'a> Checker<'a> {
    fn check_expr(&mut self, node: &'a Node, env: &mut Env) -> Ty {
        match node.kind {
            NodeKind::Integer => Ty::Int,
            NodeKind::Float => Ty::Float,
            NodeKind::String => Ty::Str,
            NodeKind::FString => {
                for child in &node.children {
                    if child.kind != NodeKind::Identifier || child.value.is_none() {
                        let _ = self.check_expr(child, env);
                    }
                }
                Ty::Str
            }
            NodeKind::Boolean => Ty::Bool,
            NodeKind::None => Ty::None_,
            NodeKind::Identifier => self.lookup_value(node, env),
            NodeKind::Tuple => self.check_tuple_literal(node, env),
            NodeKind::List => self.check_list_literal(node, env),
            NodeKind::Set => self.check_set_literal(node, env),
            NodeKind::Dict => self.check_dict_literal(node, env),
            NodeKind::Unary => self.check_unary(node, env),
            NodeKind::Binary => self.check_binary(node, env),
            NodeKind::Ternary => self.check_ternary(node, env),
            NodeKind::Call => self.check_call(node, env),
            NodeKind::Property => self.check_property(node, env),
            NodeKind::Index => self.check_index(node, env),
            NodeKind::Slice => self.check_index(node, env),
            NodeKind::Lambda => self.check_lambda(node, env),
            NodeKind::Match => {
                let scrutinee = node.children.first();
                let scrut_ty = scrutinee.map(|s| self.check_expr(s, env)).unwrap_or(Ty::Any);
                self.check_match_exhaustive(node, &scrut_ty);
                let mut arm_types = Vec::new();
                for child in &node.children {
                    if child.kind == NodeKind::Case {
                        if let Some(body) = child.children.get(1) {
                            arm_types.push(self.check_expr(body, env));
                        }
                    }
                }
                if arm_types.is_empty() {
                    Ty::None_
                } else {
                    Ty::union(arm_types)
                }
            }
            NodeKind::Wildcard => Ty::Any,
            NodeKind::Typed => {
                self.error(node, "type annotation is not a value");
                Ty::Unknown
            }
            NodeKind::Tagged => {
                for child in &node.children {
                    self.check_expr(child, env);
                }
                Ty::Int
            }
            NodeKind::Spread => {
                if let Some(child) = node.children.first() {
                    self.check_expr(child, env);
                }
                Ty::Any
            }
            NodeKind::KwSpread | NodeKind::Pair => {
                for child in &node.children {
                    self.check_expr(child, env);
                }
                Ty::Any
            }
            NodeKind::Block => {
                let mut block_env = env.fork();
                for child in &node.children {
                    self.check_stmt(child, &mut block_env);
                }
                Ty::None_
            }
            NodeKind::Class => {
                Ty::Class(Rc::from(first_identifier_text(node, self.source).unwrap_or_default()))
            }
            NodeKind::Extension => Ty::Any,
            _ => {
                for child in &node.children {
                    self.check_expr(child, env);
                }
                Ty::Any
            }
        }
    }

    fn lookup_value(&mut self, node: &'a Node, env: &mut Env) -> Ty {
        let name = self.text_of(node);
        if let Some(ty) = env.lookup(&name).cloned() {
            return ty;
        }
        if let Some(ty) = self.globals.get(&name).cloned() {
            return ty;
        }
        if self.aliases.contains_key(&name) {
            self.error(node, format!("'{name}' is a type alias, not a value"));
            return Ty::Unknown;
        }
        if self.classes.contains_key(&name) {
            return Ty::Class(Rc::from(name.as_str()));
        }
        if self.enums.iter().any(|n| n == &name) {
            return Ty::Any;
        }
        if let Some(sig) = self.lookup_fn(&name) {
            return Ty::Func {
                params: sig.params[sig.receiver..].into(),
                names: sig.param_names[sig.receiver..].into(),
                ret: Rc::new(sig.ret.clone()),
                required: sig.required.saturating_sub(sig.receiver),
            };
        }
        if self.import_names.iter().any(|n| n == &name) {
            return Ty::Any;
        }
        if let Some(ty) = builtin_ty(&name) {
            return ty;
        }
        self.error(node, format!("undefined name '{name}'"));
        Ty::Unknown
    }

    fn lookup_fn(&self, name: &str) -> Option<FnSig> {
        self.fn_sigs.get(name).cloned()
    }

    fn check_tuple_literal(&mut self, node: &'a Node, env: &mut Env) -> Ty {
        let items: Vec<Ty> = node
            .children
            .iter()
            .map(|c| self.check_expr(c, env))
            .collect();
        Ty::tuple(items)
    }

    fn check_list_literal(&mut self, node: &'a Node, env: &mut Env) -> Ty {
        let elems: Vec<Ty> = node
            .children
            .iter()
            .filter_map(|c| match c.kind {
                NodeKind::Spread => None,
                _ => Some(self.check_expr(c, env)),
            })
            .collect();
        if elems.is_empty() {
            return Ty::list(Ty::Any);
        }
        Ty::list(Ty::union(elems))
    }

    fn check_set_literal(&mut self, node: &'a Node, env: &mut Env) -> Ty {
        let elems: Vec<Ty> = node
            .children
            .iter()
            .map(|c| self.check_expr(c, env))
            .collect();
        if elems.is_empty() {
            return Ty::Set(Rc::new(Ty::Any));
        }
        Ty::Set(Rc::new(Ty::union(elems)))
    }

    fn check_dict_literal(&mut self, node: &'a Node, env: &mut Env) -> Ty {
        let mut keys = Vec::new();
        let mut values = Vec::new();
        for child in &node.children {
            match child.kind {
                NodeKind::Pair => {
                    if let Some(key) = child.children.first() {
                        keys.push(self.check_expr(key, env));
                    }
                    if let Some(value) = child.children.get(1) {
                        values.push(self.check_expr(value, env));
                    }
                }
                NodeKind::Spread | NodeKind::KwSpread => {
                    self.check_expr(child, env);
                    keys.push(Ty::Any);
                    values.push(Ty::Any);
                }
                _ => {
                    self.check_expr(child, env);
                }
            }
        }
        let k = if keys.is_empty() { Ty::Any } else { Ty::union(keys) };
        let v = if values.is_empty() { Ty::Any } else { Ty::union(values) };
        Ty::dict(k, v)
    }

    fn check_unary(&mut self, node: &'a Node, env: &mut Env) -> Ty {
        let Some(operand) = node.children.first() else {
            return Ty::Any;
        };
        let ty = self.check_expr(operand, env);
        let kind = node.op;
        match kind {
            Some(TokenType::Minus) | Some(TokenType::Negative) | Some(TokenType::Plus) | Some(TokenType::Positive) => {
                match ty {
                    Ty::Int => Ty::Int,
                    Ty::Float => Ty::Float,
                    Ty::Any => Ty::Any,
                    _ => {
                        self.error(node, format!("unary '-' on type '{}'", ty));
                        Ty::Unknown
                    }
                }
            }
            Some(TokenType::Not) | Some(TokenType::Bang) => {
                if matches!(ty, Ty::Bool | Ty::Any) {
                    ty
                } else {
                    self.error(node, format!("'not' on type '{}'", ty));
                    Ty::Unknown
                }
            }
            Some(TokenType::BNot) => {
                if matches!(ty, Ty::Int | Ty::Any) {
                    Ty::Int
                } else {
                    self.error(node, format!("'~' on type '{}'", ty));
                    Ty::Unknown
                }
            }
            _ => Ty::Any,
        }
    }

    fn check_ternary(&mut self, node: &'a Node, env: &mut Env) -> Ty {
        let cond = node.children.first();
        if let Some(cond) = cond {
            self.check_condition(cond, env);
        }
        let mut arm_types = Vec::new();
        for child in node.children.iter().skip(1) {
            arm_types.push(self.check_expr(child, env));
        }
        if arm_types.is_empty() {
            Ty::None_
        } else {
            Ty::union(arm_types)
        }
    }

    fn check_binary(&mut self, node: &'a Node, env: &mut Env) -> Ty {
        let Some(lhs) = node.children.first() else {
            return Ty::Any;
        };
        let Some(rhs) = node.children.get(1) else {
            return Ty::Any;
        };
        let lty = self.check_expr(lhs, env);
        let rty = self.check_expr(rhs, env);
        let span = self.span(node);
        check_binop(node.op, &lty, &rty, self, span)
    }

    fn check_index(&mut self, node: &'a Node, env: &mut Env) -> Ty {
        let Some(obj) = node.children.first() else {
            return Ty::Any;
        };
        let obj_ty = self.check_expr(obj, env);
        if let Some(index) = node.children.get(1) {
            let _ = self.check_expr(index, env);
        }
        match obj_ty {
            Ty::List(elem) | Ty::Set(elem) => refine_unknown((*elem).clone()),
            Ty::Dict(_, value) => (*value).clone(),
            Ty::Tuple(items) => {
                if let Some(index_node) = node.children.get(1) {
                    if let NodeKind::Integer = index_node.kind {
                        if let Some(Const::Int(i)) = index_node.value {
                            let index = i as usize;
                            if let Some(elem) = items.get(index) {
                                return elem.clone();
                            }
                        }
                    }
                }
                refine_unknown(Ty::union(items.iter().cloned().collect()))
            }
            Ty::Str => Ty::Str,
            Ty::Any => Ty::Any,
            Ty::Unknown => Ty::Unknown,
            other => {
                self.error(node, format!("value of type '{}' does not support indexing", other));
                Ty::Unknown
            }
        }
    }

    fn check_property(&mut self, node: &'a Node, env: &mut Env) -> Ty {
        let Some(receiver) = node.children.first() else {
            return Ty::Any;
        };
        let rty = self.check_expr(receiver, env);
        let member = node
            .children
            .get(1)
            .map(|c| self.text_of(c))
            .unwrap_or_default();
        let optional = node.op == Some(TokenType::QuestionDot);
        let ty = match &rty {
            Ty::Class(class) => {
                let class_name = class.to_string();
                if let Some(field) = self.class_field(&class_name, &member) {
                    refine_unknown(field)
                } else if let Some(method) = self.class_method(&class_name, &member) {
                    method
                } else if self.class_has_getattr(&class_name) {
                    Ty::Any
                } else {
                    self.error(node, format!("'{class_name}' object has no attribute '{member}'"));
                    Ty::Unknown
                }
            }
            Ty::None_ => {
                if optional {
                    Ty::None_
                } else {
                    self.error(
                        node,
                        format!("member access on 'none' ('{member}')"),
                    );
                    Ty::Unknown
                }
            }
            Ty::Any | Ty::List(_) | Ty::Dict(..) | Ty::Set(_) | Ty::Tuple(_) | Ty::Str | Ty::Int | Ty::Float | Ty::Bool => {
                Ty::Any
            }
            _ => Ty::Any,
        };
        if optional {
            Ty::union(vec![ty, Ty::None_])
        } else {
            ty
        }
    }

    fn check_lambda(&mut self, node: &'a Node, env: &mut Env) -> Ty {
        let mut lambda_env = env.fork();
        if let Some(params) = node.children.first() {
            if params.kind == NodeKind::Tuple {
                for param in &params.children {
                    if let Some(name) = param_name_text(param, self.source) {
                        lambda_env.declare(&name, Ty::Any);
                    }
                }
            }
        }
        if let Some(body) = node.children.get(1) {
            self.check_block_no_terminate_reset(body, &mut lambda_env);
        }
        Ty::Func {
            params: vec![Ty::Any].into(),
            names: vec!["arg0".to_string()].into(),
            ret: Rc::new(Ty::Any),
            required: 0,
        }
    }

    fn check_call(&mut self, node: &'a Node, env: &mut Env) -> Ty {        let Some(callee) = node.children.first() else {
            return Ty::Any;
        };
        match callee.kind {
            NodeKind::Identifier => {
                let name = self.text_of(callee);
                let local_fn = env.lookup(&name).cloned().or_else(|| self.globals.get(&name).cloned());
                if let Some(Ty::Func { params, names, ret, required }) = local_fn {
                    return self.apply_sig(
                        &FnSig {
                            name,
                            params: params.to_vec(),
                            param_names: names.to_vec(),
                            ret: (*ret).clone(),
                            required,
                            is_method: false,
                            receiver: 0,
                            variadic: false,
                        },
                        node,
                        0,
                        env,
                    );
                }
                if local_fn.is_some() {
                    let ty = local_fn.unwrap();
                    if matches!(ty, Ty::Any | Ty::Unknown) {
                        self.check_call_args_any(node.children.get(1), env);
                        return Ty::Any;
                    }
                    self.error(callee, format!("'{name}' of type '{ty}' is not callable"));
                    return Ty::Unknown;
                }
                if let Some(info) = self.classes.get(&name).cloned() {
                    let _ = info.ctor_required;
                    if info.ctor_params.is_empty() {
                        self.check_call_args_any(node.children.get(1), env);
                        return Ty::Class(Rc::from(name.as_str()));
                    }
                    let ctor_sig = FnSig {
                        name: name.clone(),
                        params: info.ctor_params.iter().map(|(_, ty)| ty.clone()).collect(),
                        param_names: info.ctor_params.iter().map(|(n, _)| n.clone()).collect(),
                        ret: Ty::Class(Rc::from(name.as_str())),
                        required: info.ctor_required,
                        is_method: false,
                        receiver: 0,
                        variadic: false,
                    };
                    return self.apply_sig(&ctor_sig, node, 0, env);
                }
                if let Some(sig) = self.fn_sigs.get(&name).cloned() {
                    return self.apply_sig(&sig, node, 0, env);
                }
                if let Some(_) = self.enums.iter().find(|n| *n == &name) {
                    self.check_call_args_any(node.children.get(1), env);
                    return Ty::Any;
                }
                if self.import_names.iter().any(|n| n == &name) {
                    self.check_call_args_any(node.children.get(1), env);
                    return Ty::Any;
                }
                if let Some(ty) = builtin_ty(&name) {
                    if let Ty::Func { params, names, ret, required } = ty {
                        return self.apply_sig(
                            &FnSig {
                                name,
                                params: params.to_vec(),
                                param_names: names.to_vec(),
                                ret: (*ret).clone(),
                                required,
                                is_method: false,
                                receiver: 0,
                                variadic: false,
                            },
                            node,
                            0,
                            env,
                        );
                    }
                    self.check_call_args_any(node.children.get(1), env);
                    return Ty::Any;
                }
                if let Some(ty) = env.lookup(&name).cloned() {
                    if let Ty::Func { params, names, ret, required } = ty {
                        return self.apply_sig(
                            &FnSig {
                                name,
                                params: params.to_vec(),
                                param_names: names.to_vec(),
                                ret: (*ret).clone(),
                                required,
                                is_method: false,
                                receiver: 0,
                                variadic: false,
                            },
                            node,
                            0,
                            env,
                        );
                    }
                    if matches!(ty, Ty::Any | Ty::Unknown) {
                        self.check_call_args_any(node.children.get(1), env);
                        return Ty::Any;
                    }
                    self.error(node, format!("'{name}' of type '{ty}' is not callable"));
                    return Ty::Unknown;
                }
                self.error(node, format!("undefined name '{name}'"));
                Ty::Unknown
            }
            NodeKind::Property => {
                let recv_ty = self.check_expr(&callee.children[0], env);
                let member = callee.children.get(1).map(|c| self.text_of(c)).unwrap_or_default();
                let optional = callee.op == Some(TokenType::QuestionDot);
                if optional
                    && (matches!(recv_ty, Ty::None_)
                        || matches!(&recv_ty, Ty::Union(m) if m.iter().any(|t| matches!(t, Ty::None_))))
                {
                    self.check_call_args_any(node.children.get(1), env);
                    return Ty::None_;
                }
                match recv_ty {
                    Ty::Class(class) => {
                        let class_name = class.to_string();
                        if let Some(field_ty) = self.class_field(&class_name, &member) {
                            if let Ty::Func { params, names, ret, required } = field_ty.clone() {
                                return self.apply_sig(
                                    &FnSig {
                                        name: member,
                                        params: params.to_vec(),
                                        param_names: names.to_vec(),
                                        ret: (*ret).clone(),
                                        required,
                                        is_method: false,
                                        receiver: 0,
                                        variadic: false,
                                    },
                                    node,
                                    0,
                                    env,
                                );
                            }
                            self.error(
                                callee,
                                format!("'{}' is not callable", field_ty),
                            );
                            return Ty::Unknown;
                        }
                        if let Some(method_ty) = self.class_method(&class_name, &member) {
                            if let Ty::Func { params, names, ret, required } = method_ty {
                                return self.apply_sig(
                                    &FnSig {
                                        name: member,
                                        params: params.to_vec(),
                                        param_names: names.to_vec(),
                                        ret: (*ret).clone(),
                                        required,
                                        is_method: false,
                                        receiver: 0,
                                        variadic: false,
                                    },
                                    node,
                                    0,
                                    env,
                                );
                            }
                        }
                        if self.class_has_getattr(&class_name) {
                            self.check_call_args_any(node.children.get(1), env);
                            return Ty::Any;
                        }
                        self.error(callee, format!("'{class_name}' object has no attribute '{member}'"));
                        Ty::Unknown
                    }
                    Ty::Any => {
                        if let Some(ty) = self.iter_array_sig(node, env) {
                            return ty;
                        }
                        if let Some(ty) = self.iter_minmax_sig(node, env) {
                            return ty;
                        }
                        if let Some(member_node) = callee.children.get(1) {
                            let member = self.text_of(member_node);
                            let module = self.text_of(&callee.children[0]);
                            if let Some(ty) = native_module_ret_type(&module, &member) {
                                self.check_call_args_any(node.children.get(1), env);
                                return ty;
                            }
                        }
                        self.check_call_args_any(node.children.get(1), env);
                        Ty::Any
                    }
                    other => {
                        let member_sig = match &other {
                            Ty::List(elem) => list_member_sig(elem, &member),
                            Ty::Dict(key, value) => dict_member_sig(key, value, &member),
                            _ => None,
                        };
                        if let Some((params, param_names, required, ret)) = member_sig {
                            return self.apply_sig(
                                &FnSig {
                                    name: member,
                                    params,
                                    param_names,
                                    ret,
                                    required,
                                    is_method: false,
                                    receiver: 0,
                                    variadic: false,
                                },
                                node,
                                0,
                                env,
                            );
                        }
                        let base = match &other {
                            Ty::List(_) => "list",
                            Ty::Dict(..) => "dict",
                            _ => "value",
                        };
                        self.error(callee, format!("'{base}' object has no attribute '{member}'"));
                        Ty::Unknown
                    }
                }
            }
            NodeKind::Call => {
                let inner = self.check_expr(callee, env);
                match inner {
                    Ty::Func { params, names, ret, required } => {
                        let sig = FnSig {
                            name: "<call>".to_string(),
                            params: params.to_vec(),
                            param_names: names.to_vec(),
                            ret: (*ret).clone(),
                            required,
                            is_method: false,
                            receiver: 0,
                            variadic: false,
                        };
                        self.apply_sig(&sig, node, 0, env)
                    }
                    Ty::Any => {
                        self.check_call_args_any(node.children.get(1), env);
                        Ty::Any
                    }
                    other => {
                        self.error(callee, format!("value of type '{}' is not callable", other));
                        Ty::Unknown
                    }
                }
            }
            _ => {
                let inner = self.check_expr(callee, env);
                match inner {
                    Ty::Func { params, names, ret, required } => {
                        let sig = FnSig {
                            name: "<call>".to_string(),
                            params: params.to_vec(),
                            param_names: names.to_vec(),
                            ret: (*ret).clone(),
                            required,
                            is_method: false,
                            receiver: 0,
                            variadic: false,
                        };
                        self.apply_sig(&sig, node, 0, env)
                    }
                    Ty::Any => {
                        self.check_call_args_any(node.children.get(1), env);
                        Ty::Any
                    }
                    other => {
                        self.error(callee, format!("value of type '{}' is not callable", other));
                        Ty::Unknown
                    }
                }
            }
        }
    }

    fn apply_sig(&mut self, sig: &FnSig, node: &'a Node, receiver: usize, env: &mut Env) -> Ty {
        let args = node.children.get(1);
        let _ = receiver;
        let Some(args) = args else {
            return sig.ret.clone();
        };
        if args.kind != NodeKind::Tuple {
            self.check_expr(args, env);
            return sig.ret.clone();
        }
        let mut positional: Vec<&Node> = Vec::new();
        let mut kwargs: Vec<(String, &Node)> = Vec::new();
        let mut has_spread = false;
        for arg in &args.children {
            match arg.kind {
                NodeKind::Spread => {
                    has_spread = true;
                    self.check_expr(arg, env);
                }
                NodeKind::Pair => {
                    let name = arg
                        .children
                        .first()
                        .map(|c| self.text_of(c))
                        .unwrap_or_default();
                    let value = arg.children.get(1);
                    if let Some(value) = value {
                        kwargs.push((name, value));
                    }
                }
                NodeKind::KwSpread => {
                    has_spread = true;
                    self.check_expr(arg, env);
                }
                _ => positional.push(arg),
            }
        }
        let argc = positional.len();
        if !has_spread {
            let required = sig.required.saturating_sub(sig.receiver);
            let total = sig.params.len().saturating_sub(sig.receiver);
            let max = if sig.variadic { usize::MAX } else { total };
            if argc + kwargs.len() < required {
                self.error(
                    args,
                    format!(
                        "missing arguments: '{}' requires at least {} argument(s), got {}",
                        sig.name,
                        required,
                        argc + kwargs.len()
                    ),
                );
            } else if argc > max {
                self.error(
                    args,
                    format!(
                        "too many arguments: '{}' accepts at most {} argument(s), got {}",
                        sig.name, max, argc
                    ),
                );
            }
        }
        let mut bound = vec![false; sig.params.len()];
        for slot in 0..sig.receiver {
            bound[slot] = true;
        }
        let mut slot = sig.receiver;
        for arg in &positional {
            if slot >= sig.params.len() {
                self.check_expr(arg, env);
                continue;
            }
            let vty = self.check_expr(arg, env);
            let expected = &sig.params[slot];
            if !assignable(expected, &vty, &self.classes) {
                self.error(
                    arg,
                    format!(
                        "argument '{}' of '{}' expects '{}', got '{}'",
                        sig.param_names[slot], sig.name, expected, vty
                    ),
                );
            }
            bound[slot] = true;
            slot += 1;
        }
        for (name, value) in &kwargs {
            let vty = self.check_expr(value, env);
            let slot = sig
                .param_names
                .iter()
                .enumerate()
                .find(|(_, p)| *p == name)
                .map(|(i, _)| i);
            let Some(slot) = slot else {
                if sig.variadic {
                    continue;
                }
                self.error(value, format!("no parameter '{name}' in '{}'", sig.name));
                continue;
            };
            if bound[slot] {
                self.error(value, format!("parameter '{name}' is bound twice"));
                continue;
            }
            let expected = &sig.params[slot];
            if !assignable(expected, &vty, &self.classes) {
                self.error(
                    *value,
                    format!(
                        "argument '{name}' of '{}' expects '{}', got '{}'",
                        sig.name, expected, vty
                    ),
                );
            }
            bound[slot] = true;
        }
        sig.ret.clone()
    }

    fn check_call_args_any(&mut self, args: Option<&'a Node>, env: &mut Env) {
        if let Some(args) = args {
            if args.kind == NodeKind::Tuple {
                for arg in &args.children {
                    self.check_expr(arg, env);
                }
            } else {
                self.check_expr(args, env);
            }
        }
    }
}







fn check_binop(op: Option<TokenType>, lty: &Ty, rty: &Ty, checker: &mut Checker, span: (usize, usize)) -> Ty {
    if matches!(lty, Ty::Unknown) || matches!(rty, Ty::Unknown) {
        return Ty::Unknown;
    }
    match op {
        Some(TokenType::Plus)
        | Some(TokenType::Minus)
        | Some(TokenType::Asterisk)
        | Some(TokenType::DoubleAsterisk) => {
            if matches!(op, Some(TokenType::Plus)) && *lty == Ty::Str && *rty == Ty::Str {
                return Ty::Str;
            }
            if lty.is_numeric() && rty.is_numeric() {
                if *lty == Ty::Int && *rty == Ty::Int {
                    return Ty::Int;
                }
                if matches!(lty, Ty::Any) || matches!(rty, Ty::Any) {
                    return Ty::Any;
                }
                return Ty::Float;
            }
            if matches!(lty, Ty::Any) || matches!(rty, Ty::Any) {
                return Ty::Any;
            }
            checker.binary_error(span, op.unwrap(), lty, rty);
            Ty::Unknown
        }
        Some(TokenType::Slash) => {
            if lty.is_numeric() && rty.is_numeric() {
                return Ty::Float;
            }
            if matches!(lty, Ty::Any) || matches!(rty, Ty::Any) {
                return Ty::Any;
            }
            checker.binary_error(span, op.unwrap(), lty, rty);
            Ty::Unknown
        }
        Some(TokenType::DoubleSlash) => {
            if lty.is_numeric() && rty.is_numeric() {
                return Ty::Int;            }
            if matches!(lty, Ty::Any) || matches!(rty, Ty::Any) {
                return Ty::Any;
            }
            checker.binary_error(span, op.unwrap(), lty, rty);
            Ty::Unknown
        }
        Some(TokenType::Percent) => {
            match (lty, rty) {
                (Ty::Int, Ty::Int) => Ty::Int,
                (Ty::Float, _) | (_, Ty::Float) => Ty::Float,
                (Ty::Any, _) | (_, Ty::Any) => Ty::Any,
                _ => {
                    checker.binary_error(span, op.unwrap(), lty, rty);
                    Ty::Unknown
                }
            }
        }
        Some(TokenType::BAnd) | Some(TokenType::BOr) | Some(TokenType::BXor)
        | Some(TokenType::LeftShift) | Some(TokenType::RightShift) => {
            match (lty, rty) {
                (Ty::Int, Ty::Int) => Ty::Int,
                (Ty::Any, _) | (_, Ty::Any) => Ty::Any,
                _ => {
                    checker.binary_error(span, op.unwrap(), lty, rty);
                    Ty::Unknown
                }
            }
        }
        Some(TokenType::Equal) | Some(TokenType::Unequal) => {
            if eq_comparable(lty, rty) {
                return Ty::Bool;
            }
            checker.binary_error(span, op.unwrap(), lty, rty);
            Ty::Unknown
        }
        Some(TokenType::LessThan)
        | Some(TokenType::GreaterThan)
        | Some(TokenType::LessOrEqual)
        | Some(TokenType::GreaterOrEqual) => {
            if lt_comparable(lty, rty) {
                return Ty::Bool;
            }
            checker.binary_error(span, op.unwrap(), lty, rty);
            Ty::Unknown
        }
        Some(TokenType::Is) | Some(TokenType::IsNot) | Some(TokenType::In) => Ty::Bool,
        Some(TokenType::And) | Some(TokenType::Or) => {
            match (lty, rty) {
                (Ty::Bool, Ty::Bool) => Ty::Bool,
                (Ty::Any, _) | (_, Ty::Any) => Ty::Any,
                (a, b) if a == b => a.clone(),
                _ => {
                    checker.binary_error(span, op.unwrap(), lty, rty);
                    Ty::Unknown
                }
            }
        }
        Some(TokenType::Xor) => {
            if matches!(lty, Ty::Bool | Ty::Any) && matches!(rty, Ty::Bool | Ty::Any) {
                Ty::Bool
            } else if matches!(lty, Ty::Int) && matches!(rty, Ty::Int) {
                Ty::Int
            } else {
                Ty::Any
            }
        }
        Some(TokenType::DoubleQuestion) => Ty::union(vec![lty.clone(), rty.clone()]),
        Some(TokenType::DotDot) => {
            if (lty.is_integer_ish() || matches!(lty, Ty::Float)) && (rty.is_integer_ish() || matches!(rty, Ty::Float)) {
                Ty::Any            } else if matches!(lty, Ty::Any) || matches!(rty, Ty::Any) {
                Ty::Any
            } else {
                checker.binary_error(span, op.unwrap(), lty, rty);
                Ty::Unknown
            }
        }
        _ => {
            Ty::Any
        }
    }
}

fn eq_comparable(lty: &Ty, rty: &Ty) -> bool {
    match (lty, rty) {
        (Ty::Any, _) | (_, Ty::Any) => true,
        (Ty::None_, _) | (_, Ty::None_) => true,
        (Ty::Int, Ty::Int) | (Ty::Int, Ty::Float) | (Ty::Float, Ty::Int) | (Ty::Float, Ty::Float) => true,
        (Ty::Bool, Ty::Bool) => true,
        (Ty::Str, Ty::Str) => true,
        (Ty::Class(a), Ty::Class(b)) => a == b,
        (Ty::Tuple(a), Ty::Tuple(b)) => a.len() == b.len()
            && a.iter().zip(b.iter()).all(|(x, y)| eq_comparable(x, y)),
        (Ty::List(a), Ty::List(b)) => eq_comparable(a, b),
        (Ty::Set(a), Ty::Set(b)) => eq_comparable(a, b),
        (Ty::Dict(k1, v1), Ty::Dict(k2, v2)) => eq_comparable(k1, k2) && eq_comparable(v1, v2),
        (Ty::Union(members), other) | (other, Ty::Union(members)) => {
            members.iter().all(|m| eq_comparable(m, other))
        }
        _ => false,
    }
}

fn lt_comparable(lty: &Ty, rty: &Ty) -> bool {
    match (lty, rty) {
        (Ty::Any, _) | (_, Ty::Any) => true,
        (Ty::Int, Ty::Int) | (Ty::Int, Ty::Float) | (Ty::Float, Ty::Int) | (Ty::Float, Ty::Float) => true,
        (Ty::Str, Ty::Str) => true,
        (Ty::Union(members), other) | (other, Ty::Union(members)) => {
            members.iter().all(|m| lt_comparable(m, other))
        }
        _ => false,
    }
}

impl Checker<'_> {
    fn binary_error(&mut self, span: (usize, usize), op: TokenType, lty: &Ty, rty: &Ty) -> Ty {
        let name = match op {
            TokenType::Plus => "+",
            TokenType::Minus => "-",
            TokenType::Asterisk => "*",
            TokenType::Slash => "/",
            TokenType::DoubleSlash => "//",
            TokenType::Percent => "%",
            TokenType::DoubleAsterisk => "**",
            TokenType::BAnd => "&",
            TokenType::BOr => "|",
            TokenType::BXor => "^",
            TokenType::LeftShift => "<<",
            TokenType::RightShift => ">>",
            TokenType::Equal => "==",
            TokenType::Unequal => "!=",
            TokenType::LessThan => "<",
            TokenType::GreaterThan => ">",
            TokenType::LessOrEqual => "<=",
            TokenType::GreaterOrEqual => ">=",
            TokenType::And => "and",
            TokenType::Or => "or",
            TokenType::DotDot => "..",
            _ => "op",
        };
        self.error_span(span.0, span.1,
            format!("unsupported operand types for '{name}': '{}' and '{}'", lty, rty),
        );
        Ty::Unknown
    }
}

fn list_member_sig(elem: &Ty, name: &str) -> Option<(Vec<Ty>, Vec<String>, usize, Ty)> {
    let e = elem.clone();
    let list_e = Ty::list(e.clone());
    let mk = |params: Vec<(String, Ty)>, required: usize, ret: Ty| {
        let names = params.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>();
        let params = params.into_iter().map(|(_, t)| t).collect::<Vec<_>>();
        Some((params, names, required, ret))
    };
    let int = Ty::Int;
    let boolean = Ty::Bool;
    match name {
        "add" | "hpush" => mk(vec![("value".into(), e.clone())], 1, Ty::None_),
        "extend" => mk(vec![("value".into(), Ty::Any)], 1, Ty::None_),
        "insert" => mk(vec![("value".into(), e.clone()), ("index".into(), int.clone())], 2, list_e),
        "extend_insert" => mk(vec![("value".into(), Ty::Any), ("index".into(), int.clone())], 2, list_e),
        "remove" => mk(vec![("obj".into(), e.clone())], 1, boolean.clone()),
        "remove_at" => mk(vec![("index".into(), int.clone())], 1, boolean.clone()),
        "index_of" => mk(vec![("obj".into(), e.clone())], 1, int.clone()),
        "contains" => mk(vec![("obj".into(), e.clone())], 1, boolean.clone()),
        "order" => mk(vec![("fn".into(), Ty::Any)], 1, Ty::None_),
        "sort" => mk(vec![], 0, Ty::None_),
        "reverse" => mk(vec![], 0, Ty::None_),
        "binary_search" => mk(vec![("obj".into(), e.clone())], 1, int.clone()),
        "lower_bound" => mk(vec![("obj".into(), e.clone())], 1, int.clone()),
        "upper_bound" => mk(vec![("obj".into(), e.clone())], 1, int.clone()),
        "hpop" => mk(vec![], 0, e.clone()),
        "pop" => mk(vec![("index".into(), int.clone())], 0, e.clone()),
        "map" => mk(vec![("type".into(), Ty::Any)], 1, Ty::list(Ty::Any)),
        "resize" => mk(vec![("size".into(), int.clone())], 1, Ty::None_),
        _ => None,
    }
}

fn dict_member_sig(key: &Ty, value: &Ty, name: &str) -> Option<(Vec<Ty>, Vec<String>, usize, Ty)> {
    let list_k = Ty::list(key.clone());
    let list_v = Ty::list(Ty::Any);
    let mk = |params: Vec<(String, Ty)>, required: usize, ret: Ty| {
        let names = params.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>();
        let params = params.into_iter().map(|(_, t)| t).collect::<Vec<_>>();
        Some((params, names, required, ret))
    };
    let boolean = Ty::Bool;
    match name {
        "add" => mk(vec![("key".into(), key.clone()), ("value".into(), value.clone())], 2, Ty::None_),
        "remove" => mk(vec![("key".into(), key.clone())], 1, boolean.clone()),
        "get" => mk(vec![("key".into(), key.clone()), ("default_value".into(), Ty::Any)], 1, value.clone()),
        "contains_key" => mk(vec![("key".into(), key.clone())], 1, boolean.clone()),
        "contains_value" => mk(vec![("value".into(), value.clone())], 1, boolean.clone()),
        "clear" => mk(vec![], 0, Ty::None_),
        "keys" => mk(vec![], 0, list_k),
        "values" => mk(vec![], 0, list_v),
        _ => None,
    }
}

pub fn assignable(target: &Ty, value: &Ty, classes: &HashMap<String, ClassInfo>) -> bool {
    if matches!(target, Ty::Any) || matches!(value, Ty::Any) {
        return true;
    }
    if matches!(target, Ty::Unknown) || matches!(value, Ty::Unknown) {
        return true;    }
    match (target, value) {
        (_, Ty::Union(members)) => {
            members.iter().all(|m| assignable(target, m, classes))
        }
        (Ty::Union(members), value) => {
            members.iter().any(|m| assignable(m, value, classes))
        }
        (Ty::List(a), Ty::List(b)) => assignable_elem(a, b),
        (Ty::Set(a), Ty::Set(b)) => assignable_elem(a, b),
        (Ty::Dict(k1, v1), Ty::Dict(k2, v2)) => {
            assignable_elem(k1, k2) && assignable_elem(v1, v2)
        }
        (Ty::Tuple(a), Ty::Tuple(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| assignable(x, y, classes))
        }
        (Ty::Class(a), Ty::Class(b)) => {
            if a == b {
                return true;
            }
            is_subclass_with(classes, b, a)
        }
        (Ty::Func { params: a_params, names: _, ret: a_ret, required: a_req }, Ty::Func { params: b_params, names: _, ret: b_ret, required: b_req }) if a_req == b_req => {
            a_params.len() == b_params.len()
                && a_params
                    .iter()
                    .zip(b_params.iter())
                    .all(|(a, b)| assignable(a, b, classes))
                && assignable(a_ret, b_ret, classes)
        }
        (Ty::Int, Ty::Int) | (Ty::Float, Ty::Float) | (Ty::Bool, Ty::Bool) | (Ty::Str, Ty::Str)
        | (Ty::None_, Ty::None_) => true,
        (Ty::Float, Ty::Int) => true,
        _ => false,
    }
}

pub(crate) fn native_module_ret_type(module: &str, member: &str) -> Option<Ty> {
    let int = Ty::Int;
    let float = Ty::Float;
    let boolean = Ty::Bool;
    let string = Ty::Str;
    match (module, member) {
        ("math", "sign") | ("math", "isqrt") => Some(int.clone()),
        ("math", "is_nan") | ("math", "is_infinite") | ("math", "is_finite") => Some(boolean.clone()),
        ("os", "pid") | ("os", "cpuCount") | ("os", "tickCount") => Some(int.clone()),
        ("os", "is64bit") => Some(boolean.clone()),
        ("os", "name") | ("os", "arch") | ("os", "hostname") | ("os", "username") | ("os", "version")
        | ("os", "sep") | ("os", "pathsep") | ("os", "linesep") => Some(string.clone()),
        ("sys", "pid") | ("sys", "cpuCount") | ("sys", "tickCount")
        | ("sys", "getcalldepth") | ("sys", "getrecursionlimit") => Some(int.clone()),
        ("sys", "is64bit") => Some(boolean.clone()),
        ("sys", "name") | ("sys", "arch") | ("sys", "hostname") | ("sys", "username") | ("sys", "version")
        | ("sys", "sep") | ("sys", "pathsep") | ("sys", "linesep") => Some(string.clone()),
        ("time", "epoch") | ("time", "epoch_ms") | ("time", "monotonic_ms") => Some(int.clone()),
        ("time", "monotonic") => Some(float.clone()),
        ("fs", "exists") | ("fs", "file") | ("fs", "dir") | ("fs", "readonly") | ("fs", "hidden") => Some(boolean.clone()),
        ("fs", "size") => Some(int.clone()),
        ("fs", "created") | ("fs", "modified") | ("fs", "accessed")
        | ("fs", "join") | ("fs", "parent") | ("fs", "name") | ("fs", "stem") | ("fs", "ext")
        | ("fs", "absolute") | ("fs", "relative") | ("fs", "normalize")
        | ("fs", "cwd") | ("fs", "temp") | ("fs", "home") | ("fs", "exe") | ("fs", "app") => Some(string.clone()),
        ("codes", "ord") | ("codes", "ascii") => Some(int.clone()),
        ("codes", "bin") | ("codes", "oct") | ("codes", "hex") | ("codes", "chr") => Some(string.clone()),
        ("random", "int") => Some(int.clone()),
        ("random", "float") => Some(float.clone()),
        ("random", "bool") | ("random", "chance") => Some(boolean.clone()),
        ("random", "uuid") => Some(string.clone()),
        ("re", "test") => Some(boolean.clone()),
        ("re", "escape") => Some(string.clone()),
        _ => None,
    }
}

fn assignable_elem(a: &Ty, b: &Ty) -> bool {
    if matches!(a, Ty::Any) || matches!(b, Ty::Any) {
        return true;
    }
    a == b
}

fn is_subclass_with(classes: &HashMap<String, ClassInfo>, child: &str, base: &str) -> bool {
    if child == base {
        return true;
    }
    let mut stack = vec![child.to_string()];
    let mut seen = Vec::new();
    while let Some(current) = stack.pop() {
        if seen.iter().any(|n| n == &current) {
            continue;
        }
        seen.push(current.clone());
        let Some(info) = classes.get(&current) else {
            continue;
        };
        for b in &info.bases {
            if b == base {
                return true;
            }
            stack.push(b.clone());
        }
    }
    false
}

fn builtin_ty(name: &str) -> Option<Ty> {
    fn func(params: Vec<Ty>, ret: Ty) -> Ty {
        let names = (0..params.len()).map(|i| format!("arg{i}")).collect::<Vec<_>>();
        let required = params.len();
        Ty::Func {
            params: params.into(),
            names: names.into(),
            ret: Rc::new(ret),
            required,
        }
    }
    match name {
        "int" => Some(func(vec![Ty::Any], Ty::Int)),
        "float" => Some(func(vec![Ty::Any], Ty::Float)),
        "bool" => Some(func(vec![Ty::Any], Ty::Bool)),
        "str" => Some(func(vec![Ty::Any], Ty::Str)),
        "len" => Some(func(vec![Ty::Any], Ty::Int)),
        "hash" => Some(func(vec![Ty::Any], Ty::Int)),
        "type" | "repr" => Some(func(vec![Ty::Any], Ty::Str)),
        "exit" => Some(func(vec![Ty::Any], Ty::None_)),
        "list" | "tuple" | "set" => Some(func(vec![Ty::Any], Ty::list(Ty::Any))),
        "dict" => Some(func(vec![Ty::Any], Ty::dict(Ty::Any, Ty::Any))),
        "throw" => Some(func(vec![Ty::Any], Ty::Any)),
        "throw_if" | "throw_unless" => Some(func(vec![Ty::Any, Ty::Any], Ty::Any)),
        _ => None,
    }
}



pub fn check_program(source: &Source, root: &Node) -> Result<CheckedProgram, Vec<LangError>> {
    Checker::new(source, root).run()
}
