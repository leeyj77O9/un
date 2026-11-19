//! `tir` ??compile-mode Typed IR and core-function eligibility.
//!
//! Compile mode lowers a small, statically typed *core subset* of Un to a
//! direct-to-hardware representation (`TirProgram`). Everything outside the
//! subset stays with the VM interpreter; the ordinary `UV_JIT` path can
//! additionally compile the `.unbc` Int subset at runtime, unchanged.
//!
//! The core subset is intentionally narrow and matches `docs/compile/COMPILE_DESIGN.md`:
//! params and locals of `int`, `float`, `bool` (or a `none` return), no
//! dynamic boundary (`any`, classes, collections, natives, aliases), no
//! `match`/`try`/`for` over non-`range` iterators. Core functions are
//! compiled to Cranelift and registered into `jit_cache`; the VM calls them
//! transparently through the same `try_call_jit` path as the bytecode JIT.
//!
//! Invariants enforced by `validate` (defense in depth ??the checks are
//! duplicated deliberately so a bug in the builder cannot reach the FFI
//! boundary unchecked):
//!
//! - every local is declared before use, with a consistent type,
//! - the return statement matches the declared return type,
//! - every loop is `while` or `for` over `iter.range`,
//! - calls target only other core functions,
//! - boolean conditions, integer range bounds,
//! - terminating control flow (a non-`none` return type means every
//!   fallthrough path returns).

use crate::ast::{Const, Node, NodeKind};
use crate::source::Source;
use crate::token::TokenType;
use crate::typeck::{CheckedProgram, FnCheck, Ty};
use std::collections::HashMap;

/// The scalar (unboxed) types supported by the core subset.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TirTy {
    Int,
    Float,
    Bool,
    Str,
    ListInt,
    ListFloat,
    DictStrInt,
    DictStrFloat,
    /// A class instance (Obj handle). The u16 indexes
    /// `TirProgram::class_names`.
    Class(u16),
    /// The `catch err` binding: a raw VM error object, type-erased.
    ErrorObj,
}

impl TirTy {
    fn from_ty(ty: &Ty) -> Option<TirTy> {
        match ty {
            Ty::Int => Some(TirTy::Int),
            Ty::Float => Some(TirTy::Float),
            Ty::Bool => Some(TirTy::Bool),
            Ty::Str => Some(TirTy::Str),
            Ty::List(elem) => match elem.as_ref() {
                Ty::Int => Some(TirTy::ListInt),
                Ty::Float => Some(TirTy::ListFloat),
                _ => None,
            },
            Ty::Dict(key, value) => match key.as_ref() {
                Ty::Str => match value.as_ref() {
                    Ty::Int => Some(TirTy::DictStrInt),
                    Ty::Float => Some(TirTy::DictStrFloat),
                    _ => None,
                },
                _ => None,
            },
            Ty::Class(_) => Some(TirTy::Class(u16::MAX)),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TirBinOp {
    Add,
    Sub,
    Mul,
    /// `//` ??truncated integer division (wraps on overflow via guard).
    IDiv,
    /// `/` ??float division (operands promoted).
    Div,
    Mod,
    BAnd,
    BOr,
    BXor,
    Shl,
    Shr,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TirCmp {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TirUnOp {
    Neg,
    Not,
}

#[derive(Clone, Debug)]
pub enum TirExpr {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Local(u16),
    /// i64 ??f64 promotion (int operand used where a float is required).
    Promote(Box<TirExpr>),
    Select {
        cond: Box<TirExpr>,
        then_expr: Box<TirExpr>,
        else_expr: Box<TirExpr>,
    },
    Bin(TirBinOp, Box<TirExpr>, Box<TirExpr>),
    Cmp(TirCmp, Box<TirExpr>, Box<TirExpr>),
    Un(TirUnOp, Box<TirExpr>),
    /// A list of numbers, constructed inline (all elements same type).
    List(Vec<TirExpr>),
    /// A string-keyed dictionary with homogeneous int/float values.
    Dict(Vec<(TirExpr, TirExpr)>),
    /// `len(x)` for Str/List/Dict.
    Len(Box<TirExpr>),
    /// `xs.pop()` pops and returns the last element.
    ListPop(Box<TirExpr>),
    /// `xs.pop(i)` pops and returns the element at index `i`.
    ListPopAt(Box<TirExpr>, Box<TirExpr>),
    /// `xs.contains(v)`
    ListContains(Box<TirExpr>, Box<TirExpr>),
    /// `xs.index_of(v)` ??position of `v` or -1.
    ListIndexOf(Box<TirExpr>, Box<TirExpr>),
    /// `dict[key]` where `key` is a string expression.
    DictGet(Box<TirExpr>, Box<TirExpr>),
    /// `list[idx]` where `idx` is an int expression. Negative indices wrap.
    ListGet(Box<TirExpr>, Box<TirExpr>),
    /// `Name(args...)` where `Name` resolves to a class constructor.
    ClassNew { name: Box<str>, args: Vec<TirExpr>, ret: TirTy },
    /// `receiver.member` where `receiver` is class-typed.
    MemberGet { receiver: Box<TirExpr>, member: Box<str>, ret: TirTy },
    /// `receiver.method(args...)` where `receiver` is class-typed.
    MethodCall { receiver: Box<TirExpr>, member: Box<str>, args: Vec<TirExpr>, ret: TirTy },
    /// Cast `int(x)` where `x` is Int or Float.
    CastInt(Box<TirExpr>),
    /// Cast `float(x)` where `x` is Int or Float.
    CastFloat(Box<TirExpr>),
    /// `math.<intrinsic>(args...)` — single/double-precision float intrinsics.
    Math(MathFn, Vec<TirExpr>),
    /// Index into `TirProgram::functions`.
    Call(u32, Vec<TirExpr>),
}

/// Math intrinsics lowered to the corresponding `un_math_*` FFI symbol
/// (`native/math.rs`). `Log` carries the two-argument `esp`/`base` call:
/// `math.log(v, b)`. `Pow` is `math.pow(v, e)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathFn {
    Sin, Cos, Tan, Asin, Acos, Atan, Sqrt, Cbrt, Exp, Ln, Log2, Log10,
    /// `ln(v)` when arity is 1; the `v`/`base` pair otherwise.
    Log, Floor, Ceil, Trunc, Abs, Pow,
}

#[derive(Clone, Debug)]
pub enum TirStmt {
    /// First binding of a local.
    Let(u16, TirExpr),
    /// Overwrite of a previously bound local.
    Set(u16, TirExpr),
    If {
        cond: TirExpr,
        then_body: Vec<TirStmt>,
        else_body: Vec<TirStmt>,
    },
    While {
        cond: TirExpr,
        body: Vec<TirStmt>,
    },
    /// `for var in collection` where collection is `list[int]`/`list[float]`.
    ForEach {
        var: u16,
        collection: TirExpr,
        body: Vec<TirStmt>,
    },
    /// `xs[i] = value` (list element assignment, negative indices wrap).
    ListSet {
        list: TirExpr,
        index: TirExpr,
        value: TirExpr,
    },
    /// `d["k"] = value` (dict insert/overwrite).
    DictSet {
        dict: TirExpr,
        key: TirExpr,
        value: TirExpr,
    },
    /// `receiver.member = value` where `receiver` is class-typed.
    MemberSet {
        receiver: TirExpr,
        member: Box<str>,
        value: TirExpr,
    },
    /// `xs.add(v)` on `list[int]`/`list[float]`.
    ListAdd { receiver: TirExpr, arg: TirExpr },
    /// `xs.insert(v, i)` on `list[int]`/`list[float]`.
    ListInsert { receiver: TirExpr, arg: TirExpr, index: TirExpr },
    /// `xs.reverse()` in place.
    ListReverse { receiver: TirExpr },
    /// `xs.sort()` in place by the VM's comparator.
    ListSort { receiver: TirExpr },
    /// `try body catch err? catch_body finally? final_body`.
    /// Error objects reached from the bridge calls are bound to `var`
    /// (a raw Obj slot) when given; otherwise dropped. `finally` always
    /// runs when control leaves the try, both via the success path and
    /// the catch path (matching the interpreter).
    Try {
        body: Vec<TirStmt>,
        var: Option<u16>,
        catch_body: Vec<TirStmt>,
        finally_body: Option<Vec<TirStmt>>,
    },
    /// `throw(expr)` ??park `expr` (any core value) in the pending-error
    /// slot; control moves to the nearest enclosing catch label (or the
    /// unguarded exit of the function).
    Throw(TirExpr),
    ForRange {
        var: u16,
        start: TirExpr,
        end: TirExpr,
        step: TirExpr,
        body: Vec<TirStmt>,
    },
    Break,
    Skip,
    Return(Option<TirExpr>),
    /// Expression statement ??must be a call.
    ExprStmt(TirExpr),
    /// A call to a registered native module member, e.g. `io.write(x)`.
    /// Lowered to `unrt_call_native(module, member, args...)`.
    NativeCall {
        module: String,
        member: String,
        args: Vec<TirExpr>,
    },
}

#[derive(Clone, Debug)]
pub struct TirFn {
    pub name: String,
    pub proto_span: (usize, usize),
    pub index: usize,
    pub params: Vec<TirTy>,
    /// Return type; `None` means the function returns `none`.
    pub ret: Option<TirTy>,
    /// Local types, param slots first, then declaration order.
    pub locals: Vec<TirTy>,
    pub body: Vec<TirStmt>,
}

pub struct TirProgram {
    pub functions: Vec<TirFn>,
    /// Class name table for `TirTy::Class` ids, ordered lexicographically.
    pub class_names: Vec<String>,
    /// Field name ??declared type for each interned class (parallel to `class_names`).
    pub class_fields: Vec<std::collections::HashMap<String, Ty>>,
    /// Method name ??`Ty::Func` for each interned class (parallel to `class_names`).
    pub class_methods: Vec<std::collections::HashMap<String, Ty>>,
    /// Members declared with a `set` accessor, inherited from bases
    /// (parallel to `class_names`).
    pub class_setters: Vec<std::collections::HashSet<String>>,
}

/// The class tables gathered from the checked program, threaded through
/// lowering. Kept separate so every lowering function can resolve class
/// types without re-walking the AST.
struct ClassTable {
    /// class name ??class id (index into `names`/`fields`/`methods`).
    index: std::collections::HashMap<String, u16>,
    names: Vec<String>,
    fields: Vec<std::collections::HashMap<String, Ty>>,
    methods: Vec<std::collections::HashMap<String, Ty>>,
    setters: Vec<std::collections::HashSet<String>>,
}

impl ClassTable {
    fn new(checked: &CheckedProgram) -> Self {
        let mut names: Vec<String> = checked.classes.keys().cloned().collect();
        names.sort_unstable();
        let index: std::collections::HashMap<String, u16> = names
            .iter()
            .enumerate()
            .map(|(i, n)| (n.clone(), i as u16))
            .collect();
        // Merge member tables through the class chain (own declaration
        // wins), so base-class methods are visible on derived classes.
        fn resolve<'a>(
            checked: &'a CheckedProgram,
            name: &str,
            cache: &mut std::collections::HashMap<String, std::collections::HashMap<String, Ty>>,
        ) -> std::collections::HashMap<String, Ty> {
            if let Some(cached) = cache.get(name) {
                return cached.clone();
            }
            let mut merged: std::collections::HashMap<String, Ty> =
                std::collections::HashMap::new();
            if let Some(info) = checked.classes.get(name) {
                for base in info.bases.iter() {
                    // Compute (and store) the base table first so multiple
                    // uses of the same base share one clone.
                    resolve(checked, base, cache);
                }
                for base in info.bases.iter() {
                    if let Some(m) = cache.get(base.as_str()) {
                        for (k, v) in m {
                            merged.entry(k.clone()).or_insert_with(|| v.clone());
                        }
                    }
                }
                // Own entries always win.
                for (k, v) in &info.methods {
                    merged.insert(k.clone(), v.clone());
                }
            }
            cache.insert(name.to_string(), merged.clone());
            merged
        }
        let mut merged_cache: std::collections::HashMap<
            String,
            std::collections::HashMap<String, Ty>,
        > = std::collections::HashMap::new();
        // Merge setter tables through the class chain the same way.
        fn resolve_setters<'a>(
            checked: &'a CheckedProgram,
            name: &str,
            cache: &mut std::collections::HashMap<String, std::collections::HashSet<String>>,
        ) -> std::collections::HashSet<String> {
            if let Some(cached) = cache.get(name) {
                return cached.clone();
            }
            let mut merged: std::collections::HashSet<String> = std::collections::HashSet::new();
            if let Some(info) = checked.classes.get(name) {
                for base in info.bases.iter() {
                    resolve_setters(checked, base, cache);
                }
                for base in info.bases.iter() {
                    if let Some(m) = cache.get(base.as_str()) {
                        for s in m {
                            merged.insert(s.clone());
                        }
                    }
                }
                for s in &info.setters {
                    merged.insert(s.clone());
                }
            }
            cache.insert(name.to_string(), merged.clone());
            merged
        }
        let mut setters_cache: std::collections::HashMap<
            String,
            std::collections::HashSet<String>,
        > = std::collections::HashMap::new();
        let fields = names
            .iter()
            .map(|n| checked.classes[n].fields.clone())
            .collect();
        let methods = names
            .iter()
            .map(|n| resolve(checked, n, &mut merged_cache))
            .collect();
        let setters = names
            .iter()
            .map(|n| resolve_setters(checked, n, &mut setters_cache))
            .collect();
        ClassTable { index, names, fields, methods, setters }
    }

    /// Look up the setter set resolved for class id `id`.
    fn setters(&self, id: u16) -> Option<&std::collections::HashSet<String>> {
        self.setters.get(id as usize)
    }

    fn id(&self, name: &str) -> Option<u16> {
        self.index.get(name).copied()
    }

    /// Look up a field's declared type on class `id`.
    fn field(&self, id: u16, name: &str) -> Option<&Ty> {
        self.fields.get(id as usize)?.get(name)
    }

    /// Look up a method's `Ty::Func` on class `id`.
    fn method(&self, id: u16, name: &str) -> Option<&Ty> {
        self.methods.get(id as usize)?.get(name)
    }
}

/// Errors encountered while lowering. Checked programs are guaranteed to be
/// well-typed, so any error here is an internal bug; the CLI reports it as
/// a hard error, not a panic.
#[derive(Debug)]
pub struct TirError(pub String);

impl std::fmt::Display for TirError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for TirError {}

const MAX_CORE_ARITY: usize = 8;



/// Structural equality for TIR types: classes compare by "any class instance"
/// (the checker has already validated assignability), everything else is
/// exact.
fn same_ty(a: TirTy, b: TirTy) -> bool {
    match (a, b) {
        (TirTy::Class(_), TirTy::Class(_)) => true,
        _ => a == b,
    }
}

/// Walk the AST collecting every `fn` declaration node (the same nodes the
/// compiler turns into protos).
pub fn collect_fn_nodes(root: &Node) -> Vec<&Node> {
    fn visit<'a>(node: &'a Node, out: &mut Vec<&'a Node>) {
        match node.kind {
            NodeKind::Function | NodeKind::Extension => {
                out.push(node);
                if let Some(body) = node.children.iter().find(|c| c.kind == NodeKind::Block) {
                    visit(body, out);
                }
            }
            NodeKind::Class => {
                if let Some(body) = node.children.iter().find(|c| c.kind == NodeKind::Block) {
                    for child in &body.children {
                        visit(child, out);
                    }
                }
            }
            NodeKind::Enum | NodeKind::Use => {}
            _ => {
                for child in &node.children {
                    visit(child, out);
                }
            }
        }
    }
    let mut out = Vec::new();
    visit(root, &mut out);
    out
}

/// Eligibility for the core subset: params/locals/return must all be
/// `int`/`float`/`bool` (or a `none` return). Collections, classes, unions,
/// `any`, and methods are excluded. Body contents are checked by
/// `body_is_core`.
/// True when the function mentions `Str` anywhere. The in-process (Vm) JIT
/// cannot call the VM runtime for these operations, so those functions are
/// left to the interpreter. Object-mode (`un obj`) fully supports `Str`.
pub fn fn_uses_str(f: &TirFn) -> bool {
    if f.locals.iter().any(|t| matches!(t, TirTy::Str)) {
        return true;
    }
    f.body.iter().any(stmt_uses_str)
}

/// True when the function mentions a class-typed value anywhere: class-typed
/// locals, class construction, or instance member access. `un build`/`un obj`
/// reject these before emission (the native exe has no class registry to
/// register an instance against); `un build --embed` handles full programs.
pub fn fn_uses_classes(f: &TirFn) -> bool {
    if f
        .locals
        .iter()
        .any(|t| matches!(t, TirTy::Class(_)))
    {
        return true;
    }
    f.body.iter().any(stmt_uses_classes)
}

fn stmt_uses_classes(s: &TirStmt) -> bool {
    match s {
        TirStmt::Let(_, e) | TirStmt::Set(_, e) | TirStmt::ExprStmt(e) => expr_uses_classes(e),
        TirStmt::If { cond, then_body, else_body } => {
            expr_uses_classes(cond) || then_body.iter().any(stmt_uses_classes) || else_body.iter().any(stmt_uses_classes)
        }
        TirStmt::While { cond, body } => {
            expr_uses_classes(cond) || body.iter().any(stmt_uses_classes)
        }
        TirStmt::ForRange { body, .. } => body.iter().any(stmt_uses_classes),
        TirStmt::ForEach { collection, body, .. } => {
            expr_uses_classes(collection) || body.iter().any(stmt_uses_classes)
        }
        TirStmt::ListSet { list, index, value } => {
            expr_uses_classes(list) || expr_uses_classes(index) || expr_uses_classes(value)
        }
        TirStmt::ListAdd { receiver, arg } => expr_uses_classes(receiver) || expr_uses_classes(arg),
        TirStmt::ListInsert { receiver, arg, index } => {
            expr_uses_classes(receiver) || expr_uses_classes(arg) || expr_uses_classes(index)
        }
        TirStmt::ListReverse { receiver } | TirStmt::ListSort { receiver } => expr_uses_classes(receiver),
        TirStmt::DictSet { dict, key, value } => {
            expr_uses_classes(dict) || expr_uses_classes(key) || expr_uses_classes(value)
        }
        TirStmt::MemberSet { receiver, value, .. } => {
            expr_uses_classes(receiver) || expr_uses_classes(value)
        }
        TirStmt::NativeCall { args, .. } => args.iter().any(expr_uses_classes),
        TirStmt::Try { body, catch_body, finally_body, .. } => {
            body.iter().any(stmt_uses_classes)
                || catch_body.iter().any(stmt_uses_classes)
                || finally_body
                    .as_ref()
                    .is_some_and(|f| f.iter().any(stmt_uses_classes))
        }
        TirStmt::Throw(e) => expr_uses_classes(e),
        TirStmt::Return(None) => false,
        TirStmt::Return(Some(e)) => expr_uses_classes(e),
        TirStmt::Break | TirStmt::Skip => false,
    }
}

fn expr_uses_classes(e: &TirExpr) -> bool {
    match e {
        TirExpr::ClassNew { .. } | TirExpr::MemberGet { .. } | TirExpr::MethodCall { .. } => true,
        TirExpr::Promote(inner) | TirExpr::Un(_, inner) => expr_uses_classes(inner),
        TirExpr::Select { cond, then_expr, else_expr } => {
            expr_uses_classes(cond) || expr_uses_classes(then_expr) || expr_uses_classes(else_expr)
        }
        TirExpr::Bin(_, lhs, rhs) | TirExpr::Cmp(_, lhs, rhs) => {
            expr_uses_classes(lhs) || expr_uses_classes(rhs)
        }
        TirExpr::Call(_, args) => args.iter().any(expr_uses_classes),
        TirExpr::List(elements) => elements.iter().any(expr_uses_classes),
        TirExpr::Math(_, args) => args.iter().any(expr_uses_classes),
        TirExpr::CastInt(inner) | TirExpr::CastFloat(inner) => expr_uses_classes(inner),
        TirExpr::Dict(entries) => entries.iter().any(|(k, v)| expr_uses_classes(k) || expr_uses_classes(v)),
        TirExpr::DictGet(a, b) | TirExpr::ListGet(a, b) => {
            expr_uses_classes(a) || expr_uses_classes(b)
        }
        TirExpr::Len(inner) => expr_uses_classes(inner),
        TirExpr::ListPop(receiver) => expr_uses_classes(receiver),
        TirExpr::ListPopAt(receiver, _) => expr_uses_classes(receiver),
        TirExpr::ListContains(receiver, _) | TirExpr::ListIndexOf(receiver, _) => {
            expr_uses_classes(receiver)
        }
        TirExpr::Int(_) | TirExpr::Float(_) | TirExpr::Bool(_) | TirExpr::Str(_) | TirExpr::Local(_) => false,
    }
}

/// True when the function mentions a heap collection (`list[int]`/`list[float]`
/// or `dict[str, int]`/`dict[str, float]`) type or value anywhere. These are
/// heap objects that only the Object-mode (`un obj`) bridge can materialize.
pub fn fn_uses_heap(f: &TirFn) -> bool {
    if f.locals
        .iter()
        .any(|t| matches!(t, TirTy::ListInt | TirTy::ListFloat | TirTy::DictStrInt | TirTy::DictStrFloat | TirTy::Class(_)))
    {
        return true;
    }
    f.body.iter().any(stmt_uses_heap)
}

fn stmt_uses_heap(s: &TirStmt) -> bool {
    match s {
        TirStmt::Let(_, e) | TirStmt::Set(_, e) | TirStmt::ExprStmt(e) => expr_uses_heap(e),
        TirStmt::If { cond, then_body, else_body } => {
            expr_uses_heap(cond) || then_body.iter().any(stmt_uses_heap) || else_body.iter().any(stmt_uses_heap)
        }
        TirStmt::While { cond, body } => {
            expr_uses_heap(cond) || body.iter().any(stmt_uses_heap)
        }
        TirStmt::ForRange { body, .. } => body.iter().any(stmt_uses_heap),
        TirStmt::ForEach { collection, body, .. } => {
            expr_uses_heap(collection) || body.iter().any(stmt_uses_heap)
        }
        TirStmt::ListSet { list, index, value } => {
            expr_uses_heap(list) || expr_uses_heap(index) || expr_uses_heap(value)
        }
        TirStmt::ListAdd { receiver, arg } => expr_uses_heap(receiver) || expr_uses_heap(arg),
        TirStmt::ListInsert { receiver, arg, index } => {
            expr_uses_heap(receiver) || expr_uses_heap(arg) || expr_uses_heap(index)
        }
        TirStmt::ListReverse { receiver } | TirStmt::ListSort { receiver } => expr_uses_heap(receiver),
        TirStmt::DictSet { dict, key, value } => {
            expr_uses_heap(dict) || expr_uses_heap(key) || expr_uses_heap(value)
        }
        TirStmt::MemberSet { receiver, value, .. } => expr_uses_heap(receiver) || expr_uses_heap(value),
        TirStmt::NativeCall { args, .. } => args.iter().any(expr_uses_heap),
        TirStmt::Try { .. } => true,
        TirStmt::Throw(_) => true,
        TirStmt::Return(None) => false,
        TirStmt::Return(Some(e)) => expr_uses_heap(e),
        TirStmt::Break | TirStmt::Skip => false,
    }
}

fn expr_uses_heap(e: &TirExpr) -> bool {
    match e {
        TirExpr::List(_) | TirExpr::Dict(_) | TirExpr::DictGet(_, _) | TirExpr::ListGet(_, _)
        | TirExpr::ClassNew { .. } | TirExpr::MemberGet { .. } | TirExpr::MethodCall { .. } => true,
        TirExpr::Promote(inner)
        | TirExpr::Un(_, inner) => expr_uses_heap(inner),
        TirExpr::Select { cond, then_expr, else_expr } => {
            expr_uses_heap(cond) || expr_uses_heap(then_expr) || expr_uses_heap(else_expr)
        }
        TirExpr::Bin(_, lhs, rhs) | TirExpr::Cmp(_, lhs, rhs) => {
            expr_uses_heap(lhs) || expr_uses_heap(rhs)
        }
        TirExpr::Call(_, args) => args.iter().any(expr_uses_heap),
        TirExpr::Int(_) | TirExpr::Float(_) | TirExpr::Bool(_) | TirExpr::Str(_) | TirExpr::Local(_) => false,
        TirExpr::Math(_, _) => false,
        TirExpr::CastInt(inner) | TirExpr::CastFloat(inner) => {
            expr_uses_heap(inner)
        }
        TirExpr::Len(_)
        | TirExpr::ListPop(_)
        | TirExpr::ListPopAt(_, _)
        | TirExpr::ListContains(_, _)
        | TirExpr::ListIndexOf(_, _) => true,
    }
}

fn stmt_uses_str(s: &TirStmt) -> bool {
    match s {
        TirStmt::Let(_, e) | TirStmt::Set(_, e) | TirStmt::ExprStmt(e) => expr_uses_str(e),
        TirStmt::If { cond, then_body, else_body } => {
            expr_uses_str(cond) || then_body.iter().any(stmt_uses_str) || else_body.iter().any(stmt_uses_str)
        }
        TirStmt::While { cond, body } => {
            expr_uses_str(cond) || body.iter().any(stmt_uses_str)
        }
        TirStmt::ForRange { body, .. } => body.iter().any(stmt_uses_str),
        TirStmt::ForEach { collection, body, .. } => {
            expr_uses_str(collection) || body.iter().any(stmt_uses_str)
        }
        TirStmt::ListSet { list, index, value } => {
            expr_uses_str(list) || expr_uses_str(index) || expr_uses_str(value)
        }
        TirStmt::ListAdd { receiver, arg } => expr_uses_str(receiver) || expr_uses_str(arg),
        TirStmt::ListInsert { receiver, arg, index } => {
            expr_uses_str(receiver) || expr_uses_str(arg) || expr_uses_str(index)
        }
        TirStmt::ListReverse { receiver } | TirStmt::ListSort { receiver } => expr_uses_str(receiver),
        TirStmt::DictSet { dict, key, value } => {
            expr_uses_str(dict) || expr_uses_str(key) || expr_uses_str(value)
        }
        TirStmt::MemberSet { receiver, value, .. } => expr_uses_str(receiver) || expr_uses_str(value),
        TirStmt::NativeCall { args, .. } => args.iter().any(expr_uses_str),
        TirStmt::Try { body, catch_body, finally_body, .. } => {
            body.iter().any(stmt_uses_str)
                || catch_body.iter().any(stmt_uses_str)
                || finally_body
                    .as_ref()
                    .is_some_and(|f| f.iter().any(stmt_uses_str))
        }
        TirStmt::Throw(e) => expr_uses_str(e),
        TirStmt::Return(None) => false,
        TirStmt::Return(Some(e)) => expr_uses_str(e),
        TirStmt::Break | TirStmt::Skip => false,
    }
}

fn expr_uses_str(e: &TirExpr) -> bool {
    match e {
        TirExpr::Str(_) => true,
        TirExpr::List(elements) => elements.iter().any(expr_uses_str),
        // Dicts are heap collections gated by `fn_uses_heap`, not by the Str
        // gate (keeps `un native` stderr byte-identical to `un run`).
        TirExpr::Dict(_) | TirExpr::DictGet(_, _) | TirExpr::ListGet(_, _)
        | TirExpr::ClassNew { .. } | TirExpr::MemberGet { .. } | TirExpr::MethodCall { .. } => false,
        TirExpr::Int(_) | TirExpr::Float(_) | TirExpr::Bool(_) | TirExpr::Local(_) => false,
        TirExpr::Promote(inner) => expr_uses_str(inner),
        TirExpr::Math(_, _) => false,
        TirExpr::CastInt(inner) | TirExpr::CastFloat(inner) => {
            expr_uses_str(inner)
        }
        TirExpr::Select { cond, then_expr, else_expr } => {
            expr_uses_str(cond) || expr_uses_str(then_expr) || expr_uses_str(else_expr)
        }
        TirExpr::Bin(_, lhs, rhs) | TirExpr::Cmp(_, lhs, rhs) => {
            expr_uses_str(lhs) || expr_uses_str(rhs)
        }
        TirExpr::Un(_, inner) => expr_uses_str(inner),
        TirExpr::Call(_, args) => args.iter().any(expr_uses_str),
        TirExpr::Len(inner) => expr_uses_str(inner),
        // List method results never contain Str (element types are int/float).
        TirExpr::ListPop(_)
        | TirExpr::ListPopAt(_, _)
        | TirExpr::ListContains(_, _)
        | TirExpr::ListIndexOf(_, _) => false,
    }
}

pub fn is_core_fn(fcheck: &FnCheck) -> bool {
    let FnCheck { sig, locals, .. } = fcheck;
    if sig.receiver != 0 || sig.is_method || sig.variadic {
        return false;
    }
    if sig.params.len() > MAX_CORE_ARITY {
        return false;
    }
    if sig.params.iter().any(|ty| TirTy::from_ty(ty).is_none()) {
        return false;
    }
    match &sig.ret {
        Ty::Int | Ty::Float | Ty::Bool | Ty::None_ | Ty::Str => {}
        Ty::List(elem) => match elem.as_ref() {
            Ty::Int | Ty::Float => {}
            _ => return false,
        },
        Ty::Dict(key, value) => match (key.as_ref(), value.as_ref()) {
            (Ty::Str, Ty::Int | Ty::Float) => {}
            _ => return false,
        },
        Ty::Class(_) => {}
        _ => return false,
    }
    if locals.iter().any(|(_, ty)| TirTy::from_ty(ty).is_none()) {
        return false;
    }
    true
}

/// UV_TIR_DEBUG diagnostics for core-eligibility rejections.
fn tirdbg_reject(node: &Node) {
    if std::env::var_os("UV_TIR_DEBUG").is_some() {
        eprintln!(
            "TIRDBG body reject kind={:?} op={:?} at {}",
            node.kind, node.op, node.start
        );
    }
}

/// Body-level scan: every statement/expression kind must be core-supported,
/// every call must target a core function directly (a name that resolves to
/// a same-module function candidate).
fn body_is_core(node: &Node, source: &Source, checked: &CheckedProgram, fcheck: &FnCheck) -> bool {
    // The function/extension node wraps a name, a parameter tuple, and
    // (optionally) a return-type identifier ??those are structural and are
    // not checked here. Only the body Block is checked.
    let Some(body) = body_block_of(node) else {
        return false;
    };
    let mut locals: HashMap<String, ()> = HashMap::new();
    collect_locals(body, &mut locals, source, fcheck);

    fn visit(
        node: &Node,
        checked: &CheckedProgram,
        source: &Source,
        locals: &HashMap<String, ()>,
        out: &mut bool,
    ) {
        if !*out {
            return;
        }
        match node.kind {
            NodeKind::Identifier => {
                let name = source.span(node.start, node.length);
                if !locals.contains_key(&name) {
                    tirdbg_reject(node); *out = false;
                    return;
                }
            }
            NodeKind::Block
            | NodeKind::If
            | NodeKind::ElIf
            | NodeKind::Else
            | NodeKind::Branch
            | NodeKind::IfCase
            | NodeKind::While
            | NodeKind::For
            | NodeKind::Return
            | NodeKind::Break
            | NodeKind::Skip
            | NodeKind::Assign
            | NodeKind::Typed => {
                for child in &node.children {
                    visit(child, checked, source, locals, out);
                }
            }
            NodeKind::Integer | NodeKind::Float | NodeKind::Boolean | NodeKind::None | NodeKind::String => {}
            NodeKind::Unary => {
                let Some(op) = node.op else {
                    tirdbg_reject(node); *out = false;
                    return;
                };
                if !matches!(
                    op,
                    TokenType::Minus | TokenType::Positive | TokenType::Not | TokenType::BNot | TokenType::Wait | TokenType::Go
                ) {
                    tirdbg_reject(node); *out = false;
                    return;
                }
                // `go`/`wait`/`defer` do not belong to the core subset.
                if matches!(op, TokenType::Wait | TokenType::Go) {
                    tirdbg_reject(node); *out = false;
                    return;
                }
                visit(&node.children[0], checked, source, locals, out);
            }
            NodeKind::Binary => {
                let Some(op) = node.op else {
                    tirdbg_reject(node); *out = false;
                    return;
                };
                let ok = matches!(
                    op,
                    TokenType::Plus
                        | TokenType::Minus
                        | TokenType::Asterisk
                        | TokenType::Slash
                        | TokenType::DoubleSlash
                        | TokenType::Percent
                        | TokenType::BAnd
                        | TokenType::BOr
                        | TokenType::BXor
                        | TokenType::LeftShift
                        | TokenType::RightShift
                        | TokenType::Equal
                        | TokenType::Unequal
                        | TokenType::LessThan
                        | TokenType::GreaterThan
                        | TokenType::LessOrEqual
                        | TokenType::GreaterOrEqual
                        | TokenType::And
                        | TokenType::Or
                        | TokenType::DotDot
                );
                if !ok {
                    tirdbg_reject(node); *out = false;
                    return;
                }
                for child in &node.children {
                    visit(child, checked, source, locals, out);
                }
            }
            NodeKind::Spread | NodeKind::KwSpread => {
                // Keyword/spread arguments cancel the core fast path.
                tirdbg_reject(node); *out = false;
            }
            NodeKind::Call => {
                // `int(x)`/`float(x)` language casts: accepted with any core arg.
                if let Some(first) = node.children.first() {
                    if first.kind == NodeKind::Identifier {
                        let name = source.span(first.start, first.length).to_string();
                        if (name == "int" || name == "float") && !checked.sigs.contains_key(name.as_str()) {
                            if let Some(arg_tuple) = node.children.get(1) {
                                for arg in &arg_tuple.children {
                                    visit(arg, checked, source, locals, out);
                                }
                                return;
                            }
                        }
                    }
                }
                // `len(x)` / `throw(x)` builtins: allowed when the arguments visit
                // and the name isn't shadowed by a user-declared function.
                if let Some(first) = node.children.first() {
                    if first.kind == NodeKind::Identifier {
                        let name = source.span(first.start, first.length).to_string();
                        if (name == "len" || name == "throw") && !checked.sigs.contains_key(name.as_str()) {
                            if let Some(arg_tuple) = node.children.get(1) {
                                for arg in &arg_tuple.children {
                                    visit(arg, checked, source, locals, out);
                                }
                                return;
                            }
                        }
                    }
                }
                let Some(callee) = node.children.first() else {
                    tirdbg_reject(node); *out = false;
                    return;
                };
                // `mod.member(...)` for a registered native module (e.g.
                // `io.write(x)`): accepted as a NativeCall statement.
                if callee.kind == NodeKind::Property {
                    if callee.kind == NodeKind::Property {
                        let head = callee.children.first();
                        let member = callee.children.get(1);
                        if let (Some(head), Some(member)) = (head, member) {
                            if head.kind == NodeKind::Identifier && member.kind == NodeKind::Identifier {
                                let head_name = source.span(head.start, head.length).to_string();
                                let member_name = source.span(member.start, member.length).to_string();
                                if let Some(members) = crate::runtime::world::native_members(&head_name) {
                                    if members.iter().any(|(k, _)| k == &member_name) {
                                        for arg in &node.children[1].children {
                                            visit(arg, checked, source, locals, out);
                                        }
                                        return;
                                    }
                                }
                            }
                        }
                    }
                }
                // A class receiver `p.method(...)`: the checker validated the
                // member lookup; dispatch is dynamic at runtime.
                if callee.kind == NodeKind::Property {
                    if let Some(head) = callee.children.first() {
                        visit(head, checked, source, locals, out);
                    }
                    for arg in &node.children[1].children {
                        visit(arg, checked, source, locals, out);
                    }
                    return;
                }
                // Only direct calls to same-module core functions.
                let Some(callee) = node.children.first() else {
                    tirdbg_reject(node); *out = false;
                    return;
                };
                // A class constructor `Point(...)`: arg checks only; the
                // plain-native pipeline bails for class functions at a later
                // stage with a dedicated message, and `un native` routes them
                // to the interpreter.
                if callee.kind == NodeKind::Identifier {
                    let name = source.span(callee.start, callee.length).to_string();
                    if checked.classes.contains_key(&name) {
                        if let Some(arg_tuple) = node.children.get(1) {
                            for arg in &arg_tuple.children {
                                visit(arg, checked, source, locals, out);
                            }
                            return;
                        }
                    }
                }
                if callee.kind != NodeKind::Identifier {
                    tirdbg_reject(node); *out = false;
                    return;
                }
                let Some(sig) = checked.sigs.get(source.span(callee.start, callee.length).as_str()) else {
                    tirdbg_reject(node); *out = false;
                    return;
                };
                if sig.is_method || sig.receiver != 0 {
                    tirdbg_reject(node); *out = false;
                    return;
                }
                if TirTy::from_ty(&sig.ret).is_none() && !matches!(sig.ret, Ty::None_) {
                    tirdbg_reject(node); *out = false;
                    return;
                }
                if sig.params.iter().any(|ty| TirTy::from_ty(ty).is_none()) {
                    tirdbg_reject(node); *out = false;
                    return;
                }
                for arg in &node.children[1].children {
                    visit(arg, checked, source, locals, out);
                }
            }
            NodeKind::Pair => {
                for child in &node.children {
                    visit(child, checked, source, locals, out);
                }
            }
            NodeKind::Dict => {
                if node.children.is_empty() {
                    tirdbg_reject(node); *out = false;
                    return;
                }
                for child in &node.children {
                    visit(child, checked, source, locals, out);
                }
            }
            // An empty list/dict literal can't be typed in the core; reject
            // the function so it runs in the interpreter instead.
            NodeKind::List => {
                if node.children.is_empty() {
                    tirdbg_reject(node); *out = false;
                    return;
                }
                for child in &node.children {
                    visit(child, checked, source, locals, out);
                }
            }
            // Only the receiver is visited ??the member name is an attribute
            // name, not a local.
            NodeKind::Property => {
                let receiver = node.children.first();
                if let Some(receiver) = receiver {
                    visit(receiver, checked, source, locals, out);
                }
            }
            NodeKind::Index => {
                // Dict indexing `d["k"]` is lowered by the builder; list/tuple
                // indexing is rejected there. Children are visited here so
                // dict targets flow into eligibility.
                for child in &node.children {
                    visit(child, checked, source, locals, out);
                }
            }
            NodeKind::Try => {
                // try body first, then catch clause(s) and the finally clause
                // in source order. `catch`/`finally` only ever appear under a
                // `try` in Un's grammar.
                for child in &node.children {
                    visit(child, checked, source, locals, out);
                }
            }
            NodeKind::Catch => {
                // Children are either (binding identifier?, block) or block.
                // The binding identifier is a local reference introduced by
                // the try statement ??account for it before visiting the
                // catch body.
                let mut scoped = locals.clone();
                if let Some(first) = node.children.first() {
                    if first.kind == NodeKind::Identifier {
                        let name = source.span(first.start, first.length).to_string();
                        scoped.insert(name, ());
                    }
                }
                for child in &node.children {
                    if child.kind != NodeKind::Block {
                        continue;
                    }
                    visit(child, checked, source, &scoped, out);
                }
            }
            NodeKind::Finally => {
                for child in &node.children {
                    visit(child, checked, source, locals, out);
                }
            }
            NodeKind::Slice | NodeKind::Tuple | NodeKind::Set
            | NodeKind::FString | NodeKind::Match | NodeKind::Case
            | NodeKind::Use | NodeKind::Using | NodeKind::Defer
            | NodeKind::Function | NodeKind::Lambda | NodeKind::Class | NodeKind::Enum | NodeKind::Tagged
            | NodeKind::Accessor | NodeKind::Wildcard => {
                tirdbg_reject(node); *out = false;
            }
            _ => {
                // Unknown or dynamic node: not core.
                tirdbg_reject(node); *out = false;
            }
        }
    }
    // Params bound by the checker are all core-typed at this point (else
    // `is_core_fn` already rejected), so locals membership is a formality.
    let mut out = true;
    visit(body, checked, source, &locals, &mut out);
    out
}

/// Collect the set of names this body can reference. `fcheck.locals` is the
/// insertion-ordered set of every name bound by the body (parameters first,
/// then locals in source order) ??a binding exists for any way Un declares a
/// name, including tuple destructuring. Anything else referenced by the body
/// is a closure capture or a global, both of which are out of the core subset.
///
/// A fork arm ending in `->`, `break` or `throw` does not merge its local
/// declarations back into the outer environment, so `fcheck.locals` misses
/// names declared only inside terminated arms. We therefore also scan the
/// AST for every binding site (`name =`, `name: T =`, `for name in`,
/// `catch name`) and admit those as locals as well.
fn collect_locals(body: &Node, locals: &mut HashMap<String, ()>, source: &Source, fcheck: &FnCheck) {
    collect_binding_names(body, source, locals);
    for (name, _) in &fcheck.locals {
        locals.insert(name.clone(), ());
    }
}

fn collect_binding_names(node: &Node, source: &Source, out: &mut HashMap<String, ()>) {
    match node.kind {
        NodeKind::Assign | NodeKind::Typed => {
            if let Some(target) = node.children.first() {
                collect_binding_target(target, source, out);
            }
            for child in node.children.iter().skip(1) {
                collect_binding_names(child, source, out);
            }
            return;
        }
        NodeKind::For => {
            if let Some(target) = node.children.first() {
                collect_binding_target(target, source, out);
            }
            for child in node.children.iter().skip(1) {
                collect_binding_names(child, source, out);
            }
            return;
        }
        NodeKind::Catch => {
            if let Some(first) = node.children.first() {
                if first.kind == NodeKind::Identifier {
                    let name = source.span(first.start, first.length).to_string();
                    out.insert(name, ());
                }
            }
            for child in &node.children {
                collect_binding_names(child, source, out);
            }
            return;
        }
        _ => {
            for child in &node.children {
                collect_binding_names(child, source, out);
            }
        }
    }
}

fn collect_binding_target(target: &Node, source: &Source, out: &mut HashMap<String, ()>) {
    match target.kind {
        NodeKind::Identifier => {
            let name = source.span(target.start, target.length).to_string();
            out.insert(name, ());
        }
        NodeKind::Tuple | NodeKind::List => {
            for child in &target.children {
                collect_binding_target(child, source, out);
            }
        }
        _ => {}
    }
}

fn body_block_of(node: &Node) -> Option<&Node> {
    node.children.iter().find(|c| c.kind == NodeKind::Block)
}






impl TirProgram {
    /// Lower all core functions found in the checked program. Functions are
    /// lowered in source order; `Call` indices name the position in the
    /// returned `functions` vector.
    pub fn lower(source: &Source, root: &Node, checked: &CheckedProgram) -> Result<TirProgram, TirError> {
        let debug_trace = std::env::var_os("UV_TIR_DEBUG").is_some();
        let fn_nodes = collect_fn_nodes(root);
        if debug_trace {
            eprintln!("TIRDBG collected {} fn nodes, checked has {} functions", fn_nodes.len(), checked.functions.len());
            for node in &fn_nodes {
                let span = node.start;
                let matched = checked.fn_by_span.get(&span).copied();
                let kind = format!("{:?}", node.kind);
                eprintln!("TIRDBG node span={} kind={} matched={:?}", span, kind, matched);
            }
            for (i, f) in checked.functions.iter().enumerate() {
                eprintln!("TIRDBG checked[{}] {} params={} locals={} ret={:?} is_method={} variadic={}",
                    i, f.sig.name, f.sig.params.len(), f.locals.len(), f.sig.ret, f.sig.is_method, f.sig.variadic);
                for (i2, (n, t)) in f.locals.iter().enumerate() {
                    eprintln!("TIRDBG   local[{}] {} = {}", i2, n, t.display());
                }
            }
        }
        // Pass 1: eligibility, in source order.
        let mut eligible: Vec<usize> = Vec::new(); // indices into checked.functions
        for node in &fn_nodes {
            let Some(&fn_index) = checked.fn_by_span.get(&node.start) else {
                continue;
            };
            let fcheck = &checked.functions[fn_index];
            let core_ok = is_core_fn(fcheck);
            let body_ok = body_is_core(node, source, checked, fcheck);
            if debug_trace {
                let name = &fcheck.sig.name;
                eprintln!("TIRDBG gate {} is_core={} body_core={}", name, core_ok, body_ok);
            }
            if core_ok && body_ok {
                eligible.push(fn_index);
            }
        }
        let tir_index: HashMap<usize, usize> = eligible
            .iter()
            .enumerate()
            .map(|(tir_i, &fn_i)| (fn_i, tir_i))
            .collect();
        // Pass 2: lower each eligible function.
        let classes = ClassTable::new(checked);
        let mut functions = Vec::with_capacity(eligible.len());
        for node in &fn_nodes {
            let Some(&fn_index) = checked.fn_by_span.get(&node.start) else {
                continue;
            };
            let Some(tir_i) = tir_index.get(&fn_index).copied() else {
                continue;
            };
            let fcheck = &checked.functions[fn_index];
            let lowered = lower_function(node, source, fcheck, checked, tir_i, &tir_index, &classes)
                .map_err(|e| TirError(format!("{}: {}", fcheck.sig.name, e)))?;
            functions.push(lowered);
        }
        // Pass 3: independent validation of every lowered function.
        let program = TirProgram {
            functions,
            class_names: classes.names.clone(),
            class_fields: classes.fields.clone(),
            class_methods: classes.methods.clone(),
            class_setters: classes.setters.clone(),
        };
        program.validate()?;
        Ok(program)
    }
}

/// Type inference scoped to the core subset. Mirrors the checker's numeric
/// rules for exactly the types the core supports.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CoreTy {
    Int,
    Float,
    Bool,
    Str,
    ListInt,
    ListFloat,
    DictStrInt,
    DictStrFloat,
    /// A class instance (the u16 indexes `TirProgram::class_names`).
    Class(u16),
    None_,
}

impl From<TirTy> for CoreTy {
    fn from(ty: TirTy) -> Self {
        match ty {
            TirTy::Int => CoreTy::Int,
            TirTy::Float => CoreTy::Float,
            TirTy::Bool => CoreTy::Bool,
            TirTy::Str => CoreTy::Str,
            TirTy::ListInt => CoreTy::ListInt,
            TirTy::ListFloat => CoreTy::ListFloat,
            TirTy::DictStrInt => CoreTy::DictStrInt,
            TirTy::DictStrFloat => CoreTy::DictStrFloat,
            TirTy::Class(id) => CoreTy::Class(id),
            TirTy::ErrorObj => CoreTy::Class(u16::MAX),
        }
    }
}

struct LowerCtx<'a> {
    source: &'a Source,
    checked: &'a CheckedProgram,
    tir_index: &'a HashMap<usize, usize>,
    slots: HashMap<String, u16>,
    locals: Vec<TirTy>,
    classes: &'a ClassTable,
}

fn lower_function(
    node: &Node,
    source: &Source,
    fcheck: &FnCheck,
    checked: &CheckedProgram,
    tir_i: usize,
    tir_index: &HashMap<usize, usize>,
    classes: &ClassTable,
) -> Result<TirFn, String> {
    let mut ctx = LowerCtx {
        source,
        checked,
        tir_index,
        slots: HashMap::new(),
        locals: Vec::new(),
        classes,
    };
    // Parameters bind slots 0..n in declaration order.
    for (i, (name, ty)) in fcheck.sig.param_names.iter().zip(&fcheck.sig.params).enumerate() {
        let tty = match ty {
            Ty::Class(name) => {
                let id = classes.id(name).ok_or_else(|| format!("unknown class '{}'", name))?;
                TirTy::Class(id)
            }
            other => {
                let Some(tty) = TirTy::from_ty(other) else {
                    return Err(format!("parameter '{name}' is not core-typed"));
                };
                tty
            }
        };
        if i >= 256 {
            return Err("more than 255 parameters".to_string());
        }
        ctx.slots.insert(name.clone(), i as u16);
        ctx.locals.push(tty);
    }
    let body_node = node
        .children
        .iter()
        .find(|c| c.kind == NodeKind::Block)
        .ok_or_else(|| "function without a body".to_string())?;
    let mut body = Vec::new();
    lower_block(body_node, &mut ctx, &mut body)?;
    let ret = match &fcheck.sig.ret {
        Ty::Int => Some(TirTy::Int),
        Ty::Float => Some(TirTy::Float),
        Ty::Bool => Some(TirTy::Bool),
        Ty::None_ => None,
        Ty::Str => Some(TirTy::Str),
        Ty::List(elem) => match elem.as_ref() {
            Ty::Int => Some(TirTy::ListInt),
            Ty::Float => Some(TirTy::ListFloat),
            other => {
                return Err(format!(
                    "return type 'list[{}]' is not core-supported",
                    other.display()
                ))
            }
        },
        Ty::Dict(key, value) => match (key.as_ref(), value.as_ref()) {
            (Ty::Str, Ty::Int) => Some(TirTy::DictStrInt),
            (Ty::Str, Ty::Float) => Some(TirTy::DictStrFloat),
            (Ty::Str, other) => {
                return Err(format!(
                    "return type 'dict[str, {}]' is not core-supported",
                    other.display()
                ))
            }
            (other, _) => {
                return Err(format!(
                    "return type 'dict[{}, ...]' is not core-supported (keys must be str)",
                    other.display()
                ))
            }
        },
        Ty::Class(name) => {
            let id = classes.id(name).ok_or_else(|| format!("unknown class '{name}'"))?;
            Some(TirTy::Class(id))
        }
        other => {
            return Err(format!(
                "return type '{}' is not core-supported",
                other.display()
            ))
        }
    };
    Ok(TirFn {
        name: fcheck.sig.name.clone(),
        proto_span: (node.start, node.length),
        index: tir_i,
        params: {
            let mut params = Vec::with_capacity(fcheck.sig.params.len());
            for t in &fcheck.sig.params {
                match t {
                    Ty::Class(name) => {
                        let id = classes.id(name).ok_or_else(|| format!("unknown class '{}'", name))?;
                        params.push(TirTy::Class(id));
                    }
                    other => {
                        let Some(tty) = TirTy::from_ty(other) else {
                            return Err("parameter type is not core-supported".to_string());
                        };
                        params.push(tty);
                    }
                }
            }
            params
        },
        ret,
        locals: ctx.locals,
        body,
    })
}

/// Recognize `module.member(args)` where `module` is a registered native
/// module (e.g. `io.write("hi")`). Returns the module/member names and the
/// argument nodes, or `None` if this call isn't a native call.
fn native_call_shape<'a>(
    node: &'a Node,
    ctx: &LowerCtx,
) -> Option<(String, String, Vec<&'a Node>)> {
    if node.kind != NodeKind::Call {
        return None;
    }
    let callee = node.children.first()?;
    if callee.kind != NodeKind::Property {
        return None;
    }
    let head = callee.children.first()?;
    let member = callee.children.get(1)?;
    if head.kind != NodeKind::Identifier || member.kind != NodeKind::Identifier {
        return None;
    }
    let module = ctx.source.span(head.start, head.length).to_string();
    let member = ctx.source.span(member.start, member.length).to_string();
    let members = crate::runtime::world::native_members(&module)?;
    if !members.iter().any(|(k, _)| k == &member) {
        return None;
    }
    let args_tuple = node.children.get(1)?;
    let args = args_tuple.children.iter().collect();
    Some((module, member, args))
}

fn lower_block(node: &Node, ctx: &mut LowerCtx, out: &mut Vec<TirStmt>) -> Result<(), String> {
    for child in &node.children {
        lower_stmt(child, ctx, out)?;
    }
    Ok(())
}

fn lower_stmt(node: &Node, ctx: &mut LowerCtx, out: &mut Vec<TirStmt>) -> Result<(), String> {
    match node.kind {
        NodeKind::Assign => lower_assign(node, ctx, out),
        NodeKind::Typed => Err("annotation without assignment is not supported in the core".to_string()),
        NodeKind::If => lower_if(node, ctx, out),
        NodeKind::IfCase | NodeKind::Branch => Err("internal: unexpected branch node".to_string()),
        NodeKind::While => lower_while(node, ctx, out),
        NodeKind::For => lower_for(node, ctx, out),
        NodeKind::Return => lower_return(node, ctx, out),
        NodeKind::Break => {
            out.push(TirStmt::Break);
            Ok(())
        }
        NodeKind::Skip => {
            out.push(TirStmt::Skip);
            Ok(())
        }
        NodeKind::Block => lower_block(node, ctx, out),
        NodeKind::Try => lower_try(node, ctx, out),
        NodeKind::Call => {
            // `throw(x)` builtin: lower directly to the exception statement
            // (the value is an Obj, dispatch happens at the nearest enclosing
            // try).
            if let Some(first) = node.children.first() {
                if first.kind == NodeKind::Identifier {
                    let callee = ctx.source.span(first.start, first.length).to_string();
                    if callee == "throw" {
                        let args_node = node
                            .children
                            .get(1)
                            .ok_or_else(|| "throw() without arguments".to_string())?;
                        if args_node.kind != NodeKind::Tuple {
                            return Err("throw() arguments must be positional".to_string());
                        }
                        if args_node.children.len() != 1 {
                            return Err("throw() expects exactly one argument".to_string());
                        }
                        let (expr, _) = lower_expr(&args_node.children[0], ctx)?;
                        out.push(TirStmt::Throw(expr));
                        return Ok(());
                    }
                }
            }
            // `mod.member(args)` for a registered native module (e.g.
            // `io.write("hi")`): lower directly to a NativeCall statement.
            if let Some((module, member, arg_nodes)) = native_call_shape(node, ctx) {
                let args = arg_nodes
                    .into_iter()
                    .map(|a| lower_expr(a, ctx).map(|(e, _)| e))
                    .collect::<Result<Vec<_>, _>>()?;
                out.push(TirStmt::NativeCall { module, member, args });
                return Ok(());
            }
            // List method statements: `xs.add(v)`, `xs.reverse()`,
            // `xs.sort()`, `xs.insert(v, i)`.
            if let Some(stmt) = lower_list_method_stmt(node, ctx)? {
                out.push(stmt);
                return Ok(());
            }
            let (expr, _) = lower_expr(node, ctx)?;
            out.push(TirStmt::ExprStmt(expr));
            Ok(())
        }
        _ => Err(format!(
            "statement kind {:?} is not supported in the core",
            node.kind
        )),
    }
}

fn forbid_flow_controls(node: &Node, source: &Source) -> Result<(), String> {
    match node.kind {
        NodeKind::Return | NodeKind::Break | NodeKind::Skip => {
            let span = format!("{}, {}", node.start, node.length);
            let _ = span;
            // Point the user at the offending statement.
            let _ = source;
            return Err(
                "'return'/'break'/'skip' inside 'try' is not supported in this native mode; \
                 restructure the code or run with `un build --embed`"
                    .to_string(),
            );
        }
        _ => {}
    }
    for child in &node.children {
        forbid_flow_controls(child, source)?;
    }
    Ok(())
}

fn lower_try(node: &Node, ctx: &mut LowerCtx, out: &mut Vec<TirStmt>) -> Result<(), String> {
    // `try body { catch err? }? catch_body finally?` ??Un's grammar puts the
    // block first, then the catch clause(s) and the optional finally.
    let body_block = node
        .children
        .first()
        .ok_or_else(|| "'try' without a body".to_string())?;
    if body_block.kind != NodeKind::Block {
        return Err("'try' body must be a block".to_string());
    }
    // Only the body and the finally may contain "uncatchable" control flow
    // restrictions; the catch body is a clean block.
    forbid_flow_controls(body_block, ctx.source)?;

    let mut catch: Option<&Node> = None;
    let mut finally: Option<&Node> = None;
    for child in node.children.iter().skip(1) {
        match child.kind {
            NodeKind::Catch => {
                if catch.is_some() {
                    return Err(
                        "multiple 'catch' clauses are not supported; use a single catch and branch inside it"
                            .to_string(),
                    );
                }
                catch = Some(child);
            }
            NodeKind::Finally => {
                if finally.is_some() {
                    return Err("only one 'finally' clause is supported".to_string());
                }
                finally = Some(child);
            }
            _ => return Err(format!("unexpected node kind {:?} under 'try'", child.kind)),
        }
    }

    let mut body_out = Vec::new();
    lower_block(body_block, ctx, &mut body_out)?;

    // Declare the catch binding only after the body ??declaration order
    // must match the runtime slot numbers.
    let mut var_slot = None;
    match catch {
        Some(catch_node) => {
            forbid_flow_controls(catch_node, ctx.source)?;
            if catch_node.children.len() == 2 {
                let name_node = &catch_node.children[0];
                if name_node.kind != NodeKind::Identifier {
                    return Err("catch binding must be a plain variable".to_string());
                }
                let name = ctx.source.span(name_node.start, name_node.length).to_string();
                if slot_of(ctx, &name).is_some() {
                    return Err(format!(
                        "catch binding '{name}' shadows an existing local; rename the binding"
                    ));
                }
                let slot = declare(ctx, &name, TirTy::ErrorObj);
                var_slot = Some(slot);
            } else if catch_node.children.len() != 1 {
                return Err("'catch' has unexpected shape".to_string());
            }
        }
        None => {}
    }

    let catch_body: &Node = match catch {
        Some(catch_node) => catch_node
            .children
            .iter()
            .find(|c| c.kind == NodeKind::Block)
            .ok_or_else(|| "'catch' without a body".to_string())?,
        None => body_block, // unused when catch.is_none()
    };
    let catch_body_out = if catch.is_some() {
        let mut out_v = Vec::new();
        lower_block(catch_body, ctx, &mut out_v)?;
        out_v
    } else {
        Vec::new()
    };
    let finally_body_out = finally
        .map(|fnode| {
            forbid_flow_controls(fnode, ctx.source)?;
            let fblk = fnode
                .children
                .iter()
                .find(|c| c.kind == NodeKind::Block)
                .ok_or_else(|| "'finally' without a body".to_string())?;
            let mut out_v = Vec::new();
            lower_block(fblk, ctx, &mut out_v)?;
            Ok::<_, String>(out_v)
        })
        .transpose()?;

    out.push(TirStmt::Try {
        body: body_out,
        var: var_slot,
        catch_body: catch_body_out,
        finally_body: finally_body_out,
    });
    Ok(())
}

fn slot_of(ctx: &LowerCtx, name: &str) -> Option<u16> {
    ctx.slots.get(name).copied()
}

fn declare(ctx: &mut LowerCtx, name: &str, ty: TirTy) -> u16 {
    let slot = ctx.locals.len() as u16;
    ctx.slots.insert(name.to_string(), slot);
    ctx.locals.push(ty);
    slot
}

fn lower_assign(node: &Node, ctx: &mut LowerCtx, out: &mut Vec<TirStmt>) -> Result<(), String> {
    let target = node
        .children
        .first()
        .ok_or_else(|| "assignment without target".to_string())?;
    let value = node
        .children
        .get(1)
        .ok_or_else(|| "assignment without value".to_string())?;
    match target.kind {
        NodeKind::Identifier => {
            let name = ctx.source.span(target.start, target.length);
            let (expr, ty) = lower_expr(value, ctx)?;
            if let Some(slot) = slot_of(ctx, &name) {
                // Re-assignment: declared type is invariant (checker enforced it).
                out.push(TirStmt::Set(slot, expr));
            } else {
                let tty = tir_ty_of(ty)?;
                let slot = declare(ctx, &name, tty);
                out.push(TirStmt::Let(slot, expr));
            }
            Ok(())
        }
        NodeKind::Index => {
            let collection = target
                .children
                .first()
                .ok_or_else(|| "index without a target".to_string())?;
            let key_node = target
                .children
                .get(1)
                .ok_or_else(|| "index without a key".to_string())?;
            let (c_expr, c_ty) = lower_expr(collection, ctx)?;
            let (k_expr, k_ty) = lower_expr(key_node, ctx)?;
            let (v_expr, v_ty) = lower_expr(value, ctx)?;
            match (c_ty, k_ty) {
                (CoreTy::ListInt, CoreTy::Int) => {
                    if v_ty != CoreTy::Int {
                        return Err("list elements must all have the same type".to_string());
                    }
                    out.push(TirStmt::ListSet { list: c_expr, index: k_expr, value: v_expr });
                    Ok(())
                }
                (CoreTy::ListFloat, CoreTy::Int) => {
                    if v_ty != CoreTy::Float {
                        return Err("list elements must all have the same type".to_string());
                    }
                    out.push(TirStmt::ListSet { list: c_expr, index: k_expr, value: v_expr });
                    Ok(())
                }
                (CoreTy::DictStrInt, CoreTy::Str) => {
                    if v_ty != CoreTy::Int {
                        return Err("dict values must all have the same type".to_string());
                    }
                    out.push(TirStmt::DictSet { dict: c_expr, key: k_expr, value: v_expr });
                    Ok(())
                }
                (CoreTy::DictStrFloat, CoreTy::Str) => {
                    if v_ty != CoreTy::Float {
                        return Err("dict values must all have the same type".to_string());
                    }
                    out.push(TirStmt::DictSet { dict: c_expr, key: k_expr, value: v_expr });
                    Ok(())
                }
                (CoreTy::DictStrInt | CoreTy::DictStrFloat, other) => {
                    let _ = other;
                    Err("dict keys must be strings".to_string())
                }
                (CoreTy::ListInt | CoreTy::ListFloat, _) => {
                    Err("list index must be an int".to_string())
                }
                _ => Err("only list/dict indexing is assignable in the core".to_string()),
            }
        }
        NodeKind::Property => {
            let receiver = target
                .children
                .first()
                .ok_or_else(|| "property without a receiver".to_string())?;
            let member_node = target
                .children
                .get(1)
                .ok_or_else(|| "property without a member".to_string())?;
            let member = ctx.source.span(member_node.start, member_node.length).to_string();
            let (recv, rty) = lower_expr(receiver, ctx)?;
            let id = match rty {
                CoreTy::Class(id) => id,
                _ => return Err("only class member assignment is supported in the core".to_string()),
            };
            // Checker already validated the member exists; the field type
            // guides the value coercion (Int ??Float widening is allowed).
            let field_ty = ctx.classes.field(id, &member).cloned();
            let field_ty = match field_ty {
                Some(ty) => ty,
                None => {
                    // Property with a `set` accessor (no backing field):
                    // the static value type is `any` — accept any value and
                    // let the VM dispatch through the property setter.
                    if !ctx.classes.setters(id).is_some_and(|s| s.contains(member.as_str())) {
                        return Err(format!(
                            "class '{}' has no field '{}'",
                            ctx.classes.names[id as usize], member
                        ));
                    }
                    let (v_expr, _) = lower_expr(value, ctx)?;
                    out.push(TirStmt::MemberSet {
                        receiver: recv,
                        member: member.into_boxed_str(),
                        value: v_expr,
                    });
                    return Ok(());
                }
            };
            let (v_expr, v_ty) = lower_expr(value, ctx)?;
            let field_is_class = matches!(&field_ty, Ty::Class(_));
            if field_is_class {
                if !matches!(v_ty, CoreTy::Class(_)) {
                    return Err("cannot assign non-class value to a class field".to_string());
                }
            } else {
                let target_ty = class_member_ty(ctx, &field_ty)?;
                match (target_ty, v_ty) {
                    (CoreTy::Float, CoreTy::Int) => {} // widening
                    (t, v) if t == v => {}
                        _ => {
                            return Err(format!(
                                "cannot assign value to class field '{}'",
                                member
                            ))
                        }
                    }
            }
            out.push(TirStmt::MemberSet {
                receiver: recv,
                member: member.into_boxed_str(),
                value: v_expr,
            });
            Ok(())
        }
        _ => Err("only plain variable or index assignment is supported in the core".to_string()),
    }
}

fn tir_ty_of(ty: CoreTy) -> Result<TirTy, String> {
    match ty {
        CoreTy::Int => Ok(TirTy::Int),
        CoreTy::Float => Ok(TirTy::Float),
        CoreTy::Bool => Ok(TirTy::Bool),
        CoreTy::Str => Ok(TirTy::Str),
        CoreTy::ListInt => Ok(TirTy::ListInt),
        CoreTy::ListFloat => Ok(TirTy::ListFloat),
        CoreTy::DictStrInt => Ok(TirTy::DictStrInt),
        CoreTy::DictStrFloat => Ok(TirTy::DictStrFloat),
        CoreTy::None_ => {
            Err("cannot infer a concrete local type in the core".to_string())
        }
        CoreTy::Class(id) => Ok(TirTy::Class(id)),
    }
}

fn lower_if(node: &Node, ctx: &mut LowerCtx, out: &mut Vec<TirStmt>) -> Result<(), String> {
    // If ??[Branch|IfCase ..., Else?]; bodies are Blocks.
    let mut arms: Vec<(&Node, &Node)> = Vec::new();
    let mut else_body: Option<&Node> = None;
    for child in &node.children {
        match child.kind {
            NodeKind::Branch | NodeKind::ElIf | NodeKind::IfCase => {
                let cond = child
                    .children
                    .first()
                    .ok_or_else(|| "branch without condition".to_string())?;
                let body = child
                    .children
                    .get(1)
                    .ok_or_else(|| "branch without body".to_string())?;
                arms.push((cond, body));
            }
            NodeKind::Else => {
                for b in &child.children {
                    else_body = Some(b);
                }
            }
            _ => return Err("unexpected node in if".to_string()),
        }
    }
    // Fold right-nested: else_out ends up as the deepest else chain.
    let mut else_out: Vec<TirStmt> = Vec::new();
    if let Some(body) = else_body {
        lower_block(body, ctx, &mut else_out)?;
    }
    for (cond, body) in arms.into_iter().rev() {
        let (cond_expr, cond_ty) = lower_expr(cond, ctx)?;
        if cond_ty != CoreTy::Bool {
            return Err("condition must be a bool".to_string());
        }
        let mut then_out = Vec::new();
        lower_block(body, ctx, &mut then_out)?;
        else_out = vec![TirStmt::If {
            cond: cond_expr,
            then_body: then_out,
            else_body: else_out,
        }];
    }
    out.extend(else_out);
    Ok(())
}

fn lower_while(node: &Node, ctx: &mut LowerCtx, out: &mut Vec<TirStmt>) -> Result<(), String> {
    let cond = node
        .children
        .first()
        .ok_or_else(|| "while without condition".to_string())?;
    let body = node
        .children
        .get(1)
        .ok_or_else(|| "while without body".to_string())?;
    let (cond_expr, cond_ty) = lower_expr(cond, ctx)?;
    if cond_ty != CoreTy::Bool {
        return Err("condition must be a bool".to_string());
    }
    let mut body_out = Vec::new();
    lower_block(body, ctx, &mut body_out)?;
    out.push(TirStmt::While {
        cond: cond_expr,
        body: body_out,
    });
    Ok(())
}

fn lower_for(node: &Node, ctx: &mut LowerCtx, out: &mut Vec<TirStmt>) -> Result<(), String> {
    let target = node
        .children
        .first()
        .ok_or_else(|| "for without target".to_string())?;
    let iter = node
        .children
        .get(1)
        .ok_or_else(|| "for without iterator".to_string())?;
    let body = node
        .children
        .get(2)
        .ok_or_else(|| "for without body".to_string())?;
    let NodeKind::Identifier = target.kind else {
        return Err("for loop target must be a plain variable in the core".to_string());
    };
    let name = ctx.source.span(target.start, target.length);
    // `for v in xs` where xs is a local list[int]/list[float].
    if let Some(slot) = slot_of(ctx, &ctx.source.span(iter.start, iter.length).to_string()) {
        let list_ty = ctx.locals[slot as usize];
        if matches!(list_ty, TirTy::ListInt | TirTy::ListFloat) {
            let elem_ty = if list_ty == TirTy::ListInt { TirTy::Int } else { TirTy::Float };
            let var = declare(ctx, &name, elem_ty);
            let mut body_out = Vec::new();
            lower_block(body, ctx, &mut body_out)?;
            out.push(TirStmt::ForEach {
                var,
                collection: TirExpr::Local(slot),
                body: body_out,
            });
            return Ok(());
        }
    }
    // `for v in [1, 2, 3]` ??a list literal directly in the iterator position.
    if iter.kind == NodeKind::List {
        let (collection, cty) = lower_expr(iter, ctx)?;
        let elem_ty = match cty {
            CoreTy::ListInt => TirTy::Int,
            CoreTy::ListFloat => TirTy::Float,
            _ => return Err("for ... in requires a core list".to_string()),
        };
        let var = declare(ctx, &name, elem_ty);
        let mut body_out = Vec::new();
        lower_block(body, ctx, &mut body_out)?;
        out.push(TirStmt::ForEach {
            var,
            collection,
            body: body_out,
        });
        return Ok(());
    }
    let (start, end, step) = parse_range_call(iter, ctx)?;
    let var = declare(ctx, &name, TirTy::Int);
    let mut body_out = Vec::new();
    lower_block(body, ctx, &mut body_out)?;
    out.push(TirStmt::ForRange {
        var,
        start,
        end,
        step,
        body: body_out,
    });
    Ok(())
}

/// Match `iter.range(a, b[, step])`, `iter.range(n)`, or the `a..b` DotDot
/// form ??the only iterators the core supports ??and evaluate bounds/step
/// into expressions.
fn parse_range_call(
    node: &Node,
    ctx: &mut LowerCtx,
) -> Result<(TirExpr, TirExpr, TirExpr), String> {
    if node.kind == NodeKind::Binary && node.op == Some(TokenType::DotDot) {
        let start = node.children.first().ok_or_else(|| "range needs a start".to_string())?;
        let end = node.children.get(1).ok_or_else(|| "range needs an end".to_string())?;
        let (start_expr, start_ty) = lower_expr(start, ctx)?;
        let (end_expr, end_ty) = lower_expr(end, ctx)?;
        if start_ty != CoreTy::Int || end_ty != CoreTy::Int {
            return Err("range bounds must be ints".to_string());
        }
        return Ok((start_expr, end_expr, TirExpr::Int(1)));
    }
    if node.kind != NodeKind::Call {
        return Err("for: iterator must be iter.range(...) in the core".to_string());
    }
    let callee = node
        .children
        .first()
        .ok_or_else(|| "for: iterator must be iter.range(...) in the core".to_string())?;
    let args = node
        .children
        .get(1)
        .ok_or_else(|| "for: iterator must be iter.range(...) in the core".to_string())?;
    if callee.kind != NodeKind::Property {
        return Err("for: iterator must be iter.range(...) in the core".to_string());
    }
    let recv = callee
        .children
        .first()
        .ok_or_else(|| "for: iterator must be iter.range(...) in the core".to_string())?;
    let member = callee
        .children
        .get(1)
        .ok_or_else(|| "for: iterator must be iter.range(...) in the core".to_string())?;
    let recv_text = ctx.source.span(recv.start, recv.length);
    let member_text = ctx.source.span(member.start, member.length);
    if recv_text != "iter" || member_text != "range" {
        return Err("for: iterator must be iter.range(...) in the core".to_string());
    }
    if args.kind != NodeKind::Tuple {
        return Err("for: iter.range arguments must be positional in the core".to_string());
    }
    let arg_children = args.children.as_slice();
    fn eval_arg(node: &Node, ctx: &mut LowerCtx) -> Result<TirExpr, String> {
        let (expr, ty) = lower_expr(node, ctx)?;
        if ty != CoreTy::Int {
            return Err("iter.range bounds must be int".to_string());
        }
        Ok(expr)
    }
    match arg_children.len() {
        1 => {
            let end = eval_arg(&arg_children[0], ctx)?;
            Ok((TirExpr::Int(0), end, TirExpr::Int(1)))
        }
        2 => {
            let start = eval_arg(&arg_children[0], ctx)?;
            let end = eval_arg(&arg_children[1], ctx)?;
            Ok((start, end, TirExpr::Int(1)))
        }
        3 => {
            let start = eval_arg(&arg_children[0], ctx)?;
            let end = eval_arg(&arg_children[1], ctx)?;
            // Step must be a constant integer in the core (dynamic step ??interpreter).
            let Some(step_const) = const_int(&arg_children[2]) else {
                return Err("iter.range step must be a constant int in the core".to_string());
            };
            if step_const == 0 {
                return Err("iter.range step must not be zero".to_string());
            }
            Ok((start, end, TirExpr::Int(step_const)))
        }
        _ => Err("iter.range takes 1 to 3 arguments in the core".to_string()),
    }
}

fn lower_return(node: &Node, ctx: &mut LowerCtx, out: &mut Vec<TirStmt>) -> Result<(), String> {
    match node.children.first() {
        None => out.push(TirStmt::Return(None)),
        Some(expr) => {
            let (lowered, _) = lower_expr(expr, ctx)?;
            out.push(TirStmt::Return(Some(lowered)));
        }
    }
    Ok(())
}

fn const_int(node: &Node) -> Option<i64> {
    match node.kind {
        NodeKind::Integer => match node.value {
            Some(Const::Int(v)) => Some(v),
            _ => None,
        },
        NodeKind::Unary if node.op == Some(TokenType::Minus) => {
            const_int(&node.children[0]).map(|v| -v)
        }
        NodeKind::Unary if node.op == Some(TokenType::Positive) => const_int(&node.children[0]),
        _ => None,
    }
}

/// Map a checker Ty (as declared in a class field or method return) to the
/// core type system. `Class(name)` resolves through the class table.
fn class_member_ty(ctx: &LowerCtx, ty: &Ty) -> Result<CoreTy, String> {
    match ty {
        Ty::Int => Ok(CoreTy::Int),
        Ty::Float => Ok(CoreTy::Float),
        Ty::Bool => Ok(CoreTy::Bool),
        Ty::Str => Ok(CoreTy::Str),
        Ty::None_ => Ok(CoreTy::None_),
        Ty::List(elem) => match elem.as_ref() {
            Ty::Int => Ok(CoreTy::ListInt),
            Ty::Float => Ok(CoreTy::ListFloat),
            other => Err(format!("field of type 'list[{}]' is not supported in the core", other.display())),
        },
        Ty::Dict(key, value) => match (key.as_ref(), value.as_ref()) {
            (Ty::Str, Ty::Int) => Ok(CoreTy::DictStrInt),
            (Ty::Str, Ty::Float) => Ok(CoreTy::DictStrFloat),
            (Ty::Str, other) => Err(format!("field of type 'dict[str, {}]' is not supported in the core", other.display())),
            (other, _) => Err(format!("field of type 'dict[{}, ...]' is not supported in the core", other.display())),
        },
        Ty::Class(name) => {
            let id = ctx.classes.id(name).ok_or_else(|| format!("unknown class '{name}'"))?;
            Ok(CoreTy::Class(id))
        }
        other => Err(format!("field of type '{}' is not supported in the core", other.display())),
    }
}

fn lower_expr(node: &Node, ctx: &mut LowerCtx) -> Result<(TirExpr, CoreTy), String> {
    match node.kind {
        NodeKind::Integer => match node.value {
            Some(Const::Int(v)) => Ok((TirExpr::Int(v), CoreTy::Int)),
            _ => Err("bad integer literal".to_string()),
        },
        NodeKind::Float => match node.value {
            Some(Const::Float(v)) => Ok((TirExpr::Float(v), CoreTy::Float)),
            _ => Err("bad float literal".to_string()),
        },
        NodeKind::Boolean => match node.value {
            Some(Const::Bool(v)) => Ok((TirExpr::Bool(v), CoreTy::Bool)),
            _ => Err("bad bool literal".to_string()),
        },
        NodeKind::String => match &node.value {
            Some(Const::Str(v)) => Ok((TirExpr::Str(v.clone()), CoreTy::Str)),
            _ => Err("bad string literal".to_string()),
        },
        NodeKind::None => Ok((TirExpr::Int(0), CoreTy::None_)),
        NodeKind::Dict => {
            let mut entries: Vec<(TirExpr, TirExpr)> = Vec::with_capacity(node.children.len());
            let mut value_cty: Option<CoreTy> = None;
            for child in &node.children {
                if child.kind != NodeKind::Pair {
                    return Err("dict literal must be {key: value} pairs".to_string());
                }
                let key_node = child
                    .children
                    .first()
                    .ok_or_else(|| "dict pair without a key".to_string())?;
                let value_node = child
                    .children
                    .get(1)
                    .ok_or_else(|| "dict pair without a value".to_string())?;
                let (ke, kty) = lower_expr(key_node, ctx)?;
                if kty != CoreTy::Str {
                    return Err("dict keys must be strings in the core".to_string());
                }
                let (ve, vty) = lower_expr(value_node, ctx)?;
                if let Some(prev) = value_cty {
                    if prev != vty {
                        return Err("dict values must all have the same type".to_string());
                    }
                } else {
                    value_cty = Some(vty);
                }
                entries.push((ke, ve));
            }
            if entries.is_empty() {
                return Err("empty dict literal cannot be typed in the core".to_string());
            }
            let ret = match value_cty.expect("non-empty dict") {
                CoreTy::Int => CoreTy::DictStrInt,
                CoreTy::Float => CoreTy::DictStrFloat,
                _ => return Err("dict values must be int or float in the core".to_string()),
            };
            Ok((TirExpr::Dict(entries), ret))
        }
        NodeKind::Index => {
            let target_node = node
                .children
                .first()
                .ok_or_else(|| "index without a target".to_string())?;
            let key_node = node
                .children
                .get(1)
                .ok_or_else(|| "index without a key".to_string())?;
            let (target, tty) = lower_expr(target_node, ctx)?;
            match tty {
                CoreTy::DictStrInt | CoreTy::DictStrFloat => {
                    let elem_cty = match tty {
                        CoreTy::DictStrInt => CoreTy::Int,
                        _ => CoreTy::Float,
                    };
                    let (key, kty) = lower_expr(key_node, ctx)?;
                    if kty != CoreTy::Str {
                        return Err("dict key must be a string".to_string());
                    }
                    Ok((
                        TirExpr::DictGet(Box::new(target), Box::new(key)),
                        elem_cty,
                    ))
                }
                CoreTy::ListInt | CoreTy::ListFloat => {
                    let (key, kty) = lower_expr(key_node, ctx)?;
                    if kty != CoreTy::Int {
                        return Err("list index must be an int".to_string());
                    }
                    let elem_cty = match tty {
                        CoreTy::ListInt => CoreTy::Int,
                        _ => CoreTy::Float,
                    };
                    Ok((
                        TirExpr::ListGet(Box::new(target), Box::new(key)),
                        elem_cty,
                    ))
                }
                _ => {
                    return Err(
                        "only list/dict indexing is supported in the core".to_string()
                    )
                }
            }
        }
        NodeKind::List => {
            let mut elements = Vec::with_capacity(node.children.len());
            let mut elem_cty: Option<CoreTy> = None;
            for child in &node.children {
                let (e, c) = lower_expr(child, ctx)?;
                if let Some(prev) = elem_cty {
                    if prev != c {
                        return Err("list elements must all have the same type".to_string());
                    }
                } else {
                    elem_cty = Some(c);
                }
                elements.push(e);
            }
            // List literal: empty lists are rejected (no way to infer type).
            let elem_cty = match elem_cty {
                Some(c) => c,
                None => return Err("empty list literal cannot be typed in the core".to_string()),
            };
            let ret = match elem_cty {
                CoreTy::Int => CoreTy::ListInt,
                CoreTy::Float => CoreTy::ListFloat,
                _ => return Err("core lists require int or float elements".to_string()),
            };
            Ok((TirExpr::List(elements), ret))
        }
        NodeKind::Property => {
            // `receiver.member` where receiver is class-typed.
            let receiver = node
                .children
                .first()
                .ok_or_else(|| "property without a receiver".to_string())?;
            let member_node = node
                .children
                .get(1)
                .ok_or_else(|| "property without a member".to_string())?;
            let member = ctx.source.span(member_node.start, member_node.length).to_string();
            let (recv, rty) = lower_expr(receiver, ctx)?;
            let id = match rty {
                CoreTy::Class(id) => id,
                _ => return Err("only class member access is supported in the core".to_string()),
            };
            let Some(field_ty) = ctx.classes.field(id, &member).cloned() else {
                // Not an annotated field: if `member` names an accessor
                // (get/set) on the class, its static return type is `any`
                // in this subset — direct the user at the escape hatch.
                let is_accessor = ctx
                    .classes
                    .setters(id)
                    .is_some_and(|s| s.contains(member.as_str()))
                    || ctx.classes.method(id, &member).is_some();
                if is_accessor {
                    return Err(format!(
                        "computed property '{}' of '{}' has no compile-time return type: annotate it or run with `un build --embed`",
                        member,
                        ctx.classes.names[id as usize],
                    ));
                }
                return Err(format!(
                    "class '{}' has no field or method '{}'",
                    ctx.classes.names[id as usize], member
                ));
            };
            let cty = class_member_ty(ctx, &field_ty)?;
            Ok((
                TirExpr::MemberGet { receiver: Box::new(recv), member: member.into_boxed_str(), ret: tir_ty_of(cty)? },
                cty,
            ))
        }
        NodeKind::Identifier => {
            let name = ctx.source.span(node.start, node.length);
            let slot = slot_of(ctx, &name)
                .ok_or_else(|| format!("unknown variable '{name}'"))?;
            let ty = CoreTy::from(ctx.locals[slot as usize]);
            Ok((TirExpr::Local(slot), ty))
        }
        NodeKind::Unary => lower_unary(node, ctx),
        NodeKind::Binary => lower_binary(node, ctx),
        NodeKind::Ternary => lower_ternary(node, ctx),
        NodeKind::Call => lower_call(node, ctx),
        _ => Err(format!(
            "expression kind {:?} is not supported in the core",
            node.kind
        )),
    }
}

fn lower_unary(node: &Node, ctx: &mut LowerCtx) -> Result<(TirExpr, CoreTy), String> {
    let operand = node
        .children
        .first()
        .ok_or_else(|| "unary without operand".to_string())?;
    let (expr, ty) = lower_expr(operand, ctx)?;
    match node.op {
        Some(TokenType::Minus) => match ty {
            CoreTy::Int | CoreTy::Float => Ok((TirExpr::Un(TirUnOp::Neg, Box::new(expr)), ty)),
            _ => Err("negation requires a number".to_string()),
        },
        Some(TokenType::Positive) => match ty {
            CoreTy::Int | CoreTy::Float => Ok((expr, ty)),
            _ => Err("unary plus requires a number".to_string()),
        },
        Some(TokenType::Not) | Some(TokenType::Bang) => match ty {
            CoreTy::Bool => Ok((TirExpr::Un(TirUnOp::Not, Box::new(expr)), CoreTy::Bool)),
            _ => Err("'not' requires a bool".to_string()),
        },
        Some(TokenType::BNot) => match ty {
            CoreTy::Int => Ok((
                TirExpr::Bin(TirBinOp::BXor, Box::new(expr), Box::new(TirExpr::Int(-1))),
                CoreTy::Int,
            )),
            _ => Err("'~' requires an int".to_string()),
        },
        _ => Err("unsupported unary operator in the core".to_string()),
    }
}

fn promote_int_to_float(expr: TirExpr, ty: CoreTy) -> TirExpr {
    match ty {
        CoreTy::Int => TirExpr::Promote(Box::new(expr)),
        _ => expr,
    }
}

fn lower_binary(node: &Node, ctx: &mut LowerCtx) -> Result<(TirExpr, CoreTy), String> {
    let lhs = node
        .children
        .first()
        .ok_or_else(|| "binary operation without left operand".to_string())?;
    let rhs = node
        .children
        .get(1)
        .ok_or_else(|| "binary operation without right operand".to_string())?;
    let op = node
        .op
        .ok_or_else(|| "binary operation without operator".to_string())?;
    // `x == none` / `x != none` for scalar receivers fold at compile time.
    if matches!(op, TokenType::Equal | TokenType::Unequal) {
        let l_none = lhs.kind == NodeKind::None;
        let r_none = rhs.kind == NodeKind::None;
        if l_none ^ r_none {
            return Ok((TirExpr::Bool(op == TokenType::Unequal), CoreTy::Bool));
        }
    }
    let (a, a_ty) = lower_expr(lhs, ctx)?;
    let (b, b_ty) = lower_expr(rhs, ctx)?;
    lower_binary_op(op, a, a_ty, b, b_ty)
}

fn lower_binary_op(
    op: TokenType,
    a: TirExpr,
    a_ty: CoreTy,
    b: TirExpr,
    b_ty: CoreTy,
) -> Result<(TirExpr, CoreTy), String> {
    let numeric = |t: CoreTy| matches!(t, CoreTy::Int | CoreTy::Float);
    match op {
        TokenType::Plus => {
            // Str + Str concatenates at runtime (Str values carry a live
            // reference to the VM's object; the code generator calls the
            // runtime concat).
            if a_ty == CoreTy::Str && b_ty == CoreTy::Str {
                return Ok((TirExpr::Bin(TirBinOp::Add, Box::new(a), Box::new(b)), CoreTy::Str));
            }
            if !numeric(a_ty) || !numeric(b_ty) {
                return Err("arithmetic requires numbers".to_string());
            }
            let bin = TirBinOp::Add;
            if a_ty == CoreTy::Int && b_ty == CoreTy::Int {
                return Ok((TirExpr::Bin(bin, Box::new(a), Box::new(b)), CoreTy::Int));
            }
            let a = promote_int_to_float(a, a_ty);
            let b = promote_int_to_float(b, b_ty);
            Ok((TirExpr::Bin(bin, Box::new(a), Box::new(b)), CoreTy::Float))
        }
        TokenType::Minus | TokenType::Asterisk => {
            if !numeric(a_ty) || !numeric(b_ty) {
                return Err("arithmetic requires numbers".to_string());
            }
            let bin = match op {
                TokenType::Minus => TirBinOp::Sub,
                _ => TirBinOp::Mul,
            };
            if a_ty == CoreTy::Int && b_ty == CoreTy::Int {
                return Ok((TirExpr::Bin(bin, Box::new(a), Box::new(b)), CoreTy::Int));
            }
            let a = promote_int_to_float(a, a_ty);
            let b = promote_int_to_float(b, b_ty);
            Ok((TirExpr::Bin(bin, Box::new(a), Box::new(b)), CoreTy::Float))
        }
        TokenType::Slash => {
            // int / int keeps its Int operands so the emitter can route
            // div-by-zero to a catchable error; any other numeric pair
            // promotes to f64 (matches the interpreter's float result).
            if a_ty == CoreTy::Int && b_ty == CoreTy::Int {
                return Ok((
                    TirExpr::Bin(TirBinOp::Div, Box::new(a), Box::new(b)),
                    CoreTy::Float,
                ));
            }
            if !numeric(a_ty) || !numeric(b_ty) {
                return Err("'/' requires numbers".to_string());
            }
            let a = promote_int_to_float(a, a_ty);
            let b = promote_int_to_float(b, b_ty);
            Ok((
                TirExpr::Bin(TirBinOp::Div, Box::new(a), Box::new(b)),
                CoreTy::Float,
            ))
        }
        TokenType::DoubleSlash => {
            if a_ty != CoreTy::Int || b_ty != CoreTy::Int {
                return Err("'//' requires int operands in the core".to_string());
            }
            // Truncated division producing an int (matches the runtime).
            Ok((
                TirExpr::Bin(TirBinOp::IDiv, Box::new(a), Box::new(b)),
                CoreTy::Int,
            ))
        }
        TokenType::Percent => {
            if a_ty != CoreTy::Int || b_ty != CoreTy::Int {
                return Err("'%' requires int operands in the core".to_string());
            }
            return Ok((
                TirExpr::Bin(TirBinOp::Mod, Box::new(a), Box::new(b)),
                CoreTy::Int,
            ));
        }
        TokenType::BAnd | TokenType::BOr | TokenType::BXor | TokenType::LeftShift | TokenType::RightShift => {
            if a_ty != CoreTy::Int || b_ty != CoreTy::Int {
                return Err("bitwise operators require int".to_string());
            }
            let bin = match op {
                TokenType::BAnd => TirBinOp::BAnd,
                TokenType::BOr => TirBinOp::BOr,
                TokenType::BXor => TirBinOp::BXor,
                TokenType::LeftShift => TirBinOp::Shl,
                _ => TirBinOp::Shr,
            };
            Ok((TirExpr::Bin(bin, Box::new(a), Box::new(b)), CoreTy::Int))
        }
        TokenType::Equal | TokenType::Unequal => {
            let cmp = if op == TokenType::Equal {
                TirCmp::Eq
            } else {
                TirCmp::Ne
            };
            if a_ty == CoreTy::Int && b_ty == CoreTy::Int {
                return Ok((TirExpr::Cmp(cmp, Box::new(a), Box::new(b)), CoreTy::Bool));
            }
            if numeric(a_ty) && numeric(b_ty) {
                let a = promote_int_to_float(a, a_ty);
                let b = promote_int_to_float(b, b_ty);
                return Ok((TirExpr::Cmp(cmp, Box::new(a), Box::new(b)), CoreTy::Bool));
            }
            if a_ty == b_ty {
                return Ok((TirExpr::Cmp(cmp, Box::new(a), Box::new(b)), CoreTy::Bool));
            }
            Err("'==' operands have different types".to_string())
        }
        TokenType::LessThan | TokenType::GreaterThan | TokenType::LessOrEqual | TokenType::GreaterOrEqual => {
            let cmp = match op {
                TokenType::LessThan => TirCmp::Lt,
                TokenType::GreaterThan => TirCmp::Gt,
                TokenType::LessOrEqual => TirCmp::Le,
                _ => TirCmp::Ge,
            };
            if !numeric(a_ty) || !numeric(b_ty) {
                return Err("comparison requires numbers".to_string());
            }
            if a_ty == CoreTy::Int && b_ty == CoreTy::Int {
                return Ok((TirExpr::Cmp(cmp, Box::new(a), Box::new(b)), CoreTy::Bool));
            }
            let a = promote_int_to_float(a, a_ty);
            let b = promote_int_to_float(b, b_ty);
            Ok((TirExpr::Cmp(cmp, Box::new(a), Box::new(b)), CoreTy::Bool))
        }
        TokenType::And | TokenType::Or => {
            if a_ty != CoreTy::Bool || b_ty != CoreTy::Bool {
                return Err("'and'/'or' require bool operands".to_string());
            }
            // The core forbids side effects, so `and`/`or` on bools are
            // value operations (lowered as boolean bit ops in codegen).
            let bin = if op == TokenType::And {
                TirBinOp::BAnd
            } else {
                TirBinOp::BOr
            };
            Ok((TirExpr::Bin(bin, Box::new(a), Box::new(b)), CoreTy::Bool))
        }
        _ => Err("operator is not supported in the core".to_string()),
    }
}

fn lower_ternary(node: &Node, ctx: &mut LowerCtx) -> Result<(TirExpr, CoreTy), String> {
    let cond = node
        .children
        .first()
        .ok_or_else(|| "ternary without condition".to_string())?;
    let (cond_expr, cond_ty) = lower_expr(cond, ctx)?;
    if cond_ty != CoreTy::Bool {
        return Err("ternary condition must be a bool".to_string());
    }
    let then = node
        .children
        .get(1)
        .ok_or_else(|| "ternary without then-branch".to_string())?;
    let els = node
        .children
        .get(2)
        .ok_or_else(|| "ternary without else-branch".to_string())?;
    let (then_expr, then_ty) = lower_expr(then, ctx)?;
    let (else_expr, else_ty) = lower_expr(els, ctx)?;
    if then_ty == else_ty {
        if then_ty == CoreTy::None_ {
            return Err("ternary branches cannot be none".to_string());
        }
        return Ok((
            TirExpr::Select {
                cond: Box::new(cond_expr),
                then_expr: Box::new(then_expr),
                else_expr: Box::new(else_expr),
            },
            then_ty,
        ));
    }
    let int_float = |t: CoreTy| matches!(t, CoreTy::Int | CoreTy::Float);
    if int_float(then_ty) && int_float(else_ty) {
        let then_e = promote_int_to_float(then_expr, then_ty);
        let else_e = promote_int_to_float(else_expr, else_ty);
        return Ok((
            TirExpr::Select {
                cond: Box::new(cond_expr),
                then_expr: Box::new(then_e),
                else_expr: Box::new(else_e),
            },
            CoreTy::Float,
        ));
    }
    Err("ternary branches must have the same type".to_string())
}

/// Lower list method calls that work in expression position:
/// `xs.pop()`, `xs.pop(i)`, `xs.contains(v)`, `xs.index_of(v)`.
/// Returns None for other members (the caller reports an error).
fn lower_list_method_expr(
    member: &str,
    receiver: TirExpr,
    receiver_ty: CoreTy,
    args_tuple: &Node,
    ctx: &mut LowerCtx,
) -> Result<Option<(TirExpr, CoreTy)>, String> {
    if args_tuple.kind != NodeKind::Tuple {
        return Err("call arguments must be positional in the core".to_string());
    }
    let args = lower_arg_list(args_tuple, ctx)?;
    let elem_cty = match receiver_ty {
        CoreTy::ListInt => CoreTy::Int,
        CoreTy::ListFloat => CoreTy::Float,
        _ => unreachable!(),
    };
    match member {
        "pop" => match args.len() {
            0 => Ok(Some((TirExpr::ListPop(Box::new(receiver)), elem_cty))),
            1 => Ok(Some((TirExpr::ListPopAt(Box::new(receiver), Box::new(args.into_iter().next().unwrap())), elem_cty))),
            _ => Err("'pop' expects zero or one argument".to_string()),
        },
        "contains" => {
            if args.len() != 1 {
                return Err("'contains' expects exactly one argument".to_string());
            }
            Ok(Some((TirExpr::ListContains(Box::new(receiver), Box::new(args.into_iter().next().unwrap())), CoreTy::Bool)))
        }
        "index_of" => {
            if args.len() != 1 {
                return Err("'index_of' expects exactly one argument".to_string());
            }
            Ok(Some((TirExpr::ListIndexOf(Box::new(receiver), Box::new(args.into_iter().next().unwrap())), CoreTy::Int)))
        }
        _ => Ok(None),
    }
}

/// Lower the list-method statement forms: `xs.add(v)`, `xs.insert(v, i)`,
/// `xs.reverse()`, `xs.sort()`. Returns `None` when this isn't one of those
/// shapes (the caller falls back to the expression path).
fn lower_list_method_stmt(node: &Node, ctx: &mut LowerCtx) -> Result<Option<TirStmt>, String> {
    let callee = node
        .children
        .first()
        .ok_or_else(|| "call without callee".to_string())?;
    if callee.kind != NodeKind::Property {
        return Ok(None);
    }
    let member_node = callee.children.get(1).ok_or_else(|| "call without a member".to_string())?;
    let member = ctx.source.span(member_node.start, member_node.length).to_string();
    match member.as_str() {
        "add" | "push" | "insert" | "reverse" | "sort" => {}
        _ => return Ok(None),
    }
    let head = callee.children.first().ok_or_else(|| "call without a receiver".to_string())?;
    let (receiver, rty) = lower_expr(head, ctx)?;
    if !matches!(rty, CoreTy::ListInt | CoreTy::ListFloat) {
        return Err(format!("method '{}' requires a list receiver in the core", member));
    }
    let args_node = node.children.get(1).ok_or_else(|| "call without arguments".to_string())?;
    let args = lower_arg_list(args_node, ctx)?;
    match member.as_str() {
        "add" | "push" => {
            if args.len() != 1 {
                return Err("'add' expects exactly one argument".to_string());
            }
            Ok(Some(TirStmt::ListAdd { receiver, arg: args.into_iter().next().unwrap() }))
        }
        "reverse" => {
            if !args.is_empty() {
                return Err("'reverse' takes no arguments".to_string());
            }
            Ok(Some(TirStmt::ListReverse { receiver }))
        }
        "sort" => {
            if !args.is_empty() {
                return Err("'sort' takes no arguments".to_string());
            }
            Ok(Some(TirStmt::ListSort { receiver }))
        }
        "insert" => {
            if args.len() != 2 {
                return Err("'insert' expects exactly two arguments".to_string());
            }
            let mut iter = args.into_iter();
            let item = iter.next().unwrap();
            let index = iter.next().unwrap();
            Ok(Some(TirStmt::ListInsert { receiver, arg: item, index }))
        }
        _ => Ok(None),
    }
}

/// Lower a call argument tuple into an expression list, enforcing positional
/// arguments (the core forbids keyword/spread arguments).
fn lower_arg_list(args_tuple: &Node, ctx: &mut LowerCtx) -> Result<Vec<TirExpr>, String> {
    if args_tuple.kind != NodeKind::Tuple {
        return Err("call arguments must be positional in the core".to_string());
    }
    let mut args = Vec::with_capacity(args_tuple.children.len());
    for arg in &args_tuple.children {
        let (expr, _) = lower_expr(arg, ctx)?;
        args.push(expr);
    }
    Ok(args)
}

/// Lower `math.<fn>(args...)` into a TIR math intrinsic. Numeric arguments
/// (Int promotes to Float); the result type is Float. Implemented for the
/// same set of operations the bytecode JIT handles; sqrt/log keep their
/// guard semantics (`native/math.rs`).
fn lower_math_call(
    name: &str,
    args_tuple: &Node,
    ctx: &mut LowerCtx,
) -> Result<(TirExpr, CoreTy), String> {
    let arity = args_tuple.children.len();
    let math_fn = match name {
        "sin" => (MathFn::Sin, 1),
        "cos" => (MathFn::Cos, 1),
        "tan" => (MathFn::Tan, 1),
        "asin" => (MathFn::Asin, 1),
        "acos" => (MathFn::Acos, 1),
        "atan" => (MathFn::Atan, 1),
        "sqrt" => (MathFn::Sqrt, 1),
        "cbrt" => (MathFn::Cbrt, 1),
        "exp" => (MathFn::Exp, 1),
        "ln" => (MathFn::Ln, 1),
        "log2" => (MathFn::Log2, 1),
        "log10" => (MathFn::Log10, 1),
        "floor" => (MathFn::Floor, 1),
        "ceil" => (MathFn::Ceil, 1),
        "trunc" => (MathFn::Trunc, 1),
        "abs" => (MathFn::Abs, 1),
        "pow" => (MathFn::Pow, 2),
        "log" => {
            if arity == 1 {
                (MathFn::Ln, 1)
            } else if arity == 2 {
                (MathFn::Log, 2)
            } else {
                return Err(format!("math.log expects 1 or 2 arguments, got {arity}"));
            }
        }
        _ => return Err(format!("math.{name} is not supported in the core")),
    };
    let idx = math_fn;
    if arity != idx.1 {
        return Err(format!(
            "math.{name} expects {} argument(s), got {arity}",
            idx.1
        ));
    }
    let mut promoted = Vec::with_capacity(arity);
    for arg in &args_tuple.children {
        let (expr, t) = lower_expr(arg, ctx)?;
        match t {
            CoreTy::Int | CoreTy::Float => {
                promoted.push(promote_int_to_float(expr, t));
            }
            _ => {
                return Err(format!(
                    "math.{name} requires numeric arguments"
                ))
            }
        }
    }
    Ok((TirExpr::Math(math_fn.0, promoted), CoreTy::Float))
}

fn lower_call(node: &Node, ctx: &mut LowerCtx) -> Result<(TirExpr, CoreTy), String> {
    let callee = node
        .children
        .first()
        .ok_or_else(|| "call without callee".to_string())?;
    if callee.kind == NodeKind::Property {
        let member_node = callee
            .children
            .get(1)
            .ok_or_else(|| "method call without a member name".to_string())?;
        let member = ctx.source.span(member_node.start, member_node.length).to_string();
        let head = callee
            .children
            .first()
            .ok_or_else(|| "method call without a receiver".to_string())?;
        let args_tuple = node
            .children
            .get(1)
            .ok_or_else(|| "call without arguments".to_string())?;
        if args_tuple.kind != NodeKind::Tuple {
            return Err("call arguments must be positional in the core".to_string());
        }
        // `math.sin(v)` etc — single/double-precision float intrinsics.
        if head.kind == NodeKind::Identifier {
            let module = ctx.source.span(head.start, head.length).to_string();
            if module == "math" {
                return lower_math_call(&member, args_tuple, ctx);
            }
        }
        let (recv, rty) = lower_expr(head, ctx)?;
        if matches!(rty, CoreTy::ListInt | CoreTy::ListFloat) {
            // `xs.pop()`, `xs.pop(i)`, `xs.contains(v)`, `xs.index_of(v)`.
            if let Some(list_method) = lower_list_method_expr(&member, recv, rty, args_tuple, ctx)? {
                return Ok(list_method);
            }
            // Fallthrough: unsupported list method.
            return Err(format!(
                "method '{member}' on a list is not supported in the core"
            ));
        }
        let id = match rty {
            CoreTy::Class(id) => id,
            _ => return Err("method call requires a class receiver in the core".to_string()),
        };
        if let Some(members) =
            crate::runtime::world::native_members(ctx.source.span(head.start, head.length).as_str())
        {
            if members.iter().any(|(k, _)| k == &member) {
                return Err("internal: native shape should be handled by the statement path".to_string());
            }
        }
        let args = lower_arg_list(args_tuple, ctx)?;
        let Some(method_ty) = ctx.classes.method(id, &member).cloned() else {
            return Err(format!(
                "class '{}' has no method '{}'",
                ctx.classes.names[id as usize], member
            ));
        };
        let Ty::Func { ret, .. } = &method_ty else {
            return Err(format!("class method '{}' is not callable", member));
        };
        let field_ret = class_member_ty(ctx, ret)?;
        return Ok((
            TirExpr::MethodCall {
                receiver: Box::new(recv),
                member: member.into_boxed_str(),
                args,
                ret: tir_ty_of(field_ret)?,
            },
            field_ret,
        ));
    }
    if callee.kind != NodeKind::Identifier {
        return Err("only direct calls are supported in the core".to_string());
    }
    let name = ctx.source.span(callee.start, callee.length);
    // The `int(x)`/`float(x)` language casts.
    if name == "int" || name == "float" {
        let args_node = node
            .children
            .get(1)
            .ok_or_else(|| "cast without arguments".to_string())?;
        if args_node.kind != NodeKind::Tuple {
            return Err("call arguments must be positional in the core".to_string());
        }
        if args_node.children.len() != 1 {
            return Err(format!("{name}() expects exactly one argument"));
        }
        let (arg, arg_ty) = lower_expr(&args_node.children[0], ctx)?;
        return match name.as_str() {
            "int" => match arg_ty {
                CoreTy::Int => Ok((arg, CoreTy::Int)),
                CoreTy::Float => Ok((TirExpr::CastInt(Box::new(arg)), CoreTy::Int)),
                _ => Err("int() requires a numeric argument".to_string()),
            },
            "float" => match arg_ty {
                CoreTy::Int => Ok((TirExpr::CastFloat(Box::new(arg)), CoreTy::Float)),
                CoreTy::Float => Ok((arg, CoreTy::Float)),
                _ => Err("float() requires a numeric argument".to_string()),
            },
            _ => unreachable!(),
        };
    }
    // The `len(x)` builtin ??`TirExpr::Len` (unless shadowed by a user `len`).
    if name == "len" && !ctx.checked.sigs.contains_key("len") {
        let args_node = node
            .children
            .get(1)
            .ok_or_else(|| "len() without arguments".to_string())?;
        if args_node.kind != NodeKind::Tuple {
            return Err("len() arguments must be positional".to_string());
        }
        if args_node.children.len() != 1 {
            return Err("len() expects exactly one argument".to_string());
        }
        let (arg, arg_ty) = lower_expr(&args_node.children[0], ctx)?;
        match arg_ty {
            CoreTy::Str | CoreTy::ListInt | CoreTy::ListFloat | CoreTy::DictStrInt | CoreTy::DictStrFloat => {}
            _ => return Err("len() requires a string or collection in the core".to_string()),
        }
        return Ok((TirExpr::Len(Box::new(arg)), CoreTy::Int));
    }
    // A class constructor `Point(1, 2)`.
    if let Some(cid) = ctx.classes.id(&name) {
        let args_node = node
            .children
            .get(1)
            .ok_or_else(|| "call without arguments".to_string())?;
        if args_node.kind != NodeKind::Tuple {
            return Err("call arguments must be positional in the core".to_string());
        }
        let args = lower_arg_list(args_node, ctx)?;
        return Ok((
            TirExpr::ClassNew {
                name: name.into_boxed_str(),
                args,
                ret: TirTy::Class(cid),
            },
            CoreTy::Class(cid),
        ));
    }
    if !ctx.checked.sigs.contains_key(&name) {
        return Err(format!("call to unknown function '{name}'"));
    }
    let args_node = node
        .children
        .get(1)
        .ok_or_else(|| "call without arguments".to_string())?;
    if args_node.kind != NodeKind::Tuple {
        return Err("call arguments must be positional in the core".to_string());
    }
    let args = lower_arg_list(args_node, ctx)?;
    let Some(fn_index) = fn_index_of(ctx.checked, &name, node.start) else {
        return Err(format!("call to non-core function '{name}'"));
    };
    let fcheck = &ctx.checked.functions[fn_index];
    let Some(&tir_i) = ctx.tir_index.get(&fn_index) else {
        return Err(format!("call to non-core function '{name}'"));
    };
    let ret = match &fcheck.sig.ret {
        Ty::Int => CoreTy::Int,
        Ty::Float => CoreTy::Float,
        Ty::Bool => CoreTy::Bool,
        Ty::None_ => CoreTy::None_,
        Ty::Str => CoreTy::Str,
        Ty::List(elem) => match elem.as_ref() {
            Ty::Int => CoreTy::ListInt,
            Ty::Float => CoreTy::ListFloat,
            other => {
                return Err(format!(
                    "'{name}' returns 'list[{}]' which is not core-supported",
                    other.display()
                ))
            }
        },
        Ty::Dict(key, value) => match (key.as_ref(), value.as_ref()) {
            (Ty::Str, Ty::Int) => CoreTy::DictStrInt,
            (Ty::Str, Ty::Float) => CoreTy::DictStrFloat,
            _ => {
                return Err(format!(
                    "'{name}' returns 'dict' with non-core key/value types"
                ))
            }
        },
        Ty::Class(name) => {
            let id = ctx.classes.id(name).ok_or_else(|| format!("unknown class '{name}'"))?;
            CoreTy::Class(id)
        }
        other => {
            return Err(format!(
                "'{name}' returns '{}' which is not core-supported",
                other.display()
            ))
        }
    };
    if args.len() != fcheck.sig.params.len() {
        return Err(format!(
            "'{name}' expects {} argument(s), got {}",
            fcheck.sig.params.len(),
            args.len()
        ));
    }
    Ok((TirExpr::Call(tir_i as u32, args), ret))
}

/// Resolve a called function name to its checked index. When two functions
/// share a name (nested shadowing), the one with the largest span that
/// still starts at or before the call site wins (lexical rule).
fn fn_index_of(checked: &CheckedProgram, name: &str, call_span: usize) -> Option<usize> {
    let mut best_containing: Option<(usize, usize)> = None; // (span, index)
    let mut non_containing: Option<usize> = None;
    for (span, &index) in &checked.fn_by_span {
        let fcheck = &checked.functions[index];
        if fcheck.sig.name != name {
            continue;
        }
        if *span <= call_span {
            match best_containing {
                None => best_containing = Some((*span, index)),
                Some((best_span, _)) if *span > best_span => {
                    best_containing = Some((*span, index))
                }
                _ => {}
            }
        } else {
            if non_containing.is_some() {
                return None; // ambiguous
            }
            non_containing = Some(index);
        }
    }
    best_containing.map(|(_, i)| i).or(non_containing)
}

// ---------------------------------------------------------------------------
// Validator: an independent checker for the lowered TIR. Anything the builder
// got wrong is caught here instead of at the FFI boundary.
// ---------------------------------------------------------------------------

fn locals_ty(locals: &[TirTy], slot: u16) -> Option<TirTy> {
    locals.get(slot as usize).copied()
}

fn validate(f: &TirFn, program: &TirProgram) -> Result<(), String> {
    if f.params.len() > f.locals.len() || f.params.len() > MAX_CORE_ARITY {
        return Err("parameter count exceeds local slot budget".to_string());
    }
    // Seed with the parameters only; each `Let` appends its slot.
    let debug_trace = std::env::var_os("UV_TIR_DEBUG").is_some();
    if debug_trace {
        eprintln!(
            "TIRDBG lower fn={} params={:?} locals={:?}",
            f.name,
            f.params,
            f.locals,
        );
    }
    let mut locals = f.locals[..f.params.len()].to_vec();
    for stmt in &f.body {
        collect_decls(stmt, &mut locals)?;
    }
    let mut slot = f.params.len();
    validate_stmt(&f.body, f, program, &mut locals, &mut slot, 0)?;
    if f.ret.is_some() && !body_terminates(&f.body) {
        return Err("function may reach its end without returning a value".to_string());
    }
    Ok(())
}

fn collect_decls(stmt: &TirStmt, locals: &mut Vec<TirTy>) -> Result<(), String> {
    match stmt {
        TirStmt::Let(slot, _) => {
            if (*slot as usize) != locals.len() {
                return Err(format!(
                    "local slot {} does not follow the declaration order (expected {})",
                    slot,
                    locals.len()
                ));
            }
            locals.push(TirTy::Int);
        }
        TirStmt::If { then_body, else_body, .. } => {
            for body in [then_body, else_body] {
                for s in body {
                    collect_decls(s, locals)?;
                }
            }
        }
        TirStmt::While { body, .. } => {
            for s in body {
                collect_decls(s, locals)?;
            }
        }
        TirStmt::ForRange { var, body, .. } => {
            // The range variable is an implicit `let` binding of type int.
            if (*var as usize) != locals.len() {
                return Err(format!(
                    "local slot {} does not follow the declaration order (expected {})",
                    var,
                    locals.len()
                ));
            }
            locals.push(TirTy::Int);
            for s in body {
                collect_decls(s, locals)?;
            }
        }
        TirStmt::ForEach { var, collection: _, body } => {
            // The element variable is an implicit `let` binding; its slot
            // comes right after the collection, same as a ForRange var.
            if (*var as usize) != locals.len() {
                return Err(format!(
                    "local slot {} does not follow the declaration order (expected {})",
                    var,
                    locals.len()
                ));
            }
            // Int placeholder; validate_stmt substitutes the real element type.
            locals.push(TirTy::Int);
            for s in body {
                collect_decls(s, locals)?;
            }
        }
        TirStmt::ListSet { .. } | TirStmt::DictSet { .. } | TirStmt::ListAdd { .. } | TirStmt::ListInsert { .. } | TirStmt::ListReverse { .. } | TirStmt::ListSort { .. } | TirStmt::MemberSet { .. } => {
            // Assignments to indices and list statement-method calls declare no locals.
        }
        TirStmt::Set(_, _) | TirStmt::Break | TirStmt::Skip | TirStmt::NativeCall { .. } | TirStmt::ExprStmt(_) | TirStmt::Return(_) => {
            // No local declarations in these statements.
        }
        TirStmt::Try { body, var, catch_body, finally_body } => {
            // TIR try executes: body ??(on error) optional handler ??finally,
            // so collect declarations in that order.
            for s in body {
                collect_decls(s, locals)?;
            }
            if let Some(slot) = var {
                if (*slot as usize) != locals.len() {
                    return Err(format!(
                        "local slot {} does not follow the declaration order (expected {})",
                        slot,
                        locals.len()
                    ));
                }
                // The `catch err` binding is a raw VM error object — typed
                // `ErrorObj` so eligibility can tell it apart from a class
                // instance (`Class(_)`).
                locals.push(TirTy::ErrorObj);
            }
            for s in catch_body {
                collect_decls(s, locals)?;
            }
            if let Some(finally_body) = finally_body {
                for s in finally_body {
                    collect_decls(s, locals)?;
                }
            }
        }
        TirStmt::Throw(_) => {}
    }
    Ok(())
}

/// Map a checker Ty (class field / method signature) to a core TIR type.
/// Class types get the id `u16::MAX` ??validator equality for classes ignores
/// the id, and the runtime dispatch only cares about the Obj handle.
fn ty_to_tir_simple(ty: &Ty) -> Result<TirTy, String> {
    Ok(match ty {
        Ty::Int => TirTy::Int,
        Ty::Float => TirTy::Float,
        Ty::Bool => TirTy::Bool,
        Ty::Str => TirTy::Str,
        Ty::List(elem) => match elem.as_ref() {
            Ty::Int => TirTy::ListInt,
            Ty::Float => TirTy::ListFloat,
            other => return Err(format!("list[{}] field is not supported", other.display())),
        },
        Ty::Dict(key, value) => match (key.as_ref(), value.as_ref()) {
            (Ty::Str, Ty::Int) => TirTy::DictStrInt,
            (Ty::Str, Ty::Float) => TirTy::DictStrFloat,
            (Ty::Str, other) => return Err(format!("dict[str, {}] field is not supported", other.display())),
            (other, _) => return Err(format!("dict[{}, ...] field is not supported", other.display())),
        },
        Ty::Class(_) => TirTy::Class(u16::MAX),
        other => return Err(format!("field of type '{}' is not supported in the core", other.display())),
    })
}

fn expr_ty(expr: &TirExpr, f: &TirFn, program: &TirProgram, locals: &[TirTy]) -> Result<TirTy, String> {
    match expr {
        TirExpr::Int(_) => Ok(TirTy::Int),
        TirExpr::Float(_) => Ok(TirTy::Float),
        TirExpr::Math(_, args) => {
            // All arguments are promoted to float at lowering time.
            for arg in args {
                let _ = expr_ty(arg, f, program, locals)?;
            }
            Ok(TirTy::Float)
        }
        TirExpr::CastInt(inner) => match expr_ty(inner, f, program, locals)? {
            TirTy::Int | TirTy::Float => Ok(TirTy::Int),
            _ => Err("int() requires a numeric argument".to_string()),
        },
        TirExpr::CastFloat(inner) => match expr_ty(inner, f, program, locals)? {
            TirTy::Int | TirTy::Float => Ok(TirTy::Float),
            _ => Err("float() requires a numeric argument".to_string()),
        }
        TirExpr::Bool(_) => Ok(TirTy::Bool),
        TirExpr::Str(_) => Ok(TirTy::Str),
        TirExpr::ListPop(receiver) | TirExpr::ListPopAt(receiver, _) => {
            let list_ty = expr_ty(receiver, f, program, locals)?;
            match list_ty {
                TirTy::ListInt => Ok(TirTy::Int),
                TirTy::ListFloat => Ok(TirTy::Float),
                _ => Err("list.pop requires a list[int] or list[float]".to_string()),
            }
        }
        TirExpr::ListContains(receiver, _) => {
            match expr_ty(receiver, f, program, locals)? {
                TirTy::ListInt | TirTy::ListFloat => Ok(TirTy::Bool),
                _ => Err("list.contains requires a list[int] or list[float]".to_string()),
            }
        }
        TirExpr::ListIndexOf(receiver, _) => {
            match expr_ty(receiver, f, program, locals)? {
                TirTy::ListInt | TirTy::ListFloat => Ok(TirTy::Int),
                _ => Err("list.index_of requires a list[int] or list[float]".to_string()),
            }
        }
        TirExpr::Len(arg) => {
            match expr_ty(arg, f, program, locals)? {
                TirTy::Str | TirTy::ListInt | TirTy::ListFloat | TirTy::DictStrInt | TirTy::DictStrFloat => {
                    Ok(TirTy::Int)
                }
                _ => Err("len() requires a string or collection in the core".to_string()),
            }
        }
        TirExpr::ListGet(target, index) => {
            if expr_ty(index, f, program, locals)? != TirTy::Int {
                return Err("list index must be an int".to_string());
            }
            match expr_ty(target, f, program, locals)? {
                TirTy::ListInt => Ok(TirTy::Int),
                TirTy::ListFloat => Ok(TirTy::Float),
                _ => Err("indexing requires a list[int] or list[float]".to_string()),
            }
        }
        TirExpr::Dict(entries) => {
            if entries.is_empty() {
                return Err("empty dict literal cannot be typed in the core".to_string());
            }
            let mut vty = None;
            for (key, value) in entries {
                if expr_ty(key, f, program, locals)? != TirTy::Str {
                    return Err("dict keys must be strings".to_string());
                }
                let t = expr_ty(value, f, program, locals)?;
                if let Some(prev) = vty {
                    if prev != t {
                        return Err("dict values must all have the same type".to_string());
                    }
                } else {
                    vty = Some(t);
                }
            }
            match vty {
                Some(TirTy::Int) => Ok(TirTy::DictStrInt),
                Some(TirTy::Float) => Ok(TirTy::DictStrFloat),
                _ => Err("dict values must be int or float in the core".to_string()),
            }
        }
        TirExpr::DictGet(target, key) => {
            if expr_ty(key, f, program, locals)? != TirTy::Str {
                return Err("dict key must be a string".to_string());
            }
            match expr_ty(target, f, program, locals)? {
                TirTy::DictStrInt => Ok(TirTy::Int),
                TirTy::DictStrFloat => Ok(TirTy::Float),
                _ => Err("indexing requires a dict[str, int] or dict[str, float]".to_string()),
            }
        }
        TirExpr::ClassNew { name, args, ret } => {
            // The class must exist in the table; the checker has already
            // validated the constructor signature for us.
            if !program.class_names.iter().any(|n| n.as_str() == name.as_ref()) {
                return Err(format!("unknown class '{name}'"));
            }
            for arg in args {
                let _ = expr_ty(arg, f, program, locals)?;
            }
            Ok(*ret)
        }
        TirExpr::MemberGet { receiver, member, ret } => {
            let recv_ty = expr_ty(receiver, f, program, locals)?;
            let TirTy::Class(id) = recv_ty else {
                return Err("member access requires a class receiver".to_string());
            };
            let cid = id as usize;
            let Some(field_ty) = program
                .class_fields
                .get(cid)
                .and_then(|fields| fields.get(member.as_ref()))
                .cloned()
            else {
                return Err(format!(
                    "class '{}' has no field '{}'",
                    program.class_names[cid], member
                ));
            };
            let expected = ty_to_tir_simple(&field_ty)?;
            if !same_ty(expected, *ret) {
                return Err(format!(
                    "member '{}' of '{}' has an inconsistent type",
                    member, program.class_names[cid]
                ));
            }
            Ok(*ret)
        }
        TirExpr::MethodCall { receiver, member, args, ret } => {
            let recv_ty = expr_ty(receiver, f, program, locals)?;
            let TirTy::Class(id) = recv_ty else {
                return Err("method call requires a class receiver".to_string());
            };
            let cid = id as usize;
            let Some(method_ty) = program
                .class_methods
                .get(cid)
                .and_then(|methods| methods.get(member.as_ref()))
                .cloned()
            else {
                return Err(format!(
                    "class '{}' has no method '{}'",
                    program.class_names[cid], member
                ));
            };
            let Ty::Func { params, ret: func_ret, .. } = &method_ty else {
                return Err(format!("class method '{member}' is not callable"));
            };
            if params.len() != args.len() {
                return Err(format!(
                    "method '{}' expects {} argument(s), got {}",
                    member,
                    params.len(),
                    args.len()
                ));
            }
            for (arg, param) in args.iter().zip(params.iter()) {
                let arg_ty = expr_ty(arg, f, program, locals)?;
                match (ty_to_tir_simple(param), arg_ty) {
                    (Ok(p), a) if same_ty(p, a) => {}
                    (Ok(TirTy::Float), TirTy::Int) => {} // int widens to float
                    _ => {
                        return Err(format!(
                            "argument type mismatch for method '{}'",
                            member
                        ))
                    }
                }
            }
            let expected = ty_to_tir_simple(func_ret)?;
            if !same_ty(expected, *ret) {
                return Err(format!(
                    "method '{}' of '{}' has an inconsistent return type",
                    member, program.class_names[cid]
                ));
            }
            Ok(*ret)
        }
        TirExpr::List(elements) => {
            if elements.is_empty() {
                return Err("empty list literal cannot be typed in the core".to_string());
            }
            let ty = expr_ty(&elements[0], f, program, locals)?;
            for element in &elements[1..] {
                let ety = expr_ty(element, f, program, locals)?;
                if ety != ty {
                    return Err("list elements must all have the same type".to_string());
                }
            }
            if ty == TirTy::Int {
                Ok(TirTy::ListInt)
            } else if ty == TirTy::Float {
                Ok(TirTy::ListFloat)
            } else {
                Err("core lists require int or float elements".to_string())
            }
        }
        TirExpr::Local(slot) => {
            locals_ty(locals, *slot).ok_or_else(|| format!("local slot {} out of range", slot))
        }
        TirExpr::Promote(inner) => {
            let int_ty = expr_ty(inner, f, program, locals)?;
            if int_ty != TirTy::Int {
                return Err("promotion requires an int operand".to_string());
            }
            Ok(TirTy::Float)
        }
        TirExpr::Select { cond, then_expr, else_expr } => {
            if expr_ty(cond, f, program, locals)? != TirTy::Bool {
                return Err("select condition must be a bool".to_string());
            }
            let t = expr_ty(then_expr, f, program, locals)?;
            let e = expr_ty(else_expr, f, program, locals)?;
            if !same_ty(t, e) {
                return Err("select branches must have the same type".to_string());
            }
            Ok(t)
        }
        TirExpr::Bin(op, lhs, rhs) => {
            let a = expr_ty(lhs, f, program, locals)?;
            let b = expr_ty(rhs, f, program, locals)?;
            use TirBinOp::*;
            let numeric = |t: TirTy| matches!(t, TirTy::Int | TirTy::Float);
            match op {
                Add => {
                    if a == TirTy::Str && b == TirTy::Str {
                        Ok(TirTy::Str)
                    } else if a == TirTy::Int && b == TirTy::Int {
                        Ok(TirTy::Int)
                    } else if numeric(a) && numeric(b) {
                        Ok(TirTy::Float)
                    } else {
                        Err("arithmetic requires numbers (or two Str)".to_string())
                    }
                }
                Sub | Mul => {
                    if a == TirTy::Int && b == TirTy::Int {
                        Ok(TirTy::Int)
                    } else if numeric(a) && numeric(b) {
                        Ok(TirTy::Float)
                    } else {
                        Err("arithmetic requires numbers".to_string())
                    }
                }
                Div => {
                    if numeric(a) && numeric(b) {
                        Ok(TirTy::Float)
                    } else {
                        Err("'/' requires numbers".to_string())
                    }
                }
                IDiv | Mod => {
                    if a == TirTy::Int && b == TirTy::Int {
                        Ok(TirTy::Int)
                    } else {
                        Err("'//' and '%' require int operands in the core".to_string())
                    }
                }
                BAnd | BOr | BXor | Shl | Shr => {
                    if a == TirTy::Int && b == TirTy::Int {
                        Ok(TirTy::Int)
                    } else {
                        Err("bitwise operators require int".to_string())
                    }
                }
            }
        }
        TirExpr::Cmp(_, lhs, rhs) => {
            let a = expr_ty(lhs, f, program, locals)?;
            let b = expr_ty(rhs, f, program, locals)?;
            let numeric_pair = matches!((a, b), (TirTy::Int, TirTy::Int) | (TirTy::Int, TirTy::Float) | (TirTy::Float, TirTy::Int) | (TirTy::Float, TirTy::Float));
            if numeric_pair
                || (same_ty(a, b) && !matches!(a, TirTy::ListInt | TirTy::ListFloat | TirTy::DictStrInt | TirTy::DictStrFloat | TirTy::Class(_)))
            {
                return Ok(TirTy::Bool);
            }
            Err("comparison operands have incompatible types".to_string())
        }
        TirExpr::Un(op, inner) => {
            let t = expr_ty(inner, f, program, locals)?;
            match op {
                TirUnOp::Neg => match t {
                    TirTy::Int => Ok(TirTy::Int),
                    TirTy::Float => Ok(TirTy::Float),
                    TirTy::Bool => Err("negation of bool".to_string()),
                    TirTy::Str => Err("negation of str".to_string()),
                    TirTy::ListInt | TirTy::ListFloat | TirTy::DictStrInt | TirTy::DictStrFloat
                    | TirTy::Class(_) | TirTy::ErrorObj => {
                        Err("negation of collection or class instance".to_string())
                    }
                },
                TirUnOp::Not => match t {
                    TirTy::Bool => Ok(TirTy::Bool),
                    TirTy::Int => Ok(TirTy::Int),
                    TirTy::Float => Err("'not' requires a bool or int".to_string()),
                    TirTy::Str => Err("'not' requires a bool or int".to_string()),
                    TirTy::ListInt | TirTy::ListFloat | TirTy::DictStrInt | TirTy::DictStrFloat
                    | TirTy::Class(_) | TirTy::ErrorObj => {
                        Err("'not' requires a bool or int".to_string())
                    }
                },
            }
        }
        TirExpr::Call(index, args) => {
            let callee = program
                .functions
                .get(*index as usize)
                .ok_or_else(|| format!("call to unknown function {}", index))?;
            if args.len() != callee.params.len() {
                return Err(format!(
                    "'{}' expects {} arguments, got {}",
                    callee.name,
                    callee.params.len(),
                    args.len()
                ));
            }
            for (arg, param) in args.iter().zip(&callee.params) {
                let arg_ty = expr_ty(arg, f, program, locals)?;
                if !same_ty(arg_ty, *param) {
                    return Err(format!(
                        "argument type mismatch for '{}': expected '{}', got '{}'",
                        callee.name,
                        param_name(*param),
                        param_name(arg_ty)
                    ));
                }
            }
            match &callee.ret {
                Some(t) => Ok(*t),
                None => Ok(TirTy::Bool), // none return; the value is never consumed
            }
        }
    }
}

fn param_name(ty: TirTy) -> &'static str {
    match ty {
        TirTy::Int => "int",
        TirTy::Float => "float",
        TirTy::Bool => "bool",
        TirTy::Str => "str",
        TirTy::ListInt => "list[int]",
        TirTy::ListFloat => "list[float]",
        TirTy::DictStrInt => "dict[str, int]",
        TirTy::DictStrFloat => "dict[str, float]",
        TirTy::Class(_) => "class",
        TirTy::ErrorObj => "error-obj",
    }
}

fn validate_stmt(
    body: &[TirStmt],
    f: &TirFn,
    program: &TirProgram,
    locals: &mut Vec<TirTy>,
    slot: &mut usize,
    loop_depth: u32,
) -> Result<(), String> {
    for stmt in body {
        match stmt {
            TirStmt::Let(s, expr) => {
                let ty = expr_ty(expr, f, program, locals)?;
                if (*slot as u16) != *s {
                    return Err(format!(
                        "Let slot {} does not match declaration order {}",
                        s, slot
                    ));
                }
                if (*slot as usize) >= locals.len() {
                    locals.push(ty);
                } else {
                    locals[*slot as usize] = ty;
                }
                *slot += 1;
            }
            TirStmt::Set(s, expr) => {
                let ty = expr_ty(expr, f, program, locals)?;
                let expected = locals_ty(locals, *s)
                    .ok_or_else(|| format!("local slot {} out of range", s))?;
                if ty != expected {
                    return Err(format!(
                        "assignment of '{}' to '{}' slot {}",
                        param_name(ty),
                        param_name(expected),
                        s
                    ));
                }
            }
            TirStmt::If { cond, then_body, else_body } => {
                if expr_ty(cond, f, program, locals)? != TirTy::Bool {
                    return Err("if condition must be a bool".to_string());
                }
                validate_stmt(then_body, f, program, locals, slot, loop_depth)?;
                validate_stmt(else_body, f, program, locals, slot, loop_depth)?;
            }
            TirStmt::While { cond, body } => {
                if expr_ty(cond, f, program, locals)? != TirTy::Bool {
                    return Err("while condition must be a bool".to_string());
                }
                validate_stmt(body, f, program, locals, slot, loop_depth + 1)?;
            }
            TirStmt::ForRange { var, start, end, step, body } => {
                if expr_ty(start, f, program, locals)? != TirTy::Int {
                    return Err("range start must be an int".to_string());
                }
                if expr_ty(end, f, program, locals)? != TirTy::Int {
                    return Err("range end must be an int".to_string());
                }
                if expr_ty(step, f, program, locals)? != TirTy::Int {
                    return Err("range step must be an int".to_string());
                }
                // The range variable is an implicit `let` binding of type int.
                if (*var as usize) != *slot {
                    return Err(format!(
                        "range loop variable slot {} does not match declaration order {}",
                        var, slot
                    ));
                }
                if *slot as usize == locals.len() {
                    locals.push(TirTy::Int);
                } else {
                    locals[*slot as usize] = TirTy::Int;
                }
                *slot += 1;
                validate_stmt(body, f, program, locals, slot, loop_depth + 1)?;
            }
            TirStmt::ForEach { var, collection, body } => {
                let ety = match expr_ty(collection, f, program, locals)? {
                    TirTy::ListInt => TirTy::Int,
                    TirTy::ListFloat => TirTy::Float,
                    _ => return Err("for ... in requires a core list".to_string()),
                };
                if (*var as usize) != *slot {
                    return Err(format!(
                        "for ... in loop variable slot {} does not match declaration order {}",
                        var, slot
                    ));
                }
                if *slot as usize == locals.len() {
                    locals.push(ety);
                } else {
                    locals[*slot as usize] = ety;
                }
                *slot += 1;
                validate_stmt(body, f, program, locals, slot, loop_depth + 1)?;
            }
            TirStmt::ListSet { list, index, value } => {
                let list_ty = expr_ty(list, f, program, locals)?;
                let idx_ty = expr_ty(index, f, program, locals)?;
                if idx_ty != TirTy::Int {
                    return Err("list index must be an int".to_string());
                }
                let elem_ty = match list_ty {
                    TirTy::ListInt => TirTy::Int,
                    TirTy::ListFloat => TirTy::Float,
                    _ => return Err("index assignment requires a list[int] or list[float]".to_string()),
                };
                if expr_ty(value, f, program, locals)? != elem_ty {
                    return Err("list elements must all have the same type".to_string());
                }
            }
            TirStmt::DictSet { dict, key, value } => {
                let dict_ty = expr_ty(dict, f, program, locals)?;
                let key_ty = expr_ty(key, f, program, locals)?;
                if key_ty != TirTy::Str {
                    return Err("dict key must be a string".to_string());
                }
                let elem_ty = match dict_ty {
                    TirTy::DictStrInt => TirTy::Int,
                    TirTy::DictStrFloat => TirTy::Float,
                    _ => return Err("index assignment requires a dict[str, int] or dict[str, float]".to_string()),
                };
                if expr_ty(value, f, program, locals)? != elem_ty {
                    return Err("dict values must all have the same type".to_string());
                }
            }
            TirStmt::MemberSet { receiver, member, value } => {
                let recv_ty = expr_ty(receiver, f, program, locals)?;
                let TirTy::Class(id) = recv_ty else {
                    return Err("member assignment requires a class receiver".to_string());
                };
                let cid = id as usize;
                let field_ty = program
                    .class_fields
                    .get(cid)
                    .and_then(|fields| fields.get(member.as_ref()))
                    .cloned();
                let Some(field_ty) = field_ty else {
                    // Property with a setter and no backing field: accept any
                    // value; the VM validates the actual assignment.
                    return if program
                        .class_setters
                        .get(cid)
                        .is_some_and(|s| s.iter().any(|set| set == member.as_ref()))
                    {
                        Ok(())
                    } else {
                        Err(format!(
                            "class '{}' has no field '{}'",
                            program.class_names[cid], member
                        ))
                    };
                };
                let expected = ty_to_tir_simple(&field_ty)?;
                let vty = expr_ty(value, f, program, locals)?;
                let widening = matches!(expected, TirTy::Float) && matches!(vty, TirTy::Int);
                if !widening && !same_ty(expected, vty) {
                    return Err(format!(
                        "cannot assign value to class field '{}'",
                        member
                    ));
                }
            }
            TirStmt::ListAdd { receiver, arg } => {
                let list_ty = expr_ty(receiver, f, program, locals)?;
                let elem = match list_ty {
                    TirTy::ListInt => TirTy::Int,
                    TirTy::ListFloat => TirTy::Float,
                    _ => return Err("list.add requires a list[int] or list[float]".to_string()),
                };
                if expr_ty(arg, f, program, locals)? != elem {
                    return Err("list.add argument must match the list element type".to_string());
                }
            }
            TirStmt::ListInsert { receiver, arg, index } => {
                let list_ty = expr_ty(receiver, f, program, locals)?;
                let elem = match list_ty {
                    TirTy::ListInt => TirTy::Int,
                    TirTy::ListFloat => TirTy::Float,
                    _ => return Err("list.insert requires a list[int] or list[float]".to_string()),
                };
                if expr_ty(arg, f, program, locals)? != elem {
                    return Err("list.insert value must match the list element type".to_string());
                }
                if expr_ty(index, f, program, locals)? != TirTy::Int {
                    return Err("list.insert index must be an int".to_string());
                }
            }
            TirStmt::ListReverse { receiver } | TirStmt::ListSort { receiver } => {
                match expr_ty(receiver, f, program, locals)? {
                    TirTy::ListInt | TirTy::ListFloat => {}
                    _ => return Err("list.reverse/list.sort requires a list[int] or list[float]".to_string()),
                }
            }
            TirStmt::Try { body, var, catch_body, finally_body } => {
                // Declaration order is body ??`err` binding ??catch body ??
                // finally body, which matches the runtime's execution order
                // on either path. Locals are shared between all parts.
                validate_stmt(body, f, program, locals, slot, loop_depth)?;
                if let Some(var_slot) = var {
                    if (*var_slot as usize) >= locals.len() {
                        locals.push(TirTy::ErrorObj);
                    } else {
                        locals[*var_slot as usize] = TirTy::ErrorObj;
                    }
                    *slot += 1;
                }
                validate_stmt(catch_body, f, program, locals, slot, loop_depth)?;
                if let Some(finally_body) = finally_body {
                    validate_stmt(finally_body, f, program, locals, slot, loop_depth)?;
                }
            }
            TirStmt::Throw(expr) => {
                // The thrown value is an Obj (usually Str) ??checked structurally;
                // the VM binds it to the catch variable if there is one.
                let _ = expr_ty(expr, f, program, locals)?;
            }
            TirStmt::Return(expr) => {
                let expected = f.ret;
                match (expected, expr) {
                    (None, None) => {}
                    (Some(_), None) => {
                        return Err("return without a value in a function with a return type".to_string());
                    }
                    (None, Some(_)) => {
                        return Err("return of a value from a 'none' function".to_string());
                    }
                    (Some(ret), Some(expr)) => {
                        let ty = expr_ty(expr, f, program, locals)?;
                        if !same_ty(ty, ret) {
                            return Err(format!(
                                "return of '{}' where '{}' was declared",
                                param_name(ty),
                                param_name(ret)
                            ));
                        }
                    }
                }
            }
            TirStmt::Break | TirStmt::Skip => {
                if loop_depth == 0 {
                    return Err("break/skip outside a loop".to_string());
                }
            }
            TirStmt::ExprStmt(expr) => {
                // Method calls are allowed as bare statements (the result is
                // discarded): `c.bump()` performs the call for its effect.
                if !matches!(expr, TirExpr::Call(..) | TirExpr::MethodCall { .. }) {
                    return Err("expression statement must be a call".to_string());
                }
                let _ = expr_ty(expr, f, program, locals)?;
            }
            TirStmt::NativeCall { args, .. } => {
                for arg in args {
                    let _ = expr_ty(arg, f, program, locals)?;
                }
            }
        }
    }
    Ok(())
}

fn body_terminates(body: &[TirStmt]) -> bool {
    body.iter().any(stmt_terminates)
}

fn stmt_terminates(stmt: &TirStmt) -> bool {
    match stmt {
        TirStmt::Return(_) => true,
        TirStmt::Throw(_) => true,
        TirStmt::If { then_body, else_body, .. } => {
            body_terminates(then_body) && body_terminates(else_body)
        }
        _ => false,
    }
}

impl TirProgram {
    /// Validate every function in the program.
    pub fn validate(&self) -> Result<(), TirError> {
        for f in &self.functions {
            validate(f, self).map_err(|e| TirError(format!("{}: {}", f.name, e)))?;
        }
        Ok(())
    }
}
