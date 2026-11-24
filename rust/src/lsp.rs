
use std::collections::HashMap;
use std::collections::HashSet;
use std::io::{self, Read, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::OnceLock;

use unvm::ast::{Node, NodeKind};
use unvm::error::LangError;
use unvm::json::write_json_string;
use unvm::lexer::lex;
use unvm::optimize::optimize;
use unvm::parser::parse;
use unvm::source::Source;
use unvm::textual_infer::{self, InferredTypes};
use unvm::token::{Token, TokenType};
use unvm::typeck;


#[derive(Clone, Debug, PartialEq)]
enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    fn obj(pairs: Vec<(&str, Json)>) -> Json {
        Json::Obj(
            pairs
                .into_iter()
                .map(|(key, value)| (key.to_string(), value))
                .collect(),
        )
    }

    fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(text) => Some(text),
            _ => None,
        }
    }

    fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Num(number) => Some(*number),
            _ => None,
        }
    }

    fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(value) => Some(*value),
            _ => None,
        }
    }

    fn as_arr(&self) -> Option<&[Json]> {
        match self {
            Json::Arr(items) => Some(items),
            _ => None,
        }
    }
}

impl From<&str> for Json {
    fn from(value: &str) -> Json {
        Json::Str(value.to_string())
    }
}

impl From<String> for Json {
    fn from(value: String) -> Json {
        Json::Str(value)
    }
}

impl From<f64> for Json {
    fn from(value: f64) -> Json {
        Json::Num(value)
    }
}

impl From<bool> for Json {
    fn from(value: bool) -> Json {
        Json::Bool(value)
    }
}

struct JsonReader<'a> {
    chars: &'a [char],
    pos: usize,
}

impl<'a> JsonReader<'a> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t' | '\n' | '\r')) {
            self.pos += 1;
        }
    }

    fn expect(&mut self, expected: char) -> Result<(), String> {
        match self.peek() {
            Some(ch) if ch == expected => {
                self.pos += 1;
                Ok(())
            }
            other => Err(format!("expected '{expected}', found {other:?}")),
        }
    }

    fn value(&mut self) -> Result<Json, String> {
        self.skip_ws();
        match self.peek() {
            Some('{') => self.object(),
            Some('[') => self.array(),
            Some('"') => Ok(Json::Str(self.string()?)),
            Some('t') => self.literal("true", Json::Bool(true)),
            Some('f') => self.literal("false", Json::Bool(false)),
            Some('n') => self.literal("null", Json::Null),
            Some(ch) if ch == '-' || ch.is_ascii_digit() => self.number(),
            other => Err(format!("unexpected token {other:?}")),
        }
    }

    fn object(&mut self) -> Result<Json, String> {
        self.expect('{')?;
        let mut entries = Vec::new();
        self.skip_ws();
        if self.peek() == Some('}') {
            self.pos += 1;
            return Ok(Json::Obj(entries));
        }
        loop {
            self.skip_ws();
            let key = self.string()?;
            self.skip_ws();
            self.expect(':')?;
            let value = self.value()?;
            entries.push((key, value));
            self.skip_ws();
            match self.peek() {
                Some(',') => {
                    self.pos += 1;
                }
                Some('}') => {
                    self.pos += 1;
                    return Ok(Json::Obj(entries));
                }
                other => return Err(format!("expected ',' or '}}', found {other:?}")),
            }
        }
    }

    fn array(&mut self) -> Result<Json, String> {
        self.expect('[')?;
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(']') {
            self.pos += 1;
            return Ok(Json::Arr(items));
        }
        loop {
            items.push(self.value()?);
            self.skip_ws();
            match self.peek() {
                Some(',') => {
                    self.pos += 1;
                }
                Some(']') => {
                    self.pos += 1;
                    return Ok(Json::Arr(items));
                }
                other => return Err(format!("expected ',' or ']', found {other:?}")),
            }
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect('"')?;
        let mut out = String::new();
        loop {
            let ch = *self.chars.get(self.pos).ok_or("unterminated string")?;
            self.pos += 1;
            match ch {
                '"' => return Ok(out),
                '\\' => {
                    let escape = *self.chars.get(self.pos).ok_or("unterminated escape")?;
                    self.pos += 1;
                    match escape {
                        '"' => out.push('"'),
                        '\\' => out.push('\\'),
                        '/' => out.push('/'),
                        'b' => out.push('\u{8}'),
                        'f' => out.push('\u{c}'),
                        'n' => out.push('\n'),
                        'r' => out.push('\r'),
                        't' => out.push('\t'),
                        'u' => {
                            let high = self.hex4()?;
                            let code_point = if (0xD800..0xDC00).contains(&high) {
                                self.expect('\\')?;
                                self.expect('u')?;
                                let low = self.hex4()?;
                                0x10000 + (((high - 0xD800) << 10) | (low - 0xDC00))
                            } else {
                                high
                            };
                            out.push(char::from_u32(code_point).ok_or("invalid unicode scalar")?);
                        }
                        other => return Err(format!("invalid escape '\\{other}'")),
                    }
                }
                _ => out.push(ch),
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let mut value: u32 = 0;
        for _ in 0..4 {
            let ch = *self.chars.get(self.pos).ok_or("unterminated \\u escape")?;
            self.pos += 1;
            let digit = ch.to_digit(16).ok_or("invalid hex digit")?;
            value = value * 16 + digit;
        }
        Ok(value)
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.pos;
        while matches!(
            self.peek(),
            Some(ch) if ch.is_ascii_digit() || ch == '-' || ch == '+' || ch == '.' || ch == 'e' || ch == 'E'
        ) {
            self.pos += 1;
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        text.parse::<f64>()
            .map(Json::Num)
            .map_err(|_| "invalid number".to_string())
    }

    fn literal(&mut self, text: &str, value: Json) -> Result<Json, String> {
        for expected in text.chars() {
            self.expect(expected)?;
        }
        Ok(value)
    }
}

fn parse_json(text: &str) -> Result<Json, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut reader = JsonReader { chars: &chars, pos: 0 };
    let value = reader.value()?;
    reader.skip_ws();
    if reader.pos != chars.len() {
        return Err("trailing data after JSON value".to_string());
    }
    Ok(value)
}

fn write_json_value(value: &Json, out: &mut String) {
    match value {
        Json::Null => out.push_str("null"),
        Json::Bool(true) => out.push_str("true"),
        Json::Bool(false) => out.push_str("false"),
        Json::Num(number) => {
            if number.fract() == 0.0 && number.abs() < 1e15 {
                out.push_str(&format!("{}", *number as i64));
            } else {
                out.push_str(&number.to_string());
            }
        }
        Json::Str(text) => write_json_string(text, out),
        Json::Arr(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_json_value(item, out);
            }
            out.push(']');
        }
        Json::Obj(entries) => {
            out.push('{');
            for (index, (key, value)) in entries.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_json_string(key, out);
                out.push(':');
                write_json_value(value, out);
            }
            out.push('}');
        }
    }
}

fn to_json(value: &Json) -> String {
    let mut out = String::new();
    write_json_value(value, &mut out);
    out
}


/// Indices into `SEMANTIC_TOKEN_TYPES`. The discriminants are part of the
/// semantic tokens legend, so they must never be reordered.
#[derive(Clone, Copy)]
#[repr(u32)]
enum SemanticToken {
    Namespace = 0,
    Class = 1,
    Enum = 2,
    EnumMember = 3,
    Parameter = 4,
    Variable = 5,
    Property = 6,
    Function = 7,
    Type = 8,
    TypeParameter = 9,
    TypeAlias = 10,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DefKind {
    Function,
    Extension,
    Class,
    Enum,
    EnumMember,
    Variable,
    Parameter,
    Property,
    Namespace,
    TypeParameter,
    TypeAlias,
}

impl DefKind {
    fn symbol_kind(self) -> f64 {
        match self {
            DefKind::Function => 12.0,            DefKind::Extension => 6.0,            DefKind::Class => 5.0,            DefKind::Enum => 10.0,            DefKind::EnumMember => 22.0,            DefKind::Variable => 13.0,            DefKind::Parameter => 13.0,            DefKind::Property => 8.0,            DefKind::Namespace => 2.0,            DefKind::TypeParameter | DefKind::TypeAlias => 26.0,        }
    }

    fn completion_kind(self) -> f64 {
        match self {
            DefKind::Function | DefKind::Extension => 3.0,            DefKind::Class | DefKind::Enum => 7.0,            DefKind::EnumMember => 20.0,            DefKind::Variable | DefKind::Parameter => 6.0,            DefKind::Property => 10.0,            DefKind::Namespace => 9.0,            DefKind::TypeParameter | DefKind::TypeAlias => 25.0,        }
    }

    fn semantic_token(self) -> u32 {
        match self {
            DefKind::Namespace => SemanticToken::Namespace as u32,
            DefKind::Class => SemanticToken::Class as u32,
            DefKind::Enum => SemanticToken::Enum as u32,
            DefKind::EnumMember => SemanticToken::EnumMember as u32,
            DefKind::Parameter => SemanticToken::Parameter as u32,
            DefKind::Variable => SemanticToken::Variable as u32,
            DefKind::Property => SemanticToken::Property as u32,
            DefKind::Function | DefKind::Extension => SemanticToken::Function as u32,
            DefKind::TypeParameter => SemanticToken::TypeParameter as u32,
            DefKind::TypeAlias => SemanticToken::TypeAlias as u32,
        }
    }

    fn outline_visible(self, scope: usize) -> bool {
        !matches!(self, DefKind::Parameter) && !(self == DefKind::Variable && scope != 0)
    }
}

#[derive(Clone)]
struct Def {
    name: String,
    kind: DefKind,
    start: usize,
    end: usize,
    name_start: usize,
    name_end: usize,
    scope: usize,
}

#[derive(Clone)]
struct UseSite {
    name: String,
    start: usize,
    end: usize,
    scope: usize,
    member: bool,
}

/// A `use a.b [as x]` declaration, with the byte spans of every path segment.
/// Kept in addition to `use_aliases` so the LSP can resolve user modules the
/// same way `module.rs::resolve_module` does, without touching the runtime.
#[derive(Clone, Debug)]
struct ModuleUse {
    parts: Vec<String>,
    alias: Option<String>,
    imports: Vec<(String, Option<String>)>,
    wildcard: bool,
    /// Number of leading `.` characters (parser folds `..` into two dots).
    /// `0` is an absolute import; `1` is the importing file's own directory,
    /// `2` its parent, and so on — see `module.rs::relative_base`.
    depth: usize,
    path_start: usize,
    path_end: usize,
    segments: Vec<(usize, usize)>,
}

impl ModuleUse {
    /// Name the module is bound to in the importing scope. `use test.assert`
    /// binds `assert` (the last segment), matching `module::import_map`.
    fn binding(&self) -> Option<&str> {
        self.alias
            .as_deref()
            .or_else(|| self.parts.last().map(String::as_str))
    }
}

struct Analysis {
    source: Source,
    root: Option<Node>,
    tokens: Option<Vec<Token>>,
    defs: Vec<Def>,
    uses: Vec<UseSite>,
    type_spans: Vec<(usize, usize)>,
    namespace_spans: Vec<(usize, usize)>,
    use_aliases: Vec<(String, String)>,
    module_uses: Vec<ModuleUse>,
    var_class_hints: HashMap<String, String>,
    error: Option<LangError>,
}

/// Hand-written descriptions for native modules, builtins and common
/// functions. Anything not listed gets a generated fallback so every native
/// symbol has a description on hover.
mod docs {
    pub fn module(name: &str) -> Option<&'static str> {
        match name {
            "io" => Some("입출력 — 콘솔 출력, 파일 스트림, 읽기/쓰기"),
            "math" => Some("수학 함수와 상수"),
            "fs" => Some("파일 시스템 조회·읽기·쓰기"),
            "os" => Some("운영체제 및 환경 정보"),
            "sys" => Some("런타임/시스템 정보와 디버그 훅"),
            "time" => Some("시간, 대기, 타임스탬프"),
            "random" => Some("난수 생성과 무작위 선택"),
            "iter" => Some("반복자 생성과 변환"),
            "bytes" => Some("바이트 시퀀스 생성·변환·조회"),
            "types" => Some("타입 검사와 캐스팅"),
            "inspect" => Some("객체 속성과 전역 심볼 검사"),
            "flow" => Some("제어 흐름 및 동시성 유틸리티"),
            "atomic" => Some("원자적 카운터/플래그 (협력적 단일 스레드)"),
            "re" => Some("정규식 매칭"),
            "codes" => Some("문자와 코드 값 변환"),
            "http" => Some("HTTP 클라이언트"),
            "ws" => Some("WebSocket 클라이언트"),
            "stream" => Some("스트림 처리"),
            "stopwatch" => Some("경과 시간 측정"),
            "net" => Some("네트워크 유틸리티"),
            _ => None,
        }
    }

    pub fn builtin(name: &str) -> Option<&'static str> {
        match name {
            "len" => Some("값의 길이(요소 수)를 반환합니다."),
            "type" => Some("값의 런타임 타입을 반환합니다."),
            "hash" => Some("값의 해시 값을를 반환합니다."),
            "repr" => Some("값의 표현 문자열을 반환합니다."),
            "exit" => Some("주어진 종료 코드로 프로그램을 종료합니다."),
            "throw" => Some("예외를 던집니다."),
            "throw_if" => Some("조건이 참이면 예외를 던집니다."),
            "throw_unless" => Some("조건이 거짓이면 예외를 던집니다."),
            _ => None,
        }
    }

    pub fn function(module: &str, name: &str) -> Option<&'static str> {
        match (module, name) {
            ("io", "write") => Some("값을 표준 출력이나 지정 스트림에 씁니다."),
            ("io", "read") => Some("표준 입력에서 한 줄을 읽습니다."),
            ("io", "open") => Some("파일 스트림을 엽니다."),
            ("math", "sqrt") => Some("제곱근을 반환합니다."),
            ("math", "pow") => Some("거듭제곱을 계산합니다."),
            ("math", "abs") => Some("절댓값을 반환합니다."),
            ("math", "clamp") => Some("값을 지정 범위로 제한합니다."),
            ("fs", "read") => Some("파일 전체를 문자열로 읽습니다."),
            ("fs", "write") => Some("파일에 문자열을 씁니다."),
            ("fs", "exists") => Some("경로의 존재 여부를 반환합니다."),
            ("random", "int") => Some("범위 안의 난수 정수를 반환합니다."),
            ("random", "choice") => Some("시퀀스에서 무작위 원소를 선택합니다."),
            ("random", "shuffle") => Some("시퀀스를 제자리에서 섞습니다."),
            ("iter", "range") => Some("범위 반복자를 만듭니다."),
            ("iter", "map") => Some("반복자의 각 원소를 변환합니다."),
            ("iter", "filter") => Some("조건에 맞는 원소만 남깁니다."),
            ("time", "sleep") => Some("지정한 시간(초) 동안 대기합니다."),
            ("bytes", "from_hex") => Some("16진수 문자열을 바이트로 변환합니다."),
            ("bytes", "to_hex") => Some("바이트를 16진수 문자열로 변환합니다."),
            ("list", "add") => Some("리스트 끝에 값을 추가합니다."),
            ("str", "split") => Some("구분자를 기준으로 문자열을 나눕니다."),
            ("dict", "get") => Some("키에 해당하는 값을 반환합니다."),
            _ => None,
        }
    }
}

struct NativeSig {
    name: String,
    label: String,
}

struct NativeCatalog {
    modules: HashMap<String, Vec<NativeSig>>,
    module_names: Vec<String>,
    builtins: HashMap<String, String>,
}

fn native_catalog() -> &'static NativeCatalog {
    static NATIVES: OnceLock<NativeCatalog> = OnceLock::new();
    NATIVES.get_or_init(|| {
        crate::native::init("");
        let mut modules = HashMap::new();
        let mut module_names = Vec::new();
        // Discover modules from the registry that `native::init` just populated,
        // so adding a native module never requires editing this file again.
        for module in unvm::runtime::world::native_module_names() {
            let Some(members) = unvm::runtime::world::native_members(&module) else {
                continue;
            };
            let mut sigs = Vec::new();
            for (name, obj) in members {
                let label = native_signature(&obj).unwrap_or_else(|| name.clone());
                sigs.push(NativeSig { name, label });
            }
            if !sigs.is_empty() {
                module_names.push(module.clone());
                modules.insert(module, sigs);
            }
        }
        let mut builtins = HashMap::new();
        for name in ["len", "exit", "type", "hash", "throw", "throw_if", "throw_unless", "repr"] {
            if let Some(obj) = unvm::runtime::world::try_get_global(name) {
                if let Some(label) = native_signature(&obj) {
                    builtins.insert(name.to_string(), label);
                }
            }
        }
        NativeCatalog { modules, module_names, builtins }
    })
}

fn native_signature(obj: &unvm::runtime::obj::Obj) -> Option<String> {
    use unvm::runtime::obj::Payload;
    let data = obj.borrow();
    match &data.payload {
        Payload::Func(cell) => {
            let name = cell.borrow().name.clone().unwrap_or_default();
            let args: Vec<String> = cell
                .borrow()
                .args
                .borrow()
                .iter()
                .map(|arg| {
                    if arg.optional {
                        format!("{}?", arg.name)
                    } else {
                        arg.name.clone()
                    }
                })
                .collect();
            Some(format!("{name}({})", args.join(", ")))
        }
        _ => None,
    }
}

/// Maps native `use` aliases to their module, e.g. `use fs as filesystem`
/// yields ("filesystem", "fs"). `use io` yields ("io", "io").
fn module_of(receiver: &str, aliases: &[(String, String)]) -> Option<String> {
    for (alias, module) in aliases {
        if alias == receiver {
            return Some(module.clone());
        }
    }
    native_catalog().modules.get(receiver).map(|_| receiver.to_string())
}

/// Counts the leading `.` of a `use` path. The parser keeps identifier spans
/// unchanged for relative imports (the dots are folded into a `Const::Str`
/// value), so they have to be read back from the source text. Returns the
/// depth and the start of the dot run, so the whole path can be highlighted.
fn use_path_depth(source: &Source, first_segment: usize) -> (usize, usize) {
    let code = &source.code;
    let mut cursor = first_segment.min(code.len());
    let mut run_start = cursor;
    let mut depth = 0;
    loop {
        let mut probe = cursor;
        while probe > 0 && code[probe - 1].is_whitespace() {
            probe -= 1;
        }
        if probe == 0 || code[probe - 1] != '.' {
            break;
        }
        depth += 1;
        cursor = probe - 1;
        run_start = cursor;
    }
    (depth, run_start)
}

/// Collects `a` / `b` / `c` (with their spans) from a `use a.b.c` path node.
/// Leading dots are *not* part of the AST, so `module_use_from_node` records
/// their count separately via `use_path_depth`.
fn collect_use_path(
    node: &Node,
    source: &Source,
    parts: &mut Vec<String>,
    segments: &mut Vec<(usize, usize)>,
) {
    match node.kind {
        NodeKind::Identifier => {
            parts.push(source.span(node.start, node.length));
            segments.push((node.start, node.start + node.length));
        }
        NodeKind::Property => {
            for child in &node.children {
                collect_use_path(child, source, parts, segments);
            }
        }
        NodeKind::Unary => {
            // `use a.b.*` stores the wildcard as a Unary around the path.
            if let Some(child) = node.children.first() {
                collect_use_path(child, source, parts, segments);
            }
        }
        _ => {}
    }
}

/// Extracts a full `use` declaration from the AST, including every path
/// segment so go-to-definition can navigate `use a.b` piece by piece.
fn module_use_from_node(node: &Node, source: &Source) -> Option<ModuleUse> {
    if node.kind != NodeKind::Use {
        return None;
    }
    let path = node.children.first()?;
    let mut parts = Vec::new();
    let mut segments = Vec::new();
    collect_use_path(path, source, &mut parts, &mut segments);
    if parts.is_empty() || segments.is_empty() {
        return None;
    }
    let (depth, dots_start) = use_path_depth(source, segments[0].0);
    let path_start = dots_start;
    let path_end = segments[segments.len() - 1].1;
    let mut alias = None;
    let mut imports = Vec::new();
    let mut wildcard = path.kind == NodeKind::Unary;
    for child in node.children.iter().skip(1) {
        match child.kind {
            NodeKind::Tuple => {
                for entry in &child.children {
                    match entry.kind {
                        NodeKind::Unary => wildcard = true,
                        NodeKind::Pair => {
                            let name = entry
                                .children
                                .first()
                                .map(|node| source.span(node.start, node.length))
                                .unwrap_or_default();
                            let rename = entry
                                .children
                                .get(1)
                                .map(|node| source.span(node.start, node.length));
                            imports.push((name, rename));
                        }
                        NodeKind::Identifier => {
                            imports.push((source.span(entry.start, entry.length), None));
                        }
                        _ => {}
                    }
                }
            }
            NodeKind::Unary => wildcard = true,
            NodeKind::Identifier => alias = Some(source.span(child.start, child.length)),
            _ => {}
        }
    }
    Some(ModuleUse {
        parts,
        alias,
        imports,
        wildcard,
        depth,
        path_start,
        path_end,
        segments,
    })
}

struct Walker<'a> {
    source: &'a Source,
    defs: Vec<Def>,
    uses: Vec<UseSite>,
    type_spans: Vec<(usize, usize)>,
    namespace_spans: Vec<(usize, usize)>,
    use_aliases: Vec<(String, String)>,
    module_uses: Vec<ModuleUse>,
    var_class_hints: HashMap<String, String>,
    skip: HashSet<(usize, usize)>,
    next_scope: usize,
}

impl<'a> Walker<'a> {
    fn new(source: &'a Source) -> Self {
        Walker {
            source,
            defs: Vec::new(),
            uses: Vec::new(),
            type_spans: Vec::new(),
            namespace_spans: Vec::new(),
            use_aliases: Vec::new(),
            module_uses: Vec::new(),
            var_class_hints: HashMap::new(),
            skip: HashSet::new(),
            next_scope: 0,
        }
    }

    fn new_scope(&mut self) -> usize {
        self.next_scope += 1;
        self.next_scope
    }

    fn span_of(&self, node: &Node) -> (usize, usize) {
        (node.start, node.start + node.length)
    }

    fn push_def(&mut self, name_node: &Node, kind: DefKind, scope: usize, start: usize, end: usize) {
        let name = self.source.span(name_node.start, name_node.length);
        let (name_start, name_end) = self.span_of(name_node);
        self.defs.push(Def {
            name,
            kind,
            start,
            end,
            name_start,
            name_end,
            scope,
        });
        self.skip.insert((name_start, name_end));
    }

    /// Registers the `[T, U: bound]` list attached to a function, class or enum
    /// declaration. Type parameters are definitions, so they receive the `type`
    /// semantic token (and go-to-definition) instead of being mistaken for
    /// ordinary variable uses. Returns true when a list was present.
    fn walk_type_params(&mut self, node: &Node, scope: usize) -> bool {
        let Some(params) = node
            .children
            .iter()
            .find(|child| child.kind == NodeKind::TypeParams)
        else {
            return false;
        };
        for param in &params.children {
            let (start, end) = self.span_of(param);
            self.push_def(param, DefKind::TypeParameter, scope, start, end);
            // A bound (`T: num`) is stored as a `Const::Str` value rather than a
            // child node, so find its text in the source to colour it as a type.
            if param.value.is_some() {
                let code = &self.source.code;
                let mut cursor = end;
                while cursor < code.len() && code[cursor].is_whitespace() {
                    cursor += 1;
                }
                if cursor < code.len() && code[cursor] == ':' {
                    cursor += 1;
                    while cursor < code.len() && code[cursor].is_whitespace() {
                        cursor += 1;
                    }
                    let bound_start = cursor;
                    // A bound may be a union (`T: int | float`). Scan the whole
                    // expression, then colour each alternative separately so
                    // `float` is not left unstyled.
                    while cursor < code.len()
                        && (code[cursor].is_alphanumeric()
                            || code[cursor] == '_'
                            || code[cursor].is_whitespace()
                            || code[cursor] == '|')
                    {
                        cursor += 1;
                    }
                    let bound = code[bound_start..cursor].iter().collect::<String>();
                    let mut offset = 0usize;
                    for atom in bound.split('|') {
                        let leading = atom.chars().take_while(|ch| ch.is_whitespace()).count();
                        let text = atom.trim();
                        if !text.is_empty() {
                            let start = bound_start + offset + leading;
                            let end = start + text.chars().count();
                            self.type_spans.push((start, end));
                            self.skip.insert((start, end));
                        }
                        offset += atom.chars().count() + 1;
                    }
                }
            }
        }
        true
    }

    fn walk_type(&mut self, node: &Node, scope: usize) {
        match node.kind {
            NodeKind::Identifier => {
                let (start, end) = self.span_of(node);
                self.type_spans.push((start, end));
                self.skip.insert((start, end));
                // Keep the name so references/rename can find type uses (e.g.
                // `x: Id`, bounds, generic args) in addition to declarations.
                let name = self.source.span(node.start, node.length).to_string();
                self.uses.push(UseSite { name, start, end, scope, member: false });
            }
            NodeKind::Property => {
                for child in &node.children {
                    self.walk_type(child, scope);
                }
            }
            NodeKind::Tuple | NodeKind::List | NodeKind::Set => {
                for child in &node.children {
                    self.walk_type(child, scope);
                }
            }
            NodeKind::Typed | NodeKind::Assign => {
                for child in &node.children {
                    self.walk_type(child, scope);
                }
            }
            NodeKind::Unary => {
                for child in &node.children {
                    self.walk_type(child, scope);
                }
            }
            _ => {}
        }
    }

    fn walk_namespace_path(&mut self, node: &Node, scope: usize, register_root: bool) {
        match node.kind {
            NodeKind::Identifier => {
                let (start, end) = self.span_of(node);
                self.namespace_spans.push((start, end));
                self.skip.insert((start, end));
                if register_root {
                    self.push_def(node, DefKind::Namespace, scope, start, end);
                }
            }
            NodeKind::Property => {
                for (index, child) in node.children.iter().enumerate() {
                    self.walk_namespace_path(child, scope, register_root && index == 0);
                }
            }
            NodeKind::Unary => {
            }
            _ => {}
        }
    }

    /// Records the callee name used in an assignment (e.g. `x = Point(a, b)`
    /// yields `x -> Point`) and the annotation when present, so `.` member
    /// completions can resolve the receiver's type.
    fn record_hint(&mut self, target: &Node, rhs: &Node) {
        let Some(name_node) = target_name_node(target) else {
            return;
        };
        let name = self.source.span(name_node.start, name_node.length).to_string();
        if let Some(annotation) = annotation_name(target, self.source) {
            self.var_class_hints.insert(name, annotation);
            return;
        }
        if let Some(builtin) = builtin_type_of_rhs(self.source, rhs) {
            self.var_class_hints.insert(name, builtin);
            return;
        }
        let callee = match rhs.kind {
            NodeKind::Call => rhs.children.first(),
            _ => None,
        };
        if let Some(text) = callee.map(|callee| self.callee_name(callee)) {
            if !text.is_empty() {
                self.var_class_hints.insert(name, text);
            }
        }
    }

    fn callee_name(&self, node: &Node) -> String {
        match node.kind {
            NodeKind::Identifier | NodeKind::Property => {
                self.source.span(node.start, node.length).to_string()
            }
            _ => String::new(),
        }
    }

    fn collect_target(&mut self, node: &Node, scope: usize, kind: DefKind) {
        match node.kind {
            NodeKind::Identifier => {
                let (start, end) = self.span_of(node);
                self.push_def(node, kind, scope, start, end);
            }
            NodeKind::Parameter | NodeKind::Unary | NodeKind::Spread => {
                if let Some(first) = node.children.first() {
                    self.collect_target(first, scope, kind);
                }
            }
            NodeKind::Typed => {
                if let Some(first) = node.children.first() {
                    self.collect_target(first, scope, kind);
                }
                if let Some(type_node) = node.children.get(1) {
                    self.walk_type(type_node, scope);
                }
                for child in node.children.iter().skip(2) {
                    self.walk(child, scope);
                }
            }
            NodeKind::Assign => {
                if let Some(first) = node.children.first() {
                    self.collect_target(first, scope, kind);
                }
                for child in node.children.iter().skip(1) {
                    self.walk(child, scope);
                }
            }
            NodeKind::Tuple | NodeKind::List | NodeKind::Set => {
                for child in &node.children {
                    self.collect_target(child, scope, kind);
                }
            }
            _ => self.walk(node, scope),
        }
    }

    /// Walk a match/for pattern. Pattern variables are definitions local to the
    /// arm, constructor arguments bind new names, and narrowing (`x: T` /
    /// `x is T`) colours the type. Without this, pattern bindings either leak
    /// into the enclosing scope as ordinary variables or are recorded only as
    /// name uses, so goto-definition and references cannot find them.
    fn walk_pattern(&mut self, node: &Node, scope: usize) {
        match node.kind {
            NodeKind::Wildcard => {}
            NodeKind::OrPattern => {
                for child in &node.children {
                    self.walk_pattern(child, scope);
                }
            }
            NodeKind::Guard => {
                if let Some(pattern) = node.children.first() {
                    self.walk_pattern(pattern, scope);
                }
                for condition in node.children.iter().skip(1) {
                    self.walk(condition, scope);
                }
            }
            NodeKind::Call => {
                // Constructor pattern (`Enum.Variant(a, b)`): the callee is a
                // value use, the arguments are new pattern bindings.
                if let Some(callee) = node.children.first() {
                    self.walk(callee, scope);
                }
                for arg in node.children.iter().skip(1) {
                    self.walk_pattern(arg, scope);
                }
            }
            NodeKind::Pair => {
                // Named field `name: pattern`; only the value binds a name.
                if let Some(value) = node.children.get(1) {
                    self.walk_pattern(value, scope);
                }
            }
            NodeKind::Typed => {
                if let Some(target) = node.children.first() {
                    self.walk_pattern(target, scope);
                }
                if let Some(type_node) = node.children.get(1) {
                    self.walk_type(type_node, scope);
                }
                for child in node.children.iter().skip(2) {
                    self.walk(child, scope);
                }
            }
            NodeKind::Tuple | NodeKind::List | NodeKind::Set => {
                for child in &node.children {
                    self.walk_pattern(child, scope);
                }
            }
            _ => self.collect_target(node, scope, DefKind::Variable),
        }
    }

    /// A match arm gets its own scope so its bindings do not leak into the
    /// enclosing function or collide with other arms.
    fn walk_case(&mut self, node: &Node) {
        let arm_scope = self.new_scope();
        if let Some(pattern) = node.children.first() {
            self.walk_pattern(pattern, arm_scope);
        }
        for child in node.children.iter().skip(1) {
            self.walk(child, arm_scope);
        }
    }

    fn walk(&mut self, node: &Node, scope: usize) {
        match node.kind {
            NodeKind::Function | NodeKind::Extension | NodeKind::Lambda => {
                let is_lambda = node.kind == NodeKind::Lambda;
                let new_scope = self.new_scope();
                let has_type_params = self.walk_type_params(node, new_scope);
                let mut name_index: Option<usize> = None;
                if !is_lambda {
                    let index = if node.kind == NodeKind::Extension { 1 } else { 0 };
                    name_index = Some(index);
                    if let Some(name) = node.children.get(index) {
                        if name.kind == NodeKind::Identifier {
                            let kind = if node.kind == NodeKind::Extension {
                                DefKind::Extension
                            } else {
                                DefKind::Function
                            };
                            let (start, end) = self.span_of(node);
                            self.push_def(name, kind, scope, start, end);
                        }
                    }
                }
                let params_index = if is_lambda {
                    Some(0)
                } else if node.kind == NodeKind::Extension {
                    Some(2)
                } else {
                    Some(1)
                };
                if let Some(index) = params_index {
                    if let Some(params) = node.children.get(index) {
                        for param in &params.children {
                            self.collect_target(param, new_scope, DefKind::Parameter);
                        }
                    }
                }
                let ret_index = if is_lambda {
                    None                } else if node.kind == NodeKind::Extension {
                    Some(4)
                } else {
                    Some(3)
                };
                for (index, child) in node.children.iter().enumerate() {
                    if name_index == Some(index) {
                        continue;
                    }
                    if ret_index == Some(index) {
                        self.walk_type(child, new_scope);
                        continue;
                    }
                    if has_type_params && child.kind == NodeKind::TypeParams {
                        continue;
                    }
                    self.walk(child, new_scope);
                }
            }
            NodeKind::TypeAlias => {
                // `type Name = Ty`: bind the name as a type definition and
                // colour the aliased type expression, so go-to-definition,
                // hover and completion work for aliases just like classes.
                if let Some(name) = node.children.first() {
                    if name.kind == NodeKind::Identifier {
                        let (start, end) = self.span_of(node);
                        self.push_def(name, DefKind::TypeAlias, scope, start, end);
                    }
                }
                if let Some(typ) = node.children.get(1) {
                    self.walk_type(typ, scope);
                }
            }
            NodeKind::Class => {
                if let Some(name) = node.children.first() {
                    if name.kind == NodeKind::Identifier {
                        let (start, end) = self.span_of(node);
                        self.push_def(name, DefKind::Class, scope, start, end);
                    }
                }
                let has_type_params = self.walk_type_params(node, scope);
                for child in node.children.iter().skip(1) {
                    if has_type_params && child.kind == NodeKind::TypeParams {
                        continue;
                    }
                    match child.kind {
                        NodeKind::Tuple => {
                            for param in &child.children {
                                self.collect_target(param, scope, DefKind::Parameter);
                            }
                        }
                        NodeKind::ClassBases => {
                            for base in &child.children {
                                self.walk_type(base, scope);
                            }
                        }
                        NodeKind::Block => {
                            for statement in &child.children {
                                match statement.kind {
                                    NodeKind::Typed | NodeKind::Assign => {
                                        self.collect_target(statement, scope, DefKind::Property);
                                    }
                                    NodeKind::Accessor => {
                                        if let Some(first) = statement.children.first() {
                                            if first.kind == NodeKind::Identifier {
                                                let (start, end) = self.span_of(first);
                                                self.push_def(first, DefKind::Property, scope, start, end);
                                            }
                                        }
                                        for rest in statement.children.iter().skip(1) {
                                            self.walk(rest, scope);
                                        }
                                    }
                                    _ => self.walk(statement, scope),
                                }
                            }
                        }
                        _ => self.walk(child, scope),
                    }
                }
            }
            NodeKind::Enum => {
                if let Some(name) = node.children.first() {
                    if name.kind == NodeKind::Identifier {
                        let (start, end) = self.span_of(node);
                        self.push_def(name, DefKind::Enum, scope, start, end);
                    }
                }
                let has_type_params = self.walk_type_params(node, scope);
                for member in node.children.iter().skip(1) {
                    if has_type_params && member.kind == NodeKind::TypeParams {
                        continue;
                    }
                    match member.kind {
                        NodeKind::Identifier => {
                            let (start, end) = self.span_of(member);
                            self.push_def(member, DefKind::EnumMember, scope, start, end);
                        }
                        NodeKind::Assign => {
                            if let Some(target) = member.children.first() {
                                if target.kind == NodeKind::Identifier {
                                    let (start, end) = self.span_of(target);
                                    self.push_def(target, DefKind::EnumMember, scope, start, end);
                                }
                            }
                            for rest in member.children.iter().skip(1) {
                                self.walk(rest, scope);
                            }
                        }
                        NodeKind::Typed => {
                            if let Some(target) = member.children.first() {
                                if target.kind == NodeKind::Identifier {
                                    let (start, end) = self.span_of(target);
                                    self.push_def(target, DefKind::EnumMember, scope, start, end);
                                }
                            }
                            if let Some(type_node) = member.children.get(1) {
                                self.walk_type(type_node, scope);
                            }
                        }
                        _ => self.walk(member, scope),
                    }
                }
            }
            NodeKind::Call => {
                // Explicit type arguments (`id[int](x)`) are stored as a
                // trailing TypeParams child. Mark them as type tokens instead
                // of letting them fall through as ordinary variable uses.
                if let Some(params) = node.children.last() {
                    if params.kind == NodeKind::TypeParams {
                        for arg in &params.children {
                            let (start, end) = self.span_of(arg);
                            self.type_spans.push((start, end));
                            self.skip.insert((start, end));
                        }
                    }
                }
                for child in &node.children {
                    self.walk(child, scope);
                }
            }
            NodeKind::Assign => {
                if let Some(target) = node.children.first() {
                    if let Some(rhs) = node.children.get(1) {
                        self.record_hint(target, rhs);
                    }
                    self.collect_target(target, scope, DefKind::Variable);
                }
                for child in node.children.iter().skip(1) {
                    self.walk(child, scope);
                }
            }
            NodeKind::Match => {
                // The first child is the scrutinee; the rest are a Block of
                // Case arms, each handled by the `Case` arm below.
                for child in &node.children {
                    self.walk(child, scope);
                }
            }
            NodeKind::Case => self.walk_case(node),
            NodeKind::Typed => {
                if let Some(target) = node.children.first() {
                    self.collect_target(target, scope, DefKind::Variable);
                }
                if let Some(type_node) = node.children.get(1) {
                    self.walk_type(type_node, scope);
                }
                for child in node.children.iter().skip(2) {
                    self.walk(child, scope);
                }
            }
            NodeKind::For => {
                if let Some(target) = node.children.first() {
                    self.walk_pattern(target, scope);
                }
                for child in node.children.iter().skip(1) {
                    self.walk(child, scope);
                }
            }
            NodeKind::Catch => {
                if let Some(first) = node.children.first() {
                    if first.kind == NodeKind::Identifier {
                        let (start, end) = self.span_of(first);
                        self.push_def(first, DefKind::Variable, scope, start, end);
                    }
                }
                for child in node.children.iter().skip(1) {
                    self.walk(child, scope);
                }
            }
            NodeKind::Use => {
                if let Some(usage) = module_use_from_node(node, self.source) {
                    self.module_uses.push(usage);
                }
                if let Some(path) = node.children.first() {
                    self.walk_namespace_path(path, scope, true);
                    let module_text = self.source.span(path.start, path.length);
                    let root = module_text.split('.').next().unwrap_or("").to_string();
                    let alias = match node.children.get(1) {
                        Some(child) if child.kind == NodeKind::Identifier => {
                            self.source.span(child.start, child.length).to_string()
                        }
                        _ => root.clone(),
                    };
                    self.use_aliases.push((alias, root));
                }
                if let Some(alias) = node.children.last() {
                    if node.children.len() > 1 && alias.kind == NodeKind::Identifier {
                        let (start, end) = self.span_of(alias);
                        self.push_def(alias, DefKind::Namespace, scope, start, end);
                    }
                }
            }
            NodeKind::Identifier => {
                let key = self.span_of(node);
                if self.skip.contains(&key) {
                    return;
                }
                let name = self.source.span(node.start, node.length);
                if name == "self" || name == "super" {
                    return;
                }
                let mut member = false;
                let mut cursor = node.start;
                while cursor > 0 {
                    cursor -= 1;
                    let previous = self.source.code[cursor];
                    if previous.is_whitespace() {
                        continue;
                    }
                    member = previous == '.';
                    break;
                }
                self.uses.push(UseSite {
                    name,
                    start: node.start,
                    end: node.start + node.length,
                    scope,
                    member,
                });
            }
            _ => {
                for child in &node.children {
                    self.walk(child, scope);
                }
            }
        }
    }
}

/// Returns the Identifier node bound by an assignment target.
fn target_name_node(node: &Node) -> Option<&Node> {
    match node.kind {
        NodeKind::Identifier => Some(node),
        NodeKind::Parameter | NodeKind::Unary | NodeKind::Spread => {
            node.children.first().and_then(target_name_node)
        }
        NodeKind::Typed | NodeKind::Assign => node.children.first().and_then(target_name_node),
        _ => None,
    }
}

/// Returns the annotation for `target: list[int]` targets — reduced to the
/// base type name (`list`), or `None` when the annotation isn't a simple
/// base type the member-completion path knows about.
fn annotation_name(node: &Node, source: &Source) -> Option<String> {
    if node.kind != NodeKind::Typed {
        return None;
    }
    let annotation = node.children.get(1)?;
    let text = source.span(annotation.start, annotation.length).trim().to_string();
    // Strip generic brackets and union marker: `list[int]` -> `list`, `int|str` -> `int`.
    let head = text
        .split(['[', '|', '('])
        .next()
        .unwrap_or(&text)
        .trim()
        .to_string();
    // Only simple lower-case base names are member-completion types; UDF class
    // names (capitalized) already resolve via `defs`. Both former branches
    // reduced to the same builtin-type check.
    if is_builtin_type(&head) {
        Some(head)
    } else {
        None
    }
}

/// Infer the type of `rhs` from its textual shape: collection literals, constructor
/// calls, interesting builtins.
fn builtin_type_of_rhs(source: &Source, rhs: &Node) -> Option<String> {
    let text = source.span(rhs.start, rhs.length);
    let trimmed = text.trim_start();
    let head: String = trimmed
        .split(['[', '|', '(', ' '])
        .next()
        .unwrap_or("")
        .to_string();
    if textual_infer::is_bytes_literal(trimmed) {
        return Some("bytes".to_string());
    }
    // Collection literal prefixes
    if trimmed.starts_with('[') {
        return Some("list".to_string());
    }
    if trimmed.starts_with('{') {
        return Some("dict".to_string());
    }
    if trimmed.starts_with('"') || trimmed.starts_with('\'') || trimmed.starts_with('`') {
        return Some("str".to_string());
    }
    match head.as_str() {
        h if is_builtin_type(h) => Some(head),
        h if h.starts_with("iter.") || h.starts_with("iter") => Some("iter".to_string()),
        _ => None,
    }
}

/// Thin wrapper over the shared textual-inference builtin type set.
fn is_builtin_type(name: &str) -> bool {
    textual_infer::is_builtin_type(name)
}

/// Declaration lines recognised by the fallback text scanner.
///
/// Every entry binds a name; `class`/`enum` additionally open an indented
/// scope, and `skip_invalid` preserves where the original if-chain skipped the
/// rest of a malformed line (a malformed `get`/`set` still reaches the
/// assignment-hint scan below).
struct DeclPrefix {
    keyword: &'static str,
    kind: DefKind,
    opens_scope: bool,
    type_params: bool,
    skip_invalid: bool,
}

const DECL_PREFIXES: &[DeclPrefix] = &[
    DeclPrefix { keyword: "class ", kind: DefKind::Class, opens_scope: true, type_params: true, skip_invalid: true },
    DeclPrefix { keyword: "enum ", kind: DefKind::Enum, opens_scope: true, type_params: true, skip_invalid: true },
    DeclPrefix { keyword: "fn ", kind: DefKind::Function, opens_scope: false, type_params: true, skip_invalid: true },
    DeclPrefix { keyword: "get ", kind: DefKind::Property, opens_scope: false, type_params: false, skip_invalid: false },
    DeclPrefix { keyword: "set ", kind: DefKind::Property, opens_scope: false, type_params: false, skip_invalid: false },
    // `type <ident> = ...` is a contextual keyword. `type(x)` has no space and
    // `type = 5` has no identifier after the space, so neither reaches here;
    // keeping skip_invalid false lets the latter fall through to the assignment
    // hint scan below.
    DeclPrefix { keyword: "type ", kind: DefKind::TypeAlias, opens_scope: false, type_params: false, skip_invalid: false },
];

/// Text-scan a document when parsing fails (e.g. while the user is typing).
/// Produces enough `defs`/namespace data to power completion, references and
/// document symbols — no parser involved, so it works on broken documents.
fn fallback_analysis(text: &str, name: &str, error: Option<LangError>) -> Analysis {
    let code: Vec<char> = text.chars().collect();
    let line_starts: Vec<usize> = {
        let mut starts = vec![0usize];
        for (i, ch) in code.iter().enumerate() {
            if *ch == '\n' {
                starts.push(i + 1);
            }
        }
        starts
    };

    struct Frame {
        indent: i64,
        def_index: usize,
    }

    let mut defs: Vec<Def> = Vec::new();
    let mut type_spans: Vec<(usize, usize)> = Vec::new();
    let mut namespace_spans: Vec<(usize, usize)> = Vec::new();
    let mut use_aliases: Vec<(String, String)> = Vec::new();
    let mut module_uses: Vec<ModuleUse> = Vec::new();
    let mut var_class_hints: HashMap<String, String> = HashMap::new();
    let mut stack: Vec<Frame> = Vec::new();
    let mut scope: usize = 0;

    let push_def = |defs: &mut Vec<Def>,
                    kind: DefKind,
                    name: &str,
                    start: usize,
                    scope: usize| {
        let start = start.min(code.len());
        let end = start.saturating_add(name.chars().count()).min(code.len());
        defs.push(Def {
            name: name.to_string(),
            kind,
            start,
            end,
            name_start: start,
            name_end: end,
            scope,
        });
        defs.len().saturating_sub(1)
    };

    let mut i = 0usize;
    while i < line_starts.len() {
        let line_start = line_starts[i];
        let line_end = line_starts
            .get(i + 1)
            .copied()
            .unwrap_or(code.len());
        let raw: String = code[line_start..line_end].iter().collect();
        let trimmed = raw.trim();
        let indent_chars = raw.chars().take_while(|ch| *ch == ' ' || *ch == '\t').count();
        let indent = indent_chars as i64;

        if trimmed.is_empty() || trimmed.starts_with('#') {
            i += 1;
            continue;
        }

        while let Some(top) = stack.last() {
            if indent <= top.indent {
                if let Some(def) = defs.get_mut(top.def_index) {
                    def.end = line_start;
                }
                stack.pop();
            } else {
                break;
            }
        }

        fn word(text: &str) -> &str {
            let end = text
                .char_indices()
                .find(|(_, ch)| ch.is_whitespace() || *ch == '(' || *ch == '[')
                .map(|(i, _)| i)
                .unwrap_or(text.len());
            &text[..end]
        }

        // Colour `[T, U: bound]` in a declaration line even when the document
        // is too incomplete to parse, so generic highlighting survives typing.
        let mut push_decl_type_params = |rest: &str,
                                         line_start: usize,
                                         indent_chars: usize,
                                         prefix_len: usize,
                                         name_len: usize| {
            let rest = rest.trim();
            let Some(open_rel) = rest.get(name_len..).and_then(|tail| tail.find('[')) else {
                return;
            };
            let open = line_start + indent_chars + prefix_len + name_len + open_rel;
            let close = rest[open_rel..]
                .find(']')
                .map(|rel| open + rel)
                .unwrap_or(code.len());
            let mut cursor = (open + 1).min(code.len());
            while cursor < close && cursor < code.len() {
                while cursor < close && (code[cursor].is_whitespace() || code[cursor] == ',') {
                    cursor += 1;
                }
                if cursor >= close || !code[cursor].is_alphabetic() {
                    break;
                }
                let start = cursor;
                while cursor < close && (code[cursor].is_alphanumeric() || code[cursor] == '_') {
                    cursor += 1;
                }
                type_spans.push((start, cursor));
                let mut bound = cursor;
                while bound < close && code[bound].is_whitespace() {
                    bound += 1;
                }
                if bound < close && code[bound] == ':' {
                    bound += 1;
                    while bound < close && code[bound].is_whitespace() {
                        bound += 1;
                    }
                    let bound_start = bound;
                    // Union bounds (`T: int | float`) need every alternative
                    // coloured, not just the first identifier.
                    while bound < close
                        && (code[bound].is_alphanumeric()
                            || code[bound] == '_'
                            || code[bound].is_whitespace()
                            || code[bound] == '|')
                    {
                        bound += 1;
                    }
                    let bound_text = code[bound_start..bound].iter().collect::<String>();
                    let mut offset = 0usize;
                    for atom in bound_text.split('|') {
                        let leading = atom.chars().take_while(|ch| ch.is_whitespace()).count();
                        let text = atom.trim();
                        if !text.is_empty() {
                            let start = bound_start + offset + leading;
                            type_spans.push((start, start + text.chars().count()));
                        }
                        offset += atom.chars().count() + 1;
                    }
                    cursor = bound;
                }
            }
        };
        let is_ident = |text: &str| -> bool {
            !text.is_empty()
                && text
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        };

        if let Some(rest) = trimmed.strip_prefix("use ") {
            let rest = rest.trim();
            let path = word(rest);
            if !path.is_empty() {
                // The fallback text scanner sees the dots (the AST does not).
                // Strip them from `parts`/`segments` just like parse_use_path
                // and keep their count for the module resolver.
                let depth = path.chars().take_while(|ch| *ch == '.').count();
                let path_start = line_start + indent_chars + "use ".len();
                let path_end = path_start + path.chars().count();
                namespace_spans.push((path_start, path_end));
                let mut segments = Vec::new();
                let mut cursor = path_start + depth;
                let mut parts = Vec::new();
                for part in path.split('.') {
                    if part.is_empty() || part == "*" {
                        continue;
                    }
                    let width = part.chars().count();
                    segments.push((cursor, cursor + width));
                    parts.push(part.to_string());
                    cursor += width + 1;
                }
                let root = parts.first().cloned().unwrap_or_default();
                let alias = rest.split_once(" as ").map(|(_, alias)| alias.trim().to_string());
                if let Some(alias) = &alias {
                    let alias_start =
                        raw.find(" as ").map(|offset| line_start + offset + 4).unwrap_or(line_start);
                    namespace_spans.push((alias_start, alias_start + alias.chars().count()));
                    use_aliases.push((alias.clone(), root.clone()));
                } else {
                    use_aliases.push((root.clone(), root.clone()));
                }
                module_uses.push(ModuleUse {
                    parts,
                    alias,
                    imports: Vec::new(),
                    wildcard: path.contains(".*"),
                    depth,
                    path_start,
                    path_end,
                    segments,
                });
            }
            i += 1;
            continue;
        }
        let decl = DECL_PREFIXES
            .iter()
            .find_map(|decl| trimmed.strip_prefix(decl.keyword).map(|rest| (decl, rest)));
        if let Some((decl, rest)) = decl {
            let name = word(rest.trim());
            if is_ident(name) {
                let start = line_start + indent_chars + decl.keyword.len();
                let def_index = push_def(&mut defs, decl.kind, name, start, scope);
                if decl.type_params {
                    push_decl_type_params(rest, line_start, indent_chars, decl.keyword.len(), name.len());
                }
                if decl.opens_scope {
                    scope += 1;
                    stack.push(Frame { indent, def_index });
                }
                i += 1;
                continue;
            }
            if decl.skip_invalid {
                i += 1;
                continue;
            }
        }
        // Assignment hints: `ident = ClassName(` / `ident = "..."` / `ident = [...]`, etc.
        if let Some(eq) = trimmed.find('=') {
            let before = trimmed[..eq].trim();
            let after = trimmed[eq + 1..].trim();
            if before.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_') && !before.contains('.') {
                // Literal shapes: string / list / dict
                if after.starts_with('"') || after.starts_with('\'') || after.starts_with('`') {
                    var_class_hints.insert(before.to_string(), "str".to_string());
                } else if after.starts_with('[') {
                    var_class_hints.insert(before.to_string(), "list".to_string());
                } else if after.starts_with('{') {
                    var_class_hints.insert(before.to_string(), "dict".to_string());
                } else if after.starts_with("list(") || after.starts_with("set(") || after.starts_with("dict(") || after.starts_with("iter(") || after.starts_with("iter.") || after.starts_with("date(") || after.starts_with("stopwatch(") {
                    if let Some(open) = after.find('(') {
                        let callee = &after[..open];
                        let name = if callee.starts_with("iter") {
                            "iter".to_string()
                        } else {
                            callee.rsplit('.').next().unwrap_or(callee).to_string()
                        };
                        var_class_hints.insert(before.to_string(), name);
                    }
                } else if let Some(open) = after.find('(') {
                    let callee = &after[..open];
                    if callee.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '.') {
                        if let Some(name) = callee.rsplit('.').next() {
                            var_class_hints.insert(before.to_string(), name.to_string());
                        }
                    }
                }
            }
        }
        // Type-annotation hints: `ident: list[int]` / `str` / etc.
        if let Some(colon) = trimmed.find(':') {
            let before = trimmed[..colon].trim();
            if !before.contains('.')
                && before.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
                && before.chars().next().map(|c| !c.is_ascii_digit()).unwrap_or(false)
            {
                // Strip generics and unions: `list[int]` -> list, `int|str` -> int
                let after = trimmed[colon + 1..].trim();
                let head = after
                    .split(['[', '|', ' '])
                    .next()
                    .unwrap_or(after)
                    .trim()
                    .to_string();
                if is_builtin_type(&head) {
                    var_class_hints.insert(before.to_string(), head);
                }
                if stack.last().map(|f| indent > f.indent).unwrap_or(false) {
                    let start = line_start + indent_chars + trimmed[..colon].find(before).unwrap_or(0);
                    push_def(&mut defs, DefKind::Property, before, start, scope);
                }
            }
        }
        i += 1;
    }
    while let Some(top) = stack.last() {
        if let Some(def) = defs.get_mut(top.def_index) {
            def.end = code.len();
        }
        stack.pop();
    }

    Analysis {
        source: Source::new(name, text),
        root: None,
        tokens: None,
        defs,
        uses: Vec::new(),
        type_spans,
        namespace_spans,
        use_aliases,
        module_uses,
        var_class_hints,
        error,
    }
}

/// Analyzes a single Un document (lex + parse + symbol collection, no VM).
fn analyze(text: &str, name: &str) -> Analysis {
    let mut source = Source::new(name, text);
    let tokens = match lex(&mut source) {
        Ok(tokens) => tokens,
        Err(error) => {
            return fallback_analysis(text, name, Some(error));
        }
    };
    let shared = Rc::new(source.clone());
    match parse(tokens.clone(), shared) {
        Ok(root) => {
            let mut walker = Walker::new(&source);
            walker.walk(&root, 0);
            let defs = walker.defs;
            let uses = walker.uses;
            let type_spans = walker.type_spans;
            let namespace_spans = walker.namespace_spans;
            let use_aliases = walker.use_aliases;
            let module_uses = walker.module_uses;
            let var_class_hints = walker.var_class_hints;
            Analysis {
                source,
                root: Some(root),
                tokens: Some(tokens),
                defs,
                uses,
                type_spans,
                namespace_spans,
                use_aliases,
                module_uses,
                var_class_hints,
                error: None,
            }
        }
        Err(error) => {
            fallback_analysis(text, name, Some(error))
        }
    }
}

fn reference_sites(analysis: &Analysis, name: &str, scope: usize) -> Vec<(usize, usize)> {
    let has_local = analysis
        .defs
        .iter()
        .any(|def| def.scope == scope && def.name == name);
    let mut sites: Vec<(usize, usize)> = Vec::new();
    if has_local {
        sites.extend(
            analysis
                .defs
                .iter()
                .filter(|def| def.scope == scope && def.name == name)
                .map(|def| (def.name_start, def.name_end)),
        );
        sites.extend(
            analysis
                .uses
                .iter()
                .filter(|site| site.scope == scope && site.name == name)
                .map(|site| (site.start, site.end)),
        );
        return sites;
    }
    sites.extend(
        analysis
            .defs
            .iter()
            .filter(|def| def.scope == 0 && def.name == name)
            .map(|def| (def.name_start, def.name_end)),
    );
    let local_scopes: HashSet<usize> = analysis
        .defs
        .iter()
        .filter(|def| def.scope != 0 && def.name == name)
        .map(|def| def.scope)
        .collect();
    sites.extend(
        analysis
            .uses
            .iter()
            .filter(|site| site.name == name && !local_scopes.contains(&site.scope))
            .map(|site| (site.start, site.end)),
    );
    sites
}

fn symbol_scope_at(analysis: &Analysis, offset: usize) -> Option<(String, usize)> {
    for def in &analysis.defs {
        if offset >= def.name_start && offset < def.name_end {
            return Some((def.name.clone(), def.scope));
        }
    }
    for site in &analysis.uses {
        if offset >= site.start && offset < site.end {
            return Some((site.name.clone(), site.scope));
        }
    }
    None
}

fn resolve_semantic(analysis: &Analysis, site: &UseSite) -> u32 {
    if site.member {
        if let Some(def) = analysis
            .defs
            .iter()
            .find(|def| def.name == site.name && def.kind == DefKind::EnumMember)
        {
            return def.kind.semantic_token();
        }
        if let Some(def) = analysis
            .defs
            .iter()
            .find(|def| def.name == site.name && def.kind == DefKind::Property)
        {
            return def.kind.semantic_token();
        }
        let mut cursor = site.end;
        while cursor < analysis.source.code.len() && analysis.source.code[cursor].is_whitespace() {
            cursor += 1;
        }
        if cursor < analysis.source.code.len() && analysis.source.code[cursor] == '(' {
            return DefKind::Function.semantic_token();
        }
    }
    if let Some(def) = analysis
        .defs
        .iter()
        .find(|def| def.scope == site.scope && def.name == site.name)
    {
        return def.kind.semantic_token();
    }
    if let Some(def) = analysis
        .defs
        .iter()
        .find(|def| def.scope == 0 && def.name == site.name)
    {
        return def.kind.semantic_token();
    }
    // Bare builtin calls (`len(...)`, `type(...)`, ...) have no local
    // definition. Like member calls, colour any use followed by `(` as a
    // function so semantic tokens do not override the TextMate call colour.
    let mut cursor = site.end;
    while cursor < analysis.source.code.len() && analysis.source.code[cursor].is_whitespace() {
        cursor += 1;
    }
    if cursor < analysis.source.code.len() && analysis.source.code[cursor] == '(' {
        return DefKind::Function.semantic_token();
    }
    if site.member {
        return DefKind::Property.semantic_token();
    }
    DefKind::Variable.semantic_token()
}

// Keep the legend in lockstep with `SemanticToken`; its order is observable.
const _: () = assert!(SEMANTIC_TOKEN_TYPES.len() == SemanticToken::TypeAlias as u32 as usize + 1);

// New entries are appended so the existing discriminants stay stable.
const SEMANTIC_TOKEN_TYPES: [&str; 11] = [
    "namespace",
    "class",
    "enum",
    "enumMember",
    "parameter",
    "variable",
    "property",
    "function",
    "type",
    "typeParameter",
    "typeAlias",
];

fn semantic_tokens_json(analysis: &Analysis) -> Json {
    let source = &analysis.source;
    let mut tokens: Vec<(usize, usize, u32)> = Vec::new();
    for def in &analysis.defs {
        tokens.push((def.name_start, def.name_end, def.kind.semantic_token()));
    }
    for &(start, end) in &analysis.type_spans {
        tokens.push((start, end, SemanticToken::Type as u32));
    }
    for &(start, end) in &analysis.namespace_spans {
        tokens.push((start, end, SemanticToken::Namespace as u32));
    }
    for site in &analysis.uses {
        tokens.push((site.start, site.end, resolve_semantic(analysis, site)));
    }
    tokens.sort_by_key(|&(start, _, _)| start);
    tokens.dedup();
    let mut seen = HashSet::new();
    let mut data: Vec<Json> = Vec::with_capacity(tokens.len() * 5);
    let mut previous_line: i64 = 0;
    let mut previous_column: i64 = 0;
    for &(start, end, token_type) in &tokens {
        if end <= start || !seen.insert(start) {
            continue;
        }
        let (line, column) = offset_to_position(source, start);
        let length: u32 = source.code[start..end].iter().map(|ch| ch.len_utf16() as u32).sum();
        let delta_line = line - previous_line;
        let delta_column = if line == previous_line {
            column - previous_column
        } else {
            column
        };
        data.push(Json::Num(delta_line as f64));
        data.push(Json::Num(delta_column as f64));
        data.push(Json::Num(length as f64));
        data.push(Json::Num(token_type as f64));
        data.push(Json::Num(0.0));
        previous_line = line;
        previous_column = column;
    }
    Json::obj(vec![("data", Json::Arr(data))])
}


fn utf16_len(ch: char) -> i64 {
    ch.len_utf16() as i64
}

fn offset_to_position(source: &Source, offset: usize) -> (i64, i64) {
    let offset = offset.min(source.code.len());
    let line = (source.get_line(offset) - 1) as i64;
    let line_start = source.get_line_start(offset);
    let mut character: i64 = 0;
    for &ch in &source.code[line_start..offset] {
        character += utf16_len(ch);
    }
    (line, character)
}

fn position_to_offset(source: &Source, line: i64, character: i64) -> usize {
    if line < 0 || character < 0 {
        return 0;
    }
    let code = &source.code;
    let mut current_line: i64 = 0;
    let mut line_start = 0usize;
    for (index, &ch) in code.iter().enumerate() {
        if ch == '\n' {
            if current_line == line {
                return utf16_boundary(&code[line_start..index], character, line_start);
            }
            current_line += 1;
            line_start = index + 1;
        }
    }
    if current_line == line {
        return utf16_boundary(&code[line_start..], character, line_start);
    }
    code.len()
}

fn utf16_boundary(chars: &[char], character: i64, base: usize) -> usize {
    let mut width: i64 = 0;
    for (index, &ch) in chars.iter().enumerate() {
        if width >= character {
            return base + index;
        }
        width += utf16_len(ch);
        if width > character {
            return base + index + 1;
        }
    }
    base + chars.len()
}

fn position_json(line: i64, character: i64) -> Json {
    Json::obj(vec![
        ("line", Json::Num(line as f64)),
        ("character", Json::Num(character as f64)),
    ])
}

fn range_json(source: &Source, start: usize, length: usize) -> Json {
    let (start_line, start_character) = offset_to_position(source, start);
    let (end_line, end_character) = offset_to_position(source, start + length);
    Json::obj(vec![
        ("start", position_json(start_line, start_character)),
        ("end", position_json(end_line, end_character)),
    ])
}

fn span_range_json(source: &Source, start: usize, end: usize) -> Json {
    let (start_line, start_character) = offset_to_position(source, start);
    let (end_line, end_character) = offset_to_position(source, end);
    Json::obj(vec![
        ("start", position_json(start_line, start_character)),
        ("end", position_json(end_line, end_character)),
    ])
}

fn hover_json(value: String, source: &Source, start: usize, end: usize) -> Json {
    Json::obj(vec![
        (
            "contents",
            Json::obj(vec![
                ("kind", Json::from("markdown")),
                ("value", Json::from(value)),
            ]),
        ),
        ("range", span_range_json(source, start, end)),
    ])
}

fn word_range_at(source: &Source, offset: usize) -> Option<(usize, usize)> {
    let code = &source.code;
    let is_word = |ch: char| ch.is_ascii_alphanumeric() || ch == '_';
    let mut start = offset.min(code.len());
    let mut end = start;
    while start > 0 && is_word(code[start - 1]) {
        start -= 1;
    }
    while end < code.len() && is_word(code[end]) {
        end += 1;
    }
    if start == end {
        None
    } else {
        Some((start, end))
    }
}


fn respond(out: &mut dyn Write, id: Json, result: Json) {
    let message = Json::Obj(vec![
        ("jsonrpc".to_string(), Json::from("2.0")),
        ("id".to_string(), id),
        ("result".to_string(), result),
    ]);
    write_message(out, &message);
}

fn respond_error(out: &mut dyn Write, id: Json, code: i64, message: &str) {
    let message = Json::Obj(vec![
        ("jsonrpc".to_string(), Json::from("2.0")),
        ("id".to_string(), id),
        (
            "error".to_string(),
            Json::Obj(vec![
                ("code".to_string(), Json::Num(code as f64)),
                ("message".to_string(), Json::from(message)),
            ]),
        ),
    ]);
    write_message(out, &message);
}

fn notify(out: &mut dyn Write, method: &str, params: Json) {
    let message = Json::Obj(vec![
        ("jsonrpc".to_string(), Json::from("2.0")),
        ("method".to_string(), Json::from(method)),
        ("params".to_string(), params),
    ]);
    write_message(out, &message);
}

fn write_message(out: &mut dyn Write, message: &Json) {
    let body = to_json(message);
    let frame = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
    let _ = out.write_all(frame.as_bytes());
    let _ = out.flush();
}

/// `file:///C:/dev/un/x.un` -> `C:\dev\un\x.un`. Non-file URIs (untitled
/// buffers) have no local path and return `None`.
fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let raw = uri.strip_prefix("file://")?;
    let mut decoded = String::new();
    let chars: Vec<char> = raw.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '%' && index + 2 < chars.len() {
            if let (Some(high), Some(low)) =
                (chars[index + 1].to_digit(16), chars[index + 2].to_digit(16))
            {
                if let Some(ch) = char::from_u32(high * 16 + low) {
                    decoded.push(ch);
                    index += 3;
                    continue;
                }
            }
        }
        decoded.push(chars[index]);
        index += 1;
    }
    if cfg!(windows) {
        Some(PathBuf::from(decoded.trim_start_matches('/').replace('/', "\\")))
    } else {
        Some(PathBuf::from(decoded))
    }
}

fn path_to_uri(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let with_leading_slash = if text.starts_with('/') {
        text
    } else {
        format!("/{text}")
    };
    let mut uri = String::from("file://");
    for ch in with_leading_slash.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '/' | ':' | '-' | '_' | '.' | '~') {
            uri.push(ch);
        } else {
            for byte in ch.to_string().bytes() {
                uri.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    uri
}

fn path_key(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string()
}

/// `module.rs::module_exists`: a `.un` path must be a file; otherwise try the
/// same path with `.un` appended, or a directory.
fn module_candidate(base: &Path, parts: &[String]) -> Option<PathBuf> {
    if parts.is_empty() {
        return None;
    }
    let mut joined = PathBuf::new();
    for part in parts {
        joined.push(part);
    }
    let candidate = base.join(joined);
    let text = candidate.to_string_lossy();
    let file = if text.ends_with(".un") {
        PathBuf::from(text.as_ref())
    } else {
        PathBuf::from(format!("{text}.un"))
    };
    if file.is_file() || candidate.is_dir() {
        Some(candidate)
    } else {
        None
    }
}

/// Same lookup order as `module.rs::resolve_module`: the importing file's
/// directory first, then the workspace/current directory (the LSP equivalent
/// of `World::path()`).
fn resolve_module_root(
    doc_uri: &str,
    parts: &[String],
    workspace_roots: &[PathBuf],
) -> Option<PathBuf> {
    if parts.is_empty() || parts.iter().any(|part| part.is_empty()) {
        return None;
    }
    if let Some(path) = uri_to_path(doc_uri) {
        if let Some(dir) = path.parent() {
            if let Some(found) = module_candidate(dir, parts) {
                return Some(found);
            }
        }
    }
    for root in workspace_roots {
        if let Some(found) = module_candidate(root, parts) {
            return Some(found);
        }
    }
    if let Ok(current) = std::env::current_dir() {
        if let Some(found) = module_candidate(&current, parts) {
            return Some(found);
        }
    }
    None
}

/// Base directory of an explicit relative import — the LSP counterpart of
/// `module.rs::relative_base`. Depth 1 is the importing file's directory,
/// depth 2 its parent, etc. Reaching the filesystem root keeps that root as
/// the base. Unlike `resolve_module_root`, there is deliberately no workspace
/// or current-directory fallback here.
fn relative_base(doc_uri: &str, depth: usize) -> Option<PathBuf> {
    if depth == 0 {
        return None;
    }
    let mut base = uri_to_path(doc_uri)?.parent()?.to_path_buf();
    for _ in 1..depth {
        if let Some(parent) = base.parent() {
            base = parent.to_path_buf();
        } else {
            break;
        }
    }
    Some(base)
}

/// Resolves a `use` declaration against the correct base: relative imports
/// walk up from the importing document, absolute ones keep the original
/// "source dir -> workspace/CWD" lookup order.
fn resolve_use_root(
    doc_uri: &str,
    decl: &ModuleUse,
    workspace_roots: &[PathBuf],
) -> Option<PathBuf> {
    if decl.depth > 0 {
        let base = relative_base(doc_uri, decl.depth)?;
        module_candidate(&base, &decl.parts)
    } else {
        resolve_module_root(doc_uri, &decl.parts, workspace_roots)
    }
}

/// Concrete `.un` files behind a module root: the file itself, `dir/mod.un`,
/// or every `.un` file in a directory (sorted, mirroring `module.rs::load`).
fn module_files(root: &Path) -> Vec<PathBuf> {
    let text = root.to_string_lossy();
    if text.ends_with(".un") {
        return if root.is_file() {
            vec![root.to_path_buf()]
        } else {
            Vec::new()
        };
    }
    let with_extension = PathBuf::from(format!("{text}.un"));
    if with_extension.is_file() {
        return vec![with_extension];
    }
    if !root.is_dir() {
        return Vec::new();
    }
    let mod_file = root.join("mod.un");
    if mod_file.is_file() {
        return vec![mod_file];
    }
    let mut files: Vec<PathBuf> = match std::fs::read_dir(root) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file() && path.extension().map(|ext| ext == "un").unwrap_or(false)
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    files.sort();
    files
}

#[derive(Clone)]
struct ResolvedModuleUse {
    decl: ModuleUse,
    root: PathBuf,
    files: Vec<PathBuf>,
}

/// Character immediately preceding a word, when it is a `.` member access —
/// e.g. `x` for a cursor on `member` in `x.member`.
fn member_receiver_before(source: &Source, word_start: usize) -> Option<String> {
    let code = &source.code;
    let mut cursor = word_start.checked_sub(1)?;
    while cursor > 0 && code[cursor].is_whitespace() {
        cursor -= 1;
    }
    if code[cursor] != '.' {
        return None;
    }
    let end = cursor;
    while cursor > 0 && is_ident_char(code[cursor - 1]) {
        cursor -= 1;
    }
    if cursor == end {
        return None;
    }
    Some(code[cursor..end].iter().collect())
}

struct Server {
    documents: HashMap<String, String>,
    /// Last analysis per document, keyed by the exact text it was built from.
    /// Editing triggers a rebuild; every other request reuses it.
    analyses: HashMap<String, (String, std::rc::Rc<Analysis>)>,
    /// `use` resolution for the last analysis of each document (same text key).
    module_indexes: HashMap<String, (String, Rc<Vec<ResolvedModuleUse>>)>,
    /// Parsed user module files, keyed by canonical path. Open documents take
    /// precedence so unsaved edits are reflected immediately.
    module_file_cache: HashMap<String, (String, Rc<Analysis>)>,
    /// Workspace roots from `initialize` — the second stop in module lookup.
    workspace_roots: Vec<PathBuf>,
    compile_mode: bool,
    semantic_highlighting: bool,
    inlay_hints: bool,
    inlay_param_types: bool,
    inlay_return_types: bool,
    inlay_variable_types: bool,
    shutdown: bool,
    exit_code: Option<i32>,
}

fn json_bool(params: &Json, key: &str, default: bool) -> bool {
    params
        .get(key)
        .and_then(Json::as_bool)
        .unwrap_or(default)
}

impl Server {
    fn new() -> Self {
        Server {
            documents: HashMap::new(),
            analyses: HashMap::new(),
            module_indexes: HashMap::new(),
            module_file_cache: HashMap::new(),
            workspace_roots: Vec::new(),
            compile_mode: false,
            semantic_highlighting: true,
            inlay_hints: true,
            inlay_param_types: true,
            inlay_return_types: true,
            inlay_variable_types: true,
            shutdown: false,
            exit_code: None,
        }
    }

    fn apply_options(&mut self, params: &Json) {
        self.semantic_highlighting = json_bool(params, "semanticHighlighting", true);
        self.inlay_hints = json_bool(params, "inlayHints", true);
        self.inlay_param_types = json_bool(params, "inlayParameterTypes", true);
        self.inlay_return_types = json_bool(params, "inlayReturnTypes", true);
        self.inlay_variable_types = json_bool(params, "inlayVariableTypes", true);
    }
}

impl Server {
    fn analyze_uri(&mut self, uri: &str) -> Option<std::rc::Rc<Analysis>> {
        let text = self.documents.get(uri)?.clone();
        let cached = self
            .analyses
            .get(uri)
            .filter(|(cached_text, _)| cached_text == &text)
            .map(|(_, analysis)| analysis.clone());
        if let Some(analysis) = cached {
            let fresh = self
                .module_indexes
                .get(uri)
                .map(|(cached_text, _)| cached_text != &text)
                .unwrap_or(true);
            if fresh {
                let modules = Rc::new(self.resolve_document_modules(uri, &analysis));
                self.module_indexes.insert(uri.to_owned(), (text, modules));
            }
            return Some(analysis);
        }
        let analysis = Rc::new(analyze(&text, uri));
        let modules = Rc::new(self.resolve_document_modules(uri, &analysis));
        self.analyses.insert(uri.to_owned(), (text.clone(), analysis.clone()));
        self.module_indexes.insert(uri.to_owned(), (text, modules));
        Some(analysis)
    }

    fn resolved_uses(&self, uri: &str) -> Option<Rc<Vec<ResolvedModuleUse>>> {
        self.module_indexes.get(uri).map(|(_, uses)| uses.clone())
    }

    /// Resolves every `use` of a document against the same roots the runtime
    /// uses, and records the `.un` files that back it.
    fn resolve_document_modules(&self, uri: &str, analysis: &Analysis) -> Vec<ResolvedModuleUse> {
        analysis
            .module_uses
            .iter()
            .filter_map(|decl| {
                let root = resolve_use_root(uri, decl, &self.workspace_roots)?;
                let files = module_files(&root);
                if files.is_empty() {
                    return None;
                }
                Some(ResolvedModuleUse {
                    decl: decl.clone(),
                    root,
                    files,
                })
            })
            .collect()
    }

    /// Loads (and caches) a module file. An open editor document with the same
    /// path wins over the on-disk copy so unsaved edits show up immediately.
    fn load_module_analysis(&mut self, path: &Path) -> Option<(Rc<Analysis>, PathBuf)> {
        let key = path_key(path);
        let uri = path_to_uri(path);
        if let Some(text) = self.documents.get(&uri).cloned() {
            return Some((Rc::new(analyze(&text, &key)), path.to_path_buf()));
        }
        if let Some((_, analysis)) = self.module_file_cache.get(&key) {
            return Some((analysis.clone(), path.to_path_buf()));
        }
        let text = std::fs::read_to_string(path).ok()?;
        let analysis = Rc::new(analyze(&text, &key));
        self.module_file_cache.insert(key, (text, analysis.clone()));
        Some((analysis, path.to_path_buf()))
    }

    fn invalidate_module_cache(&mut self, uri: &str) {
        if let Some(path) = uri_to_path(uri) {
            self.module_file_cache.remove(&path_key(&path));
        }
    }

    fn find_resolved_use<'a>(
        uses: &'a [ResolvedModuleUse],
        receiver: &str,
    ) -> Option<&'a ResolvedModuleUse> {
        uses.iter()
            .find(|resolved| resolved.decl.binding() == Some(receiver))
    }

    /// Completion items for `alias.|` after `use a.b [as alias]`.
    fn user_module_completions(&mut self, receiver: &str, uri: &str) -> Option<Vec<Json>> {
        let uses = self.resolved_uses(uri)?;
        let usage = Self::find_resolved_use(&uses, receiver)?;
        let mut items = Vec::new();
        let mut seen = HashSet::new();
        for file in &usage.files {
            let Some((analysis, _)) = self.load_module_analysis(file) else {
                continue;
            };
            for def in &analysis.defs {
                if def.scope != 0 || !seen.insert(def.name.clone()) {
                    continue;
                }
                if !matches!(
                    def.kind,
                    DefKind::Function
                        | DefKind::Extension
                        | DefKind::Class
                        | DefKind::Enum
                        | DefKind::EnumMember
                        | DefKind::Variable
                        | DefKind::Property
                        | DefKind::TypeAlias
                ) {
                    continue;
                }
                let detail = match def.kind {
                    DefKind::Function | DefKind::Extension => {
                        signature_from_callee(&analysis, &def.name, def.name_start)
                            .map(|(label, _)| label)
                    }
                    _ => None,
                };
                items.push(completion_item_with_detail(
                    &def.name,
                    def.kind.completion_kind(),
                    detail,
                ));
            }
        }
        if items.is_empty() {
            None
        } else {
            Some(items)
        }
    }

    /// Bare completions contributed by `use m { a as b }` / `use m.*`.
    fn user_module_import_completions(&mut self, uri: &str) -> Vec<Json> {
        let Some(uses) = self.resolved_uses(uri) else {
            return Vec::new();
        };
        let mut items = Vec::new();
        let mut seen = HashSet::new();
        for usage in uses.iter() {
            if usage.decl.imports.is_empty() && !usage.decl.wildcard {
                continue;
            }
            for file in &usage.files {
                let Some((analysis, _)) = self.load_module_analysis(file) else {
                    continue;
                };
                let mut selected: Vec<(String, String)> = usage
                    .decl
                    .imports
                    .iter()
                    .filter(|(name, _)| name != "*")
                    .map(|(name, rename)| {
                        (name.clone(), rename.clone().unwrap_or_else(|| name.clone()))
                    })
                    .collect();
                if usage.decl.wildcard {
                    for def in &analysis.defs {
                        if def.scope == 0 {
                            selected.push((def.name.clone(), def.name.clone()));
                        }
                    }
                }
                for (name, label) in selected {
                    if !seen.insert(label.clone()) {
                        continue;
                    }
                    let detail = analysis
                        .defs
                        .iter()
                        .find(|def| def.scope == 0 && def.name == name)
                        .and_then(|def| match def.kind {
                            DefKind::Function | DefKind::Extension => {
                                signature_from_callee(&analysis, name.as_str(), def.name_start)
                                    .map(|(label, _)| label)
                            }
                            _ => None,
                        });
                    items.push(completion_item_with_detail(&label, 5.0, detail));
                }
            }
        }
        items
    }

    /// Target of `alias.member`: the member's definition in the module file.
    fn module_member_target(
        &mut self,
        uri: &str,
        alias: &str,
        member: &str,
    ) -> Option<(String, Rc<Analysis>, Def)> {
        let uses = self.resolved_uses(uri)?;
        let usage = Self::find_resolved_use(&uses, alias)?;
        for file in &usage.files {
            let Some((analysis, path)) = self.load_module_analysis(file) else {
                continue;
            };
            if let Some(def) = analysis
                .defs
                .iter()
                .find(|def| def.scope == 0 && def.name == member)
                .cloned()
            {
                return Some((path_to_uri(&path), analysis, def));
            }
        }
        None
    }

    /// Target of a segment inside a `use a.b` path.
    fn use_segment_target(
        &mut self,
        uri: &str,
        offset: usize,
    ) -> Option<(String, Rc<Analysis>, usize, usize)> {
        let uses = self.resolved_uses(uri)?;
        let mut best: Option<(String, Rc<Analysis>, usize, usize, usize)> = None;
        for resolved in uses.iter() {
            for (index, &(start, end)) in resolved.decl.segments.iter().enumerate() {
                if offset >= start && offset < end {
                    let root = if index + 1 == resolved.decl.parts.len() {
                        resolved.root.clone()
                    } else if resolved.decl.depth > 0 {
                        // Intermediate segments of a relative path walk up
                        // from the importing document, never the workspace.
                        let base = relative_base(uri, resolved.decl.depth)?;
                        module_candidate(&base, &resolved.decl.parts[..=index])?
                    } else {
                        resolve_module_root(
                            uri,
                            &resolved.decl.parts[..=index],
                            &self.workspace_roots,
                        )?
                    };
                    let files = module_files(&root);
                    let file = files.first()?;
                    let (analysis, path) = self.load_module_analysis(file)?;
                    let width = end - start;
                    if best.as_ref().map(|entry| width > entry.4).unwrap_or(true) {
                        best = Some((path_to_uri(&path), analysis, 0, 0, width));
                    }
                }
            }
        }
        best.map(|(uri, analysis, start, end, _)| (uri, analysis, start, end))
    }

    fn diagnostics_for(&mut self, uri: &str) -> Vec<Json> {
        let analysis = match self.analyze_uri(uri) {
            Some(analysis) => analysis,
            None => return Vec::new(),
        };
        if let Some(error) = &analysis.error {
            return vec![diagnostic_json(
                &analysis.source,
                error.start,
                error.length,
                &error.message,
            )];
        }
        // Native modules live in the registry, not on disk; user module paths
        // must resolve with the same lookup rules the runtime applies.
        let mut diagnostics = Vec::new();
        for usage in &analysis.module_uses {
            if usage.parts.is_empty()
                || (usage.depth == 0
                    && usage.parts.len() == 1
                    && native_catalog().modules.contains_key(&usage.parts[0]))
            {
                continue;
            }
            if resolve_use_root(uri, usage, &self.workspace_roots).is_some() {
                continue;
            }
            let length = usage.path_end.saturating_sub(usage.path_start).max(1);
            let label = if usage.depth > 0 {
                format!("{}{}", ".".repeat(usage.depth), usage.parts.join("."))
            } else {
                usage.parts.join(".")
            };
            diagnostics.push(diagnostic_json(
                &analysis.source,
                usage.path_start,
                length,
                &format!("module '{label}' not found"),
            ));
        }
        if !diagnostics.is_empty() {
            return diagnostics;
        }
        if !self.compile_mode {
            return Vec::new();
        }
        let shared = Rc::new(analysis.source.clone());
        let root = match analysis.root.as_ref() {
            Some(root) => root,
            None => return Vec::new(),
        };
        let optimized = match optimize(root) {
            Ok(result) => result.root,
            Err(panic) => {
                let end = shared.code.len();
                return vec![diagnostic_json(
                    &shared,
                    0,
                    end.max(1),
                    &format!("compile error: {}", panic.message),
                )];
            }
        };
        match typeck::check_program(&shared, &optimized) {
            Ok(_) => Vec::new(),
            Err(errors) => errors
                .iter()
                .map(|error| {
                    diagnostic_json(&shared, error.start, error.length, &error.message)
                })
                .collect(),
        }
    }

    fn publish_diagnostics(&mut self, out: &mut dyn Write, uri: &str) {
        let diagnostics = self.diagnostics_for(uri);
        notify(
            out,
            "textDocument/publishDiagnostics",
            Json::obj(vec![
                ("uri", Json::from(uri)),
                ("diagnostics", Json::Arr(diagnostics)),
            ]),
        );
    }

    fn publish_all_diagnostics(&mut self, out: &mut dyn Write) {
        let uris: Vec<String> = self.documents.keys().cloned().collect();
        for uri in uris {
            self.publish_diagnostics(out, &uri);
        }
    }
}

fn diagnostic_json(source: &Source, start: usize, length: usize, message: &str) -> Json {
    Json::obj(vec![
        ("range", range_json(source, start, length.max(1))),
        ("severity", Json::Num(1.0)),
        ("source", Json::from("unvm")),
        ("message", Json::from(message)),
    ])
}

fn document_uri(params: &Json) -> Option<String> {
    params
        .get("textDocument")
        .and_then(|doc| doc.get("uri"))
        .and_then(Json::as_str)
        .map(|uri| uri.to_string())
}

fn call_span_len(node: &Node) -> usize {
    node.length
}

fn is_ident_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

/// If the cursor sits right after a dot, returns the receiver text before
/// the dot — e.g. `self` in `self.` or `io` in `io.`.
fn member_receiver(source: &Source, offset: usize) -> Option<String> {
    if offset == 0 {
        return None;
    }
    let code = &source.code;
    let dot = code.get(offset.wrapping_sub(1)).copied()?;
    if dot != '.' {
        return None;
    }
    let end = offset - 1;
    let mut start = end;
    while start > 0 && is_ident_char(code[start - 1]) {
        start -= 1;
    }
    if start == end {
        return None;
    }
    Some(code[start..end].iter().collect())
}

fn builtin_catalog() -> &'static HashMap<String, Vec<NativeSig>> {
    static CATALOG: OnceLock<HashMap<String, Vec<NativeSig>>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let _ = native_catalog(); // registry must be initialised exactly once
        let mut map = HashMap::new();
        for type_name in [
            "str", "list", "dict", "set", "iter", "date", "stopwatch", "pool", "lock", "stream",
            "random", "atomic_int", "atomic_bool", "http_client", "web_socket",
        ] {
            let Some(attrs) = unvm::runtime::world::original_class_attrs(type_name) else {
                continue;
            };
            let mut sigs = Vec::new();
            for (name, obj) in attrs {
                let label = native_signature(&obj).unwrap_or_else(|| name.clone());
                sigs.push(NativeSig { name, label });
            }
            if !sigs.is_empty() {
                map.insert(type_name.to_string(), sigs);
            }
        }
        map
    })
}

fn builtin_members(name: &str) -> Option<&'static [NativeSig]> {
    builtin_catalog().get(name).map(|sigs| sigs.as_slice())
}

fn builtin_member_completions(members: &[NativeSig]) -> Vec<Json> {
    members
        .iter()
        .map(|sig| completion_item_with_detail(&sig.name, 2.0, Some(sig.label.clone()))) // Method
        .collect()
}
fn enclosing_class(analysis: &Analysis, offset: usize) -> Option<&Def> {
    analysis
        .defs
        .iter()
        .filter(|def| matches!(def.kind, DefKind::Class | DefKind::Enum))
        .filter(|def| offset >= def.start && offset < def.end)
        .min_by_key(|def| def.end - def.start)
}

fn resolve_receiver_class(analysis: &Analysis, receiver: &str, offset: usize) -> Option<(usize, usize, String)> {
    if let Some(def) = analysis
        .defs
        .iter()
        .find(|def| def.name == receiver && matches!(def.kind, DefKind::Class | DefKind::Enum))
    {
        return Some((def.start, def.end, def.name.clone()));
    }
    if let Some(callee) = analysis.var_class_hints.get(receiver) {
        if builtin_members(callee).is_some() {
            return Some((0, 0, callee.clone()));
        }
        if let Some(def) = analysis
            .defs
            .iter()
            .find(|def| def.name == *callee && matches!(def.kind, DefKind::Class | DefKind::Enum))
        {
            return Some((def.start, def.end, def.name.clone()));
        }
    }
    if receiver == "self" {
        if let Some(def) = enclosing_class(analysis, offset) {
            return Some((def.start, def.end, def.name.clone()));
        }
    }
    None
}

fn completion_item(label: &str, kind: f64) -> Json {
    Json::obj(vec![
        ("label", Json::from(label)),
        ("kind", Json::Num(kind)),
    ])
}

fn completion_item_with_detail(label: &str, kind: f64, detail: Option<String>) -> Json {
    let mut obj = vec![
        ("label", Json::from(label)),
        ("kind", Json::Num(kind)),
    ];
    if let Some(detail) = detail {
        obj.push(("detail", Json::from(detail)));
    }
    Json::obj(obj)
}

fn snippet_completion_item(label: &str, snippet: &str, detail: &str) -> Json {
    Json::obj(vec![
        ("label", Json::from(label)),
        ("kind", Json::Num(15.0)),
        ("detail", Json::from(detail)),
        ("insertText", Json::from(snippet)),
        ("insertTextFormat", Json::Num(2.0)),
    ])
}

fn class_member_completions(analysis: &Analysis, start: usize, end: usize) -> Vec<Json> {
    let mut items = Vec::new();
    let mut seen = HashSet::new();
    let inferred = infer_types(analysis);
    for def in &analysis.defs {
        if def.start < start || def.end > end {
            continue;
        }
        if !seen.insert(def.name.clone()) {
            continue;
        }
        match def.kind {
            DefKind::Function | DefKind::Extension => {
                let signature = signature_from_callee(analysis, &def.name, def.name_start)
                    .map(|(label, _params)| label);
                let detail = signature.or_else(|| inferred.fn_returns.get(&def.name).map(|ret| format!("fn {}() -> {ret}", def.name)));
                items.push(completion_item_with_detail(&def.name, 3.0, detail));
            }
            DefKind::Variable | DefKind::Property => {
                let detail = inferred.vars.get(&def.name).map(|t| format!("{t}"));
                items.push(completion_item_with_detail(&def.name, 5.0, detail));
            }
            DefKind::EnumMember => {
                items.push(completion_item_with_detail(&def.name, 13.0, None));
            }
            _ => {}
        }
    }
    items
}

fn namespace_member_completions(module: &str) -> Vec<Json> {
    let mut items = Vec::new();
    if let Some(sigs) = native_catalog().modules.get(module) {
        for sig in sigs {
            items.push(completion_item_with_detail(&sig.name, 3.0, Some(sig.label.clone())));
        }
    }
    items
}

/// innermost Call node whose span covers `offset` (if any)
fn innermost_call<'a>(node: &'a Node, offset: usize) -> Option<&'a Node> {
    if offset < node.start || offset > node.start + node.length {
        return None;
    }
    let mut best: Option<&Node> = match node.kind {
        NodeKind::Call => Some(node),
        _ => None,
    };
    for child in &node.children {
        if let Some(inner) = innermost_call(child, offset) {
            best = match best {
                Some(current) if call_span_len(inner) >= call_span_len(current) => Some(current),
                _ => Some(inner),
            };
        }
    }
    best
}

fn sig_params(root: &Node, source: &Source, name_start: usize, name_end: usize) -> Option<(String, Vec<String>, Option<String>)> {
    fn find<'a>(node: &'a Node, name_start: usize, name_end: usize) -> Option<&'a Node> {
        match node.kind {
            NodeKind::Function | NodeKind::Extension | NodeKind::Class => {
                let index = if node.kind == NodeKind::Extension { 1 } else { 0 };
                if let Some(name) = node.children.get(index) {
                    if name.kind == NodeKind::Identifier && (name.start, name.start + name.length) == (name_start, name_end) {
                        return Some(node);
                    }
                }
            }
            _ => {}
        }
        for child in &node.children {
            if let Some(found) = find(child, name_start, name_end) {
                return Some(found);
            }
        }
        None
    }
    let node = find(root, name_start, name_end)?;
    let is_extension = node.kind == NodeKind::Extension;
    let params_index = if is_extension { 2 } else { 1 };
    let ret_index = if is_extension { 4 } else { 3 };
    let name = source.span(name_start, name_end - name_start);
    let params_text: Vec<String> = match node.children.get(params_index) {
        Some(params_node) => params_node
            .children
            .iter()
            .map(|param| {
                source
                    .span(param.start, param.length)
                    .trim()
                    .to_string()
            })
            .collect(),
        None => Vec::new(),
    };
    let ret = node
        .children
        .get(ret_index)
        .map(|ret| source.span(ret.start, ret.length).trim().trim_start_matches("->").trim().to_string());
    let label = match &ret {
        Some(ret) => format!("{name}({}) -> {ret}", params_text.join(", ")),
        None => format!("{name}({})", params_text.join(", ")),
    };
    Some((label, params_text, ret))
}

/// Consecutive whole-line `#` comments immediately above a declaration.
/// The lexer discards comments, so read them from the original source text.
fn doc_comment(source: &Source, position: usize) -> String {
    let code = &source.code;
    let mut cursor = source.get_line_start(position);
    let mut lines: Vec<String> = Vec::new();
    while cursor > 0 {
        let end = cursor - 1; // the newline that ends the previous line
        let mut start = end;
        while start > 0 && code[start - 1] != '\n' {
            start -= 1;
        }
        let text: String = code[start..end].iter().collect();
        let trimmed = text.trim();
        if !trimmed.starts_with('#') {
            break;
        }
        lines.push(trimmed.trim_start_matches('#').trim().to_string());
        cursor = start;
    }
    lines.reverse();
    lines.join("  \n") // Markdown hard line break
}

/// Compact hover: an optional doc comment followed by a signature code block.
fn def_hover_markdown(analysis: &Analysis, def: &Def) -> String {
    let mut text = String::new();
    let mut doc = doc_comment(&analysis.source, def.start);
    if doc.is_empty() && def.kind == DefKind::Namespace {
        doc = docs::module(&def.name)
            .map(str::to_string)
            .unwrap_or_default();
    }
    if !doc.is_empty() {
        text.push_str(&doc);
        text.push_str("\n\n");
    }
    let signature = if matches!(def.kind, DefKind::Function | DefKind::Extension) {
        analysis.root.as_ref().and_then(|root| {
            sig_params(root, &analysis.source, def.name_start, def.name_end)
                .map(|(label, _, _)| label)
        })
    } else {
        None
    };
    if let Some(label) = signature {
        text.push_str("```un\n");
        text.push_str(&label);
        text.push_str("\n```\n");
    } else {
        let declaration = analysis.source.get_line_text(def.start).trim().to_string();
        text.push_str("```un\n");
        text.push_str(&declaration);
        text.push_str("\n```\n");
    }
    text
}

/// Hover for builtins and native members, which have no local def.
fn builtin_hover_markdown(
    analysis: &Analysis,
    word: &str,
    start: usize,
    end: usize,
) -> Option<Json> {
    let _ = native_catalog(); // ensure the registry is initialised
    let doc: String;
    let label = if let Some(receiver) = member_receiver_before(&analysis.source, start) {
        let module = analysis
            .use_aliases
            .iter()
            .find(|(alias, _)| alias == &receiver)
            .map(|(_, module)| module.clone())
            .unwrap_or_else(|| receiver.clone());
        doc = docs::function(&module, word)
            .map(str::to_string)
            .unwrap_or_else(|| format!("`{module}` 모듈의 네이티브 함수"));
        if let Some(sigs) = native_catalog().modules.get(&module) {
            sigs.iter()
                .find(|sig| sig.name == word)
                .map(|sig| sig.label.clone())
        } else {
            resolve_receiver_class(analysis, &receiver, start)
                .map(|(_, _, name)| name)
                .as_deref()
                .and_then(builtin_members)
                .and_then(|sigs| signature_for_native(word, sigs).map(|(label, _)| label))
        }
    } else {
        doc = docs::builtin(word)
            .map(str::to_string)
            .unwrap_or_else(|| "빌트인 함수".to_string());
        native_catalog().builtins.get(word).cloned()
    }?;
    Some(hover_json(
        format!("{doc}\n\n```un\n{label}\n```\n"),
        &analysis.source,
        start,
        end,
    ))
}

fn signature_for_native(name: &str, members: &[NativeSig]) -> Option<(String, Vec<String>)> {
    members
        .iter()
        .find(|sig| sig.name == name)
        .map(|sig| {
            let params: Vec<String> = sig
                .label
                .trim_start_matches(name)
                .trim_start_matches('(')
                .trim_end_matches(')')
                .split(',')
                .map(|arg| arg.trim().to_string())
                .filter(|arg| !arg.is_empty())
                .collect();
            (sig.label.clone(), params)
        })
}

fn signature_from_callee(analysis: &Analysis, callee_text: &str, offset: usize) -> Option<(String, Vec<String>)> {
    let parts: Vec<&str> = callee_text.split('.').collect();
    if parts.len() == 1 {
        let name = parts[0];
        let direct = analysis
            .defs
            .iter()
            .find(|def| def.name == name && matches!(def.kind, DefKind::Function | DefKind::Extension))
            .and_then(|def| {
                sig_params(analysis.root.as_ref()?, &analysis.source, def.name_start, def.name_end)
            })
            .map(|(label, params, _ret)| (label, params));
        if direct.is_some() {
            return direct;
        }
        // constructor call: resolve to the class's __init__
        return analysis
            .defs
            .iter()
            .find(|def| def.name == name && def.kind == DefKind::Class)
            .and_then(|class_def| {
                analysis
                    .defs
                    .iter()
                    .find(|member| {
                        member.name == "__init__"
                            && matches!(member.kind, DefKind::Function | DefKind::Extension)
                            && member.start >= class_def.start
                            && member.end <= class_def.end
                    })
                    .and_then(|member| {
                        sig_params(
                            analysis.root.as_ref()?,
                            &analysis.source,
                            member.name_start,
                            member.name_end,
                        )
                    })
            })
            .map(|(label, params, _ret)| (label, params));
    }
    let receiver = parts[0];
    let member = parts[parts.len() - 1];
    // Resolve the receiver to a class: hint-typed local var, class name, or `self`.
    let class_name = resolve_receiver_class(analysis, receiver, offset).map(|(_, _, name)| name);
    let member_def = class_name
        .as_deref()
        .and_then(|class_name| {
            analysis
                .defs
                .iter()
                .find(|def| {
                    def.name == member
                        && matches!(def.kind, DefKind::Function | DefKind::Extension)
                        && analysis.defs.iter().any(|candidate| {
                            candidate.name == class_name
                                && candidate.start <= def.start
                                && def.end <= candidate.end
                        })
                })
        })
        .or_else(|| {
            analysis
                .defs
                .iter()
                .find(|def| def.name == member && matches!(def.kind, DefKind::Function | DefKind::Extension))
        });
    if let Some(def) = member_def {
        return sig_params(analysis.root.as_ref()?, &analysis.source, def.name_start, def.name_end)
            .map(|(label, params, _ret)| (label, params));
    }
    // Built-in type members: `s.split(`, `xs.sort(`, ...
    if let Some(ref class_name) = class_name {
        if let Some(sigs) = builtin_members(class_name) {
            if let Some(result) = signature_for_native(member, sigs) {
                return Some(result);
            }
        }
    }
    // Native module members: `io.write`, `fs.size`, `math.sqrt`
    let root = analysis
        .use_aliases
        .iter()
        .find(|(alias, _module)| alias == receiver)
        .map(|(_alias, module)| module.clone())
        .unwrap_or_else(|| receiver.to_string());
    native_catalog()
        .modules
        .get(&root)
        .and_then(|sigs| signature_for_native(member, sigs))
}

fn signature_help(analysis: &Analysis, offset: usize) -> Option<Json> {
    let root = analysis.root.as_ref();
    let (label, params, active) = match root {
        Some(root) => {
            let call = innermost_call(root, offset)?;
            let callee = call.children.first()?;
            if !matches!(callee.kind, NodeKind::Identifier | NodeKind::Property) {
                return None;
            }
            let callee_text = analysis.source.span(callee.start, callee.length);
            let (label, params) = signature_from_callee(analysis, callee_text.trim(), offset)?;
            let arguments = call.children.get(1)?;
            let mut active = 0usize;
            for arg in &arguments.children {
                if arg.start < offset {
                    active += 1;
                } else {
                    break;
                }
            }
            (label, params, active)
        }
        None => {
            let (label, params, active) = signature_help_textual(analysis, offset)?;
            (label, params, active)
        }
    };
    let active = active.min(params.len().saturating_sub(1));
    let signatures = Json::Arr(vec![Json::obj(vec![
        ("label", Json::from(label)),
        (
            "parameters",
            Json::Arr(
                params
                    .iter()
                    .map(|param| Json::obj(vec![("label", Json::from(param.as_str()))]))
                    .collect(),
            ),
        ),
    ])]);
    Some(Json::obj(vec![
        ("signatures", signatures),
        ("activeParameter", Json::Num(active as f64)),
        ("activeSignature", Json::Num(0.0)),
    ]))
}

/// Textual signature lookup for documents that don't parse (the normal state
/// while typing). Finds the innermost unclosed `(` before the cursor, reads the
/// callee before it, and resolves UDF / built-in / native signatures from text.
fn signature_help_textual(analysis: &Analysis, offset: usize) -> Option<(String, Vec<String>, usize)> {
    let code = &analysis.source.code;
    let open = textual_infer::innermost_open_paren(code, offset)?;
    // Callee text before the '(' — identifier characters and dots.
    let end = open;
    let mut start = end;    while start > 0 {
        let ch = code[start - 1];
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' {
            start -= 1;
        } else {
            break;
        }
    }
    if start == end {
        return None;
    }
    let callee_text: String = code[start..end].iter().collect();
    let (label, params) = signature_from_callee(analysis, callee_text.trim(), offset)
        .or_else(|| signature_from_callee_textual(analysis, callee_text.trim()))?;
    // Active parameter: top-level commas between the '(' and the cursor.
    let active = textual_infer::split_top_level_chars(code, open + 1, offset, ',').len().saturating_sub(1);
    Some((label, params, active))
}

fn signature_from_callee_textual(analysis: &Analysis, callee_text: &str) -> Option<(String, Vec<String>)> {
    // UDF: find `fn name(` textually and read the signature from that line.
    let parts: Vec<&str> = callee_text.split('.').collect();
    if parts.len() != 1 {
        return None;
    }
    let name = parts[0];
    let code = &analysis.source.code;
    let mut i = 0usize;
    while i < code.len() {
        // find "fn <name>("
        if code[i] == 'f' && i + 2 < code.len() && code[i + 1] == 'n' && code[i + 2] == ' ' {
            let mut j = i + 3;
            let mut ident = String::new();
            while j < code.len() && (code[j].is_ascii_alphanumeric() || code[j] == '_') {
                ident.push(code[j]);
                j += 1;
            }
            if ident == name && j < code.len() && code[j] == '(' {
                // read until matching ')'
                let k = textual_infer::matching_paren_chars(code, j).unwrap_or(code.len());
                let params_text: String = code[j + 1..k].iter().collect();
                let params: Vec<String> = params_text
                    .split(',')
                    .map(|p| p.trim().to_string())
                    .filter(|p| !p.is_empty())
                    .collect();
                // return type after ')'
                let mut ret = String::new();
                let mut m = k + 1;
                while m < code.len() && code[m] != '\n' {
                    if code[m] == '-' && m + 1 < code.len() && code[m + 1] == '>' {
                        ret = code[m + 2..]
                            .iter()
                            .take_while(|ch| **ch != '\n')
                            .collect::<String>()
                            .trim()
                            .to_string();
                        break;
                    }
                    m += 1;
                }
                let label = if ret.is_empty() {
                    format!("{name}({})", params.join(", "))
                } else {
                    format!("{name}({}) -> {ret}", params.join(", "))
                };
                return Some((label, params));
            }
        }
        i += 1;
    }
    None
}

/// Lightweight type inference from textual shapes, shared by inlay hints and
/// completion details. The implementation lives in `unvm::textual_infer` so the
/// playground sees exactly the same results.
fn infer_types(analysis: &Analysis) -> InferredTypes {
    textual_infer::infer(&analysis.source.code)
}

fn matching_paren(text: &str, open: usize) -> Option<usize> {
    textual_infer::matching_paren(text, open)
}

fn split_top_level(text: &str, sep: char) -> Vec<&str> {
    textual_infer::split_top_level(text, sep)
}

fn inlay_hints(analysis: &Analysis) -> Vec<Json> {
    let inferred = infer_types(analysis);
    let mut hints = Vec::new();
    let source = &analysis.source;
    let code = &source.code;
    let mut i = 0usize;
    while i < code.len() {
        let line_start = i;
        while i < code.len() && code[i] != '\n' {
            i += 1;
        }
        let line: String = code[line_start..i].iter().collect();
        let trimmed = line.trim();
        let indent = line.chars().take_while(|ch| *ch == ' ' || *ch == '\t').count();
        let offset = |col: usize| line_start + col.min(line.chars().count());

        // Function definition hints: parameter types (after name) and return type (after ')')
        if let Some(rest) = trimmed.strip_prefix("fn ") {
            let colon = trimmed.find('(').map(|open| open).unwrap_or(0);
            let _ = colon;
            if let Some(open) = rest.find('(') {
                if let Some(close) = matching_paren(rest, open) {
                    let name: String = rest.chars().take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_').collect();
                    let params_text = &rest[open + 1..close];
                    let mut search = 0usize;
                    for part in split_top_level(params_text, ',') {
                        let lead = params_text[search..].find(part).unwrap_or(0);
                        let part_start = search + lead;
                        search = part_start + part.len();
                        let p = part.trim();
                        if p.is_empty() {
                            continue;
                        }
                        let pname: String = p.chars().take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_').collect();
                        if !p.contains(':') {
                            let ptype = inferred
                                .fn_params
                                .get(&name)
                                .and_then(|params| params.iter().find(|(n, _)| n == &pname))
                                .map(|(_, t)| t.clone())
                                .unwrap_or_else(|| "any".to_string());
                            let col = indent + "fn ".len() + open + 1 + part_start + part.find(&pname).unwrap_or(0) + pname.len();
                            hints.push(inlay_hint(source, offset(col), format!(": {ptype}"), 1));
                        }
                    }
                    let after = rest[close + 1..].trim();
                    if !after.starts_with("->") {
                        let ret = inferred.fn_returns.get(&name).cloned().unwrap_or_else(|| "any".to_string());
                        let col = indent + "fn ".len() + close + 1;
                        hints.push(inlay_hint(source, offset(col), format!(" -> {ret}"), 1));
                    }
                }
            }
        }
        // Assignment hints: inferred variable type after the name
        if let Some(eq) = trimmed.find('=') {
            let left = trimmed[..eq].trim();
            if !left.contains(':') {
                let name = left.trim();
                if !name.is_empty()
                    && name.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
                    && !name.chars().next().unwrap().is_ascii_digit()
                {
                    if let Some(t) = inferred.vars.get(name) {
                        if t != "any" {
                            let col = indent + trimmed.find(name).unwrap_or(0) + name.len();
                            hints.push(inlay_hint(source, offset(col), format!(": {t}"), 1));
                        }
                    }
                }
            }
        }
        i += 1;
    }
    hints
}

fn inlay_hint(source: &Source, offset: usize, label: String, kind: u32) -> Json {
    let (line, character) = offset_to_position(source, offset);
    Json::obj(vec![
        ("position", position_json(line, character)),
        ("label", Json::from(label)),
        ("kind", Json::Num(kind as f64)),
    ])
}

fn text_document_position(params: &Json) -> Option<(String, i64, i64)> {    let uri = document_uri(params)?;
    let position = params.get("position")?;
    let line = position.get("line").and_then(Json::as_f64)? as i64;
    let character = position.get("character").and_then(Json::as_f64)? as i64;
    Some((uri, line, character))
}

const KEYWORD_COMPLETIONS: &[&str] = &[
    "if", "elif", "else", "match", "for", "while", "break", "skip", "try", "catch", "finally",
    "defer", "use", "using", "fn", "class", "enum", "go", "wait", "in", "is", "as",
    "and", "or", "xor", "not",
    // Contextual keywords. They are identifiers to the lexer, so offering them
    // here cannot break `type(x)`, a variable named `type`, or `case` as a name.
    // `return` is deliberately absent: Un returns with `->`, not a keyword.
    "type", "case", "get", "set",
];

const KEYWORD_SNIPPETS: &[(&str, &str, &str)] = &[
    ("if", "if ${1:condition}\n    $0", "if statement"),
    ("elif", "elif ${1:condition}\n    $0", "elif branch"),
    ("else", "else\n    $0", "else branch"),
    ("for", "for ${1:item} in ${2:iter}\n    $0", "for loop"),
    ("while", "while ${1:condition}\n    $0", "while loop"),
    ("match", "match ${1:value} { ${2:pattern} -> $0 }", "match expression"),
    ("type", "type ${1:Name} = ${2:int | str}", "type alias"),
    ("case", "case ${1:pattern} if ${2:condition} -> ${3:body},", "guarded match arm"),
    ("get", "get ${1:name} -> ${2:field}", "getter"),
    ("set", "set ${1:name} = ${2:value}", "setter"),
    ("->", "-> ${1:expr}", "return expression"),
    ("try", "try\n    $0\ncatch ${1:e}\n    ", "try-catch"),
    ("catch", "catch ${1:e}\n    $0", "catch block"),
    ("finally", "finally\n    $0", "finally block"),
    ("fn", "fn ${1:name}(${2:params})\n    $0", "function"),
    ("class", "class ${1:Name}\n    $0", "class"),
    ("enum", "enum ${1:Name}\n    $0", "enum"),
    ("defer", "defer ${1:expr}", "defer statement"),
    ("go", "go ${1:expr}", "spawn task"),
    ("wait", "wait ${1:expr}", "wait for task"),
    ("using", "using ${1:name} = ${2:expr}\n    $0", "using statement"),
];

/// Primitive and container type names from the current spec. Fixed-width
/// aliases (`u8`…`f64`) also need `use types`, but are offered so the editor can
/// complete them and the static checker gives the precise diagnostic.
const TYPE_COMPLETIONS: &[&str] = &[
    "int", "float", "bool", "str", "bytes", "any",
    "list", "set", "dict", "tuple",
    "u8", "u16", "u32", "u64", "i8", "i16", "i32", "i64", "f32", "f64",
];

const LANGUAGE_COMPLETIONS: &[&str] = &["none", "true", "false", "self", "field", "value"];

fn is_valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let first = match chars.next() {
        Some(ch) => ch,
        None => return false,
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_') && name != "self"
}

fn folding_ranges_json(source: &Source, tokens: &[Token]) -> Vec<Json> {
    let mut folds = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    for token in tokens {
        match token.typ {
            TokenType::Indent => {
                let line = source.get_line(token.start).saturating_sub(1) as i64 - 1;
                stack.push(line.max(0) as usize);
            }
            TokenType::Dedent => {
                if let Some(start_line) = stack.pop() {
                    let end_line = source.get_line(token.start).saturating_sub(1) as i64 - 1;
                    if end_line > start_line as i64 {
                        folds.push(Json::obj(vec![
                            ("startLine", Json::Num(start_line as f64)),
                            ("endLine", Json::Num(end_line as f64)),
                            ("kind", Json::from("region")),
                        ]));
                    }
                }
            }
            _ => {}
        }
    }
    folds
}

fn symbols_for(analysis: &Analysis) -> Vec<Json> {
    let visible: Vec<&Def> = analysis
        .defs
        .iter()
        .filter(|def| def.kind.outline_visible(def.scope))
        .collect();
    let count = visible.len();
    let parent_of: Vec<Option<usize>> = (0..count)
        .map(|index| {
            let current = &visible[index];
            let mut best: Option<usize> = None;
            for (other, candidate) in visible.iter().enumerate() {
                if other == index {
                    continue;
                }
                if candidate.start <= current.start
                    && current.end <= candidate.end
                    && (candidate.end - candidate.start) >= (current.end - current.start)
                    && !(candidate.start == current.start && candidate.end == current.end)
                {
                    let span = candidate.end - candidate.start;
                    if best.map(|best_index| visible[best_index].end - visible[best_index].start > span).unwrap_or(true) {
                        best = Some(other);
                    }
                }
            }
            best
        })
        .collect();

    fn symbol_json(analysis: &Analysis, visible: &[&Def], index: usize, parent_of: &[Option<usize>]) -> Json {
        let def = visible[index];
        let children: Vec<Json> = visible
            .iter()
            .enumerate()
            .filter(|(child, _)| parent_of[*child] == Some(index))
            .map(|(child, _)| symbol_json(analysis, visible, child, parent_of))
            .collect();
        Json::obj(vec![
            ("name", Json::from(def.name.as_str())),
            ("kind", Json::Num(def.kind.symbol_kind())),
            ("range", span_range_json(&analysis.source, def.start, def.end)),
            (
                "selectionRange",
                span_range_json(&analysis.source, def.name_start, def.name_end),
            ),
            ("children", Json::Arr(children)),
        ])
    }

    parent_of
        .iter()
        .enumerate()
        .filter(|(_index, parent)| parent.is_none())
        .map(|(index, _)| symbol_json(analysis, &visible, index, &parent_of))
        .collect()
}

fn handle_message(server: &mut Server, out: &mut dyn Write, body: &str) {
    let message = match parse_json(body) {
        Ok(message) => message,
        Err(_) => return,
    };
    let method = message
        .get("method")
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string();
    let id = message.get("id").cloned();
    let params = message.get("params").cloned().unwrap_or(Json::Null);

    match (id, method.as_str()) {
        (Some(id), "initialize") => {
            let options = params
                .get("initializationOptions")
                .cloned()
                .unwrap_or(Json::Null);
            server.compile_mode = options
                .get("compileMode")
                .and_then(Json::as_bool)
                .unwrap_or(false);
            // Module paths are resolved relative to the workspace (the LSP
            // counterpart of `World::path()` for the CLI).
            server.workspace_roots = params
                .get("workspaceFolders")
                .and_then(Json::as_arr)
                .into_iter()
                .flatten()
                .filter_map(|folder| folder.get("uri").and_then(Json::as_str))
                .filter_map(uri_to_path)
                .collect();
            if let Some(root) = params
                .get("rootUri")
                .and_then(Json::as_str)
                .and_then(uri_to_path)
            {
                if !server.workspace_roots.contains(&root) {
                    server.workspace_roots.push(root);
                }
            }
            server.module_indexes.clear();
            server.apply_options(&options);
            let result = Json::obj(vec![
                (
                    "capabilities",
                    Json::obj(vec![
                        (
                            "textDocumentSync",
                            Json::obj(vec![
                                ("openClose", Json::from(true)),
                                ("change", Json::Num(1.0)),                                ("save", Json::from(false)),
                            ]),
                        ),
                        (
                            "completionProvider",
                            Json::obj(vec![("triggerCharacters", Json::Arr(vec![Json::from(".")]))]),
                        ),
                        (
                            "signatureHelpProvider",
                            Json::obj(vec![
                                ("triggerCharacters", Json::Arr(vec![Json::from("("), Json::from(",")])),
                                ("retriggerCharacters", Json::Arr(vec![Json::from(","), Json::from(")")])),
                            ]),
                        ),
                        ("definitionProvider", Json::from(true)),
                        ("hoverProvider", Json::from(true)),
                        ("inlayHintProvider", Json::from(true)),
                        ("documentSymbolProvider", Json::from(true)),
                        ("foldingRangeProvider", Json::from(true)),
                        ("referencesProvider", Json::from(true)),
                        ("renameProvider", Json::from(true)),
                        (
                            "semanticTokensProvider",
                            Json::obj(vec![
                                ("full", Json::from(true)),
                                (
                                    "legend",
                                    Json::obj(vec![
                                        (
                                            "tokenTypes",
                                            Json::Arr(
                                                SEMANTIC_TOKEN_TYPES
                                                    .iter()
                                                    .map(|name| Json::from(*name))
                                                    .collect(),
                                            ),
                                        ),
                                        ("tokenModifiers", Json::Arr(vec![])),
                                    ]),
                                ),
                            ]),
                        ),
                    ]),
                ),
                (
                    "serverInfo",
                    Json::obj(vec![
                        ("name", Json::from("un-lsp")),
                        ("version", Json::from("0.3.0")),
                    ]),
                ),
            ]);
            respond(out, id, result);
        }
        (None, "un/compileMode") => {
            server.compile_mode = params
                .get("enabled")
                .and_then(Json::as_bool)
                .unwrap_or(false);
            server.publish_all_diagnostics(out);
        }
        (None, "un/setOptions") => {
            if let Some(compile_mode) = params.get("compileMode").and_then(Json::as_bool) {
                server.compile_mode = compile_mode;
            }
            server.apply_options(&params);
            server.publish_all_diagnostics(out);
        }
        (Some(id), "shutdown") => {
            server.shutdown = true;
            respond(out, id, Json::Null);
        }
        (None, "exit") => {
            server.exit_code = Some(if server.shutdown { 0 } else { 1 });
        }
        (None, "initialized") => {}
        (None, "textDocument/didOpen") => {
            let uri = params
                .get("textDocument")
                .and_then(|doc| doc.get("uri"))
                .and_then(Json::as_str)
                .map(|uri| uri.to_string());
            if let Some(uri) = uri {
                let text = params
                    .get("textDocument")
                    .and_then(|doc| doc.get("text"))
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string();
                server.invalidate_module_cache(&uri);
                server.documents.insert(uri.clone(), text);
                server.publish_diagnostics(out, &uri);
            }
        }
        (None, "textDocument/didChange") => {
            let uri = document_uri(&params);
            if let Some(uri) = uri {
                let text = params
                    .get("contentChanges")
                    .and_then(Json::as_arr)
                    .and_then(|changes| changes.last())
                    .and_then(|change| change.get("text"))
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string();
                server.invalidate_module_cache(&uri);
                server.documents.insert(uri.clone(), text);
                server.publish_diagnostics(out, &uri);
            }
        }
        (None, "textDocument/didClose") => {
            let uri = document_uri(&params);
            if let Some(uri) = uri {
                server.documents.remove(&uri);
                notify(
                    out,
                    "textDocument/publishDiagnostics",
                    Json::obj(vec![
                        ("uri", Json::from(uri.as_str())),
                        ("diagnostics", Json::Arr(vec![])),
                    ]),
                );
            }
        }
        (Some(id), "textDocument/completion") => {
            let uri = document_uri(&params).unwrap_or_default();
            let _trigger_character = params
                .get("context")
                .and_then(|context| context.get("triggerCharacter"))
                .and_then(Json::as_str)
                .map(|s| s.to_string());
            let position = text_document_position(&params);
            let member_completions = server
                .analyze_uri(&uri)
                .and_then(|analysis| {
                    let (_, line, character) = position.as_ref()?;
                    let offset = position_to_offset(&analysis.source, *line, *character);
                    member_receiver(&analysis.source, offset).map(|receiver| {
                        (analysis, receiver, offset)
                    })
                });

            let mut items: Vec<Json> = Vec::new();
            let mut handled_as_member = false;

            // Member completion is driven by the document itself, not the
            // trigger reason — `self.|` should offer members even on manual
            // Ctrl+Space or after typing more word characters.
            if let Some((analysis, receiver, offset)) = member_completions {
                if let Some(user_items) = server.user_module_completions(&receiver, &uri) {
                    items = user_items;
                    handled_as_member = true;
                } else if let Some(module) = module_of(&receiver, &analysis.use_aliases) {
                    items = namespace_member_completions(&module);
                    handled_as_member = true;
                } else if let Some((start, end, name)) =
                    resolve_receiver_class(&analysis, &receiver, offset)
                {
                    items = builtin_members(&name).map(builtin_member_completions).unwrap_or_else(|| class_member_completions(&analysis, start, end));
                    handled_as_member = true;
                }
            }

            if !handled_as_member {
                items = KEYWORD_COMPLETIONS
                    .iter()
                    .map(|label| completion_item(label, 14.0))
                    .chain(TYPE_COMPLETIONS.iter().map(|label| completion_item(label, 14.0)))
                    .chain(LANGUAGE_COMPLETIONS.iter().map(|label| completion_item(label, 21.0)))
                    .collect();
                for (label, snippet, detail) in KEYWORD_SNIPPETS {
                    items.push(snippet_completion_item(label, snippet, detail));
                }
                let mut module_names: HashSet<String> = native_catalog()
                    .module_names
                    .iter()
                    .cloned()
                    .collect();
                for name in &native_catalog().module_names {
                    items.push(completion_item(name, 9.0)); // Module
                }
                if let Some(uses) = server.resolved_uses(&uri) {
                    for usage in uses.iter() {
                        if let Some(name) = usage.decl.binding() {
                            if module_names.insert(name.to_string()) {
                                items.push(completion_item(name, 9.0)); // User module
                            }
                        }
                    }
                }
                if let Some(analysis) = server.analyze_uri(&uri) {
                    let inferred = infer_types(&analysis);
                    let mut seen = HashSet::new();
                    for def in &analysis.defs {
                        if seen.insert(def.name.clone()) {
                            let detail = match def.kind {
                                DefKind::Function | DefKind::Extension => {
                                    signature_from_callee(&analysis, &def.name, def.name_start)
                                        .map(|(label, _params)| label)
                                        .or_else(|| inferred.fn_returns.get(&def.name).map(|ret| format!("fn {}() -> {ret}", def.name)))
                                }
                                DefKind::Variable | DefKind::Property => {
                                    inferred.vars.get(&def.name).cloned()
                                }
                                _ => None,
                            };
                            items.push(completion_item_with_detail(
                                &def.name,
                                def.kind.completion_kind(),
                                detail,
                            ));
                        }
                    }
                }
                let existing: HashSet<String> = items
                    .iter()
                    .filter_map(|item| item.get("label").and_then(Json::as_str))
                    .map(str::to_string)
                    .collect();
                for item in server.user_module_import_completions(&uri) {
                    let duplicate = item
                        .get("label")
                        .and_then(Json::as_str)
                        .map(|label| existing.contains(label))
                        .unwrap_or(false);
                    if !duplicate {
                        items.push(item);
                    }
                }
            }
            let result = Json::obj(vec![
                ("isIncomplete", Json::from(false)),
                ("items", Json::Arr(items)),
            ]);
            respond(out, id, result);
        }
        (Some(id), "textDocument/inlayHint") => {
            let uri = document_uri(&params).unwrap_or_default();
            let hints = if server.inlay_hints {
                server
                    .analyze_uri(&uri)
                    .map(|analysis| {
                        inlay_hints(&analysis)
                            .into_iter()
                            .filter(|hint| {
                                let label = hint
                                    .get("label")
                                    .and_then(Json::as_str)
                                    .unwrap_or("");
                                if label.starts_with(" -> ") {
                                    server.inlay_return_types
                                } else if label.starts_with(": int")
                                    || label.starts_with(": float")
                                    || label.starts_with(": str")
                                    || label.starts_with(": bool")
                                    || label.starts_with(": list")
                                    || label.starts_with(": dict")
                                    || label.starts_with(": set")
                                    || label.starts_with(": iter")
                                    || label.starts_with(": any")
                                {
                                    server.inlay_param_types || server.inlay_variable_types
                                } else {
                                    true
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            respond(out, id, Json::Arr(hints));
        }
        (Some(id), "textDocument/signatureHelp") => {
            let (uri, line, character) = match text_document_position(&params) {
                Some(position) => position,
                None => {
                    respond(out, id, Json::Null);
                    return;
                }
            };
            let result = server
                .analyze_uri(&uri)
                .and_then(|analysis| {
                    let offset = position_to_offset(&analysis.source, line, character);
                    signature_help(&analysis, offset)
                })
                .unwrap_or(Json::Null);
            respond(out, id, result);
        }
        (Some(id), "textDocument/definition") => {
            let (uri, line, character) = match text_document_position(&params) {
                Some(position) => position,
                None => {
                    respond(out, id, Json::Null);
                    return;
                }
            };
            let mut locations: Vec<Json> = Vec::new();
            if let Some(analysis) = server.analyze_uri(&uri) {
                let offset = position_to_offset(&analysis.source, line, character);
                if let Some((start, end)) = word_range_at(&analysis.source, offset) {
                    let word = analysis.source.span(start, end - start);
                    // `use a.b`: jump to the module file backing this segment.
                    if let Some((target, target_analysis, a, b)) =
                        server.use_segment_target(&uri, offset)
                    {
                        locations.push(Json::obj(vec![
                            ("uri", Json::from(target)),
                            ("range", span_range_json(&target_analysis.source, a, b)),
                        ]));
                    }
                    // `x.member`: jump to the member's declaration in the module.
                    if locations.is_empty() {
                        if let Some(receiver) = member_receiver_before(&analysis.source, start) {
                            if let Some((target, target_analysis, def)) =
                                server.module_member_target(&uri, &receiver, &word)
                            {
                                locations.push(Json::obj(vec![
                                    ("uri", Json::from(target)),
                                    (
                                        "range",
                                        span_range_json(
                                            &target_analysis.source,
                                            def.name_start,
                                            def.name_end,
                                        ),
                                    ),
                                ]));
                            }
                        }
                    }
                    // Preserve the original same-document behaviour as fallback.
                    if locations.is_empty() {
                        for def in analysis.defs.iter().filter(|def| def.name == word) {
                            locations.push(Json::obj(vec![
                                ("uri", Json::from(uri.as_str())),
                                (
                                    "range",
                                    span_range_json(&analysis.source, def.name_start, def.name_end),
                                ),
                            ]));
                        }
                    }
                }
            }
            respond(out, id, Json::Arr(locations));
        }
        (Some(id), "textDocument/hover") => {
            let (uri, line, character) = match text_document_position(&params) {
                Some(position) => position,
                None => {
                    respond(out, id, Json::Null);
                    return;
                }
            };
            let mut hover = None;
            if let Some(analysis) = server.analyze_uri(&uri) {
                let offset = position_to_offset(&analysis.source, line, character);
                if let Some((start, end)) = word_range_at(&analysis.source, offset) {
                    let word = analysis.source.span(start, end - start);
                    if let Some((target, _, _, _)) = server.use_segment_target(&uri, offset) {
                        let doc = docs::module(&word)
                            .map(str::to_string)
                            .unwrap_or_else(|| format!("`{word}` 모듈"));
                        hover = Some(hover_json(
                            format!("{doc}\n\n`{target}`"),
                            &analysis.source,
                            start,
                            end,
                        ));
                    }
                    if hover.is_none() {
                        if let Some(receiver) = member_receiver_before(&analysis.source, start) {
                            if let Some((target, target_analysis, def)) =
                                server.module_member_target(&uri, &receiver, &word)
                            {
                                let body = def_hover_markdown(&target_analysis, &def);
                                hover = Some(hover_json(
                                    format!("{body}\n_defined in `{target}`_"),
                                    &analysis.source,
                                    start,
                                    end,
                                ));
                            }
                        }
                    }
                    if hover.is_none() {
                        hover = analysis
                            .defs
                            .iter()
                            .find(|def| def.name == word)
                            .map(|def| {
                                hover_json(
                                    def_hover_markdown(&analysis, def),
                                    &analysis.source,
                                    start,
                                    end,
                                )
                            });
                    }
                    if hover.is_none() && native_catalog().module_names.contains(&word) {
                        let doc = docs::module(&word)
                            .map(str::to_string)
                            .unwrap_or_else(|| format!("`{word}` 모듈"));
                        hover = Some(hover_json(
                            format!("{doc}\n\n`{word}`"),
                            &analysis.source,
                            start,
                            end,
                        ));
                    }
                    if hover.is_none() {
                        hover = builtin_hover_markdown(&analysis, &word, start, end);
                    }
                }
            }
            respond(out, id, hover.unwrap_or(Json::Null));
        }
        (Some(id), "textDocument/documentSymbol") => {
            let uri = document_uri(&params).unwrap_or_default();
            let symbols = server
                .analyze_uri(&uri)
                .map(|analysis| symbols_for(&analysis))
                .unwrap_or_default();
            respond(out, id, Json::Arr(symbols));
        }
        (Some(id), "textDocument/semanticTokens/full") => {
            let uri = document_uri(&params).unwrap_or_default();
            let result = if server.semantic_highlighting {
                server
                    .analyze_uri(&uri)
                    .map(|analysis| semantic_tokens_json(&analysis))
                    .unwrap_or_else(|| Json::obj(vec![("data", Json::Arr(vec![]))]))
            } else {
                Json::obj(vec![("data", Json::Arr(vec![]))])
            };
            respond(out, id, result);
        }
        (Some(id), "textDocument/foldingRange") => {
            let uri = document_uri(&params).unwrap_or_default();
            let folds = server
                .analyze_uri(&uri)
                .and_then(|analysis| {
                    analysis
                        .tokens
                        .as_ref()
                        .map(|tokens| folding_ranges_json(&analysis.source, tokens))
                })
                .unwrap_or_default();
            respond(out, id, Json::Arr(folds));
        }
        (Some(id), "textDocument/references") => {
            let (uri, line, character) = match text_document_position(&params) {
                Some(position) => position,
                None => {
                    respond(out, id, Json::Arr(vec![]));
                    return;
                }
            };
            let locations = server
                .analyze_uri(&uri)
                .map(|analysis| {
                    let offset = position_to_offset(&analysis.source, line, character);
                    match symbol_scope_at(&analysis, offset) {
                        None => Vec::new(),
                        Some((name, scope)) => reference_sites(&analysis, &name, scope)
                            .into_iter()
                            .map(|(start, end)| {
                                Json::obj(vec![
                                    ("uri", Json::from(uri.as_str())),
                                    ("range", span_range_json(&analysis.source, start, end)),
                                ])
                            })
                            .collect(),
                    }
                })
                .unwrap_or_default();
            respond(out, id, Json::Arr(locations));
        }
        (Some(id), "textDocument/rename") => {
            let (uri, line, character) = match text_document_position(&params) {
                Some(position) => position,
                None => {
                    respond_error(out, id, -32602, "missing position");
                    return;
                }
            };
            let new_name = params
                .get("newName")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string();
            if !is_valid_identifier(&new_name) {
                respond_error(out, id, -32602, "newName must be a valid Un identifier");
                return;
            }
            let locations = server
                .analyze_uri(&uri)
                .map(|analysis| {
                    let offset = position_to_offset(&analysis.source, line, character);
                    match symbol_scope_at(&analysis, offset) {
                        None => Vec::new(),
                        Some((name, scope)) => reference_sites(&analysis, &name, scope)
                            .into_iter()
                            .map(|(start, end)| {
                                Json::obj(vec![
                                    ("range", span_range_json(&analysis.source, start, end)),
                                    ("newText", Json::from(new_name.as_str())),
                                ])
                            })
                            .collect(),
                    }
                })
                .unwrap_or_default();
            if locations.is_empty() {
                respond(out, id, Json::Null);
                return;
            }
            let mut changes = Vec::new();
            changes.push((uri, Json::Arr(locations)));
            respond(
                out,
                id,
                Json::obj(vec![("changes", Json::Obj(changes))]),
            );
        }
        (Some(id), _) => respond_error(out, id, -32601, "method not found"),
        (None, _) => {}
    }
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn content_length(header: &[u8]) -> Option<usize> {
    let text = std::str::from_utf8(header).ok()?;
    for line in text.split("\r\n") {
        let mut parts = line.splitn(2, ':');
        let name = parts.next()?.trim().to_ascii_lowercase();
        if name == "content-length" {
            return parts.next()?.trim().parse::<usize>().ok();
        }
    }
    None
}

fn take_message(buffer: &mut Vec<u8>) -> Option<String> {
    let header_end = find_header_end(buffer)?;
    let length = content_length(&buffer[..header_end])?;
    let total = header_end + 4 + length;
    if buffer.len() < total {
        return None;
    }
    let body = buffer[header_end + 4..total].to_vec();
    buffer.drain(..total);
    Some(String::from_utf8_lossy(&body).into_owned())
}

pub fn run() -> i32 {
    let mut server = Server::new();
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let mut stdout = io::stdout();
    let mut buffer: Vec<u8> = Vec::with_capacity(16 * 1024);
    let mut chunk = [0u8; 8192];

    loop {
        match input.read(&mut chunk) {
            Ok(0) => return server.exit_code.unwrap_or(0),
            Ok(count) => {
                buffer.extend_from_slice(&chunk[..count]);
                while let Some(body) = take_message(&mut buffer) {
                    let result = catch_unwind(AssertUnwindSafe(|| {
                        handle_message(&mut server, &mut stdout, &body)
                    }));
                    if let Err(panic) = result {
                        eprintln!("un-lsp: internal error: {panic:?}");
                    }
                    if let Some(code) = server.exit_code {
                        return code;
                    }
                }
            }
            Err(_) => return 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span_texts(analysis: &Analysis) -> Vec<String> {
        analysis
            .type_spans
            .iter()
            .map(|&(start, end)| analysis.source.code[start..end].iter().collect())
            .collect()
    }

    #[test]
    fn generic_params_and_bounds_are_type_tokens() {
        let analysis = analyze("fn add[T: numeric](a: T, b: T) -> T\n    -> a + b\n", "g.un");
        assert!(analysis.error.is_none(), "parse failed: {:?}", analysis.error);
        let params: Vec<&str> = analysis
            .defs
            .iter()
            .filter(|def| def.kind == DefKind::TypeParameter)
            .map(|def| def.name.as_str())
            .collect();
        assert_eq!(params, vec!["T"], "type parameter declaration missing");
        let spans = span_texts(&analysis);
        assert!(spans.iter().any(|text| text == "numeric"), "bound not coloured: {spans:?}");
        // Parameter annotations may be walked more than once; semantic tokens
        // are de-duplicated by source offset, so only presence matters here.
        assert!(spans.iter().filter(|text| *text == "T").count() >= 3, "annotation types missing: {spans:?}");
    }

    #[test]
    fn explicit_type_arguments_are_type_tokens() {
        let analysis = analyze("fn id[T](x: T) -> T\n    -> x\ny = id[int](3)\n", "g.un");
        assert!(analysis.error.is_none(), "parse failed: {:?}", analysis.error);
        let spans = span_texts(&analysis);
        assert!(spans.iter().any(|text| text == "int"), "explicit type argument missing: {spans:?}");
    }

    #[test]
    fn generic_enum_variant_types_are_coloured() {
        let analysis = analyze("enum Test[T]\n    a: int\n    b: T\n", "g.un");
        assert!(analysis.error.is_none(), "parse failed: {:?}", analysis.error);
        let spans = span_texts(&analysis);
        assert!(spans.iter().any(|text| text == "int"), "variant type missing: {spans:?}");
        assert!(spans.iter().any(|text| text == "T"), "generic variant type missing: {spans:?}");
    }

    #[test]
    fn fallback_keeps_generic_type_tokens() {
        let analysis = fallback_analysis("fn id[T](x: T) -> T\n    -> x\nfn broken(\n", "g.un", None);
        let spans = span_texts(&analysis);
        assert!(spans.iter().any(|text| text == "T"), "fallback type parameter missing: {spans:?}");
    }

    #[test]
    fn type_alias_is_a_definition() {
        let analysis = analyze(
            "type Id = int | str\nfn main() -> none\n    x: Id = 42\n",
            "alias.un",
        );
        assert!(analysis.error.is_none(), "parse failed: {:?}", analysis.error);
        let aliases: Vec<&str> = analysis
            .defs
            .iter()
            .filter(|def| def.kind == DefKind::TypeAlias)
            .map(|def| def.name.as_str())
            .collect();
        assert_eq!(aliases, vec!["Id"], "type alias definition missing");
        let spans = span_texts(&analysis);
        assert!(spans.iter().any(|text| text.contains("int")), "aliased type not coloured: {spans:?}");
        assert!(spans.iter().any(|text| text == "Id"), "alias use site not coloured: {spans:?}");
    }

    #[test]
    fn fallback_keeps_type_alias_def_without_eating_assignments() {
        let analysis = fallback_analysis(
            "type Id = int | str\ntype = \"hello\"\ntype(1)\nfn broken(\n",
            "alias.un",
            None,
        );
        let aliases: Vec<&str> = analysis
            .defs
            .iter()
            .filter(|def| def.kind == DefKind::TypeAlias)
            .map(|def| def.name.as_str())
            .collect();
        assert_eq!(aliases, vec!["Id"], "fallback type alias definition missing");
        assert_eq!(
            analysis.var_class_hints.get("type").map(String::as_str),
            Some("str"),
            "`type = \"hello\"` must still reach the assignment hint scan"
        );
    }

    #[test]
    fn match_snippet_uses_current_brace_syntax() {
        let (_, snippet, _) = KEYWORD_SNIPPETS
            .iter()
            .find(|(label, _, _)| *label == "match")
            .expect("match snippet missing");
        assert!(snippet.starts_with("match "), "bad snippet: {snippet}");
        assert!(snippet.contains('{') && snippet.contains('}'), "match body must use braces: {snippet}");
        assert!(!snippet.contains("case"), "stale `case` keyword in snippet: {snippet}");
    }

    #[test]
    fn union_generic_bound_colours_all_alternatives() {
        let analysis = analyze(
            "fn add[T: int | float](a: T, b: T) -> T\n    -> a + b\n",
            "g.un",
        );
        assert!(analysis.error.is_none(), "parse failed: {:?}", analysis.error);
        let spans = span_texts(&analysis);
        assert!(spans.iter().any(|text| text == "int"), "bound `int` missing: {spans:?}");
        assert!(spans.iter().any(|text| text == "float"), "bound `float` missing: {spans:?}");
    }

    #[test]
    fn fallback_union_generic_bound_colours_all_alternatives() {
        let analysis = fallback_analysis(
            "fn add[T: int | float](a: T, b: T) -> T\n    -> a + b\nfn broken(\n",
            "g.un",
            None,
        );
        let spans = span_texts(&analysis);
        assert!(spans.iter().any(|text| text == "int"), "bound `int` missing: {spans:?}");
        assert!(spans.iter().any(|text| text == "float"), "bound `float` missing: {spans:?}");
    }

    #[test]
    fn type_uses_are_navigable_references() {
        let analysis = analyze(
            "type Id = int | str\nfn main() -> none\n    x: Id = 42\n",
            "alias.un",
        );
        let use_names: Vec<&str> = analysis.uses.iter().map(|site| site.name.as_str()).collect();
        assert!(
            use_names.contains(&"Id"),
            "type annotation use not recorded: {use_names:?}"
        );
        let sites = reference_sites(&analysis, "Id", 1);
        assert!(sites.len() >= 2, "references must include the declaration and the use: {sites:?}");
    }

    #[test]
    fn match_pattern_bindings_are_arm_local_defs() {
        let analysis = analyze(
            "match x {\n    case Test.a(v) -> v + 1,\n    case y is int -> y,\n    _ -> 0\n}\n",
            "p.un",
        );
        assert!(analysis.error.is_none(), "parse failed: {:?}", analysis.error);
        let vars: Vec<&str> = analysis
            .defs
            .iter()
            .filter(|def| def.kind == DefKind::Variable)
            .map(|def| def.name.as_str())
            .collect();
        assert!(vars.contains(&"v"), "constructor pattern binding missing: {vars:?}");
        assert!(vars.contains(&"y"), "narrowed pattern binding missing: {vars:?}");
        let v_scope = analysis
            .defs
            .iter()
            .find(|def| def.kind == DefKind::Variable && def.name == "v")
            .expect("v def")
            .scope;
        assert_ne!(v_scope, 0, "pattern bindings must be arm-local, not top-level");
        let spans = span_texts(&analysis);
        assert!(spans.iter().any(|text| text == "int"), "narrowing type not coloured: {spans:?}");
        let sites = reference_sites(&analysis, "v", v_scope);
        assert!(sites.len() >= 2, "binding references must include body use: {sites:?}");
    }

    #[test]
    fn for_pattern_bindings_are_defs() {
        let analysis = analyze("for Test.a(v) in xs\n    io.write(v)\n", "p.un");
        assert!(analysis.error.is_none(), "parse failed: {:?}", analysis.error);
        assert!(
            analysis.defs.iter().any(|def| def.kind == DefKind::Variable && def.name == "v"),
            "for-pattern binding missing"
        );
    }

    #[test]
    fn semantic_legend_distinguishes_type_symbols() {
        assert_eq!(SEMANTIC_TOKEN_TYPES.len(), 11);
        assert_eq!(SEMANTIC_TOKEN_TYPES[SemanticToken::TypeParameter as usize], "typeParameter");
        assert_eq!(SEMANTIC_TOKEN_TYPES[SemanticToken::TypeAlias as usize], "typeAlias");
        assert_ne!(
            DefKind::TypeParameter.semantic_token(),
            DefKind::TypeAlias.semantic_token()
        );
    }

    #[test]
    fn bare_builtin_calls_are_function_tokens() {
        let analysis = analyze("io.write(type(1), len([1]))", "main.un");
        let call_token = |name: &str| {
            analysis
                .uses
                .iter()
                .find(|site| site.name == name)
                .map(|site| resolve_semantic(&analysis, site))
                .unwrap_or_else(|| panic!("missing call site: {name}"))
        };
        assert_eq!(call_token("type"), SemanticToken::Function as u32);
        assert_eq!(call_token("len"), SemanticToken::Function as u32);
        assert_eq!(call_token("write"), SemanticToken::Function as u32);

        let analysis = analyze("a = 1\nio.write(a)", "main.un");
        let site = analysis
            .uses
            .iter()
            .find(|site| site.name == "a")
            .expect("missing variable use");
        assert_eq!(resolve_semantic(&analysis, site), SemanticToken::Variable as u32);
    }

    #[test]
    fn native_catalog_tracks_registered_modules() {
        crate::native::init(""); // world registry is thread-local
        let catalog = native_catalog();
        let registered = unvm::runtime::world::native_module_names();
        assert_eq!(catalog.module_names.len(), registered.len(), "catalog drifted from registry");
        for name in &registered {
            assert!(catalog.module_names.contains(name), "module missing from catalog: {name}");
        }
        assert!(!namespace_member_completions("bytes").is_empty(), "bytes module missing");
    }

    #[test]
    fn function_hover_has_doc_and_signature() {
        let analysis = analyze(
            "# Adds two values\n# second line\nfn add(a: int, b: int) -> int\n    -> a + b\nx = add(1, 2)\n",
            "main.un",
        );
        let def = analysis
            .defs
            .iter()
            .find(|def| def.name == "add")
            .expect("function definition missing");
        let text = def_hover_markdown(&analysis, def);
        assert!(text.starts_with("Adds two values  \nsecond line\n\n"), "{text}");
        assert!(text.contains("```un\nadd(a: int, b: int) -> int\n```"), "{text}");
    }

    #[test]
    fn builtin_hover_has_doc() {
        let analysis = analyze("len([1])", "main.un");
        let hover = builtin_hover_markdown(&analysis, "len", 0, 3).expect("builtin hover missing");
        let body = hover
            .get("contents")
            .and_then(|contents| contents.get("value"))
            .and_then(Json::as_str)
            .unwrap_or("");
        assert!(body.starts_with("값의 길이(요소 수)를 반환합니다.\n\n```un\nlen("), "{body}");
    }

    #[test]
    fn completions_match_current_grammar() {
        for keyword in ["type", "case", "get", "set"] {
            assert!(KEYWORD_COMPLETIONS.contains(&keyword), "missing keyword: {keyword}");
        }
        assert!(!KEYWORD_COMPLETIONS.contains(&"return"), "`return` is not an Un keyword");
        for ty in ["bytes", "list", "set", "dict", "tuple"] {
            assert!(TYPE_COMPLETIONS.contains(&ty), "missing type: {ty}");
        }
        for (label, _, _) in KEYWORD_SNIPPETS {
            assert!(*label != "return", "`return` snippet is invalid Un syntax");
        }
    }

    #[test]
    fn use_declarations_keep_full_paths_and_bindings() {
        let analysis = analyze(
            "use test.assert as a\nuse io\nuse test.fixtures.m.*\n",
            "main.un",
        );
        assert!(analysis.error.is_none(), "parse failed: {:?}", analysis.error);
        assert_eq!(analysis.module_uses.len(), 3);
        let first = &analysis.module_uses[0];
        assert_eq!(first.parts, vec!["test", "assert"]);
        assert_eq!(first.binding(), Some("a"));
        let text: Vec<String> = first
            .segments
            .iter()
            .map(|&(start, end)| analysis.source.span(start, end - start))
            .collect();
        assert_eq!(text, vec!["test", "assert"]);
        let second = &analysis.module_uses[1];
        assert_eq!(second.parts, vec!["io"]);
        assert_eq!(second.binding(), Some("io"));
        let third = &analysis.module_uses[2];
        assert_eq!(third.parts, vec!["test", "fixtures", "m"]);
        assert!(third.wildcard, "wildcard import flag missing");
    }

    #[test]
    fn fallback_keeps_module_use_data() {
        let analysis = fallback_analysis(
            "use pkg.util as u\nfn broken(\n",
            "main.un",
            None,
        );
        assert_eq!(analysis.module_uses.len(), 1);
        assert_eq!(analysis.module_uses[0].parts, vec!["pkg", "util"]);
        assert_eq!(analysis.module_uses[0].binding(), Some("u"));
    }

    #[test]
    fn user_modules_resolve_for_completion_and_navigation() {
        let root = std::env::temp_dir().join(format!("un-lsp-mod-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let package = root.join("pkg");
        std::fs::create_dir_all(&package).expect("temp module dir");
        let module = package.join("util.un");
        std::fs::write(
            &module,
            "fn helper(x: int) -> int\n    -> x + 1\nvalue = 42\n",
        )
        .expect("temp module file");
        let main = root.join("main.un");
        std::fs::write(&main, "use pkg.util as u\nv = u.helper(1)\n").expect("temp main file");
        let main_uri = path_to_uri(&main);
        let mut server = Server::new();
        server.workspace_roots.push(root.clone());
        server
            .documents
            .insert(main_uri.clone(), std::fs::read_to_string(&main).expect("read main"));

        let analysis = server.analyze_uri(&main_uri).expect("main analysis");
        assert!(analysis.error.is_none(), "parse failed: {:?}", analysis.error);
        assert_eq!(server.resolved_uses(&main_uri).map(|uses| uses.len()), Some(1));

        let items = server
            .user_module_completions("u", &main_uri)
            .expect("user module members missing");
        let labels: Vec<&str> = items
            .iter()
            .filter_map(|item| item.get("label").and_then(Json::as_str))
            .collect();
        assert!(labels.contains(&"helper"), "fn not completed: {labels:?}");
        assert!(labels.contains(&"value"), "variable not completed: {labels:?}");

        let (target, target_analysis, def) = server
            .module_member_target(&main_uri, "u", "helper")
            .expect("cross-file member definition missing");
        assert_eq!(target, path_to_uri(&module));
        assert_eq!(def.name, "helper");
        assert!(target_analysis.source.get_line_text(def.start).contains("fn helper"));

        // `pkg` (a directory module) resolves to its only `.un` file.
        let (target, _, _, _) = server
            .use_segment_target(&main_uri, 4)
            .expect("use path definition missing");
        assert_eq!(target, path_to_uri(&module));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_user_module_is_diagnosed() {
        let root = std::env::temp_dir().join(format!("un-lsp-missing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("temp dir");
        let main = root.join("main.un");
        let text = "use nowhere.to_be_found as missing\n";
        std::fs::write(&main, text).expect("temp main file");
        let main_uri = path_to_uri(&main);
        let mut server = Server::new();
        server.workspace_roots.push(root.clone());
        server.documents.insert(main_uri.clone(), text.to_string());
        let diagnostics = server.diagnostics_for(&main_uri);
        assert!(
            diagnostics.iter().any(|item| item
                .get("message")
                .and_then(Json::as_str)
                .map(|message| message.contains("module 'nowhere.to_be_found' not found"))
                .unwrap_or(false)),
            "missing module diagnostic absent: {diagnostics:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn relative_use_depth_is_recorded_from_ast_and_fallback() {
        // FE-01's parser keeps identifier spans and stores the depth in a
        // Const::Str value. Build that shape directly so the LSP side is
        // covered even when the parser change is not in this worktree yet.
        let text = "use .util as u\n";
        let source = Source::new("main.un", text);
        let path = Node::new(5, 4, NodeKind::Identifier)
            .with_value(unvm::ast::Const::Str(".util".to_string()));
        let alias = Node::new(13, 1, NodeKind::Identifier);
        let node = Node::with_children(0, 14, NodeKind::Use, None, vec![path, alias]);
        let usage = module_use_from_node(&node, &source).expect("relative use node");
        assert_eq!(usage.depth, 1);
        assert_eq!(usage.parts, vec!["util"]);
        assert_eq!(usage.binding(), Some("u"));
        assert_eq!(source.span(usage.path_start, 5 - usage.path_start), ".");
        let segments: Vec<String> = usage
            .segments
            .iter()
            .map(|&(start, end)| source.span(start, end - start))
            .collect();
        assert_eq!(segments, vec!["util"]);

        let analysis = fallback_analysis(
            "use ..pkg.util as p\nfn broken(\n",
            "main.un",
            None,
        );
        assert_eq!(analysis.module_uses.len(), 1);
        let usage = &analysis.module_uses[0];
        assert_eq!(usage.depth, 2);
        assert_eq!(usage.parts, vec!["pkg", "util"]);
        assert_eq!(usage.binding(), Some("p"));
        assert_eq!(
            analysis.source.span(usage.path_start, usage.path_end - usage.path_start),
            "..pkg.util"
        );
        let segments: Vec<String> = usage
            .segments
            .iter()
            .map(|&(start, end)| analysis.source.span(start, end - start))
            .collect();
        assert_eq!(segments, vec!["pkg", "util"]);
    }

    #[test]
    fn relative_uses_resolve_from_document_directory_only() {
        let root = std::env::temp_dir().join(format!("un-lsp-relative-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let sub = root.join("work").join("sub");
        let pkg = root.join("work").join("pkg");
        let decoy = root.join("decoy");
        std::fs::create_dir_all(&sub).expect("temp sub dir");
        std::fs::create_dir_all(&pkg).expect("temp pkg dir");
        std::fs::create_dir_all(&decoy).expect("temp decoy dir");
        let sibling = sub.join("util.un");
        let parent = pkg.join("util.un");
        std::fs::write(&sibling, "fn helper(x: int) -> int\n    -> x + 1\n")
            .expect("sibling module");
        std::fs::write(&parent, "fn other(x: int) -> int\n    -> x + 2\n")
            .expect("parent module");
        std::fs::write(decoy.join("util.un"), "fn decoy() -> int\n    -> 0\n")
            .expect("decoy module");
        let main = sub.join("main.un");
        let text = "use .util as u\nuse ..pkg.util as p\nv = u.helper(1)\nw = p.other(2)\n";
        std::fs::write(&main, text).expect("temp main file");
        let main_uri = path_to_uri(&main);
        let mut server = Server::new();
        // A decoy workspace root must never win over the explicit dot prefix.
        server.workspace_roots.push(decoy);
        server.documents.insert(main_uri.clone(), text.to_string());
        server.analyze_uri(&main_uri).expect("main analysis");

        let uses = server.resolved_uses(&main_uri).expect("relative uses missing");
        assert_eq!(uses.len(), 2);
        assert_eq!(uses[0].decl.depth, 1);
        assert_eq!(uses[1].decl.depth, 2);
        assert_eq!(path_key(&uses[0].files[0]), path_key(&sibling));
        assert_eq!(path_key(&uses[1].files[0]), path_key(&parent));

        let items = server.user_module_completions("u", &main_uri).expect("`.util` completion");
        let labels: Vec<&str> = items
            .iter()
            .filter_map(|item| item.get("label").and_then(Json::as_str))
            .collect();
        assert!(labels.contains(&"helper"), "sibling member missing: {labels:?}");
        assert!(!labels.contains(&"decoy"), "workspace fallback leaked: {labels:?}");
        let items = server.user_module_completions("p", &main_uri).expect("`..pkg` completion");
        let labels: Vec<&str> = items
            .iter()
            .filter_map(|item| item.get("label").and_then(Json::as_str))
            .collect();
        assert!(labels.contains(&"other"), "parent member missing: {labels:?}");

        let (target, _, def) = server
            .module_member_target(&main_uri, "u", "helper")
            .expect("sibling definition missing");
        assert_eq!(target, path_to_uri(&sibling));
        assert_eq!(def.name, "helper");
        let (target, _, def) = server
            .module_member_target(&main_uri, "p", "other")
            .expect("parent definition missing");
        assert_eq!(target, path_to_uri(&parent));
        assert_eq!(def.name, "other");

        // Navigating the intermediate `pkg` segment also walks up one level.
        let (target, _, _, _) = server
            .use_segment_target(&main_uri, text.find("pkg").expect("pkg segment"))
            .expect("relative segment definition missing");
        assert_eq!(target, path_to_uri(&parent));

        let _ = std::fs::remove_dir_all(&root);
    }
}
