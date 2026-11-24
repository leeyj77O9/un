
use crate::ast::{Const, Node, NodeKind};
use crate::source::Source;
use crate::token::TokenType;
use crate::typeck::{CheckedProgram, FnCheck, Ty};
use std::collections::HashMap;

/// FE-01 adds `NodeKind::{TypeAlias, Guard, OrPattern, Bytes}` in parallel;
/// matching the derived `Debug` name keeps this module source-compatible both
/// before and after those variants land.
fn kind_is(node: &Node, name: &str) -> bool {
    format!("{:?}", node.kind) == name
}

/// Element/key kind stored in the generic (handle-based) native collections.
///
/// The standalone runtime stores every collection element as an opaque object
/// handle, so a collection only needs the *kind* of its elements to know how
/// to box on write and unbox on read. `Obj` is an opaque handle whose static
/// type is intentionally unknown to the code generator.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ObjElem {
    Obj,
    Int,
    Float,
    Bool,
    Str,
    Class(u16),
    Lambda,
    /// A collection element that is itself a list. `depth` counts the number
    /// of list layers (>= 1) and `leaf` is the innermost scalar kind. Stored
    /// flat (rather than `Box<ObjElem>`) so `ObjElem`/`TirTy` stay `Copy` for
    /// the many by-value call sites in TIR lowering and code generation.
    List { depth: u8, leaf: LeafElem },
}

/// Innermost scalar kind of a nested `ObjElem::List`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LeafElem {
    Obj,
    Int,
    Float,
    Bool,
    Str,
    Class(u16),
    Lambda,
}

impl LeafElem {
    fn from_elem(elem: ObjElem) -> Self {
        match elem {
            ObjElem::Obj => Self::Obj,
            ObjElem::Int => Self::Int,
            ObjElem::Float => Self::Float,
            ObjElem::Bool => Self::Bool,
            ObjElem::Str => Self::Str,
            ObjElem::Class(id) => Self::Class(id),
            ObjElem::Lambda => Self::Lambda,
            // `list_elem` flattens nested lists, so this is unreachable.
            ObjElem::List { .. } => Self::Obj,
        }
    }
}

impl ObjElem {
    fn from_leaf(leaf: LeafElem) -> Self {
        match leaf {
            LeafElem::Obj => Self::Obj,
            LeafElem::Int => Self::Int,
            LeafElem::Float => Self::Float,
            LeafElem::Bool => Self::Bool,
            LeafElem::Str => Self::Str,
            LeafElem::Class(id) => Self::Class(id),
            LeafElem::Lambda => Self::Lambda,
        }
    }
}

/// Wrap an element kind as a list-of-that-kind, flattening an existing nested
/// list so `list[list[int]]` is encoded as `List { depth: 2, leaf: Int }`.
fn list_elem(elem: ObjElem) -> ObjElem {
    match elem {
        ObjElem::List { depth, leaf } => ObjElem::List {
            depth: depth.saturating_add(1),
            leaf,
        },
        other => ObjElem::List {
            depth: 1,
            leaf: LeafElem::from_elem(other),
        },
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TirTy {
    Int,
    Float,
    Bool,
    Str,
    /// Opaque handle to a binary `bytes` value (NAT-07/RT-03). Deliberately
    /// kept out of the standalone `nrt_supported` allowlist: NRT has no bytes
    /// ABI yet, so programs using it must use the VM/`--embed` path.
    Bytes,
    ListInt,
    ListFloat,
    ListClass(u16),
    DictStrInt,
    DictStrFloat,
    DictIntInt,
    Class(u16),
    ErrorObj,
    /// A statically resolved lambda: index of the synthesized `TirFn`.
    Lambda(u32),
    /// A list whose elements are closure objects.
    ListLambda,
    /// An opaque object handle (e.g. an element of a generic collection whose
    /// precise static type is not needed by the emitted code).
    Obj,
    /// A list whose element kind is carried explicitly rather than hardcoded.
    ListGen(ObjElem),
    /// A dict whose key/value kinds are carried explicitly.
    DictGen(ObjElem, ObjElem),
    /// A set whose element kind is carried explicitly (NRT tag 15).
    SetGen(ObjElem),
    /// A tuple whose element kind is carried explicitly (NRT tag 14).
    /// Tuples are list-backed at construction but carry a distinct handle tag
    /// so codegen can dispatch through `("tuple", "new" | "get")`.
    TupleGen(ObjElem),
}

/// Collapse a TIR type into the element kind used by native collections.
pub(crate) fn elem_of_tir(ty: TirTy) -> ObjElem {
    match ty {
        TirTy::Int => ObjElem::Int,
        TirTy::Float => ObjElem::Float,
        TirTy::Bool => ObjElem::Bool,
        TirTy::Str => ObjElem::Str,
        TirTy::Class(id) => ObjElem::Class(id),
        TirTy::Lambda(_) => ObjElem::Lambda,
        // Collection types used as elements are object handles, but keeping
        // their own element kind lets `xs[0][1]` lower to a typed read instead
        // of an opaque `ObjElem::Obj` handle.
        TirTy::ListInt => list_elem(ObjElem::Int),
        TirTy::ListFloat => list_elem(ObjElem::Float),
        TirTy::ListClass(id) => list_elem(ObjElem::Class(id)),
        TirTy::ListLambda => list_elem(ObjElem::Lambda),
        TirTy::ListGen(elem) => list_elem(elem),
        _ => ObjElem::Obj,
    }
}

/// Element kind for a `Ty` used where no `ClassTable` is available.
fn elem_of_tir_field(ty: &Ty) -> ObjElem {
    match ty {
        Ty::Int => ObjElem::Int,
        Ty::Float => ObjElem::Float,
        Ty::Bool => ObjElem::Bool,
        Ty::Str => ObjElem::Str,
        Ty::Class(_) => ObjElem::Class(u16::MAX),
        Ty::Func { .. } => ObjElem::Lambda,
        Ty::List(elem) => list_elem(elem_of_tir_field(elem)),
        _ => ObjElem::Obj,
    }
}

/// The TIR type of a value read out of a generic collection element.
pub(crate) fn tir_of_elem(elem: ObjElem) -> TirTy {
    match elem {
        ObjElem::Obj => TirTy::Obj,
        ObjElem::Int => TirTy::Int,
        ObjElem::Float => TirTy::Float,
        ObjElem::Bool => TirTy::Bool,
        ObjElem::Str => TirTy::Str,
        ObjElem::Class(id) => TirTy::Class(id),
        ObjElem::Lambda => TirTy::Lambda(u32::MAX),
        ObjElem::List { depth, leaf } => {
            if depth <= 1 {
                TirTy::ListGen(ObjElem::from_leaf(leaf))
            } else {
                TirTy::ListGen(ObjElem::List {
                    depth: depth - 1,
                    leaf,
                })
            }
        }
    }
}

/// NRT C ABI scalar kind for a TIR type.
///
/// The values are part of the standalone runtime ABI and must not change:
/// `0` Int, `1` Float, `2` Bool, `3` object handle. Code generators and the
/// generated driver use this table so a new `TirTy` variant is classified in
/// exactly one place.
pub fn abi_kind(ty: TirTy) -> u8 {
    match ty {
        TirTy::Int => 0,
        TirTy::Float => 1,
        TirTy::Bool => 2,
        _ => 3,
    }
}

/// True for every type whose native ABI value is an object handle (I64).
pub(crate) fn tir_is_ref(ty: TirTy) -> bool {
    abi_kind(ty) == 3
}

fn tir_ty_from_ty<'a>(classes: &'a ClassTable, ty: &'a Ty) -> Option<TirTy> {
    match ty {
        Ty::Int => Some(TirTy::Int),
        Ty::Float => Some(TirTy::Float),
        Ty::Bool => Some(TirTy::Bool),
        Ty::Str => Some(TirTy::Str),
        Ty::Bytes => Some(TirTy::Bytes),
        Ty::Class(name) => classes.id(name).map(TirTy::Class),
        Ty::List(elem) => match elem.as_ref() {
            Ty::Int => Some(TirTy::ListInt),
            Ty::Float => Some(TirTy::ListFloat),
            Ty::Class(name) => classes.id(name).map(TirTy::ListClass),
            Ty::Func { .. } => Some(TirTy::ListLambda),
            other => Some(TirTy::ListGen(
                tir_ty_from_ty(classes, other).map_or(ObjElem::Obj, elem_of_tir),
            )),
        },
        Ty::Dict(key, value) => {
            let k = match key.as_ref() {
                Ty::Str => Some(ObjElem::Str),
                Ty::Int => Some(ObjElem::Int),
                _ => None,
            };
            match k {
                Some(k) => {
                    let v = tir_ty_from_ty(classes, value).map_or(ObjElem::Obj, elem_of_tir);
                    match (k, v) {
                        (ObjElem::Str, ObjElem::Int) => Some(TirTy::DictStrInt),
                        (ObjElem::Str, ObjElem::Float) => Some(TirTy::DictStrFloat),
                        (ObjElem::Int, ObjElem::Int) => Some(TirTy::DictIntInt),
                        _ => Some(TirTy::DictGen(k, v)),
                    }
                }
                None => None,
            }
        }
        Ty::Set(elem) => tir_ty_from_ty(classes, elem).map(|t| TirTy::SetGen(elem_of_tir(t))),
        // An NRT tuple handle (tag 14) carries its element kind explicitly.
        // Derive that kind from `Ty::Tuple` so homogeneous tuples (and empty
        // literals with an annotated tuple type) keep the exact element kind
        // instead of degrading to an opaque handle.
        Ty::Tuple(items) => {
            let mut elem: Option<ObjElem> = None;
            for item in items.iter() {
                let next = tir_ty_from_ty(classes, item).map_or(ObjElem::Obj, elem_of_tir);
                elem = Some(match elem {
                    None => next,
                    Some(prev) if prev == next => prev,
                    _ => ObjElem::Obj,
                });
            }
            Some(TirTy::TupleGen(elem.unwrap_or(ObjElem::Obj)))
        }
        Ty::Func { .. } => Some(TirTy::Lambda(u32::MAX)),
        // Dynamic values are boxed NObj handles. Reusing `TirTy::Obj` keeps
        // the ABI table single-sourced: object handles are I64 bits in both
        // the object emitter and the standalone runtime.
        Ty::Any | Ty::Union(_) | Ty::Unknown => Some(TirTy::Obj),
        _ => None,
    }
}

/// Replace type variables in a `Ty` with their inferred concrete types.
fn subst_ty(ty: &Ty, subst: &HashMap<u32, Ty>) -> Ty {
    match ty {
        Ty::Generic { id, .. } => subst.get(id).cloned().unwrap_or(Ty::Any),
        Ty::List(a) => Ty::List(std::rc::Rc::new(subst_ty(a, subst))),
        Ty::Set(a) => Ty::Set(std::rc::Rc::new(subst_ty(a, subst))),
        Ty::Dict(k, v) => Ty::Dict(
            std::rc::Rc::new(subst_ty(k, subst)),
            std::rc::Rc::new(subst_ty(v, subst)),
        ),
        Ty::Tuple(items) => Ty::Tuple(items.iter().map(|t| subst_ty(t, subst)).collect()),
        Ty::Union(members) => Ty::union(members.iter().map(|t| subst_ty(t, subst)).collect()),
        Ty::Func { params, names, ret, required } => Ty::Func {
            params: params.iter().map(|t| subst_ty(t, subst)).collect(),
            names: names.clone(),
            ret: std::rc::Rc::new(subst_ty(ret, subst)),
            required: *required,
        },
        other => other.clone(),
    }
}

/// Identifier-safe rendering of a concrete type for mangled instance names.
fn mangle_ty(ty: &Ty) -> String {
    match ty {
        Ty::Int => "int".into(),
        Ty::Float => "float".into(),
        Ty::Bool => "bool".into(),
        Ty::Str => "str".into(),
        Ty::Bytes => "bytes".into(),
        Ty::None_ => "none".into(),
        Ty::Any | Ty::Unknown | Ty::Generic { .. } => "any".into(),
        Ty::List(a) => format!("list_{}", mangle_ty(a)),
        Ty::Set(a) => format!("set_{}", mangle_ty(a)),
        Ty::Dict(k, v) => format!("dict_{}_{}", mangle_ty(k), mangle_ty(v)),
        Ty::Tuple(items) => format!("tuple_{}", items.iter().map(mangle_ty).collect::<Vec<_>>().join("_")),
        Ty::Union(members) => format!("union_{}", members.iter().map(mangle_ty).collect::<Vec<_>>().join("_")),
        Ty::Class(name) => format!("class_{}", name),
        Ty::Func { .. } => "fn".into(),
    }
}

/// The result of expanding generic functions into concrete instances.
struct Expanded {
    checked: CheckedProgram,
    /// Call node start -> mangled callee name for that instantiation.
    call_names: HashMap<usize, String>,
    /// Mangled instance name -> (original function name, concrete type args).
    instance_args: HashMap<String, (String, Vec<Ty>)>,
    /// (source function node start, checked-function index) pairs to lower.
    lower: Vec<(usize, usize)>,
}

/// Clone each generic function once per distinct set of concrete type
/// arguments inferred at its call sites, and rewrite call sites to the
/// mangled instance names. Original generic declarations stay in the program
/// (so name-based lookups during validation still resolve) but are not lowered.
fn expand_generics(checked: &CheckedProgram) -> Expanded {
    let mut out = CheckedProgram {
        sigs: checked.sigs.clone(),
        classes: checked.classes.clone(),
        aliases: checked.aliases.clone(),
        functions: checked.functions.clone(),
        fn_by_span: checked.fn_by_span.clone(),
        fn_generics: checked.fn_generics.clone(),
        generic_bounds: checked.generic_bounds.clone(),
        call_instances: checked.call_instances.clone(),
    };
    let mut call_names: HashMap<usize, String> = HashMap::new();
    let mut instance_args: HashMap<String, (String, Vec<Ty>)> = HashMap::new();
    let mut extra: Vec<(usize, usize)> = Vec::new();

    // Every concrete type-argument set seen anywhere in the program. Instances
    // are created for the cross product with generic functions of matching
    // arity, so a generic callee reached only from another generic body still
    // has an instance to dispatch to.
    let mut arg_sets: Vec<Vec<Ty>> = Vec::new();
    for inst in checked.call_instances.values() {
        if inst.type_args.iter().any(|t| matches!(t, Ty::Generic { .. })) {
            continue;
        }
        if inst.type_args.is_empty() {
            continue;
        }
        if !arg_sets.contains(&inst.type_args) {
            arg_sets.push(inst.type_args.clone());
        }
    }

    let generic_names: Vec<String> = checked
        .fn_generics
        .iter()
        .filter(|(_, params)| !params.is_empty())
        .map(|(name, _)| name.clone())
        .collect();
    for name in generic_names {
        let Some(orig_idx) = checked.functions.iter().position(|f| f.sig.name == name) else {
            continue;
        };
        let params = &checked.fn_generics[&name];
        let node_start = checked
            .fn_by_span
            .iter()
            .find(|(_, &idx)| idx == orig_idx)
            .map(|(span, _)| *span);
        let Some(node_start) = node_start else { continue };

        for type_args in arg_sets.iter().filter(|a| a.len() == params.len()) {
            let subst: HashMap<u32, Ty> = params
                .iter()
                .zip(type_args.iter())
                .map(|((_, id), ty)| (*id, ty.clone()))
                .collect();
            let mangled = format!(
                "__mono_{}__{}",
                name,
                type_args.iter().map(mangle_ty).collect::<Vec<_>>().join("__")
            );
            if instance_args.contains_key(&mangled) {
                continue;
            }
            let mut func = checked.functions[orig_idx].clone();
            func.sig.name = mangled.clone();
            func.sig.params = func.sig.params.iter().map(|t| subst_ty(t, &subst)).collect();
            func.sig.ret = subst_ty(&func.sig.ret, &subst);
            func.locals = func.locals.iter().map(|(n, t)| (n.clone(), subst_ty(t, &subst))).collect();
            func.local_types = func.local_types.iter().map(|(n, t)| (n.clone(), subst_ty(t, &subst))).collect();
            let index = out.functions.len();
            out.functions.push(func);
            out.sigs.insert(mangled.clone(), out.functions[index].sig.clone());
            instance_args.insert(mangled.clone(), (name.clone(), type_args.clone()));
            extra.push((node_start, index));
        }
    }

    // Resolved call sites bind to their own instance; calls still carrying type
    // variables are resolved through the enclosing instance's arguments.
    for (&span, inst) in &checked.call_instances {
        if inst.type_args.iter().any(|t| matches!(t, Ty::Generic { .. })) || inst.type_args.is_empty() {
            call_names.insert(span, format!("@dep:{}", inst.callee));
            continue;
        }
        let mangled = format!(
            "__mono_{}__{}",
            inst.callee,
            inst.type_args.iter().map(mangle_ty).collect::<Vec<_>>().join("__")
        );
        call_names.insert(span, mangled);
    }

    let generic_set: std::collections::HashSet<String> = checked
        .fn_generics
        .iter()
        .filter(|(_, params)| !params.is_empty())
        .map(|(name, _)| name.clone())
        .collect();
    let mut lower = Vec::new();
    for (idx, f) in checked.functions.iter().enumerate() {
        if generic_set.contains(&f.sig.name) {
            continue;
        }
        let span = checked.fn_by_span.iter().find(|(_, &i)| i == idx).map(|(s, _)| *s);
        if let Some(span) = span {
            lower.push((span, idx));
        }
    }
    lower.extend(extra);

    Expanded { checked: out, call_names, instance_args, lower }
}



#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TirBinOp {
    Add,
    Sub,
    Mul,
    IDiv,
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

/// Minimal pattern form for `match` in the standalone (no-VM) build.
///
/// The bytecode `Pattern` enum is index-based and depends on VM tables, so TIR
/// carries its own lightweight representation. Only the three forms that need
/// no runtime scope object are represented here: wildcard (always matches),
/// bind (receives the scrutinee value in an arm-local slot) and literal
/// (compares via `TirExpr::Cmp(Eq, ..)`).
///
/// Or-patterns are flattened by the lowering step into several arms, each
/// carrying one of these leaf patterns; per-alternative bind slots are copied
/// into the body-visible slots by a `TirStmt::Set` at the head of the winning
/// arm's body (the "winner commit"). Tuple/list destructuring and
/// class/variant patterns still stay on the interpreter path.
#[derive(Clone, Debug)]
pub enum TirPattern {
    Wildcard,
    /// Arm-local slot that receives the scrutinee value when the arm matches.
    Bind(u16),
    /// Constant to compare against (Int/Float/Bool/Str/None).
    Literal(TirExpr),
}

/// One `match` arm. A guard runs *after* the pattern (including its bind slot)
/// succeeds; a false guard falls through to the next arm, just like the VM's
/// shared pattern/guard failure landing pad in `compiler.rs::code_match`.
#[derive(Clone, Debug)]
pub struct TirMatchArm {
    pub pattern: TirPattern,
    /// `case p if cond -> ...`; `None` for an unguarded arm.
    pub guard: Option<TirExpr>,
    /// Body statements, ending with a write to the match result local.
    pub body: Vec<TirStmt>,
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
    Promote(Box<TirExpr>),
    Select {
        cond: Box<TirExpr>,
        then_expr: Box<TirExpr>,
        else_expr: Box<TirExpr>,
    },
    Bin(TirBinOp, Box<TirExpr>, Box<TirExpr>),
    Cmp(TirCmp, Box<TirExpr>, Box<TirExpr>),
    Un(TirUnOp, Box<TirExpr>),
    List(Vec<TirExpr>),
    Dict(Vec<(TirExpr, TirExpr)>),
    Len(Box<TirExpr>),
    ListPop(Box<TirExpr>),
    ListPopAt(Box<TirExpr>, Box<TirExpr>),
    ListContains(Box<TirExpr>, Box<TirExpr>),
    ListIndexOf(Box<TirExpr>, Box<TirExpr>),
    /// `list.remove(value)`: remove the first equal element, return Bool.
    ListRemove(Box<TirExpr>, Box<TirExpr>),
    /// `dict.keys()` / `dict.values()`: snapshot into a new list.
    DictKeys(Box<TirExpr>),
    DictValues(Box<TirExpr>),
    ListRepeat { value: Box<TirExpr>, len: Box<TirExpr>, ty: TirTy },
    DictGet(Box<TirExpr>, Box<TirExpr>),
    ListGet(Box<TirExpr>, Box<TirExpr>),
    NativeCallExpr { module: Box<str>, member: Box<str>, args: Vec<TirExpr>, ret: TirTy },
    ClassNew { name: Box<str>, args: Vec<TirExpr>, ret: TirTy },
    MemberGet { receiver: Box<TirExpr>, member: Box<str>, ret: TirTy },
    MethodCall { receiver: Box<TirExpr>, member: Box<str>, args: Vec<TirExpr>, ret: TirTy },
    CastInt(Box<TirExpr>),
    CastFloat(Box<TirExpr>),
    Math(MathFn, Vec<TirExpr>),
    Call(u32, Vec<TirExpr>),
    /// Source location attached to an error-producing expression.
    Located { span: (usize, usize), inner: Box<TirExpr> },
    /// Allocate a first-class closure value from a synthesized lambda.
    MakeClosure {
        func_index: u32,
        captures: Vec<u16>,
        kinds: Vec<u8>,
        ret_kind: u8,
    },
    /// Call a callable value (closure object) held in a local or container.
    CallObj { callee: Box<TirExpr>, args: Vec<TirExpr>, ret: TirTy },
    /// Read a process-global slot (inlined module state).
    GlobalGet(u16),
    /// The `none` value (boxed by NRT as a proper object handle).
    NoneValue,
    /// A slice read `xs[a:b:c]`. Omitted bounds stay `None` and are passed to
    /// NRT as the boxed none handle (tag 4), matching the VM's `Option<Obj>`.
    Slice {
        list: Box<TirExpr>,
        start: Option<Box<TirExpr>>,
        end: Option<Box<TirExpr>>,
        step: Option<Box<TirExpr>>,
    },
    /// An empty list/dict literal whose element kinds come from the declared
    /// type (expected-type propagation from `bind_annotated` in typeck).
    EmptyCollection {
        ty: TirTy,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathFn {
    Sin, Cos, Tan, Asin, Acos, Atan, Sqrt, Cbrt, Exp, Ln, Log2, Log10,
    Log, Floor, Ceil, Trunc, Abs, Pow,
}

#[derive(Clone, Debug)]
pub enum TirStmt {
    Let(u16, TirExpr),
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
    ForEach {
        var: u16,
        collection: TirExpr,
        body: Vec<TirStmt>,
    },
    ListSet {
        list: TirExpr,
        index: TirExpr,
        value: TirExpr,
    },
    DictSet {
        dict: TirExpr,
        key: TirExpr,
        value: TirExpr,
    },
    MemberSet {
        receiver: TirExpr,
        member: Box<str>,
        value: TirExpr,
    },
    ListAdd { receiver: TirExpr, arg: TirExpr },
    ListInsert { receiver: TirExpr, arg: TirExpr, index: TirExpr },
    ListReverse { receiver: TirExpr },
    ListSort { receiver: TirExpr },
    /// Dormant: the VM/typeck does not yet expose `list.clear`, so lowering
    /// never emits this today (doing so would diverge from `un run`). Kept so
    /// enabling it later only needs a lowering arm.
    ListClear { receiver: TirExpr },
    DictClear { receiver: TirExpr },
    Try {
        body: Vec<TirStmt>,
        var: Option<u16>,
        catch_body: Vec<TirStmt>,
        finally_body: Option<Vec<TirStmt>>,
    },
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
    ExprStmt(TirExpr),
    GlobalSet(u16, TirExpr),
    NativeCall {
        module: String,
        member: String,
        args: Vec<TirExpr>,
    },
    /// Slice assignment `xs[a:b:c] = value`. Bounds use the same boxed-handle
    /// convention as `TirExpr::Slice`.
    SetSlice {
        list: TirExpr,
        value: TirExpr,
        start: Option<TirExpr>,
        end: Option<TirExpr>,
        step: Option<TirExpr>,
    },
    /// Straight-line `match` lowering (wildcard / bind / literal patterns,
    /// optional guards, or-patterns flattened into several arms). `scrutinee`
    /// is evaluated once (bound to a local by the lowering step) and reused
    /// for every arm. Each arm writes its value to `result` (a local declared
    /// by the lowering step); the match expression then reads that local.
    /// `span` is the source range of the whole `match`, used for the no-match
    /// error site, which becomes reachable when a guard fails.
    Match {
        scrutinee: u16,
        arms: Vec<TirMatchArm>,
        result: u16,
        result_ty: TirTy,
        span: (usize, usize),
    },
}

#[derive(Clone, Debug)]
pub struct TirFn {
    pub name: String,
    pub proto_span: (usize, usize),
    pub index: usize,
    pub params: Vec<TirTy>,
    /// Lowered default expressions, aligned with `params` (`None` for required).
    pub defaults: Vec<Option<TirExpr>>,
    pub ret: Option<TirTy>,
    pub locals: Vec<TirTy>,
    pub body: Vec<TirStmt>,
}

pub struct TirProgram {
    pub functions: Vec<TirFn>,
    pub class_names: Vec<String>,
    pub class_fields: Vec<std::collections::HashMap<String, Ty>>,
    pub class_methods: Vec<std::collections::HashMap<String, Ty>>,
    pub class_setters: Vec<std::collections::HashSet<String>>,
    /// Declared field order per class (for positional ctor assignment).
    pub class_field_order: Vec<Vec<String>>,
    /// True if any function writes a global (VM uses different semantics).
    pub has_global_write: bool,
}

/// True when every function can be emitted against the standalone native
/// runtime (`libnrt.a`) without any VM bridge. Supported: scalars, Str, lists
/// and dicts, classes (incl. inheritance), closures, try/catch, native modules
/// (except `random`, which is time-seeded and cannot match `un run` byte for
/// byte, and `bytes`/binary fs helpers, which have no standalone NRT ABI).
/// Dynamic `Any`/union values are tagged NObj handles whose operators dispatch
/// through the NRT bridge, so they are supported too. Module-global side
/// effects beyond simple inlined assignments stay on the `--embed` path.
pub fn nrt_supported(tir: &TirProgram) -> bool {
    fn ty_ok(ty: TirTy) -> bool {
        matches!(
            ty,
            TirTy::Int
                | TirTy::Float
                | TirTy::Bool
                | TirTy::Str
                | TirTy::ListInt
                | TirTy::ListFloat
                | TirTy::DictStrInt
                | TirTy::DictStrFloat
                | TirTy::DictIntInt
                | TirTy::Class(_)
                | TirTy::ListClass(_)
                | TirTy::ErrorObj
                | TirTy::Lambda(_)
                | TirTy::ListLambda
                | TirTy::Obj
                | TirTy::Bytes
                | TirTy::ListGen(_)
                | TirTy::DictGen(_, _)
                | TirTy::SetGen(_) | TirTy::TupleGen(_)
        )
    }
    fn expr_ok(e: &TirExpr) -> bool {
        match e {
            TirExpr::Int(_) | TirExpr::Float(_) | TirExpr::Bool(_) | TirExpr::Str(_)
            | TirExpr::Local(_) | TirExpr::NoneValue => true,
            TirExpr::Promote(a) | TirExpr::Un(_, a) | TirExpr::CastInt(a) | TirExpr::CastFloat(a)
            | TirExpr::Len(a) | TirExpr::ListPop(a) | TirExpr::ListPopAt(a, _)
            | TirExpr::ListContains(a, _) | TirExpr::ListIndexOf(a, _)
            | TirExpr::DictKeys(a) | TirExpr::DictValues(a) => expr_ok(a),
            TirExpr::ListRemove(a, b) => expr_ok(a) && expr_ok(b),
            TirExpr::Select { cond, then_expr, else_expr } => {
                expr_ok(cond) && expr_ok(then_expr) && expr_ok(else_expr)
            }
            TirExpr::Bin(_, a, b) | TirExpr::Cmp(_, a, b) => expr_ok(a) && expr_ok(b),
            TirExpr::ListGet(a, b) => expr_ok(a) && expr_ok(b),
            TirExpr::List(elems) => elems.iter().all(expr_ok),
            TirExpr::Dict(entries) => entries.iter().all(|(k, v)| expr_ok(k) && expr_ok(v)),
            TirExpr::DictGet(a, b) => expr_ok(a) && expr_ok(b),
            TirExpr::ListRepeat { value, len, .. } => expr_ok(value) && expr_ok(len),
            TirExpr::Math(_, args) | TirExpr::Call(_, args) => args.iter().all(expr_ok),
            TirExpr::NativeCallExpr { module, member, args, ret, .. } => {
                native_call_ok(module.as_ref(), member.as_ref(), Some(*ret))
                    && args.iter().all(expr_ok)
            }
            TirExpr::ClassNew { args, .. } | TirExpr::MethodCall { args, .. } => args.iter().all(expr_ok),
            TirExpr::MemberGet { receiver, .. } => expr_ok(receiver),
            TirExpr::MakeClosure { .. } | TirExpr::GlobalGet(_) | TirExpr::EmptyCollection { .. } => true,
            TirExpr::CallObj { callee, args, .. } => expr_ok(callee) && args.iter().all(expr_ok),
            TirExpr::Located { inner, .. } => expr_ok(inner),
            TirExpr::Slice { list, start, end, step } => {
                expr_ok(list)
                    && start.as_ref().map_or(true, |e| expr_ok(e))
                    && end.as_ref().map_or(true, |e| expr_ok(e))
                    && step.as_ref().map_or(true, |e| expr_ok(e))
            }
        }
    }
    fn stmt_ok(s: &TirStmt) -> bool {
        match s {
            TirStmt::Let(_, e) | TirStmt::Set(_, e) | TirStmt::ExprStmt(e) | TirStmt::Return(Some(e)) => expr_ok(e),
            TirStmt::Return(None) | TirStmt::Break | TirStmt::Skip => true,
            TirStmt::Throw(e) | TirStmt::GlobalSet(_, e) => expr_ok(e),
            TirStmt::Try { body, catch_body, finally_body, .. } => {
                body.iter().all(stmt_ok)
                    && catch_body.iter().all(stmt_ok)
                    && finally_body.as_ref().map_or(true, |b| b.iter().all(stmt_ok))
            }
            TirStmt::If { cond, then_body, else_body } => {
                expr_ok(cond) && then_body.iter().all(stmt_ok) && else_body.iter().all(stmt_ok)
            }
            TirStmt::While { cond, body } => expr_ok(cond) && body.iter().all(stmt_ok),
            TirStmt::ForRange { start, end, step, body, .. } => {
                expr_ok(start) && expr_ok(end) && expr_ok(step) && body.iter().all(stmt_ok)
            }
            TirStmt::ForEach { collection, body, .. } => expr_ok(collection) && body.iter().all(stmt_ok),
            TirStmt::ListSet { list, index, value } => expr_ok(list) && expr_ok(index) && expr_ok(value),
            TirStmt::DictSet { dict, key, value } => expr_ok(dict) && expr_ok(key) && expr_ok(value),
            TirStmt::MemberSet { receiver, value, .. } => expr_ok(receiver) && expr_ok(value),
            TirStmt::ListAdd { receiver, arg }
            | TirStmt::ListInsert { receiver, arg, .. } => expr_ok(receiver) && expr_ok(arg),
            TirStmt::ListReverse { receiver } | TirStmt::ListSort { receiver }
            | TirStmt::ListClear { receiver } | TirStmt::DictClear { receiver } => expr_ok(receiver),
            TirStmt::NativeCall { module, member, args } => {
                native_call_ok(module.as_str(), member.as_str(), None)
                    && args.iter().all(expr_ok)
            }
            TirStmt::SetSlice { list, value, start, end, step } => {
                expr_ok(list)
                    && expr_ok(value)
                    && start.as_ref().map_or(true, |e| expr_ok(e))
                    && end.as_ref().map_or(true, |e| expr_ok(e))
                    && step.as_ref().map_or(true, |e| expr_ok(e))
            }
            TirStmt::Match { arms, result_ty, .. } => {
                ty_ok(*result_ty)
                    && arms.iter().all(|arm| {
                        let pattern_ok = match &arm.pattern {
                            TirPattern::Wildcard | TirPattern::Bind(_) => true,
                            TirPattern::Literal(e) => expr_ok(e),
                        };
                        pattern_ok
                            && arm.guard.as_ref().map_or(true, expr_ok)
                            && arm.body.iter().all(stmt_ok)
                    })
            }
        }
    }
    // Both expression and statement lowering reach native calls, so they must
    // share one gate. `nrt` is linked as a staticlib only, so this crate cannot
    // query its dispatch table directly: the denials below are a hand-maintained
    // mirror. Keep in sync with NRT dispatch: nrt/src/native.rs + bytes_native
    // + net_native + list::dispatch + obj::collections_dispatch.
    // `random` is time-seeded and `flow`/`atomic`/`inspect` have no NRT
    // implementation at all; the per-module lists cover members that exist on
    // the VM path but are absent from NRT. Without this gate a plain build emits
    // an exe that fails at runtime with "native module '<module>.<member>' is
    // not supported in a no-VM executable" (GAP-01). `bytes.*`,
    // `fs.read_bytes`/`fs.write_bytes` live in nrt/src/bytes_native.rs and stay
    // allowed.
    fn native_call_ok(module: &str, member: &str, _ret: Option<TirTy>) -> bool {
        if matches!(module, "random" | "flow" | "atomic" | "inspect") {
            return false;
        }
        !matches!(
            (module, member),
            ("io", "open")
                | ("fs", "read" | "write" | "append" | "delete" | "copy" | "move" | "rename"
                    | "touch" | "mkdir" | "rmdir" | "list" | "files" | "dirs" | "walk" | "chdir")
                | ("os", "env" | "setenv" | "unsetenv" | "environ" | "expand_vars" | "exit" | "exec")
                | ("sys", "env" | "environ" | "expand_vars" | "setcalldepth" | "setrecursionlimit")
                | ("time", "sleep" | "utc_now" | "from_epoch" | "from_epoch_ms" | "stopwatch")
                | ("re", "groups")
                | ("iter", "counter" | "repeat" | "zip" | "enumerate" | "take" | "skip"
                    | "chain" | "flatten" | "all" | "any" | "count")
        )
    }
    // Closures and class methods use NRT's ABI-aware indirect-call shim
    // (`call_native_typed`), which selects XMM-capable C signatures from the
    // registered parameter/return kinds. Float params and returns are therefore
    // supported on the plain `un build` path, so no indirect-only gate remains.
    !tir.functions.is_empty()
        && !tir.has_global_write
        && tir.functions.iter().all(|f| {
            f.params.iter().copied().all(ty_ok)
                && f.locals.iter().copied().all(ty_ok)
                && f.ret.map_or(true, ty_ok)
                && f.defaults.iter().flatten().all(expr_ok)
                && f.body.iter().all(stmt_ok)
        })
}

struct ClassTable {
    index: std::collections::HashMap<String, u16>,
    names: Vec<String>,
    fields: Vec<std::collections::HashMap<String, Ty>>,
    methods: Vec<std::collections::HashMap<String, Ty>>,
    setters: Vec<std::collections::HashSet<String>>,
    field_order: Vec<Vec<String>>,
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
                    resolve(checked, base, cache);
                }
                for base in info.bases.iter() {
                    if let Some(m) = cache.get(base.as_str()) {
                        for (k, v) in m {
                            merged.entry(k.clone()).or_insert_with(|| v.clone());
                        }
                    }
                }
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
        let field_order = names.iter().map(|n| {
            let mut order = Vec::new();
            if let Some(info) = checked.classes.get(n) {
                for base in &info.bases {
                    if let Some(b) = checked.classes.get(base) {
                        for f in &b.field_order { if !order.contains(f) { order.push(f.clone()); } }
                    }
                }
                for f in &info.field_order { if !order.contains(f) { order.push(f.clone()); } }
            }
            order
        }).collect();
        ClassTable { index, names, fields, methods, setters, field_order }
    }

    fn setters(&self, id: u16) -> Option<&std::collections::HashSet<String>> {
        self.setters.get(id as usize)
    }

    fn id(&self, name: &str) -> Option<u16> {
        self.index.get(name).copied()
    }

    fn field(&self, id: u16, name: &str) -> Option<&Ty> {
        self.fields.get(id as usize)?.get(name)
    }

    fn method(&self, id: u16, name: &str) -> Option<&Ty> {
        self.methods.get(id as usize)?.get(name)
    }
}

#[derive(Debug)]
pub struct TirError(pub String);

impl std::fmt::Display for TirError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for TirError {}

const MAX_CORE_ARITY: usize = 8;



fn same_ty(a: TirTy, b: TirTy) -> bool {
    match (a, b) {
        (TirTy::Class(_), TirTy::Class(_)) => true,
        (TirTy::ListClass(_), TirTy::ListClass(_)) => true,
        // A concrete `__lambda_N` closure value is assignable to an opaque
        // `func(..) -> ..` slot (`TirTy::Lambda(u32::MAX)`): both are NRT
        // closure handles dispatched through `unrt_call_obj`. The index only
        // selects the native entry point, it is not part of the value's ABI.
        (TirTy::Lambda(_), TirTy::Lambda(_)) => true,
        (TirTy::Obj, _) | (_, TirTy::Obj) if tir_is_ref(a) && tir_is_ref(b) => true,
        _ => a == b,
    }
}

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

pub fn fn_uses_str(f: &TirFn) -> bool {
    if f.locals.iter().any(|t| matches!(t, TirTy::Str)) {
        return true;
    }
    f.body.iter().any(stmt_uses_str)
}

/// True when a function manipulates tagged dynamic (`any`/union) handles.
/// The dispatch helpers (`unrt_dyn_bin`, `unrt_dyn_un`) exist only in the
/// standalone NRT, so Vm-mode JIT codegen must leave these functions to the
/// interpreter. Obj-mode (plain `un build`) emits them normally.
pub fn fn_uses_dynamic(f: &TirFn) -> bool {
    f.params.iter().any(|t| *t == TirTy::Obj)
        || f.locals.iter().any(|t| *t == TirTy::Obj)
        || f.ret == Some(TirTy::Obj)
        // An annotated `func(..) -> ..` slot lowers to the opaque
        // `TirTy::Lambda(u32::MAX)`. Calling through it needs `unrt_call_obj`
        // and `wrap_as_obj`, which only exist in Obj mode; real lambdas carry
        // a concrete index and keep their precise codegen. Vm mode must leave
        // these functions to the interpreter, exactly like `any` handles.
        || f.params
            .iter()
            .chain(f.locals.iter())
            .any(|t| matches!(t, TirTy::Lambda(u32::MAX)))
        || f.ret == Some(TirTy::Lambda(u32::MAX))
}

/// True when a native-module call needs the `unrt_call_native` bridge, which
/// only exists in Obj mode. Vm-mode codegen must fall back for such functions.
pub fn fn_uses_native_call(f: &TirFn) -> bool {
    f.defaults.iter().flatten().any(expr_uses_native)
        || f.body.iter().any(stmt_uses_native)
}

fn expr_uses_native(e: &TirExpr) -> bool {
    match e {
        TirExpr::NativeCallExpr { .. } => true,
        TirExpr::Promote(x) | TirExpr::Un(_, x) | TirExpr::Len(x) | TirExpr::ListPop(x)
        | TirExpr::CastInt(x) | TirExpr::CastFloat(x) | TirExpr::DictGet(_, x)
        | TirExpr::ListGet(_, x) | TirExpr::MemberGet { receiver: x, .. }
        | TirExpr::DictKeys(x) | TirExpr::DictValues(x)
        | TirExpr::Located { inner: x, .. } => expr_uses_native(x),
        TirExpr::Select { cond, then_expr, else_expr } => {
            expr_uses_native(cond) || expr_uses_native(then_expr) || expr_uses_native(else_expr)
        }
        TirExpr::Bin(_, a, b) | TirExpr::Cmp(_, a, b) | TirExpr::ListPopAt(a, b)
        | TirExpr::ListContains(a, b) | TirExpr::ListIndexOf(a, b)
        | TirExpr::ListRemove(a, b) => expr_uses_native(a) || expr_uses_native(b),
        TirExpr::List(items) | TirExpr::Call(_, items) | TirExpr::Math(_, items)
        | TirExpr::ClassNew { args: items, .. } => items.iter().any(expr_uses_native),
        TirExpr::MethodCall { receiver, args, .. } => {
            expr_uses_native(receiver) || args.iter().any(expr_uses_native)
        }
        TirExpr::CallObj { callee, args, .. } => {
            expr_uses_native(callee) || args.iter().any(expr_uses_native)
        }
        TirExpr::Dict(items) => items.iter().any(|(k, v)| expr_uses_native(k) || expr_uses_native(v)),
        TirExpr::ListRepeat { value, len, .. } => expr_uses_native(value) || expr_uses_native(len),
        TirExpr::Slice { list, start, end, step } => {
            expr_uses_native(list)
                || start.as_ref().is_some_and(|e| expr_uses_native(e))
                || end.as_ref().is_some_and(|e| expr_uses_native(e))
                || step.as_ref().is_some_and(|e| expr_uses_native(e))
        }
        TirExpr::EmptyCollection { .. } => false,
        TirExpr::Int(_) | TirExpr::Float(_) | TirExpr::Bool(_) | TirExpr::Str(_)
        | TirExpr::Local(_) | TirExpr::MakeClosure { .. } | TirExpr::GlobalGet(_)
        | TirExpr::NoneValue => false,
    }
}

fn stmt_uses_native(s: &TirStmt) -> bool {
    match s {
        TirStmt::NativeCall { .. } => true,
        TirStmt::Let(_, e) | TirStmt::Set(_, e) | TirStmt::Throw(e) | TirStmt::ExprStmt(e)
        | TirStmt::GlobalSet(_, e) => expr_uses_native(e),
        TirStmt::If { cond, then_body, else_body } => {
            expr_uses_native(cond) || then_body.iter().any(stmt_uses_native) || else_body.iter().any(stmt_uses_native)
        }
        TirStmt::While { cond, body } => expr_uses_native(cond) || body.iter().any(stmt_uses_native),
        TirStmt::ForRange { start, end, step, body, .. } => {
            expr_uses_native(start) || expr_uses_native(end) || expr_uses_native(step)
                || body.iter().any(stmt_uses_native)
        }
        TirStmt::ForEach { collection, body, .. } => {
            expr_uses_native(collection) || body.iter().any(stmt_uses_native)
        }
        TirStmt::ListSet { list, index, value } | TirStmt::DictSet { dict: list, key: index, value } => {
            expr_uses_native(list) || expr_uses_native(index) || expr_uses_native(value)
        }
        TirStmt::MemberSet { receiver, value, .. } | TirStmt::ListAdd { receiver, arg: value }
        | TirStmt::ListInsert { receiver, arg: value, .. } => {
            expr_uses_native(receiver) || expr_uses_native(value)
        }
        TirStmt::Try { body, catch_body, finally_body, .. } => {
            body.iter().any(stmt_uses_native)
                || catch_body.iter().any(stmt_uses_native)
                || finally_body.as_ref().is_some_and(|b| b.iter().any(stmt_uses_native))
        }
        TirStmt::Return(Some(e)) => expr_uses_native(e),
        TirStmt::SetSlice { list, value, start, end, step } => {
            expr_uses_native(list)
                || expr_uses_native(value)
                || start.as_ref().is_some_and(|e| expr_uses_native(e))
                || end.as_ref().is_some_and(|e| expr_uses_native(e))
                || step.as_ref().is_some_and(|e| expr_uses_native(e))
        }
        TirStmt::ListReverse { .. } | TirStmt::ListSort { .. }
        | TirStmt::ListClear { .. } | TirStmt::DictClear { .. }
        | TirStmt::Break | TirStmt::Skip | TirStmt::Return(None) => false,
        TirStmt::Match { arms, .. } => arms.iter().any(|arm| {
            matches!(&arm.pattern, TirPattern::Literal(e) if expr_uses_native(e))
                || arm.guard.as_ref().is_some_and(expr_uses_native)
                || arm.body.iter().any(stmt_uses_native)
        }),
    }
}

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
        TirStmt::Let(_, e) | TirStmt::Set(_, e) | TirStmt::ExprStmt(e) | TirStmt::GlobalSet(_, e) => expr_uses_classes(e),
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
        TirStmt::ListReverse { receiver } | TirStmt::ListSort { receiver }
        | TirStmt::ListClear { receiver } | TirStmt::DictClear { receiver } => expr_uses_classes(receiver),
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
        TirStmt::SetSlice { list, value, .. } => {
            expr_uses_classes(list) || expr_uses_classes(value)
        }
        TirStmt::Match { arms, .. } => arms.iter().any(|arm| {
            matches!(&arm.pattern, TirPattern::Literal(e) if expr_uses_classes(e))
                || arm.guard.as_ref().is_some_and(expr_uses_classes)
                || arm.body.iter().any(stmt_uses_classes)
        }),
        TirStmt::Break | TirStmt::Skip => false,
    }
}

fn expr_uses_classes(e: &TirExpr) -> bool {
    match e {
        TirExpr::ClassNew { .. } | TirExpr::MemberGet { .. } | TirExpr::MethodCall { .. } => true,
        TirExpr::Promote(inner) | TirExpr::Un(_, inner) => expr_uses_classes(inner),
        TirExpr::Located { inner, .. } => expr_uses_classes(inner),
        TirExpr::MakeClosure { .. } | TirExpr::GlobalGet(_) | TirExpr::NoneValue => false,
        TirExpr::CallObj { callee, args, .. } => expr_uses_classes(callee) || args.iter().any(expr_uses_classes),
        TirExpr::Select { cond, then_expr, else_expr } => {
            expr_uses_classes(cond) || expr_uses_classes(then_expr) || expr_uses_classes(else_expr)
        }
        TirExpr::Bin(_, lhs, rhs) | TirExpr::Cmp(_, lhs, rhs) => {
            expr_uses_classes(lhs) || expr_uses_classes(rhs)
        }
        TirExpr::Call(_, args) => args.iter().any(expr_uses_classes),
        TirExpr::NativeCallExpr { args, .. } => args.iter().any(expr_uses_classes),
        TirExpr::List(elements) => elements.iter().any(expr_uses_classes),
        TirExpr::Math(_, args) => args.iter().any(expr_uses_classes),
        TirExpr::ListRepeat { .. } => false,
        TirExpr::CastInt(inner) | TirExpr::CastFloat(inner) => expr_uses_classes(inner),
        TirExpr::Dict(entries) => entries.iter().any(|(k, v)| expr_uses_classes(k) || expr_uses_classes(v)),
        TirExpr::DictGet(a, b) | TirExpr::ListGet(a, b) => {
            expr_uses_classes(a) || expr_uses_classes(b)
        }
        TirExpr::Len(inner) => expr_uses_classes(inner),
        TirExpr::ListPop(receiver) => expr_uses_classes(receiver),
        TirExpr::ListPopAt(receiver, _) => expr_uses_classes(receiver),
        TirExpr::ListContains(receiver, _) | TirExpr::ListIndexOf(receiver, _)
        | TirExpr::DictKeys(receiver) | TirExpr::DictValues(receiver)
        | TirExpr::ListRemove(receiver, _) => expr_uses_classes(receiver),
        TirExpr::Slice { list, start, end, step } => {
            expr_uses_classes(list)
                || start.as_ref().is_some_and(|e| expr_uses_classes(e))
                || end.as_ref().is_some_and(|e| expr_uses_classes(e))
                || step.as_ref().is_some_and(|e| expr_uses_classes(e))
        }
        TirExpr::EmptyCollection { .. } => false,
        TirExpr::Int(_) | TirExpr::Float(_) | TirExpr::Bool(_) | TirExpr::Str(_) | TirExpr::Local(_) => false,
    }
}

pub fn fn_uses_heap(f: &TirFn) -> bool {
    if f.locals
        .iter()
        .any(|t| matches!(t, TirTy::ListInt | TirTy::ListFloat | TirTy::ListClass(_) | TirTy::DictStrInt | TirTy::DictStrFloat | TirTy::DictIntInt | TirTy::Class(_) | TirTy::Obj | TirTy::ListGen(_) | TirTy::DictGen(_, _) | TirTy::SetGen(_) | TirTy::TupleGen(_)))
    {
        return true;
    }
    f.body.iter().any(stmt_uses_heap)
}

fn stmt_uses_heap(s: &TirStmt) -> bool {
    match s {
        TirStmt::Let(_, e) | TirStmt::Set(_, e) | TirStmt::ExprStmt(e) | TirStmt::GlobalSet(_, e) => expr_uses_heap(e),
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
        TirStmt::ListReverse { receiver } | TirStmt::ListSort { receiver }
        | TirStmt::ListClear { receiver } | TirStmt::DictClear { receiver } => expr_uses_heap(receiver),
        TirStmt::DictSet { dict, key, value } => {
            expr_uses_heap(dict) || expr_uses_heap(key) || expr_uses_heap(value)
        }
        TirStmt::MemberSet { receiver, value, .. } => expr_uses_heap(receiver) || expr_uses_heap(value),
        TirStmt::NativeCall { args, .. } => args.iter().any(expr_uses_heap),
        TirStmt::SetSlice { list, value, .. } => {
            expr_uses_heap(list) || expr_uses_heap(value)
        }
        TirStmt::Try { .. } => true,
        TirStmt::Throw(_) => true,
        TirStmt::Return(None) => false,
        TirStmt::Return(Some(e)) => expr_uses_heap(e),
        TirStmt::Match { arms, .. } => arms.iter().any(|arm| {
            matches!(&arm.pattern, TirPattern::Literal(e) if expr_uses_heap(e))
                || arm.guard.as_ref().is_some_and(expr_uses_heap)
                || arm.body.iter().any(stmt_uses_heap)
        }),
        TirStmt::Break | TirStmt::Skip => false,
    }
}

fn expr_uses_heap(e: &TirExpr) -> bool {
    match e {
        TirExpr::List(_) | TirExpr::Dict(_) | TirExpr::DictGet(_, _) | TirExpr::ListGet(_, _)
        | TirExpr::ClassNew { .. } | TirExpr::MemberGet { .. } | TirExpr::MethodCall { .. } => true,
        TirExpr::Promote(inner)
        | TirExpr::Un(_, inner) => expr_uses_heap(inner),
        TirExpr::Located { inner, .. } => expr_uses_heap(inner),
        TirExpr::MakeClosure { .. } | TirExpr::GlobalGet(_) | TirExpr::NoneValue => true,
        TirExpr::CallObj { callee, args, .. } => expr_uses_heap(callee) || args.iter().any(expr_uses_heap),
        TirExpr::Select { cond, then_expr, else_expr } => {
            expr_uses_heap(cond) || expr_uses_heap(then_expr) || expr_uses_heap(else_expr)
        }
        TirExpr::Bin(_, lhs, rhs) | TirExpr::Cmp(_, lhs, rhs) => {
            expr_uses_heap(lhs) || expr_uses_heap(rhs)
        }
        TirExpr::Call(_, args) => args.iter().any(expr_uses_heap),
        TirExpr::NativeCallExpr { args, .. } => args.iter().any(expr_uses_heap),
        TirExpr::Int(_) | TirExpr::Float(_) | TirExpr::Bool(_) | TirExpr::Str(_) | TirExpr::Local(_) => false,
        TirExpr::Math(_, _) => false,
        TirExpr::CastInt(inner) | TirExpr::CastFloat(inner) => {
            expr_uses_heap(inner)
        }
        TirExpr::Len(_)
        | TirExpr::ListPop(_)
        | TirExpr::ListPopAt(_, _)
        | TirExpr::ListContains(_, _)
        | TirExpr::ListIndexOf(_, _)
        | TirExpr::ListRemove(_, _)
        | TirExpr::DictKeys(_)
        | TirExpr::DictValues(_) => true,
        TirExpr::ListRepeat { .. } => true,
        TirExpr::Slice { .. } | TirExpr::EmptyCollection { .. } => true,
    }
}

fn stmt_uses_str(s: &TirStmt) -> bool {
    match s {
        TirStmt::Let(_, e) | TirStmt::Set(_, e) | TirStmt::ExprStmt(e) | TirStmt::GlobalSet(_, e) => expr_uses_str(e),
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
        TirStmt::ListReverse { receiver } | TirStmt::ListSort { receiver }
        | TirStmt::ListClear { receiver } | TirStmt::DictClear { receiver } => expr_uses_str(receiver),
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
        TirStmt::SetSlice { list, value, .. } => {
            expr_uses_str(list) || expr_uses_str(value)
        }
        TirStmt::Match { arms, .. } => arms.iter().any(|arm| {
            matches!(&arm.pattern, TirPattern::Literal(e) if expr_uses_str(e))
                || arm.guard.as_ref().is_some_and(expr_uses_str)
                || arm.body.iter().any(stmt_uses_str)
        }),
        TirStmt::Break | TirStmt::Skip => false,
    }
}

fn expr_uses_str(e: &TirExpr) -> bool {
    match e {
        TirExpr::Str(_) => true,
        TirExpr::List(elements) => elements.iter().any(expr_uses_str),
        TirExpr::Dict(_) | TirExpr::DictGet(_, _) | TirExpr::ListGet(_, _)
        | TirExpr::ClassNew { .. } | TirExpr::MemberGet { .. } | TirExpr::MethodCall { .. } => false,
        TirExpr::Int(_) | TirExpr::Float(_) | TirExpr::Bool(_) | TirExpr::Local(_) | TirExpr::NoneValue => false,
        TirExpr::Located { inner, .. } => expr_uses_str(inner),
        TirExpr::MakeClosure { .. } | TirExpr::GlobalGet(_) => false,
        TirExpr::CallObj { callee, args, .. } => expr_uses_str(callee) || args.iter().any(expr_uses_str),
        TirExpr::ListRepeat { .. } => false,
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
        TirExpr::NativeCallExpr { args, ret, .. } => *ret == TirTy::Str || args.iter().any(expr_uses_str),
        TirExpr::Len(inner) => expr_uses_str(inner),
        TirExpr::ListPop(_)
        | TirExpr::ListPopAt(_, _)
        | TirExpr::ListContains(_, _)
        | TirExpr::ListIndexOf(_, _)
        | TirExpr::ListRemove(_, _)
        | TirExpr::DictKeys(_)
        | TirExpr::DictValues(_) => false,
        TirExpr::Slice { list, start, end, step } => {
            expr_uses_str(list)
                || start.as_ref().is_some_and(|e| expr_uses_str(e))
                || end.as_ref().is_some_and(|e| expr_uses_str(e))
                || step.as_ref().is_some_and(|e| expr_uses_str(e))
        }
        TirExpr::EmptyCollection { .. } => false,
    }
}

/// Explain why a function is outside the core subset, with the local/parameter
/// name and a hint for annotation. Returns `None` when the function is eligible.
pub fn core_rejection_reason(fcheck: &FnCheck) -> Option<String> {
    let FnCheck { sig, locals, .. } = fcheck;
    if sig.variadic {
        return Some(format!("'{}' is a variadic function", sig.name));
    }
    if sig.params.len() > MAX_CORE_ARITY {
        return Some(format!("'{}' has more than {MAX_CORE_ARITY} parameters", sig.name));
    }
    if let Some((name, ty)) = sig.param_names.iter().zip(&sig.params).find(|(_, ty)| !is_core_ty(ty)) {
        return Some(format!(
            "parameter '{name}' of '{}' has non-core type '{ty}' - annotate it with int/float/bool/str/list[int]/dict[str,int]/dict[int,int] to enable native compilation",
            sig.name
        ));
    }
    match &sig.ret {
        Ty::Int | Ty::Float | Ty::Bool | Ty::None_ | Ty::Str => {}
        // Dynamic returns are tagged NObj handles; NRT boxes/unboxes them.
        Ty::Any | Ty::Unknown | Ty::Union(_) => {}
        // Bytes values are opaque NObj handles in NRT (nrt/src/bytes_native.rs).
        Ty::Bytes => {}
        Ty::List(elem) => {
            if !matches!(elem.as_ref(), Ty::Int | Ty::Float | Ty::Class(_)) {
                return Some(format!("'{}' returns non-core 'list[{}]'", sig.name, elem.display()));
            }
        }
        Ty::Dict(key, value) => {
            if !matches!(
                (key.as_ref(), value.as_ref()),
                (Ty::Str, Ty::Int | Ty::Float) | (Ty::Int, Ty::Int)
            ) {
                return Some(format!("'{}' returns a non-core dict", sig.name));
            }
        }
        // Set literals lower to the list ABI (no dedicated NRT tag).
        Ty::Set(_) => {}
        Ty::Class(_) | Ty::Generic { .. } | Ty::Tuple(_) => {}
        other => return Some(format!("'{}' returns non-core type '{other}'", sig.name)),
    }
    if let Some((name, ty)) = locals.iter().find(|(_, ty)| !is_core_ty(ty)) {
        return Some(format!(
            "local '{name}' of '{}' has non-core type '{ty}' - annotate the binding to enable native compilation",
            sig.name
        ));
    }
    None
}

pub fn is_core_fn(fcheck: &FnCheck) -> bool {
    core_rejection_reason(fcheck).is_none()
}

fn is_core_ty(ty: &Ty) -> bool {
    match ty {
        Ty::Int | Ty::Float | Ty::Bool | Ty::Str | Ty::None_ | Ty::Class(_) => true,
        // `any` (and inferred/union forms) lower to `TirTy::Obj`: a tagged
        // NObj handle whose operations dispatch through the NRT dynamic bridge.
        // The standalone runtime can box/unbox and dispatch on the tag, so
        // these programs no longer need `un build --embed`.
        Ty::Any | Ty::Unknown | Ty::Union(_) => true,
        Ty::Bytes => true,
        Ty::List(elem) => is_core_elem(elem),
        Ty::Dict(key, value) => {
            matches!(key.as_ref(), Ty::Str | Ty::Int) && is_core_elem(value)
        }
        Ty::Set(elem) => is_core_elem(elem),
        // Tuples lower to the existing list object ABI (no dedicated NRT tag).
        Ty::Tuple(items) => items.iter().all(is_core_elem),
        Ty::Func { .. } => true,
        // A type variable is resolved into a concrete instance before lowering;
        // keep the original declaration eligible for name-based lookups.
        Ty::Generic { .. } => true,
    }
}

/// A collection element may be any core type, plus `Any`/unions (stored as an
/// opaque handle) and nested collections.
fn is_core_elem(ty: &Ty) -> bool {
    is_core_ty(ty) || matches!(ty, Ty::Any | Ty::Union(_))
}

fn ty_has_generic(ty: &Ty) -> bool {
    match ty {
        Ty::Generic { .. } => true,
        Ty::List(a) | Ty::Set(a) => ty_has_generic(a),
        Ty::Dict(k, v) => ty_has_generic(k) || ty_has_generic(v),
        Ty::Tuple(items) | Ty::Union(items) => items.iter().any(ty_has_generic),
        Ty::Func { params, ret, .. } => params.iter().any(ty_has_generic) || ty_has_generic(ret),
        _ => false,
    }
}

fn tirdbg_reject(node: &Node) {
    if std::env::var_os("UV_TIR_DEBUG").is_some() {
        eprintln!(
            "TIRDBG body reject kind={:?} op={:?} at {}",
            node.kind, node.op, node.start
        );
    }
}

/// Static gate for match pattern heads used by `body_is_core`.
///
/// `Guard` contributes only its pattern child here (the condition is a normal
/// expression visited by the `Case` arm); `OrPattern` requires every
/// alternative to be a representable leaf. Bind names are pre-collected into
/// `locals` by `collect_binding_target`.
fn pattern_core_ok(
    node: &Node,
    source: &Source,
    locals: &HashMap<String, ()>,
    out: &mut bool,
) -> bool {
    if !*out {
        return false;
    }
    match node.kind {
        NodeKind::Wildcard
        | NodeKind::Integer
        | NodeKind::Float
        | NodeKind::String
        | NodeKind::Boolean
        | NodeKind::None => {}
        NodeKind::Identifier => {
            let name = source.span(node.start, node.length);
            if !locals.contains_key(&name) {
                tirdbg_reject(node);
                *out = false;
                return false;
            }
        }
        // `x: T` / `x is T` is a bind with a static-only annotation.
        NodeKind::Typed => {
            let Some(name) = node.children.first() else {
                tirdbg_reject(node);
                *out = false;
                return false;
            };
            return pattern_core_ok(name, source, locals, out);
        }
        // `Guard[pattern, condition]`: only the pattern is a pattern node; the
        // condition is a normal expression visited by the `Case` arm below.
        NodeKind::Guard => {
            let Some(pattern) = node.children.first() else {
                tirdbg_reject(node);
                *out = false;
                return false;
            };
            return pattern_core_ok(pattern, source, locals, out);
        }
        NodeKind::OrPattern => {
            if node.children.is_empty() {
                tirdbg_reject(node);
                *out = false;
                return false;
            }
            for alt in &node.children {
                if !pattern_core_ok(alt, source, locals, out) {
                    return false;
                }
            }
        }
        _ => {
            tirdbg_reject(node);
            *out = false;
            return false;
        }
    }
    true
}

fn body_is_core(node: &Node, source: &Source, checked: &CheckedProgram, fcheck: &FnCheck, globals: &std::collections::HashMap<String, u16>) -> bool {
    // Generic classes have type-variable fields; their instances are not
    // represented in TIR yet, so keep the whole program on the VM path.
    if checked.classes.values().any(|c| !c.generics.is_empty()) {
        return false;
    }
    let Some(body) = body_block_of(node) else {
        return false;
    };
    let mut locals: HashMap<String, ()> = HashMap::new();
    collect_locals(body, &mut locals, source, fcheck);
    for (name, _) in globals { locals.insert(name.clone(), ()); }

    let handles: std::collections::HashSet<String> = fcheck
        .local_types
        .iter()
        .filter(|(_, ty)| {
            matches!(ty, Ty::Class(c) if c.as_ref() == "http_client" || c.as_ref() == "web_socket")
        })
        .map(|(name, _)| name.clone())
        .collect();

    fn visit(
        node: &Node,
        checked: &CheckedProgram,
        source: &Source,
        locals: &HashMap<String, ()>,
        handles: &std::collections::HashSet<String>,
        out: &mut bool,
    ) {
        if !*out {
            return;
        }
        // FE-01 `Guard` / `OrPattern` are now lowered inside match arms (see
        // `lower_match_expr`); the `Case` arm below validates their shape.
        if kind_is(node, "TypeAlias") {
            tirdbg_reject(node);
            *out = false;
            return;
        }
        // Tuple *literals* lower to the list ABI, but tuple destructuring has
        // no TIR form yet: keep those functions on the VM path.
        if node.kind == NodeKind::Assign
            && node.children.first().is_some_and(|c| c.kind == NodeKind::Tuple)
        {
            tirdbg_reject(node);
            *out = false;
            return;
        }
        match node.kind {
            // Lambda bodies are lowered separately into synthesized functions;
            // their parameters and captures are not this function's locals.
            NodeKind::Lambda => return,
            NodeKind::Identifier => {
                let name = source.span(node.start, node.length);
                if !locals.contains_key(&name) {
                    // A bare top-level function name is a first-class function
                    // value: `lower_expr` synthesizes a zero-capture wrapper
                    // lambda for it. Allow it when the name resolves to a
                    // core-eligible function; non-core functions stay on the VM
                    // path.
                    if checked.sigs.contains_key(name.as_str())
                        && checked.functions.iter().any(|f| f.sig.name == name && is_core_fn(f))
                    {
                        return;
                    }
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
            | NodeKind::Return
            | NodeKind::Break
            | NodeKind::Skip
            | NodeKind::Assign => {
                for child in &node.children {
                    visit(child, checked, source, locals, handles, out);
                }
            }
            NodeKind::For => {
                let skip = node.children.first().is_some_and(|c| c.kind == NodeKind::Wildcard);
                for child in node.children.iter().skip(if skip { 1 } else { 0 }) {
                    visit(child, checked, source, locals, handles, out);
                }
            }
            NodeKind::Typed | NodeKind::Null => {}
            NodeKind::Integer | NodeKind::Float | NodeKind::Boolean | NodeKind::None | NodeKind::String => {}
            // Interpolation pieces are either string literals or expressions;
            // both are handled by the normal visitor.
            NodeKind::FString | NodeKind::Ternary | NodeKind::Slice => {
                for child in &node.children {
                    visit(child, checked, source, locals, handles, out);
                }
            }
            NodeKind::Bytes => {
                // `b"..."` materialises a list[int] of byte values in the VM.
                for child in &node.children {
                    visit(child, checked, source, locals, handles, out);
                }
            }
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
                if matches!(op, TokenType::Wait | TokenType::Go) {
                    tirdbg_reject(node); *out = false;
                    return;
                }
                visit(&node.children[0], checked, source, locals, handles, out);
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
                    visit(child, checked, source, locals, handles, out);
                }
            }
            NodeKind::Spread | NodeKind::KwSpread => {
                tirdbg_reject(node); *out = false;
            }
            NodeKind::Call => {
                if node.children.first().is_some_and(|c| c.kind == NodeKind::Index) {
                    if let Some(callee) = node.children.first() {
                        visit(callee, checked, source, locals, handles, out);
                    }
                    if let Some(args) = node.children.get(1) {
                        for a in &args.children { visit(a, checked, source, locals, handles, out); }
                    }
                    return;
                }
                if node.children.first().is_some_and(|c| c.kind == NodeKind::Lambda) {
                    if let Some(args) = node.children.get(1) {
                        for arg in &args.children { visit(arg, checked, source, locals, handles, out); }
                    }
                    return;
                }
                if let Some(first) = node.children.first() {
                    if first.kind == NodeKind::Identifier {
                        let name = source.span(first.start, first.length).to_string();
                        // A local variable may hold a statically resolved lambda.
                        if locals.contains_key(&name) && !checked.sigs.contains_key(name.as_str()) {
                            if let Some(args) = node.children.get(1) {
                                for arg in &args.children { visit(arg, checked, source, locals, handles, out); }
                            }
                            return;
                        }
                        if (name == "int" || name == "float") && !checked.sigs.contains_key(name.as_str()) {
                            if let Some(arg_tuple) = node.children.get(1) {
                                for arg in &arg_tuple.children {
                                    visit(arg, checked, source, locals, handles, out);
                                }
                                return;
                            }
                        }
                    }
                }
                if let Some(first) = node.children.first() {
                    if first.kind == NodeKind::Identifier {
                        let name = source.span(first.start, first.length).to_string();
                        if (name == "len" || name == "throw") && !checked.sigs.contains_key(name.as_str()) {
                            if let Some(arg_tuple) = node.children.get(1) {
                                for arg in &arg_tuple.children {
                                    visit(arg, checked, source, locals, handles, out);
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
                                            visit(arg, checked, source, locals, handles, out);
                                        }
                                        return;
                                    }
                                }
                            }
                        }
                    }
                }
                if callee.kind == NodeKind::Property {
                    if let Some(head) = callee.children.first() {
                        visit(head, checked, source, locals, handles, out);
                    }
                    for arg in &node.children[1].children {
                        visit(arg, checked, source, locals, handles, out);
                    }
                    return;
                }
                let Some(callee) = node.children.first() else {
                    tirdbg_reject(node); *out = false;
                    return;
                };
                if callee.kind == NodeKind::Identifier {
                    let name = source.span(callee.start, callee.length).to_string();
                    if checked.classes.contains_key(&name) {
                        if let Some(arg_tuple) = node.children.get(1) {
                            for arg in &arg_tuple.children {
                                visit(arg, checked, source, locals, handles, out);
                            }
                            return;
                        }
                    }
                }
                if callee.kind == NodeKind::Property {
                    // Method calls on native opaque handles (`http_client`,
                    // `web_socket`) are lowered as native method calls.
                    if let Some(head) = callee.children.first() {
                        if head.kind == NodeKind::Identifier {
                            let name = source.span(head.start, head.length).to_string();
                            if handles.contains(&name) {
                                if let Some(arg_tuple) = node.children.get(1) {
                                    for arg in &arg_tuple.children {
                                        visit(arg, checked, source, locals, handles, out);
                                    }
                                    return;
                                }
                            }
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
                if !is_core_ty(&sig.ret) && !matches!(sig.ret, Ty::None_) {
                    tirdbg_reject(node); *out = false;
                    return;
                }
                if sig.params.iter().any(|ty| !is_core_ty(ty)) {
                    tirdbg_reject(node); *out = false;
                    return;
                }
                for arg in &node.children[1].children {
                    visit(arg, checked, source, locals, handles, out);
                }
            }
            NodeKind::Pair => {
                for child in &node.children {
                    visit(child, checked, source, locals, handles, out);
                }
            }
            // ARC-00 approved tuple/set plain-build support; both literal
            // kinds are lowered by `lower_expr` and dispatched through NRT's
            // existing `unrt_call_native` bridge.
            NodeKind::Dict | NodeKind::List | NodeKind::Set | NodeKind::Tuple => {
                for child in &node.children {
                    visit(child, checked, source, locals, handles, out);
                }
            }
            NodeKind::Property => {
                let receiver = node.children.first();
                if let Some(receiver) = receiver {
                    visit(receiver, checked, source, locals, handles, out);
                }
            }
            NodeKind::Index => {
                for child in &node.children {
                    visit(child, checked, source, locals, handles, out);
                }
            }
            NodeKind::Try => {
                for child in &node.children {
                    visit(child, checked, source, locals, handles, out);
                }
            }
            NodeKind::Catch => {
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
                    visit(child, checked, source, &scoped, handles, out);
                }
            }
            NodeKind::Finally => {
                for child in &node.children {
                    visit(child, checked, source, locals, handles, out);
                }
            }
            // ARC-00/MATCHAOT: literal / bind / wildcard leaves are
            // representable, plus `Guard` and `OrPattern` wrappers around
            // those leaves. Tuple/list/class/variant patterns stay on the VM.
            NodeKind::Match => {
                for child in &node.children {
                    visit(child, checked, source, locals, handles, out);
                }
            }
            NodeKind::Case => {
                let Some(head) = node.children.first() else {
                    tirdbg_reject(node); *out = false;
                    return;
                };
                if !pattern_core_ok(head, source, locals, out) {
                    return;
                }
                if head.kind == NodeKind::Guard {
                    if let Some(cond) = head.children.get(1) {
                        visit(cond, checked, source, locals, handles, out);
                    }
                }
                if let Some(body) = node.children.get(1) {
                    if body.kind == NodeKind::Block {
                        // Multi-statement arm bodies have no TIR value form.
                        tirdbg_reject(body); *out = false;
                        return;
                    }
                    visit(body, checked, source, locals, handles, out);
                }
            }
            NodeKind::Wildcard => {}
            NodeKind::Use | NodeKind::Using | NodeKind::Defer
            | NodeKind::Function | NodeKind::Class | NodeKind::Enum | NodeKind::Tagged
            | NodeKind::Accessor => {
                tirdbg_reject(node); *out = false;
            }
            _ => {
                tirdbg_reject(node); *out = false;
            }
        }
    }
    let mut out = true;
    visit(body, checked, source, &locals, &handles, &mut out);
    out
}

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
        NodeKind::Case => {
            // `children[0]` is the pattern; a bind/`Typed` head declares a name
            // that is visible only inside this arm but still needs to pass the
            // identifier gate in `body_is_core`.
            if let Some(head) = node.children.first() {
                collect_binding_target(head, source, out);
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
        NodeKind::Typed => {
            if let Some(first) = target.children.first() {
                collect_binding_target(first, source, out);
            }
        }
        NodeKind::Tuple | NodeKind::List => {
            for child in &target.children {
                collect_binding_target(child, source, out);
            }
        }
        // `Guard[pattern, condition]`: only the pattern declares bindings.
        NodeKind::Guard => {
            if let Some(pattern) = target.children.first() {
                collect_binding_target(pattern, source, out);
            }
        }
        NodeKind::OrPattern => {
            for alt in &target.children {
                collect_binding_target(alt, source, out);
            }
        }
        _ => {}
    }
}

fn body_block_of(node: &Node) -> Option<&Node> {
    node.children.iter().find(|c| c.kind == NodeKind::Block)
}






impl TirProgram {
    /// Fill omitted trailing arguments with the callee's lowered defaults.
    /// Runs after all functions are lowered so call sites can reference any
    /// function defined later in the file.
    pub fn pad_default_args(&mut self) {
        fn pad_expr(e: &mut TirExpr, program: &[TirFn], class_names: &[String]) {
            match e {
                TirExpr::Located { inner, .. } => pad_expr(inner, program, class_names),
                TirExpr::MakeClosure { .. } | TirExpr::GlobalGet(_) | TirExpr::NoneValue => {}
                TirExpr::CallObj { callee, args, .. } => {
                    pad_expr(callee, program, class_names);
                    for a in args { pad_expr(a, program, class_names); }
                }
                TirExpr::Call(index, args) => {
                    for a in args.iter_mut() { pad_expr(a, program, class_names); }
                    if let Some(callee) = program.get(*index as usize) {
                        pad_args(args, &callee.defaults);
                    }
                }
                TirExpr::MethodCall { receiver, member, args, .. } => {
                    pad_expr(receiver, program, class_names);
                    for a in args.iter_mut() { pad_expr(a, program, class_names); }
                    let member_defaults = program.iter()
                        .find(|f| &f.name == member.as_ref())
                        .map(|f| f.defaults[1.min(f.defaults.len())..].to_vec());
                    if let Some(defaults) = member_defaults {
                        pad_args(args, &defaults);
                    }
                }
                TirExpr::ClassNew { name, args, .. } => {
                    for a in args.iter_mut() { pad_expr(a, program, class_names); }
                    // `__init__` takes `self` in slot 0; find the one whose
                    // receiver class matches this constructor's class name.
                    let class_id = class_names.iter().position(|n| n == name.as_ref());
                    let init = program.iter().find(|f| {
                        f.name == "__init__"
                            && match (f.params.first(), class_id) {
                                (Some(TirTy::Class(id)), Some(cid)) => *id as usize == cid,
                                _ => false,
                            }
                    });
                    if let Some(init) = init {
                        let defaults = &init.defaults[1.min(init.defaults.len())..];
                        pad_args(args, defaults);
                    }
                }
                TirExpr::List(items) | TirExpr::Math(_, items) => {
                    for item in items { pad_expr(item, program, class_names); }
                }
                TirExpr::Dict(entries) => {
                    for (k, v) in entries { pad_expr(k, program, class_names); pad_expr(v, program, class_names); }
                }
                TirExpr::Promote(a) | TirExpr::Un(_, a) | TirExpr::CastInt(a)
                | TirExpr::CastFloat(a) | TirExpr::Len(a) | TirExpr::ListPop(a)
                | TirExpr::ListContains(a, _) | TirExpr::ListIndexOf(a, _)
                | TirExpr::DictKeys(a) | TirExpr::DictValues(a)
                | TirExpr::MemberGet { receiver: a, .. } => pad_expr(a, program, class_names),
                TirExpr::ListPopAt(a, b) | TirExpr::ListGet(a, b) | TirExpr::DictGet(a, b)
                | TirExpr::ListRemove(a, b)
                | TirExpr::Bin(_, a, b) | TirExpr::Cmp(_, a, b) => {
                    pad_expr(a, program, class_names); pad_expr(b, program, class_names);
                }
                TirExpr::ListRepeat { value, len, .. } => {
                    pad_expr(value, program, class_names); pad_expr(len, program, class_names);
                }
                TirExpr::Select { cond, then_expr, else_expr } => {
                    pad_expr(cond, program, class_names); pad_expr(then_expr, program, class_names); pad_expr(else_expr, program, class_names);
                }
                TirExpr::NativeCallExpr { args, .. } => {
                    for a in args { pad_expr(a, program, class_names); }
                }
                TirExpr::Slice { list, start, end, step } => {
                    pad_expr(list, program, class_names);
                    if let Some(s) = start { pad_expr(s, program, class_names); }
                    if let Some(e) = end { pad_expr(e, program, class_names); }
                    if let Some(s) = step { pad_expr(s, program, class_names); }
                }
                TirExpr::EmptyCollection { .. } => {}
                TirExpr::Int(_) | TirExpr::Float(_) | TirExpr::Bool(_) | TirExpr::Str(_)
                | TirExpr::Local(_) => {}
            }
        }
        fn pad_args(args: &mut Vec<TirExpr>, defaults: &[Option<TirExpr>]) {
            while args.len() < defaults.len() {
                match &defaults[args.len()] {
                    Some(d) => args.push(d.clone()),
                    None => break,
                }
            }
        }
        fn pad_stmt(s: &mut TirStmt, program: &[TirFn], class_names: &[String]) {
            match s {
                TirStmt::Let(_, e) | TirStmt::Set(_, e) | TirStmt::ExprStmt(e)
                | TirStmt::Return(Some(e)) | TirStmt::Throw(e) | TirStmt::GlobalSet(_, e) => pad_expr(e, program, class_names),
                TirStmt::If { cond, then_body, else_body } => {
                    pad_expr(cond, program, class_names);
                    for st in then_body.iter_mut().chain(else_body.iter_mut()) { pad_stmt(st, program, class_names); }
                }
                TirStmt::While { cond, body } => {
                    pad_expr(cond, program, class_names);
                    for st in body { pad_stmt(st, program, class_names); }
                }
                TirStmt::ForRange { start, end, step, body, .. } => {
                    pad_expr(start, program, class_names); pad_expr(end, program, class_names); pad_expr(step, program, class_names);
                    for st in body { pad_stmt(st, program, class_names); }
                }
                TirStmt::ForEach { collection, body, .. } => {
                    pad_expr(collection, program, class_names);
                    for st in body { pad_stmt(st, program, class_names); }
                }
                TirStmt::ListSet { list, index, value } | TirStmt::DictSet { dict: list, key: index, value } => {
                    pad_expr(list, program, class_names); pad_expr(index, program, class_names); pad_expr(value, program, class_names);
                }
                TirStmt::MemberSet { receiver, value, .. } => {
                    pad_expr(receiver, program, class_names); pad_expr(value, program, class_names);
                }
                TirStmt::ListAdd { receiver, arg } => {
                    pad_expr(receiver, program, class_names); pad_expr(arg, program, class_names);
                }
                TirStmt::ListInsert { receiver, arg, index } => {
                    pad_expr(receiver, program, class_names); pad_expr(arg, program, class_names); pad_expr(index, program, class_names);
                }
                TirStmt::ListReverse { receiver } | TirStmt::ListSort { receiver }
                | TirStmt::ListClear { receiver } | TirStmt::DictClear { receiver } => pad_expr(receiver, program, class_names),
                TirStmt::NativeCall { args, .. } => {
                    for a in args { pad_expr(a, program, class_names); }
                }
                TirStmt::SetSlice { list, value, start, end, step } => {
                    pad_expr(list, program, class_names);
                    pad_expr(value, program, class_names);
                    if let Some(s) = start { pad_expr(s, program, class_names); }
                    if let Some(e) = end { pad_expr(e, program, class_names); }
                    if let Some(s) = step { pad_expr(s, program, class_names); }
                }
                TirStmt::Try { body, catch_body, finally_body, .. } => {
                    for st in body.iter_mut().chain(catch_body.iter_mut()) { pad_stmt(st, program, class_names); }
                    if let Some(fb) = finally_body { for st in fb { pad_stmt(st, program, class_names); } }
                }
                TirStmt::Match { arms, .. } => {
                    for arm in arms {
                        if let TirPattern::Literal(e) = &mut arm.pattern { pad_expr(e, program, class_names); }
                        if let Some(guard) = &mut arm.guard { pad_expr(guard, program, class_names); }
                        for st in &mut arm.body { pad_stmt(st, program, class_names); }
                    }
                }
                TirStmt::Return(None) | TirStmt::Break | TirStmt::Skip => {}
            }
        }
        let functions_snapshot = self.functions.clone();
        for f in &mut self.functions {
            for st in &mut f.body { pad_stmt(st, &functions_snapshot, &self.class_names); }
            for d in f.defaults.iter_mut().flatten() { pad_expr(d, &functions_snapshot, &self.class_names); }
        }
    }

    pub fn lower(source: &Source, root: &Node, checked: &CheckedProgram, global_names: &[String]) -> Result<TirProgram, TirError> {
        let debug_trace = std::env::var_os("UV_TIR_DEBUG").is_some();
        let expanded = expand_generics(checked);
        let checked = &expanded.checked;
        let call_names = &expanded.call_names;
        let globals: std::collections::HashMap<String, u16> = global_names.iter()
            .cloned().enumerate().map(|(i, n)| (n, i as u16)).collect();
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
        let node_by_start: HashMap<usize, &Node> = fn_nodes
            .iter()
            .map(|node| (node.start, *node))
            .collect();
        let mut eligible: Vec<usize> = Vec::new();
        for (node_start, fn_index) in &expanded.lower {
            let Some(node) = node_by_start.get(node_start) else {
                continue;
            };
            let fcheck = &checked.functions[*fn_index];
            // Uninstantiated generics (class methods with type variables, or a
            // generic function never called) have no concrete TIR type.
            if fcheck.sig.params.iter().any(ty_has_generic)
                || ty_has_generic(&fcheck.sig.ret)
                || fcheck.locals.iter().any(|(_, t)| ty_has_generic(t))
            {
                continue;
            }
            let core_ok = is_core_fn(fcheck);
            let body_ok = body_is_core(node, source, checked, fcheck, &globals);
            if debug_trace {
                let name = &fcheck.sig.name;
                eprintln!("TIRDBG gate {} is_core={} body_core={}", name, core_ok, body_ok);
                if let Some(reason) = core_rejection_reason(fcheck) {
                    eprintln!("TIRDBG   reason: {reason}");
                }
            }
            if core_ok && body_ok {
                eligible.push(*fn_index);
            }
        }
        let tir_index: HashMap<usize, usize> = eligible
            .iter()
            .enumerate()
            .map(|(tir_i, &fn_i)| (fn_i, tir_i))
            .collect();
        let classes = ClassTable::new(checked);
        let lambda_base = eligible.len() as u32;
        let mut lambdas = Vec::new();
        let mut functions = Vec::with_capacity(eligible.len());
        let mut any_global_write = false;
        for (node_start, fn_index) in &expanded.lower {
            let Some(node) = node_by_start.get(node_start) else {
                continue;
            };
            let Some(tir_i) = tir_index.get(fn_index).copied() else {
                continue;
            };
            let fcheck = &checked.functions[*fn_index];
            let lowered = lower_function(node, source, fcheck, checked, tir_i, &tir_index, call_names, &expanded.instance_args, &classes, &mut lambdas, lambda_base, &globals, &mut any_global_write)
                .map_err(|e| TirError(format!("{}: {}", fcheck.sig.name, e)))?;
            functions.push(lowered);
        }
        // Initialize inlined module globals before `main` runs, in source order.
        let mut init_body = Vec::new();
        {
            let mut init_ctx = LowerCtx {
                source,
                checked,
                tir_index: &tir_index,
                call_names,
                current_instance: String::new(),
                instance_args: &expanded.instance_args,
                slots: HashMap::new(),
                locals: Vec::new(),
                declared_types: HashMap::new(),
                classes: &classes,
                lambda_vars: HashMap::new(),
                lambdas: &mut lambdas,
                lambda_base,
                globals: &globals,
                wrote_global: &mut false,
                pending: Vec::new(),
            };
            for child in &root.children {
                if child.kind == NodeKind::Assign {
                    let is_global = child.children.first().and_then(|t| {
                        let name = source.span(t.start, t.length);
                        globals.get(&name.to_string())
                    }).is_some();
                    if !is_global { continue; }
                    if let Err(e) = lower_stmt(child, &mut init_ctx, &mut init_body) {
                        return Err(TirError(format!("module init: {e}")));
                    }
                }
            }
        }
        if let Some(main_fn) = functions.iter_mut().find(|f| f.name == "main") {
            let mut body = init_body;
            body.append(&mut main_fn.body);
            main_fn.body = body;
        }
        functions.extend(lambdas);
        let mut program = TirProgram {
            functions,
            class_names: classes.names.clone(),
            class_fields: classes.fields.clone(),
            class_methods: classes.methods.clone(),
            class_setters: classes.setters.clone(),
            class_field_order: classes.field_order.clone(),
            has_global_write: any_global_write,
        };
        program.pad_default_args();
        program.validate()?;
        Ok(program)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CoreTy {
    Int,
    Float,
    Bool,
    Str,
    ListInt,
    ListFloat,
    ListClass(u16),
    DictStrInt,
    DictStrFloat,
    DictIntInt,
    Class(u16),
    Lambda(u32),
    ListLambda,
    Obj,
    ListGen(ObjElem),
    DictGen(ObjElem, ObjElem),
    SetGen(ObjElem),
    TupleGen(ObjElem),
    None_,
}

impl From<TirTy> for CoreTy {
    fn from(ty: TirTy) -> Self {
        match ty {
            TirTy::Int => CoreTy::Int,
            TirTy::Float => CoreTy::Float,
            TirTy::Bool => CoreTy::Bool,
            TirTy::Str => CoreTy::Str,
            // No bytes variant in the core type system: treat as an opaque
            // handle. Bytes functions are excluded from core lowering.
            TirTy::Bytes => CoreTy::Obj,
            TirTy::ListInt => CoreTy::ListInt,
            TirTy::ListFloat => CoreTy::ListFloat,
            TirTy::ListClass(id) => CoreTy::ListClass(id),
            TirTy::DictStrInt => CoreTy::DictStrInt,
            TirTy::DictStrFloat => CoreTy::DictStrFloat,
            TirTy::DictIntInt => CoreTy::DictIntInt,
            TirTy::Class(id) => CoreTy::Class(id),
            TirTy::ErrorObj => CoreTy::Class(u16::MAX),
            TirTy::ListLambda => CoreTy::ListLambda,
            TirTy::Lambda(index) => CoreTy::Lambda(index),
            TirTy::Obj => CoreTy::Obj,
            TirTy::ListGen(e) => CoreTy::ListGen(e),
            TirTy::DictGen(k, v) => CoreTy::DictGen(k, v),
            TirTy::SetGen(e) => CoreTy::SetGen(e),
            TirTy::TupleGen(e) => CoreTy::TupleGen(e),
        }
    }
}

struct LowerCtx<'a> {
    source: &'a Source,
    checked: &'a CheckedProgram,
    tir_index: &'a HashMap<usize, usize>,
    /// Call node start -> mangled callee name for generic instantiations.
    call_names: &'a HashMap<usize, String>,
    /// Name of the function currently being lowered; dependency markers resolve through it.
    current_instance: String,
    /// Mangled instance name -> (original name, concrete type args).
    instance_args: &'a HashMap<String, (String, Vec<Ty>)>,
    slots: HashMap<String, u16>,
    locals: Vec<TirTy>,
    /// Typechecker-declared types by name. Used for expected-type propagation
    /// into empty list/dict literals (typeck records `list[Any]` there but the
    /// annotated binding is `list[int]` / `dict[str, int]` / ...).
    declared_types: HashMap<String, TirTy>,
    classes: &'a ClassTable,
    /// Local slot -> (synthesized lambda function index, captured local slots).
    lambda_vars: HashMap<u16, (u32, Vec<u16>)>,
    /// Synthesized lambda functions (appended to the program after lowering).
    lambdas: &'a mut Vec<TirFn>,
    /// Index offset for synthesized lambdas in the final function list.
    lambda_base: u32,
    /// Top-level global variable name -> slot index.
    globals: &'a std::collections::HashMap<String, u16>,
    /// Set when a function writes a global (VM uses StoreLocal semantics).
    wrote_global: &'a mut bool,
    /// Statements hoisted by expression lowering (currently only `match`,
    /// which needs to bind its scrutinee/result locals before the expression
    /// that consumes the value). Flushed by `lower_block`.
    pending: Vec<TirStmt>,
}

fn lower_function<'a>(
    node: &'a Node,
    source: &Source,
    fcheck: &FnCheck,
    checked: &CheckedProgram,
    tir_i: usize,
    tir_index: &HashMap<usize, usize>,
    call_names: &HashMap<usize, String>,
    instance_args: &HashMap<String, (String, Vec<Ty>)>,
    classes: &ClassTable,
    lambdas: &mut Vec<TirFn>,
    lambda_base: u32,
    globals: &'a std::collections::HashMap<String, u16>,
    wrote_global: &'a mut bool,
) -> Result<TirFn, String> {
    let mut ctx = LowerCtx {
        source,
        checked,
        tir_index,
        call_names,
        current_instance: fcheck.sig.name.clone(),
        instance_args,
        slots: HashMap::new(),
        locals: Vec::new(),
        declared_types: fcheck
            .local_types
            .iter()
            .filter_map(|(name, ty)| tir_ty_from_ty(classes, ty).map(|t| (name.clone(), t)))
            .collect(),
        classes,
        lambda_vars: HashMap::new(),
        lambdas,
        lambda_base,
        globals,
        wrote_global,
        pending: Vec::new(),
    };
    for (i, (name, ty)) in fcheck.sig.param_names.iter().zip(&fcheck.sig.params).enumerate() {
        let tty = tir_ty_from_ty(classes, ty)
            .ok_or_else(|| format!("parameter '{name}' is not core-typed"))?;
        if i >= 256 {
            return Err("more than 255 parameters".to_string());
        }
        ctx.slots.insert(name.clone(), i as u16);
        ctx.declared_types.insert(name.clone(), tty);
        ctx.locals.push(tty);
    }
    let body_node = node
        .children
        .iter()
        .find(|c| c.kind == NodeKind::Block)
        .ok_or_else(|| "function without a body".to_string())?;
    let mut body = Vec::new();
    lower_block(body_node, &mut ctx, &mut body)?;
    // Lower default expressions from the parameter list. `body_is_core` only
    // walks the body, so a non-core default makes the whole function ineligible.
    let mut defaults = vec![None; fcheck.sig.params.len()];
    if let Some(params_node) = node.children.iter().find(|c| c.kind == NodeKind::Tuple) {
        for (i, param) in params_node.children.iter().enumerate() {
            let slot = i + fcheck.sig.receiver;
            if slot >= defaults.len() {
                break;
            }
            let Some(inner) = param.children.first() else { continue };
            if inner.kind != NodeKind::Assign {
                continue;
            }
            let Some(expr_node) = inner.children.get(1) else { continue };
            let (lowered, _) = lower_expr(expr_node, &mut ctx)
                .map_err(|e| format!("parameter '{}' default: {e}", fcheck.sig.param_names[i]))?;
            if !ctx.pending.is_empty() {
                return Err(format!(
                    "parameter '{}' default uses an expression that needs hoisted statements",
                    fcheck.sig.param_names[i]
                ));
            }
            // Defaults are cloned into call sites, so they cannot refer to
            // parameters of the callee (those slots mean something else at the
            // call site). Such functions stay on the VM path.
            if expr_uses_local(&lowered) {
                return Err(format!(
                    "parameter '{}' default references another parameter",
                    fcheck.sig.param_names[i]
                ));
            }
            defaults[slot] = Some(lowered);
        }
    }
    let ret = match &fcheck.sig.ret {
        Ty::Int => Some(TirTy::Int),
        Ty::Float => Some(TirTy::Float),
        Ty::Bool => Some(TirTy::Bool),
        Ty::None_ => None,
        Ty::Str => Some(TirTy::Str),
        Ty::List(_) | Ty::Class(_) | Ty::Dict(_, _) | Ty::Tuple(_) | Ty::Set(_) | Ty::Bytes
        | Ty::Any | Ty::Union(_) | Ty::Unknown => {
            Some(
                tir_ty_from_ty(classes, &fcheck.sig.ret)
                    .ok_or_else(|| format!("return type '{}' is not core-supported", fcheck.sig.ret.display()))?,
            )
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
                    Ty::List(elem) => {
                        match elem.as_ref() {
                            Ty::Class(cname) => {
                                let id = classes.id(cname).ok_or_else(|| format!("unknown class '{}'", cname))?;
                                params.push(TirTy::ListClass(id));
                            }
                            _ => {
                                let tty = tir_ty_from_ty(classes, t)
                                    .ok_or_else(|| "parameter type is not core-supported".to_string())?;
                                params.push(tty);
                            }
                        }
                    }
                    other => {
                        let Some(tty) = tir_ty_from_ty(classes, other) else {
                            return Err("parameter type is not core-supported".to_string());
                        };
                        params.push(tty);
                    }
                }
            }
            params
        },
        defaults,
        ret,
        locals: ctx.locals,
        body,
    })
}

fn expr_uses_local(e: &TirExpr) -> bool {
    match e {
        TirExpr::Local(_) | TirExpr::GlobalGet(_) | TirExpr::NoneValue => true,
        TirExpr::List(items) | TirExpr::Math(_, items) | TirExpr::Call(_, items) => {
            items.iter().any(expr_uses_local)
        }
        TirExpr::Dict(entries) => entries.iter().any(|(k, v)| expr_uses_local(k) || expr_uses_local(v)),
        TirExpr::Promote(a) | TirExpr::Un(_, a) | TirExpr::CastInt(a) | TirExpr::CastFloat(a)
        | TirExpr::Len(a) | TirExpr::ListPop(a) | TirExpr::ListContains(a, _)
        | TirExpr::ListIndexOf(a, _) | TirExpr::MemberGet { receiver: a, .. }
        | TirExpr::DictKeys(a) | TirExpr::DictValues(a) => expr_uses_local(a),
        TirExpr::ListPopAt(a, b) | TirExpr::ListGet(a, b) | TirExpr::DictGet(a, b)
        | TirExpr::ListRemove(a, b)
        | TirExpr::Bin(_, a, b) | TirExpr::Cmp(_, a, b) => expr_uses_local(a) || expr_uses_local(b),
        TirExpr::ListRepeat { value, len, .. } => expr_uses_local(value) || expr_uses_local(len),
        TirExpr::Select { cond, then_expr, else_expr } => {
            expr_uses_local(cond) || expr_uses_local(then_expr) || expr_uses_local(else_expr)
        }
        TirExpr::NativeCallExpr { args, .. } => args.iter().any(expr_uses_local),
        TirExpr::ClassNew { args, .. } | TirExpr::MethodCall { args, .. } => args.iter().any(expr_uses_local),
        TirExpr::Int(_) | TirExpr::Float(_) | TirExpr::Bool(_) | TirExpr::Str(_) => false,
        TirExpr::Located { inner, .. } => expr_uses_local(inner),
        TirExpr::MakeClosure { captures, .. } => captures.iter().any(|s| expr_uses_local(&TirExpr::Local(*s))),
        TirExpr::CallObj { callee, args, .. } => expr_uses_local(callee) || args.iter().any(expr_uses_local),
        TirExpr::Slice { list, start, end, step } => {
            expr_uses_local(list)
                || start.as_ref().is_some_and(|e| expr_uses_local(e))
                || end.as_ref().is_some_and(|e| expr_uses_local(e))
                || step.as_ref().is_some_and(|e| expr_uses_local(e))
        }
        TirExpr::EmptyCollection { .. } => false,
    }
}

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

pub(crate) use crate::typeck::native_module_ret_type;

fn core_ty_of(ty: &Ty) -> Option<CoreTy> {
    // `bytes` is an opaque NObj handle in NRT (`nrt/src/bytes_native.rs`), so it
    // lowers to the same boxed core type as other dynamic-but-supported values.
    match ty {
        Ty::Int => Some(CoreTy::Int),
        Ty::Float => Some(CoreTy::Float),
        Ty::Bool => Some(CoreTy::Bool),
        Ty::Str => Some(CoreTy::Str),
        Ty::Bytes => Some(CoreTy::Obj),
        _ => None,
    }
}

/// Move statements hoisted by expression lowering into `out` at `at`.
fn flush_pending_at(ctx: &mut LowerCtx, out: &mut Vec<TirStmt>, at: usize) {
    if ctx.pending.is_empty() {
        return;
    }
    let pending = std::mem::take(&mut ctx.pending);
    out.splice(at..at, pending);
}

/// Append hoisted statements to the end of `out` (used when no further
/// statements will be emitted for the current construct).
fn flush_pending(ctx: &mut LowerCtx, out: &mut Vec<TirStmt>) {
    if !ctx.pending.is_empty() {
        out.append(&mut ctx.pending);
    }
}

fn lower_block(node: &Node, ctx: &mut LowerCtx, out: &mut Vec<TirStmt>) -> Result<(), String> {
    for child in &node.children {
        let at = out.len();
        lower_stmt(child, ctx, out)?;
        flush_pending_at(ctx, out, at);
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
        NodeKind::Match => {
            // Standalone `match`; the value (if any) is discarded. Expression
            // positions are handled by `lower_expr`.
            let _ = lower_match_expr(node, ctx)?;
            Ok(())
        }
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
                        let span = (node.start, node.length);
                        out.push(TirStmt::Throw(TirExpr::Located {
                            span,
                            inner: Box::new(expr),
                        }));
                        return Ok(());
                    }
                }
            }
            if let Some((module, member, arg_nodes)) = native_call_shape(node, ctx) {
                let args = arg_nodes
                    .into_iter()
                    .map(|a| lower_expr(a, ctx).map(|(e, _)| e))
                    .collect::<Result<Vec<_>, _>>()?;
                out.push(TirStmt::NativeCall { module, member, args });
                return Ok(());
            }
            if let Some(stmt) = lower_list_method_stmt(node, ctx)? {
                out.push(stmt);
                return Ok(());
            }
            if let Some(stmt) = lower_dict_method_stmt(node, ctx)? {
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
    let body_block = node
        .children
        .first()
        .ok_or_else(|| "'try' without a body".to_string())?;
    if body_block.kind != NodeKind::Block {
        return Err("'try' body must be a block".to_string());
    }
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
        None => body_block,    };
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

/// Collect identifiers in a lambda body that refer to locals of the enclosing
/// function (free variables). These become leading parameters of the
/// synthesized lambda function.
fn lambda_captures(body: &Node, param_names: &[String], ctx: &LowerCtx) -> Vec<String> {
    fn visit(node: &Node, names: &[String], ctx: &LowerCtx, out: &mut Vec<String>) {
        if node.kind == NodeKind::Identifier {
            let name = ctx.source.span(node.start, node.length).to_string();
            if ctx.slots.contains_key(&name)
                && !names.contains(&name)
                && !out.contains(&name)
            {
                out.push(name);
            }
        }
        for child in &node.children {
            visit(child, names, ctx, out);
        }
    }
    let mut out = Vec::new();
    visit(body, param_names, ctx, &mut out);
    out
}

fn synthesize_lambda(
    node: &Node,
    ctx: &mut LowerCtx,
    func_index: u32,
) -> Result<Vec<u16>, String> {
    let params_node = node.children.first().filter(|n| n.kind == NodeKind::Tuple)
        .ok_or_else(|| "lambda without parameters".to_string())?;
    let body_node = node.children.get(1).filter(|n| n.kind == NodeKind::Block)
        .ok_or_else(|| "lambda without a body".to_string())?;
    let mut param_names = Vec::new();
    let mut param_tys = Vec::new();
    for param in &params_node.children {
        let name = param_name_text_local(param, ctx.source)
            .ok_or_else(|| "lambda parameter without a name".to_string())?;
        let ty_node = param_type_node_local(param);
        let ty = match ty_node {
            Some(t) => {
                let text = ctx.source.span(t.start, t.length).to_string();
                match text.as_str() {
                    "int" => Ty::Int,
                    "float" => Ty::Float,
                    "bool" => Ty::Bool,
                    "str" => Ty::Str,
                    "none" => Ty::None_,
                    _ => return Err(format!("unsupported lambda parameter type '{text}'")),
                }
            }
            None => return Err("lambda parameter requires a type annotation".to_string()),
        };
        param_names.push(name);
        param_tys.push(ty);
    }
    let captures = lambda_captures(body_node, &param_names, ctx);
    let capture_slots: Vec<u16> = captures.iter()
        .map(|n| *ctx.slots.get(n).expect("capture is an enclosing local"))
        .collect();

    // Build a child lowering context: capture slots first, then parameters.
    let mut child = LowerCtx {
        source: ctx.source,
        checked: ctx.checked,
        tir_index: ctx.tir_index,
        call_names: ctx.call_names,
        current_instance: ctx.current_instance.clone(),
        instance_args: ctx.instance_args,
        slots: HashMap::new(),
        locals: Vec::new(),
        declared_types: HashMap::new(),
        classes: ctx.classes,
        lambda_vars: HashMap::new(),
        lambdas: ctx.lambdas,
        lambda_base: ctx.lambda_base,
        globals: ctx.globals,
        wrote_global: ctx.wrote_global,
        pending: Vec::new(),
    };
    for (name, parent_slot) in captures.iter().zip(&capture_slots) {
        let tty = ctx.locals.get(*parent_slot as usize).copied().unwrap_or(TirTy::Int);
        let child_slot = child.locals.len() as u16;
        child.slots.insert(name.clone(), child_slot);
        child.locals.push(tty);
    }
    let mut params = Vec::new();
    for (name, ty) in param_names.iter().zip(&param_tys) {
        let tty = tir_ty_from_ty(child.classes, ty)
            .ok_or_else(|| format!("lambda parameter type '{}' is not core-supported", ty.display()))?;
        let slot = child.locals.len() as u16;
        child.slots.insert(name.clone(), slot);
        child.locals.push(tty);
        params.push(tty);
    }
    // Captures precede parameters in the synthesized signature.
    let capture_tys: Vec<TirTy> = child.locals[..capture_slots.len()].to_vec();
    let mut all_params = capture_tys;
    all_params.extend(params);
    let defaults = vec![None; all_params.len()];
    // Reserve this lambda's slot BEFORE lowering the body, so nested lambdas
    // get higher indices and this lambda's absolute index stays stable.
    let absolute_index = ctx.lambda_base + func_index;
    let placeholder = TirFn {
        name: format!("__lambda_{absolute_index}"),
        proto_span: (node.start, node.length),
        index: absolute_index as usize,
        params: all_params.clone(),
        defaults: defaults.clone(),
        ret: Some(TirTy::Int),
        locals: child.locals.clone(),
        body: Vec::new(),
    };
    child.lambdas.push(placeholder);
    let mut body = Vec::new();
    lower_block(body_node, &mut child, &mut body)?;
    let ret = body.iter().find_map(|st| match st {
        TirStmt::Return(Some(e)) => infer_simple_ty(e, &child.locals),
        _ => None,
    }).or(Some(TirTy::Int));
    child.lambdas[func_index as usize] = TirFn {
        name: format!("__lambda_{absolute_index}"),
        proto_span: (node.start, node.length),
        index: absolute_index as usize,
        params: all_params,
        defaults,
        ret,
        locals: child.locals,
        body,
    };
    Ok(capture_slots)
}

fn infer_simple_ty(e: &TirExpr, locals: &[TirTy]) -> Option<TirTy> {
    match e {
        TirExpr::Int(_) => Some(TirTy::Int),
        TirExpr::Float(_) => Some(TirTy::Float),
        TirExpr::Bool(_) => Some(TirTy::Bool),
        TirExpr::Str(_) => Some(TirTy::Str),
        TirExpr::Local(s) => locals.get(*s as usize).copied(),
        TirExpr::GlobalGet(_) => Some(TirTy::Int),
        TirExpr::Promote(_) | TirExpr::CastFloat(_) => Some(TirTy::Float),
        TirExpr::CastInt(_) => Some(TirTy::Int),
        TirExpr::Bin(_, a, b) => {
            let ta = infer_simple_ty(a, locals);
            let tb = infer_simple_ty(b, locals);
            if ta == Some(TirTy::Float) || tb == Some(TirTy::Float) { Some(TirTy::Float) }
            else { ta.or(tb) }
        }
        TirExpr::Cmp(_, _, _) | TirExpr::Un(TirUnOp::Not, _) | TirExpr::Len(_)
        | TirExpr::ListContains(_, _) | TirExpr::ListRemove(_, _) => Some(TirTy::Bool),
        TirExpr::Un(TirUnOp::Neg, a) => infer_simple_ty(a, locals),
        TirExpr::Located { inner, .. } => infer_simple_ty(inner, locals),
        TirExpr::List(_) => Some(TirTy::ListInt),
        TirExpr::CallObj { ret, .. } => Some(*ret),
        _ => None,
    }
}

fn param_name_text_local(param: &Node, source: &Source) -> Option<String> {
    let inner = param.children.first()?;
    match inner.kind {
        NodeKind::Identifier => Some(source.span(inner.start, inner.length).to_string()),
        NodeKind::Typed | NodeKind::Assign => {
            let n = inner.children.first()?;
            if n.kind == NodeKind::Typed { param_name_text_local(n, source) } else {
                Some(source.span(n.start, n.length).to_string())
            }
        }
        _ => None,
    }
}
fn param_type_node_local(param: &Node) -> Option<&Node> {
    let inner = param.children.first()?;
    match inner.kind {
        NodeKind::Typed => inner.children.get(1),
        NodeKind::Assign => {
            let n = inner.children.first()?;
            if n.kind == NodeKind::Typed { n.children.get(1) } else { None }
        }
        _ => None,
    }
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
            let name = ctx.source.span(target.start, target.length).to_string();
            if slot_of(ctx, &name).is_none() {
                if let Some(&g) = ctx.globals.get(&name) {
                    let (expr, ty) = lower_expr(value, ctx)?;
                    if ty != CoreTy::Int {
                        return Err(format!("global '{name}' must be initialized with an int in a no-VM exe"));
                    }
                    *ctx.wrote_global = true;
                    out.push(TirStmt::GlobalSet(g, expr));
                    return Ok(());
                }
            }
            if value.kind == NodeKind::Lambda {
                let idx = ctx.lambdas.len() as u32;
                let captures = synthesize_lambda(value, ctx, idx)?;
                let func_index = ctx.lambda_base + idx;
                let (kinds, ret_kind) = {
                    let f = &ctx.lambdas[idx as usize];
                    let k = f.params.iter().copied().map(abi_kind).collect();
                    let r = f.ret.map_or(3, abi_kind);
                    (k, r)
                };
                let closure = TirExpr::MakeClosure { func_index, captures: captures.clone(), kinds, ret_kind };
                if let Some(slot) = slot_of(ctx, &name) {
                    ctx.lambda_vars.insert(slot, (func_index, captures));
                    out.push(TirStmt::Set(slot, closure));
                } else {
                    let slot = declare(ctx, &name, TirTy::Lambda(func_index));
                    ctx.lambda_vars.insert(slot, (func_index, captures));
                    out.push(TirStmt::Let(slot, closure));
                }
                return Ok(());
            }
            let expected = slot_of(ctx, &name).and_then(|s| ctx.locals.get(s as usize).copied());
            let (expr, ty) = lower_expr_expected(value, ctx, expected)?;
            if let Some(slot) = slot_of(ctx, &name) {
                ctx.lambda_vars.remove(&slot);
                out.push(TirStmt::Set(slot, expr));
            } else {
                let tty = tir_ty_of(ty)?;
                let slot = declare(ctx, &name, tty);
                out.push(TirStmt::Let(slot, expr));
            }
            Ok(())
        }
        NodeKind::Typed => {
            let name_node = target
                .children
                .first()
                .ok_or_else(|| "typed assignment without a name".to_string())?;
            if name_node.kind != NodeKind::Identifier {
                return Err("typed assignment target must be a plain variable in the core".to_string());
            }
            let name = ctx.source.span(name_node.start, name_node.length).to_string();
            let expected = ctx.declared_types.get(&name).copied();
            let (expr, ty) = lower_expr_expected(value, ctx, expected)?;
            if let Some(slot) = slot_of(ctx, &name) {
                out.push(TirStmt::Set(slot, expr));
            } else {
                let tty = tir_ty_of(ty)?;
                let slot = declare(ctx, &name, tty);
                out.push(TirStmt::Let(slot, expr));
            }
            Ok(())
        }
        NodeKind::Slice => {
            // `xs[a:b:c] = value` — bounds use the same `Option<Obj>` convention
            // as slice reads (omitted bounds become NRT's none handle).
            let collection = target
                .children
                .first()
                .ok_or_else(|| "slice assignment without a target".to_string())?;
            let (list, list_ty) = lower_expr(collection, ctx)?;
            if !matches!(
                list_ty,
                CoreTy::ListInt | CoreTy::ListFloat | CoreTy::ListClass(_)
                    | CoreTy::ListLambda | CoreTy::ListGen(_) | CoreTy::Obj
            ) {
                return Err("slice assignment requires a list".to_string());
            }
            let start = lower_slice_bound(target.children.get(1), ctx)?;
            let end = lower_slice_bound(target.children.get(2), ctx)?;
            let step = lower_slice_bound(target.children.get(3), ctx)?;
            let (value_expr, value_ty) = lower_expr(value, ctx)?;
            if !matches!(
                value_ty,
                CoreTy::ListInt | CoreTy::ListFloat | CoreTy::ListClass(_)
                    | CoreTy::ListLambda | CoreTy::ListGen(_) | CoreTy::Str | CoreTy::Obj
            ) {
                return Err("slice assignment value must be a list or string".to_string());
            }
            out.push(TirStmt::SetSlice {
                list,
                value: value_expr,
                start,
                end,
                step,
            });
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
                (CoreTy::ListClass(id), CoreTy::Int) => {
                    if v_ty != CoreTy::Class(id) {
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
                (CoreTy::DictIntInt, CoreTy::Int) => {
                    if v_ty != CoreTy::Int {
                        return Err("dict values must all have the same type".to_string());
                    }
                    out.push(TirStmt::DictSet { dict: c_expr, key: k_expr, value: v_expr });
                    Ok(())
                }
                (CoreTy::ListGen(elem), CoreTy::Int) => {
                    let expected = CoreTy::from(tir_of_elem(elem));
                    if v_ty != expected {
                        return Err("list elements must all have the same type".to_string());
                    }
                    out.push(TirStmt::ListSet { list: c_expr, index: k_expr, value: v_expr });
                    Ok(())
                }
                (CoreTy::DictGen(k, v), key) if elem_of_core(key) == k => {
                    let expected = CoreTy::from(tir_of_elem(v));
                    if v_ty != expected {
                        return Err("dict values must all have the same type".to_string());
                    }
                    out.push(TirStmt::DictSet { dict: c_expr, key: k_expr, value: v_expr });
                    Ok(())
                }
                (
                    CoreTy::DictStrInt | CoreTy::DictStrFloat | CoreTy::DictIntInt
                    | CoreTy::DictGen(_, _),
                    other,
                ) => {
                    let _ = other;
                    Err("dict keys must be strings or ints".to_string())
                }
                (
                    CoreTy::ListInt | CoreTy::ListFloat | CoreTy::ListClass(_)
                    | CoreTy::ListGen(_),
                    _,
                ) => {
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
            let field_ty = ctx.classes.field(id, &member).cloned();
            let field_ty = match field_ty {
                Some(ty) => ty,
                None => {
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
                    (CoreTy::Float, CoreTy::Int) => {}                    (t, v) if t == v => {}
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

fn elem_of_core(ty: CoreTy) -> ObjElem {
    tir_ty_of(ty).map_or(ObjElem::Obj, elem_of_tir)
}

fn tir_ty_of(ty: CoreTy) -> Result<TirTy, String> {
    match ty {
        CoreTy::Int => Ok(TirTy::Int),
        CoreTy::Float => Ok(TirTy::Float),
        CoreTy::Bool => Ok(TirTy::Bool),
        CoreTy::Str => Ok(TirTy::Str),
        CoreTy::ListInt => Ok(TirTy::ListInt),
        CoreTy::ListFloat => Ok(TirTy::ListFloat),
        CoreTy::ListClass(id) => Ok(TirTy::ListClass(id)),
        CoreTy::ListLambda => Ok(TirTy::ListLambda),
        CoreTy::DictStrInt => Ok(TirTy::DictStrInt),
        CoreTy::DictStrFloat => Ok(TirTy::DictStrFloat),
        CoreTy::DictIntInt => Ok(TirTy::DictIntInt),
        CoreTy::Obj => Ok(TirTy::Obj),
        CoreTy::ListGen(e) => Ok(TirTy::ListGen(e)),
        CoreTy::DictGen(k, v) => Ok(TirTy::DictGen(k, v)),
        CoreTy::SetGen(e) => Ok(TirTy::SetGen(e)),
        CoreTy::TupleGen(e) => Ok(TirTy::TupleGen(e)),
        CoreTy::None_ => {
            Err("cannot infer a concrete local type in the core".to_string())
        }
        CoreTy::Class(id) => Ok(TirTy::Class(id)),
        CoreTy::Lambda(index) => Ok(TirTy::Lambda(index)),
    }
}

fn lower_if(node: &Node, ctx: &mut LowerCtx, out: &mut Vec<TirStmt>) -> Result<(), String> {
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
    let mut else_out: Vec<TirStmt> = Vec::new();
    if let Some(body) = else_body {
        lower_block(body, ctx, &mut else_out)?;
    }
    for (cond, body) in arms.into_iter().rev() {
        let (cond_expr, cond_ty) = lower_expr(cond, ctx)?;
        // A `match` in a condition hoists statements; emit them before any
        // nested branch bodies are lowered into their own buffers.
        flush_pending(ctx, out);
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
    flush_pending(ctx, out);
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
    let name = match target.kind {
        NodeKind::Identifier => ctx.source.span(target.start, target.length).to_string(),
        NodeKind::Wildcard => {
            format!("${}", ctx.locals.len())
        }
        _ => return Err("for loop target must be a plain variable in the core".to_string()),
    };
    if let Some(slot) = slot_of(ctx, &ctx.source.span(iter.start, iter.length).to_string()) {
        let list_ty = ctx.locals[slot as usize];
        if matches!(list_ty, TirTy::ListInt | TirTy::ListFloat | TirTy::ListClass(_) | TirTy::ListGen(_)) {
            let elem_ty = match list_ty {
                TirTy::ListInt => TirTy::Int,
                TirTy::ListFloat => TirTy::Float,
                TirTy::ListClass(id) => TirTy::Class(id),
                TirTy::ListGen(e) => tir_of_elem(e),
                _ => unreachable!(),
            };
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
    if iter.kind == NodeKind::List {
        let (collection, cty) = lower_expr(iter, ctx)?;
        let elem_ty = match cty {
            CoreTy::ListInt => TirTy::Int,
            CoreTy::ListFloat => TirTy::Float,
            CoreTy::ListClass(id) => TirTy::Class(id),
            _ => return Err("for ... in requires a core list".to_string()),
        };
        flush_pending(ctx, out);
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
    flush_pending(ctx, out);
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
            (Ty::Int, Ty::Int) => Ok(CoreTy::DictIntInt),
            (Ty::Str, other) => Err(format!("field of type 'dict[str, {}]' is not supported in the core", other.display())),
            (other, _) => Err(format!("field of type 'dict[{}, ...]' is not supported in the core", other.display())),
        },
        Ty::Class(name) => {
            let id = ctx.classes.id(name).ok_or_else(|| format!("unknown class '{name}'"))?;
            Ok(CoreTy::Class(id))
        }
        Ty::Any | Ty::Union(_) | Ty::Unknown => Ok(CoreTy::Obj),
        other => Err(format!("field of type '{}' is not supported in the core", other.display())),
    }
}

/// A slice bound is either an int expression or `None` (an omitted bound).
fn lower_slice_bound(node: Option<&Node>, ctx: &mut LowerCtx) -> Result<Option<TirExpr>, String> {
    let Some(node) = node else { return Ok(None) };
    if node.kind == NodeKind::Null {
        return Ok(None);
    }
    let (expr, ty) = lower_expr(node, ctx)?;
    if ty != CoreTy::Int {
        return Err("slice bounds must be ints".to_string());
    }
    Ok(Some(expr))
}

/// Collect binding names declared by a match pattern node. Used to allocate
/// distinct slots for each or-pattern alternative and to map body references
/// onto the committed slots of the winning alternative.
fn pattern_bind_names(node: &Node, source: &Source, out: &mut Vec<String>) {
    match node.kind {
        NodeKind::Identifier => {
            let name = source.span(node.start, node.length).to_string();
            if !out.contains(&name) {
                out.push(name);
            }
        }
        NodeKind::Typed | NodeKind::Guard => {
            if let Some(first) = node.children.first() {
                pattern_bind_names(first, source, out);
            }
        }
        NodeKind::OrPattern | NodeKind::Tuple | NodeKind::List => {
            for child in &node.children {
                pattern_bind_names(child, source, out);
            }
        }
        _ => {}
    }
}

/// Flatten nested `OrPattern`s into leaf alternatives (left to right).
fn flatten_or_alternatives<'a>(node: &'a Node, out: &mut Vec<&'a Node>) {
    if node.kind == NodeKind::OrPattern {
        for child in &node.children {
            flatten_or_alternatives(child, out);
        }
    } else {
        out.push(node);
    }
}

/// Lower one leaf pattern. Bind names have already been declared in the
/// current slot scope by `lower_match_expr` (per alternative).
fn lower_match_pattern(node: &Node, ctx: &mut LowerCtx) -> Result<TirPattern, String> {
    match node.kind {
        NodeKind::Wildcard => Ok(TirPattern::Wildcard),
        NodeKind::Identifier | NodeKind::Typed => {
            let name_node = if node.kind == NodeKind::Typed {
                node.children.first().ok_or("typed pattern without a name")?
            } else {
                node
            };
            if name_node.kind != NodeKind::Identifier {
                return Err("match bind pattern must be a plain variable".to_string());
            }
            let name = ctx.source.span(name_node.start, name_node.length).to_string();
            let slot = slot_of(ctx, &name)
                .ok_or_else(|| format!("unresolved match binding '{name}'"))?;
            Ok(TirPattern::Bind(slot))
        }
        NodeKind::Integer | NodeKind::Float | NodeKind::String | NodeKind::Boolean
        | NodeKind::None => {
            let (lit, _) = lower_expr(node, ctx)?;
            Ok(TirPattern::Literal(lit))
        }
        _ => Err(format!(
            "match pattern kind {:?} requires `un build --embed`",
            node.kind
        )),
    }
}

/// Lower a `match` expression. Literal / bind / wildcard leaves, optional
/// guards and or-patterns are supported. An or-pattern is flattened into one
/// TIR arm per alternative: each alternative gets its own bind slots (the
/// equivalent of the VM's isolated `scope::new(None, false)`), and the
/// winning alternative commits them to body-visible slots with `Set`s at the
/// head of its body. Guards are lowered in the alternative's slot scope so a
/// guard can read bindings; false guards fall through to the next arm.
///
/// Scrutinee, result, committed slots and match statements are staged in
/// `ctx.pending` and flushed by the enclosing `lower_block`.
fn lower_match_expr(node: &Node, ctx: &mut LowerCtx) -> Result<(TirExpr, CoreTy), String> {
    let scrut_node = node.children.first().ok_or("match without scrutinee")?;
    let (scrut_expr, scrut_cty) = lower_expr(scrut_node, ctx)?;
    let scrut_ty = tir_ty_of(scrut_cty).unwrap_or(TirTy::Obj);
    let scrut_slot = declare(ctx, &format!("$match_scrut_{}", node.start), scrut_ty);
    ctx.pending.push(TirStmt::Let(scrut_slot, scrut_expr));

    let result_slot = declare(ctx, &format!("$match_result_{}", node.start), TirTy::Int);
    // Placeholder, patched with the final initializer once all arm bodies
    // have decided the result type. Keeping the `Let` here makes declaration
    // order (scrutinee, result, committed slots, per-arm slots) match TIR
    // validation's `collect_decls` walk.
    let result_let_at = ctx.pending.len();
    ctx.pending.push(TirStmt::Let(result_slot, TirExpr::Int(0)));

    let mut arms: Vec<TirMatchArm> = Vec::new();
    let mut result_cty: Option<CoreTy> = None;
    let mut result_ty = TirTy::Int;

    for case in node.children.iter().skip(1) {
        if case.kind != NodeKind::Case {
            continue;
        }
        let head = case.children.first().ok_or("match case without pattern")?;
        let body_node = case.children.get(1).ok_or("match case without body")?;
        if body_node.kind == NodeKind::Block {
            return Err("block bodies in match arms are not supported in the core".to_string());
        }
        let (pat_node, guard_node) = if head.kind == NodeKind::Guard {
            (
                head.children.first().ok_or("guard without a pattern")?,
                Some(head.children.get(1).ok_or("guard without a condition")?),
            )
        } else {
            (head, None)
        };

        let mut alternatives: Vec<&Node> = Vec::new();
        flatten_or_alternatives(pat_node, &mut alternatives);
        if alternatives.is_empty() {
            return Err("or-pattern has no alternatives".to_string());
        }
        let is_or = alternatives.len() > 1;

        // Bindings visible to the body/guard are those declared by every
        // alternative, mirroring typeck's `merge_arms`.
        let mut common: Vec<String> = Vec::new();
        pattern_bind_names(alternatives[0], ctx.source, &mut common);
        for alt in alternatives.iter().skip(1) {
            let mut names = Vec::new();
            pattern_bind_names(alt, ctx.source, &mut names);
            common.retain(|name| names.contains(name));
        }

        // For an or-pattern, allocate body-visible (committed) slots once and
        // initialize them from the scrutinee. These are ordinary `Let`s ahead
        // of the `Match`, so validation sees them in declaration order.
        let mut committed: HashMap<String, u16> = HashMap::new();
        if is_or {
            for name in &common {
                let slot = declare(ctx, name, scrut_ty);
                committed.insert(name.clone(), slot);
                ctx.pending.push(TirStmt::Let(slot, TirExpr::Local(scrut_slot)));
            }
        }

        for (alt_index, alt) in alternatives.iter().enumerate() {
            // Arm-local names must not leak into later arms or the enclosing
            // scope. Each alternative gets a fresh slot for every name it
            // binds; only the winner's copies reach the committed slots.
            let saved_slots = ctx.slots.clone();
            let mut alt_binds = Vec::new();
            pattern_bind_names(alt, ctx.source, &mut alt_binds);
            let mut alt_slots: HashMap<String, u16> = HashMap::new();
            for name in &alt_binds {
                let slot = if is_or {
                    let temp_name = format!("$or{}_{}_{}", node.start, alt_index, name);
                    let slot = declare(ctx, &temp_name, scrut_ty);
                    ctx.slots.insert(name.clone(), slot);
                    slot
                } else {
                    declare(ctx, name, scrut_ty)
                };
                alt_slots.insert(name.clone(), slot);
            }

            let pattern = lower_match_pattern(alt, ctx)?;

            // A guard is evaluated in the alternative's slot scope, after the
            // pattern assigned its bind slots. Nested matches in guards would
            // need a prelude block; keep those functions on the VM path.
            let guard = if let Some(guard_node) = guard_node {
                let mark = ctx.pending.len();
                let (guard_expr, guard_cty) = lower_expr(guard_node, ctx)?;
                if ctx.pending.len() != mark {
                    ctx.pending.truncate(mark);
                    return Err("nested matches in match guards require `un build --embed`".to_string());
                }
                if guard_cty != CoreTy::Bool {
                    return Err("match guard condition must be a bool".to_string());
                }
                Some(guard_expr)
            } else {
                None
            };

            // The body reads committed slots for shared bindings; slots that
            // are not common to all alternatives keep their outer meaning.
            if is_or {
                for (name, slot) in &committed {
                    ctx.slots.insert(name.clone(), *slot);
                }
            }
            // A nested `match` in the arm body hoists its own statements. Keep
            // them inside this arm (before the result assignment) instead of
            // letting them leak into the enclosing block.
            let nested_mark = ctx.pending.len();
            let (body_expr, body_cty) = lower_expr(body_node, ctx)?;
            let nested: Vec<TirStmt> = ctx.pending.drain(nested_mark..).collect();
            match result_cty {
                None => {
                    result_cty = Some(body_cty);
                    result_ty = infer_simple_ty(&body_expr, &ctx.locals)
                        .or_else(|| tir_ty_of(body_cty).ok())
                        .unwrap_or(TirTy::Int);
                    ctx.locals[result_slot as usize] = result_ty;
                }
                Some(prev) if prev != body_cty => {
                    return Err("match arms must all have the same type".to_string());
                }
                Some(_) => {}
            }

            // Winner commit: only the winning alternative's bindings are
            // copied into the slots that the arm body reads.
            let mut body_out: Vec<TirStmt> = Vec::new();
            if is_or {
                for name in &common {
                    if let (Some(&temp), Some(&dst)) = (alt_slots.get(name), committed.get(name)) {
                        body_out.push(TirStmt::Set(dst, TirExpr::Local(temp)));
                    }
                }
            }
            body_out.extend(nested);
            body_out.push(TirStmt::Set(result_slot, body_expr));
            ctx.slots = saved_slots;
            arms.push(TirMatchArm { pattern, guard, body: body_out });
        }
    }

    if arms.is_empty() {
        return Err("match without arms".to_string());
    }
    // Only scalar result types have a `Let`-initializer with a matching
    // `expr_ty`; other result types stay on the VM path.
    if !matches!(result_ty, TirTy::Int | TirTy::Float | TirTy::Bool | TirTy::Str) {
        return Err("match arm result type is not supported in this native mode".to_string());
    }
    let init = match result_ty {
        TirTy::Float => TirExpr::Float(0.0),
        TirTy::Bool => TirExpr::Bool(false),
        TirTy::Str => TirExpr::Str(String::new()),
        _ => TirExpr::Int(0),
    };
    ctx.pending[result_let_at] = TirStmt::Let(result_slot, init);
    ctx.pending.push(TirStmt::Match {
        scrutinee: scrut_slot,
        arms,
        result: result_slot,
        result_ty,
        span: (node.start, node.length),
    });

    Ok((TirExpr::Local(result_slot), result_cty.unwrap()))
}

fn lower_expr(node: &Node, ctx: &mut LowerCtx) -> Result<(TirExpr, CoreTy), String> {
    match node.kind {
        NodeKind::Match => lower_match_expr(node, ctx),
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
        NodeKind::None => Ok((TirExpr::NoneValue, CoreTy::None_)),
        NodeKind::Dict => {
            let mut entries: Vec<(TirExpr, TirExpr)> = Vec::with_capacity(node.children.len());
            let mut key_cty: Option<CoreTy> = None;
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
                if kty != CoreTy::Str && kty != CoreTy::Int {
                    return Err("dict keys must be strings or ints in the core".to_string());
                }
                if let Some(prev) = key_cty {
                    if prev != kty {
                        return Err("dict keys must all have the same type".to_string());
                    }
                } else {
                    key_cty = Some(kty);
                }
                let (ve, vty) = lower_expr(value_node, ctx)?;
                if let Some(prev) = value_cty {
                    if prev != vty {
                        // Heterogeneous `dict[str, any]` stores boxed values.
                        value_cty = Some(CoreTy::Obj);
                    }
                } else {
                    value_cty = Some(vty);
                }
                entries.push((ke, ve));
            }
            if entries.is_empty() {
                return Err("empty dict literal cannot be typed in the core".to_string());
            }
            let ret = match (key_cty.expect("non-empty dict"), value_cty.expect("non-empty dict")) {
                (CoreTy::Str, CoreTy::Int) => CoreTy::DictStrInt,
                (CoreTy::Str, CoreTy::Float) => CoreTy::DictStrFloat,
                (CoreTy::Int, CoreTy::Int) => CoreTy::DictIntInt,
                (k @ (CoreTy::Str | CoreTy::Int), v) => {
                    CoreTy::DictGen(elem_of_core(k), elem_of_core(v))
                }
                _ => return Err("dict keys must be strings or ints in the core".to_string()),
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
                CoreTy::DictStrInt | CoreTy::DictStrFloat | CoreTy::DictIntInt => {
                    let elem_cty = match tty {
                        CoreTy::DictStrInt | CoreTy::DictIntInt => CoreTy::Int,
                        _ => CoreTy::Float,
                    };
                    let expected_key = if tty == CoreTy::DictIntInt { CoreTy::Int } else { CoreTy::Str };
                    let (key, kty) = lower_expr(key_node, ctx)?;
                    if kty != expected_key {
                        return Err("dict key type does not match the dict".to_string());
                    }
                    Ok((
                        TirExpr::Located {
                            span: (node.start, node.length),
                            inner: Box::new(TirExpr::DictGet(Box::new(target), Box::new(key))),
                        },
                        elem_cty,
                    ))
                }
                CoreTy::DictGen(k, v) => {
                    let (key, kty) = lower_expr(key_node, ctx)?;
                    let expected_key = CoreTy::from(tir_of_elem(k));
                    if kty != expected_key {
                        return Err("dict key type does not match the dict".to_string());
                    }
                    Ok((
                        TirExpr::Located {
                            span: (node.start, node.length),
                            inner: Box::new(TirExpr::DictGet(Box::new(target), Box::new(key))),
                        },
                        CoreTy::from(tir_of_elem(v)),
                    ))
                }
                CoreTy::ListInt | CoreTy::ListFloat | CoreTy::ListClass(_) | CoreTy::ListLambda
                | CoreTy::ListGen(_) => {
                    let (key, kty) = lower_expr(key_node, ctx)?;
                    if kty != CoreTy::Int {
                        return Err("list index must be an int".to_string());
                    }
                    let elem_cty = match tty {
                        CoreTy::ListInt => CoreTy::Int,
                        CoreTy::ListFloat => CoreTy::Float,
                        CoreTy::ListClass(id) => CoreTy::Class(id),
                        CoreTy::ListLambda => CoreTy::Lambda(u32::MAX),
                        CoreTy::ListGen(elem) => CoreTy::from(tir_of_elem(elem)),
                        _ => return Err("indexing requires a list".to_string()),
                    };
                    Ok((
                        TirExpr::Located {
                            span: (node.start, node.length),
                            inner: Box::new(TirExpr::ListGet(Box::new(target), Box::new(key))),
                        },
                        elem_cty,
                    ))
                }
                CoreTy::TupleGen(elem) | CoreTy::SetGen(elem) => {
                    let (key, kty) = lower_expr(key_node, ctx)?;
                    if kty != CoreTy::Int {
                        return Err("tuple/set index must be an int".to_string());
                    }
                    let module = if matches!(tty, CoreTy::TupleGen(_)) { "tuple" } else { "set" };
                    let ret = tir_of_elem(elem);
                    Ok((
                        TirExpr::Located {
                            span: (node.start, node.length),
                            inner: Box::new(TirExpr::NativeCallExpr {
                                module: module.into(),
                                member: "get".into(),
                                args: vec![target, key],
                                ret,
                            }),
                        },
                        CoreTy::from(ret),
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
                        // Heterogeneous lists (including `list[any]`) store
                        // boxed handles; scalars box at the collection boundary
                        // in `emit_expr`.
                        elem_cty = Some(CoreTy::Obj);
                    }
                } else {
                    elem_cty = Some(c);
                }
                elements.push(e);
            }
            let elem_cty = match elem_cty {
                Some(c) => c,
                None => return Err("empty list literal cannot be typed in the core".to_string()),
            };
            let ret = match elem_cty {
                CoreTy::Int => CoreTy::ListInt,
                CoreTy::Float => CoreTy::ListFloat,
                CoreTy::Class(id) => CoreTy::ListClass(id),
                CoreTy::Lambda(_) => CoreTy::ListLambda,
                other => CoreTy::ListGen(elem_of_core(other)),
            };
            Ok((TirExpr::List(elements), ret))
        }
        NodeKind::Slice => {
            let target_node = node
                .children
                .first()
                .ok_or_else(|| "slice without a target".to_string())?;
            let (list, list_ty) = lower_expr(target_node, ctx)?;
            if !matches!(
                list_ty,
                CoreTy::ListInt | CoreTy::ListFloat | CoreTy::ListClass(_)
                    | CoreTy::ListLambda | CoreTy::ListGen(_) | CoreTy::Obj
            ) {
                return Err("slicing requires a list".to_string());
            }
            let start = lower_slice_bound(node.children.get(1), ctx)?;
            let end = lower_slice_bound(node.children.get(2), ctx)?;
            let step = lower_slice_bound(node.children.get(3), ctx)?;
            Ok((
                TirExpr::Located {
                    span: (node.start, node.length),
                    inner: Box::new(TirExpr::Slice {
                        list: Box::new(list),
                        start: start.map(Box::new),
                        end: end.map(Box::new),
                        step: step.map(Box::new),
                    }),
                },
                list_ty,
            ))
        }
        NodeKind::FString => {
            // The VM emits `ToStr` for each interpolation, then `FString` joins
            // the pieces. TIR already lowers Str `+` to `unrt_str_concat`, so a
            // left fold over the pieces produces the same string. Interpolations
            // must be statically Str; other types have no ToStr ABI in NRT.
            let mut pieces = Vec::with_capacity(node.children.len());
            for child in &node.children {
                let (piece, ty) = lower_expr(child, ctx)?;
                if ty != CoreTy::Str {
                    return Err("f-string interpolation requires a str expression in the core".to_string());
                }
                pieces.push(piece);
            }
            if pieces.is_empty() {
                return Ok((TirExpr::Str(String::new()), CoreTy::Str));
            }
            let mut iter = pieces.into_iter();
            let mut acc = iter.next().unwrap();
            for piece in iter {
                acc = TirExpr::Bin(TirBinOp::Add, Box::new(acc), Box::new(piece));
            }
            Ok((acc, CoreTy::Str))
        }
        NodeKind::Bytes => {
            // VM materialises `b"..."` as a list[int] of byte values.
            let mut bytes = Vec::with_capacity(node.children.len());
            for child in &node.children {
                match child.value {
                    Some(Const::Int(v)) => bytes.push(TirExpr::Int(v)),
                    _ => return Err("byte literal element must be an integer".to_string()),
                }
            }
            if bytes.is_empty() {
                return Ok((TirExpr::EmptyCollection { ty: TirTy::ListInt }, CoreTy::ListInt));
            }
            Ok((TirExpr::List(bytes), CoreTy::ListInt))
        }
        NodeKind::Tuple => {
            // NRT tag 14: build a temporary list of boxed elements, then wrap
            // it with `("tuple", "new")` through the existing native bridge.
            // The exact element kind comes from the literal (or, for an empty
            // literal, from the annotated `Ty::Tuple` via
            // `lower_expr_expected`).
            let mut elements = Vec::with_capacity(node.children.len());
            let mut elem: Option<ObjElem> = None;
            for child in &node.children {
                let (e, c) = lower_expr(child, ctx)?;
                let next = elem_of_core(c);
                elem = Some(match elem {
                    None => next,
                    Some(prev) if prev == next => prev,
                    _ => ObjElem::Obj,
                });
                elements.push(e);
            }
            let elem = elem.unwrap_or(ObjElem::Obj);
            let inner = if elements.is_empty() {
                TirExpr::EmptyCollection { ty: TirTy::ListGen(elem) }
            } else {
                TirExpr::List(elements)
            };
            Ok((
                TirExpr::NativeCallExpr {
                    module: "tuple".into(),
                    member: "new".into(),
                    args: vec![inner],
                    ret: TirTy::TupleGen(elem),
                },
                CoreTy::TupleGen(elem),
            ))
        }
        NodeKind::Set => {
            // NRT tag 15: dedup and first-occurrence ordering are implemented
            // by `("set", "new")` in the native runtime.
            let mut elements = Vec::with_capacity(node.children.len());
            let mut elem: Option<ObjElem> = None;
            for child in &node.children {
                let (e, c) = lower_expr(child, ctx)?;
                let next = elem_of_core(c);
                elem = Some(match elem {
                    None => next,
                    Some(prev) if prev == next => prev,
                    _ => ObjElem::Obj,
                });
                elements.push(e);
            }
            let elem = elem.unwrap_or(ObjElem::Obj);
            let inner = if elements.is_empty() {
                TirExpr::EmptyCollection { ty: TirTy::ListGen(elem) }
            } else {
                TirExpr::List(elements)
            };
            Ok((
                TirExpr::NativeCallExpr {
                    module: "set".into(),
                    member: "new".into(),
                    args: vec![inner],
                    ret: TirTy::SetGen(elem),
                },
                CoreTy::SetGen(elem),
            ))
        }
        NodeKind::Property => {
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
            // Local slots shadow globals (as in the VM's scope chain).
            if let Some(slot) = slot_of(ctx, &name) {
                let ty = CoreTy::from(ctx.locals[slot as usize]);
                return Ok((TirExpr::Local(slot), ty));
            }
            if let Some(&g) = ctx.globals.get(&name) {
                return Ok((TirExpr::GlobalGet(g), CoreTy::Int));
            }
            // A bare top-level function name is a first-class function value.
            // Synthesize a zero-capture wrapper lambda that forwards to the
            // target function: NRT auto-registers `__lambda_N` from main.rs,
            // and `unrt_call_obj` dispatches the resulting closure object.
            if let Some(idx) = ctx.checked.functions.iter().position(|f| f.sig.name == name) {
                let target = &ctx.checked.functions[idx];
                let target_tir_i = *ctx.tir_index.get(&idx)
                    .ok_or_else(|| format!("function '{name}' is not core-eligible"))?;
                let mut params = Vec::with_capacity(target.sig.params.len());
                for (p, ty) in target.sig.param_names.iter().zip(&target.sig.params) {
                    params.push(tir_ty_from_ty(ctx.classes, ty)
                        .ok_or_else(|| format!("parameter '{p}' of '{name}' is not core-typed"))?);
                }
                let ret = tir_ty_from_ty(ctx.classes, &target.sig.ret)
                    .ok_or_else(|| format!("return type of '{name}' is not core-typed"))?;
                let n = params.len();
                let body = vec![TirStmt::Return(Some(TirExpr::Call(
                    target_tir_i as u32,
                    (0..n).map(|i| TirExpr::Local(i as u16)).collect(),
                )))];
                let absolute_index = ctx.lambda_base + ctx.lambdas.len() as u32;
                ctx.lambdas.push(TirFn {
                    name: format!("__lambda_{absolute_index}"),
                    proto_span: (node.start, node.length),
                    index: absolute_index as usize,
                    params: params.clone(),
                    defaults: vec![None; n],
                    ret: Some(ret),
                    locals: params.clone(),
                    body,
                });
                let kinds = params.iter().copied().map(abi_kind).collect();
                let ret_kind = abi_kind(ret);
                return Ok((
                    TirExpr::MakeClosure {
                        func_index: absolute_index,
                        captures: vec![],
                        kinds,
                        ret_kind,
                    },
                    CoreTy::Lambda(absolute_index),
                ));
            }
            Err(format!("unknown variable '{name}'"))
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

/// Lower an expression with the binding's declared type available. Typeck
/// gives `[]` / `{}` (and empty tuple/set literals) the inferred `any` element
/// type but records the annotated type on the binding; TIR uses that expected
/// type to emit an empty collection with the exact element kind from
/// `Ty::Tuple` / `Ty::Set` instead of a hardcoded `TirTy::ListInt`.
fn lower_expr_expected(
    node: &Node,
    ctx: &mut LowerCtx,
    expected: Option<TirTy>,
) -> Result<(TirExpr, CoreTy), String> {
    let is_empty_list = node.kind == NodeKind::List && node.children.is_empty();
    let is_empty_dict = node.kind == NodeKind::Dict && node.children.is_empty();
    let is_empty_tuple = node.kind == NodeKind::Tuple && node.children.is_empty();
    let is_empty_set = node.kind == NodeKind::Set && node.children.is_empty();
    if !is_empty_list && !is_empty_dict && !is_empty_tuple && !is_empty_set {
        return lower_expr(node, ctx);
    }
    let compatible = expected.filter(|ty| {
        if is_empty_list {
            matches!(
                ty,
                TirTy::ListInt | TirTy::ListFloat | TirTy::ListClass(_)
                    | TirTy::ListLambda | TirTy::ListGen(_)
            )
        } else if is_empty_tuple {
            matches!(ty, TirTy::TupleGen(_))
        } else if is_empty_set {
            matches!(ty, TirTy::SetGen(_))
        } else {
            matches!(
                ty,
                TirTy::DictStrInt | TirTy::DictStrFloat | TirTy::DictIntInt | TirTy::DictGen(_, _)
            )
        }
    });
    let ty = compatible.unwrap_or_else(|| {
        // An unannotated empty tuple/set still has a well-defined lowering:
        // both carry an opaque element kind until a `Ty::Tuple` / `Ty::Set`
        // annotation supplies the precise one.
        if is_empty_tuple {
            TirTy::TupleGen(ObjElem::Obj)
        } else if is_empty_set {
            TirTy::SetGen(ObjElem::Obj)
        } else {
            TirTy::Obj
        }
    });
    if ty == TirTy::Obj {
        return Err("empty list/dict literal needs an annotated list/dict type in the core".to_string());
    }
    Ok((TirExpr::EmptyCollection { ty }, CoreTy::from(ty)))
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
            CoreTy::Obj => Ok((TirExpr::Un(TirUnOp::Neg, Box::new(expr)), CoreTy::Obj)),
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
    if matches!(op, TokenType::Equal | TokenType::Unequal) {
        let l_none = lhs.kind == NodeKind::None;
        let r_none = rhs.kind == NodeKind::None;
        if l_none ^ r_none {
            return Ok((TirExpr::Bool(op == TokenType::Unequal), CoreTy::Bool));
        }
    }
    let (a, a_ty) = lower_expr(lhs, ctx)?;
    let (b, b_ty) = lower_expr(rhs, ctx)?;
    let (expr, ty) = lower_binary_op(op, a, a_ty, b, b_ty)?;
    Ok((TirExpr::Located { span: (node.start, node.length), inner: Box::new(expr) }, ty))
}

fn lower_binary_op(
    op: TokenType,
    a: TirExpr,
    a_ty: CoreTy,
    b: TirExpr,
    b_ty: CoreTy,
) -> Result<(TirExpr, CoreTy), String> {
    let numeric = |t: CoreTy| matches!(t, CoreTy::Int | CoreTy::Float);
    // A dynamic operand means the runtime tag decides the operation. Emit the
    // same TIR node and let natcode dispatch through `unrt_dyn_bin`.
    if a_ty == CoreTy::Obj || b_ty == CoreTy::Obj {
        let bin = match op {
            TokenType::Plus => Some(TirBinOp::Add),
            TokenType::Minus => Some(TirBinOp::Sub),
            TokenType::Asterisk => Some(TirBinOp::Mul),
            TokenType::Slash => Some(TirBinOp::Div),
            TokenType::DoubleSlash => Some(TirBinOp::IDiv),
            TokenType::Percent => Some(TirBinOp::Mod),
            TokenType::BAnd => Some(TirBinOp::BAnd),
            TokenType::BOr => Some(TirBinOp::BOr),
            TokenType::BXor => Some(TirBinOp::BXor),
            TokenType::LeftShift => Some(TirBinOp::Shl),
            TokenType::RightShift => Some(TirBinOp::Shr),
            _ => None,
        };
        if let Some(bin) = bin {
            return Ok((TirExpr::Bin(bin, Box::new(a), Box::new(b)), CoreTy::Obj));
        }
        let cmp = match op {
            TokenType::Equal => Some(TirCmp::Eq),
            TokenType::Unequal => Some(TirCmp::Ne),
            TokenType::LessThan => Some(TirCmp::Lt),
            TokenType::GreaterThan => Some(TirCmp::Gt),
            TokenType::LessOrEqual => Some(TirCmp::Le),
            TokenType::GreaterOrEqual => Some(TirCmp::Ge),
            _ => None,
        };
        if let Some(cmp) = cmp {
            return Ok((TirExpr::Cmp(cmp, Box::new(a), Box::new(b)), CoreTy::Bool));
        }
    }
    match op {
        TokenType::Plus => {
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
        CoreTy::ListClass(id) => CoreTy::Class(id),
        CoreTy::ListGen(elem) => CoreTy::from(tir_of_elem(elem)),
        _ => return Err("list method requires a list".to_string()),
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
        "remove" => {
            if args.len() != 1 {
                return Err("'remove' expects exactly one argument".to_string());
            }
            Ok(Some((TirExpr::ListRemove(Box::new(receiver), Box::new(args.into_iter().next().unwrap())), CoreTy::Bool)))
        }
        _ => Ok(None),
    }
}

fn dict_kinds(ty: CoreTy) -> Option<(ObjElem, ObjElem)> {
    match ty {
        CoreTy::DictStrInt => Some((ObjElem::Str, ObjElem::Int)),
        CoreTy::DictStrFloat => Some((ObjElem::Str, ObjElem::Float)),
        CoreTy::DictIntInt => Some((ObjElem::Int, ObjElem::Int)),
        CoreTy::DictGen(k, v) => Some((k, v)),
        _ => None,
    }
}

fn lower_dict_method_expr(
    member: &str,
    receiver: TirExpr,
    receiver_ty: CoreTy,
    args_tuple: &Node,
    ctx: &mut LowerCtx,
) -> Result<Option<(TirExpr, CoreTy)>, String> {
    if !matches!(member, "keys" | "values") {
        return Ok(None);
    }
    let Some((k, v)) = dict_kinds(receiver_ty) else {
        return Err(format!("method '{member}' requires a dict receiver in the core"));
    };
    if args_tuple.kind != NodeKind::Tuple || !args_tuple.children.is_empty() {
        return Err(format!("'{member}' takes no arguments"));
    }
    let elem = if member == "keys" { k } else { v };
    let expr = if member == "keys" {
        TirExpr::DictKeys(Box::new(receiver))
    } else {
        TirExpr::DictValues(Box::new(receiver))
    };
    let _ = ctx;
    Ok(Some((expr, CoreTy::ListGen(elem))))
}

fn lower_dict_method_stmt(node: &Node, ctx: &mut LowerCtx) -> Result<Option<TirStmt>, String> {
    let callee = match node.children.first() {
        Some(c) => c,
        None => return Ok(None),
    };
    if callee.kind != NodeKind::Property {
        return Ok(None);
    }
    let member_node = match callee.children.get(1) {
        Some(m) => m,
        None => return Ok(None),
    };
    let member = ctx.source.span(member_node.start, member_node.length).to_string();
    if member != "clear" {
        return Ok(None);
    }
    let head = match callee.children.first() {
        Some(h) => h,
        None => return Ok(None),
    };
    let (receiver, rty) = lower_expr(head, ctx)?;
    if dict_kinds(rty).is_none() {
        return Err("method 'clear' requires a dict receiver in the core".to_string());
    }
    let args = match node.children.get(1) {
        Some(args_node) => lower_arg_list(args_node, ctx)?,
        None => Vec::new(),
    };
    if !args.is_empty() {
        return Err("'clear' takes no arguments".to_string());
    }
    Ok(Some(TirStmt::DictClear { receiver }))
}

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
        "add" | "push" | "insert" | "reverse" | "sort" | "clear" | "resize" => {}
        _ => return Ok(None),
    }
    let head = callee.children.first().ok_or_else(|| "call without a receiver".to_string())?;
    let (receiver, rty) = lower_expr(head, ctx)?;
    if !matches!(rty, CoreTy::ListInt | CoreTy::ListFloat | CoreTy::ListClass(_) | CoreTy::ListGen(_)) {
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
        "clear" => {
            if !args.is_empty() {
                return Err("'clear' takes no arguments".to_string());
            }
            Ok(Some(TirStmt::ListClear { receiver }))
        }
        "resize" => {
            if !args.is_empty() {
                return Err("'resize' takes no arguments".to_string());
            }
            // C# `Resize` was an internal capacity helper; `Vec` grows itself.
            // NRT exposes a no-op `("list","resize")` dispatch returning none.
            Ok(Some(TirStmt::NativeCall {
                module: "list".into(),
                member: "resize".into(),
                args: vec![receiver],
            }))
        }
        _ => Ok(None),
    }
}

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

fn lower_iter_array(
    args_tuple: &Node,
    ctx: &mut LowerCtx,
) -> Result<(TirExpr, CoreTy), String> {
    let args: Vec<&Node> = args_tuple.children.iter().collect();
    if args.len() != 2 {
        return Err(format!("iter.array expects exactly 2 arguments, got {}", args.len()));
    }
    let (value, vty) = lower_expr(args[0], ctx)?;
    let (len_expr, len_cty) = lower_expr(args[1], ctx)?;
    if len_cty != CoreTy::Int {
        return Err("iter.array size must be an int".to_string());
    }
    match vty {
        CoreTy::Int => Ok((
            TirExpr::ListRepeat { value: Box::new(value), len: Box::new(len_expr), ty: TirTy::ListInt },
            CoreTy::ListInt,
        )),
        CoreTy::Float => Ok((
            TirExpr::ListRepeat { value: Box::new(value), len: Box::new(len_expr), ty: TirTy::ListFloat },
            CoreTy::ListFloat,
        )),
        _ => Err("iter.array requires an int or float value".to_string()),
    }
}

fn lower_iter_minmax(
    member: &str,
    args_tuple: &Node,
    ctx: &mut LowerCtx,
) -> Result<(TirExpr, CoreTy), String> {
    let args = args_tuple.children.iter().collect::<Vec<&Node>>();
    if args.len() != 1 {
        return Err(format!("iter.{member} expects exactly one argument (list[int] or list[float])"));
    }
    let (arg, arg_cty) = lower_expr(args[0], ctx)?;
    let (ret_ty, core_ret) = match arg_cty {
        CoreTy::ListInt => (TirTy::Int, CoreTy::Int),
        CoreTy::ListFloat => (TirTy::Float, CoreTy::Float),
        _ => return Err(format!("iter.{member} requires a list[int] or list[float] argument")),
    };
    Ok((
        TirExpr::NativeCallExpr {
            module: "iter".into(),
            member: member.into(),
            args: vec![arg],
            ret: ret_ty,
        },
        core_ret,
    ))
}

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
        _ => {
            if let Some(ret_ty) = native_module_ret_type("math", name) {
                let core = core_ty_of(&ret_ty)
                    .ok_or_else(|| format!("math.{name} return type is not core-supported"))?;
                let args = lower_arg_list(args_tuple, ctx)?;
                return Ok((
                    TirExpr::NativeCallExpr {
                        module: "math".into(),
                        member: name.into(),
                        args,
                        ret: match ret_ty {
                            Ty::Int => TirTy::Int,
                            Ty::Float => TirTy::Float,
                            Ty::Bool => TirTy::Bool,
                            Ty::Str => TirTy::Str,
                            _ => unreachable!(),
                        },
                    },
                    core,
                ));
            }
            return Err(format!("math.{name} is not supported in the core"));
        }
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
        if head.kind == NodeKind::Identifier {
            let module = ctx.source.span(head.start, head.length).to_string();
            if module == "math" {
                return lower_math_call(&member, args_tuple, ctx);
            }
            if module == "iter" && member == "array" {
                return lower_iter_array(&args_tuple, ctx);
            }
            if module == "iter" && (member == "min" || member == "max") {
                return lower_iter_minmax(&member, args_tuple, ctx);
            }
            if let Some(ret_ty) = native_module_ret_type(&module, &member) {
                // Use the class-aware conversion so `http.connect()` /
                // `ws.connect()` can return opaque handle classes that later
                // accept method calls, and `bytes.*` returns TirTy::Bytes.
                if let Some(tir_ret) = tir_ty_from_ty(ctx.classes, &ret_ty) {
                    if matches!(
                        tir_ret,
                        TirTy::Int | TirTy::Float | TirTy::Bool | TirTy::Str
                            | TirTy::Bytes | TirTy::Class(_)
                    ) {
                        let args = lower_arg_list(args_tuple, ctx)?;
                        return Ok((
                            TirExpr::Located {
                                span: (node.start, node.length),
                                inner: Box::new(TirExpr::NativeCallExpr {
                                    module: module.clone().into_boxed_str(),
                                    member: member.clone().into_boxed_str(),
                                    args,
                                    ret: tir_ret,
                                }),
                            },
                            CoreTy::from(tir_ret),
                        ));
                    }
                }
            }
        }
        let (recv, rty) = lower_expr(head, ctx)?;
        if matches!(rty, CoreTy::ListInt | CoreTy::ListFloat | CoreTy::ListClass(_) | CoreTy::ListGen(_)) {
            if let Some(list_method) = lower_list_method_expr(&member, recv, rty, args_tuple, ctx)? {
                return Ok(list_method);
            }
            return Err(format!(
                "method '{member}' on a list is not supported in the core"
            ));
        }
        if dict_kinds(rty).is_some() {
            if let Some(dict_method) = lower_dict_method_expr(&member, recv, rty, args_tuple, ctx)? {
                return Ok(dict_method);
            }
            return Err(format!(
                "method '{member}' on a dict is not supported in the core"
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
                // A method returning `none` has no TirTy; its value is always
                // consumed as a discarded expression statement, so use Obj.
                ret: tir_ty_of(field_ret).unwrap_or(TirTy::Obj),
            },
            field_ret,
        ));
    }
    let args_tuple_for_lambda = node.children.get(1);
    if callee.kind == NodeKind::Index {
        let (cv, _) = lower_expr(callee, ctx)?;
        let args_node = args_tuple_for_lambda
            .filter(|n| n.kind == NodeKind::Tuple)
            .ok_or_else(|| "call arguments must be positional".to_string())?;
        let args = lower_arg_list(args_node, ctx)?;
        return Ok((TirExpr::CallObj {
            callee: Box::new(cv),
            args,
            ret: TirTy::Int,
        }, CoreTy::Int));
    }
    if callee.kind == NodeKind::Lambda {
        let idx = ctx.lambdas.len() as u32;
        let captures = synthesize_lambda(callee, ctx, idx)?;
        let func_index = ctx.lambda_base + idx;
        let args_node = args_tuple_for_lambda
            .filter(|n| n.kind == NodeKind::Tuple)
            .ok_or_else(|| "lambda call arguments must be positional".to_string())?;
        let mut args = lower_arg_list(args_node, ctx)?;
        let mut all: Vec<TirExpr> = captures.iter().map(|s| TirExpr::Local(*s)).collect();
        all.append(&mut args);
        return Ok((TirExpr::Call(func_index, all), CoreTy::Int));
    }
    // A callable produced by another expression (e.g. `make()(x)`) is a
    // dynamic call: the callee is a tagged NObj handle dispatched by NRT.
    if !matches!(callee.kind, NodeKind::Identifier) {
        let (cv, cty) = lower_expr(callee, ctx)?;
        if !matches!(cty, CoreTy::Obj | CoreTy::Lambda(_)) {
            return Err("only callable values can be invoked in the core".to_string());
        }
        let args_node = args_tuple_for_lambda
            .filter(|n| n.kind == NodeKind::Tuple)
            .ok_or_else(|| "call arguments must be positional".to_string())?;
        let args = lower_arg_list(args_node, ctx)?;
        return Ok((TirExpr::CallObj {
            callee: Box::new(cv),
            args,
            ret: TirTy::Obj,
        }, CoreTy::Obj));
    }
    let name = ctx.source.span(callee.start, callee.length);
    if let Some(slot) = slot_of(ctx, &name) {
        let local_ty = ctx.locals.get(slot as usize).copied();
        let lambda_info = ctx.lambda_vars.get(&slot).cloned();
        if lambda_info.is_none()
            && matches!(local_ty, Some(TirTy::Obj) | Some(TirTy::Lambda(_)))
        {
            // Calling a value whose static type is `any` *or* a function type:
            // both dispatch through the NRT `unrt_call_obj` bridge, which checks
            // the runtime tag and rejects non-callable values. The earlier `Obj`
            // fallback is kept for the Vm bail path (`any` locals and unresolved
            // callees) and for callees without a declared `Ty::Func`; a declared
            // `func(..) -> R` maps its return to R so Obj-mode codegen unboxes R
            // directly instead of returning an opaque handle.
            let args_node = args_tuple_for_lambda
                .filter(|n| n.kind == NodeKind::Tuple)
                .ok_or_else(|| "call arguments must be positional".to_string())?;
            let args = lower_arg_list(args_node, ctx)?;
            let (ret, core_ret) = ctx.checked.functions
                .iter()
                .find(|f| f.sig.name == ctx.current_instance)
                .and_then(|f| f.local_types.get(&name))
                .and_then(|ty| match ty {
                    Ty::Func { ret: fr, .. } => tir_ty_from_ty(ctx.classes, fr),
                    _ => None,
                })
                .map(|ret| (ret, match ret {
                    TirTy::Int => CoreTy::Int,
                    TirTy::Float => CoreTy::Float,
                    TirTy::Bool => CoreTy::Bool,
                    _ => CoreTy::Obj,
                }))
                .unwrap_or((TirTy::Obj, CoreTy::Obj));
            return Ok((TirExpr::CallObj {
                callee: Box::new(TirExpr::Local(slot)),
                args,
                ret,
            }, core_ret));
        }
        if let Some((_func_index, _captures)) = lambda_info {
            let args_node = args_tuple_for_lambda
                .filter(|n| n.kind == NodeKind::Tuple)
                .ok_or_else(|| "lambda call arguments must be positional".to_string())?;
            let args = lower_arg_list(args_node, ctx)?;
            let ret = ctx.locals.get(slot as usize).copied().unwrap_or(TirTy::Int);
            let call_ret = match ret {
                TirTy::Lambda(idx) => {
                    let local_idx = idx.saturating_sub(ctx.lambda_base);
                    ctx.lambdas.get(local_idx as usize).and_then(|f| f.ret).unwrap_or(TirTy::Int)
                }
                _ => TirTy::Int,
            };
            let core_ret = match call_ret { TirTy::Int => CoreTy::Int, TirTy::Float => CoreTy::Float, TirTy::Bool => CoreTy::Bool, _ => CoreTy::Int };
            return Ok((TirExpr::CallObj {
                callee: Box::new(TirExpr::Local(slot)),
                args,
                ret: call_ret,
            }, core_ret));
        }
    }
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
            CoreTy::Str | CoreTy::ListInt | CoreTy::ListFloat | CoreTy::ListClass(_)
            | CoreTy::DictStrInt | CoreTy::DictStrFloat | CoreTy::DictIntInt
            | CoreTy::ListGen(_) | CoreTy::DictGen(_, _) | CoreTy::SetGen(_)
            | CoreTy::TupleGen(_) => {}
            _ => return Err("len() requires a string or collection in the core".to_string()),
        }
        return Ok((TirExpr::Len(Box::new(arg)), CoreTy::Int));
    }
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
    // A call to a generic function is rewritten to its concrete instance.
    let mangled = ctx.call_names.get(&node.start).cloned().map(|m| {
        if let Some(callee) = m.strip_prefix("@dep:") {
            // Resolve a call inside a generic body to the instance matching the
            // enclosing instance's concrete type arguments.
            let args = ctx.instance_args.get(&ctx.current_instance).map(|(_, a)| a.clone()).unwrap_or_default();
            ctx.instance_args
                .iter()
                .find(|(_, (orig, a))| orig == callee && *a == args)
                .map(|(name, _)| name.clone())
                .unwrap_or_else(|| callee.to_string())
        } else {
            m
        }
    });
    let effective = mangled.as_deref().unwrap_or(&name);
    if !ctx.checked.sigs.contains_key(effective) {
        return Err(format!("call to unknown function '{effective}'"));
    }
    let args_node = node
        .children
        .get(1)
        .ok_or_else(|| "call without arguments".to_string())?;
    if args_node.kind != NodeKind::Tuple {
        return Err("call arguments must be positional in the core".to_string());
    }
    let args = lower_arg_list(args_node, ctx)?;
    let fn_index = if let Some(ref m) = mangled {
        ctx.checked.functions.iter().position(|f| f.sig.name == *m)
    } else {
        fn_index_of(ctx.checked, &name, node.start)
    };
    let Some(fn_index) = fn_index else {
        return Err(format!("call to non-core function '{effective}'"));
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
            Ty::Class(name) => {
                let id = ctx.classes.id(name).ok_or_else(|| format!("unknown class '{name}'"))?;
                CoreTy::ListClass(id)
            }
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
            (Ty::Int, Ty::Int) => CoreTy::DictIntInt,
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
        Ty::Any | Ty::Union(_) | Ty::Unknown => CoreTy::Obj,
        Ty::Bytes => CoreTy::Obj,
        Ty::Tuple(items) => {
            let elem = items.iter().fold(None, |acc, t| match (acc, tir_ty_from_ty(ctx.classes, t)) {
                (None, Some(tty)) => Some(tty),
                (Some(prev), Some(tty)) if elem_of_tir(prev) == elem_of_tir(tty) => Some(prev),
                _ => Some(TirTy::Obj),
            });
            CoreTy::TupleGen(elem.map_or(ObjElem::Obj, elem_of_tir))
        }
        Ty::Set(elem) => match tir_ty_from_ty(ctx.classes, elem) {
            Some(tty) => CoreTy::SetGen(elem_of_tir(tty)),
            None => CoreTy::SetGen(ObjElem::Obj),
        },
        other => {
            return Err(format!(
                "'{name}' returns '{}' which is not core-supported",
                other.display()
            ))
        }
    };
    // Missing trailing arguments are filled with the callee's lowered defaults
    // in `pad_default_args` after all functions have been lowered.
    let required = fcheck.sig.required.saturating_sub(fcheck.sig.receiver);
    if args.len() < required || args.len() > fcheck.sig.params.len() {
        return Err(format!(
            "'{name}' expects {required}..{} argument(s), got {}",
            fcheck.sig.params.len(),
            args.len()
        ));
    }
    Ok((
        TirExpr::Located {
            span: (node.start, node.length),
            inner: Box::new(TirExpr::Call(tir_i as u32, args)),
        },
        ret,
    ))
}

fn fn_index_of(checked: &CheckedProgram, name: &str, call_span: usize) -> Option<usize> {
    let mut best_containing: Option<(usize, usize)> = None;    let mut non_containing: Option<usize> = None;
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
                return None;            }
            non_containing = Some(index);
        }
    }
    best_containing.map(|(_, i)| i).or(non_containing)
}


fn locals_ty(locals: &[TirTy], slot: u16) -> Option<TirTy> {
    locals.get(slot as usize).copied()
}

fn validate(f: &TirFn, program: &TirProgram) -> Result<(), String> {
    if f.params.len() > f.locals.len() || f.params.len() > MAX_CORE_ARITY {
        return Err("parameter count exceeds local slot budget".to_string());
    }
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
        TirStmt::GlobalSet(_, _) => {}
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
        TirStmt::ListSet { .. } | TirStmt::DictSet { .. } | TirStmt::ListAdd { .. } | TirStmt::ListInsert { .. } | TirStmt::ListReverse { .. } | TirStmt::ListSort { .. } | TirStmt::MemberSet { .. } => {
        }
        TirStmt::Set(_, _) | TirStmt::Break | TirStmt::Skip | TirStmt::NativeCall { .. } | TirStmt::ExprStmt(_) | TirStmt::Return(_) => {
        }
        TirStmt::Try { body, var, catch_body, finally_body } => {
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
        TirStmt::ListClear { .. } | TirStmt::DictClear { .. } | TirStmt::SetSlice { .. } => {}
        TirStmt::Match { arms, .. } => {
            // Bind slots assigned by a successful pattern are declared here
            // (in arm order) rather than by a `Let` in the arm body. The type
            // is a placeholder; `validate_stmt` restores it from `TirFn`.
            for arm in arms {
                if let TirPattern::Bind(slot) = arm.pattern {
                    if (slot as usize) != locals.len() {
                        return Err(format!(
                            "local slot {} does not follow the declaration order (expected {})",
                            slot,
                            locals.len()
                        ));
                    }
                    locals.push(TirTy::Int);
                }
                for s in &arm.body {
                    collect_decls(s, locals)?;
                }
            }
        }
    }
    Ok(())
}

fn ty_to_tir_simple(ty: &Ty) -> Result<TirTy, String> {
    // `bytes` is now implemented in NRT (nrt/src/bytes_native.rs). A `none`
    // return has no dedicated TirTy and is represented as an opaque handle.
    Ok(match ty {
        Ty::Int => TirTy::Int,
        Ty::Float => TirTy::Float,
        Ty::Bool => TirTy::Bool,
        Ty::Str => TirTy::Str,
        Ty::Bytes => TirTy::Bytes,
        Ty::None_ => TirTy::Obj,
        Ty::List(elem) => match elem.as_ref() {
            Ty::Int => TirTy::ListInt,
            Ty::Float => TirTy::ListFloat,
            Ty::Class(_) => TirTy::ListClass(u16::MAX),
            Ty::Func { .. } => TirTy::ListLambda,
            other => TirTy::ListGen(elem_of_tir_field(other)),
        },
        Ty::Dict(key, value) => match (key.as_ref(), value.as_ref()) {
            (Ty::Str, Ty::Int) => TirTy::DictStrInt,
            (Ty::Str, Ty::Float) => TirTy::DictStrFloat,
            (Ty::Int, Ty::Int) => TirTy::DictIntInt,
            (Ty::Str, other) => TirTy::DictGen(ObjElem::Str, elem_of_tir_field(other)),
            (Ty::Int, other) => TirTy::DictGen(ObjElem::Int, elem_of_tir_field(other)),
            (other, _) => return Err(format!("dict[{}, ...] field is not supported", other.display())),
        },
        Ty::Set(elem) => TirTy::SetGen(elem_of_tir_field(elem)),
        Ty::Tuple(items) => {
            let elem = items.iter().fold(None, |acc, t| match acc {
                None => Some(elem_of_tir_field(t)),
                Some(prev) if prev == elem_of_tir_field(t) => Some(prev),
                _ => Some(ObjElem::Obj),
            });
            TirTy::TupleGen(elem.unwrap_or(ObjElem::Obj))
        }
        Ty::Class(_) => TirTy::Class(u16::MAX),
        // Dynamic fields are boxed NObj handles, like dynamic params/returns.
        Ty::Any | Ty::Union(_) | Ty::Unknown => TirTy::Obj,
        other => return Err(format!("field of type '{}' is not supported in the core", other.display())),
    })
}

fn expr_ty(expr: &TirExpr, f: &TirFn, program: &TirProgram, locals: &[TirTy]) -> Result<TirTy, String> {
    match expr {
        TirExpr::Located { inner, .. } => expr_ty(inner, f, program, locals),
        TirExpr::MakeClosure { func_index, .. } => Ok(TirTy::Lambda(*func_index)),
        TirExpr::CallObj { ret, .. } => Ok(*ret),
        TirExpr::GlobalGet(_) => Ok(TirTy::Int),
        TirExpr::NoneValue => Ok(TirTy::Str),
        TirExpr::Int(_) => Ok(TirTy::Int),
        TirExpr::Float(_) => Ok(TirTy::Float),
        TirExpr::EmptyCollection { ty } => Ok(*ty),
        TirExpr::Slice { list, start, end, step } => {
            for bound in [start, end, step].into_iter().flatten() {
                if expr_ty(bound, f, program, locals)? != TirTy::Int {
                    return Err("slice bounds must be ints".to_string());
                }
            }
            match expr_ty(list, f, program, locals)? {
                ty @ (TirTy::ListInt | TirTy::ListFloat | TirTy::ListClass(_)
                | TirTy::ListLambda | TirTy::ListGen(_) | TirTy::Obj) => Ok(ty),
                _ => Err("slicing requires a list".to_string()),
            }
        }
        TirExpr::Math(_, args) => {
            for arg in args {
                let _ = expr_ty(arg, f, program, locals)?;
            }
            Ok(TirTy::Float)
        }
        TirExpr::NativeCallExpr { args, ret, .. } => {
            for arg in args {
                let _ = expr_ty(arg, f, program, locals)?;
            }
            Ok(*ret)
        }
        TirExpr::ListRepeat { value, len, ty } => {
            match (expr_ty(value, f, program, locals)?, expr_ty(len, f, program, locals)?) {
                (TirTy::Int, TirTy::Int) => {
                    if *ty != TirTy::ListInt {
                        return Err("iter.array int result type mismatch".to_string());
                    }
                    Ok(TirTy::ListInt)
                }
                (TirTy::Float, TirTy::Int) => {
                    if *ty != TirTy::ListFloat {
                        return Err("iter.array float result type mismatch".to_string());
                    }
                    Ok(TirTy::ListFloat)
                }
                _ => Err("iter.array value must be an int or float and size must be an int".to_string()),
            }
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
                TirTy::ListClass(id) => Ok(TirTy::Class(id)),
                TirTy::ListGen(e) => Ok(tir_of_elem(e)),
                _ => Err("list.pop requires a list[int], list[float] or list[class]".to_string()),
            }
        }
        TirExpr::ListContains(receiver, _) => {
            match expr_ty(receiver, f, program, locals)? {
                TirTy::ListInt | TirTy::ListFloat | TirTy::ListClass(_) | TirTy::ListGen(_) => Ok(TirTy::Bool),
                _ => Err("list.contains requires a list[int], list[float] or list[class]".to_string()),
            }
        }
        TirExpr::ListIndexOf(receiver, _) | TirExpr::ListRemove(receiver, _) => {
            let is_remove = matches!(expr, TirExpr::ListRemove(_, _));
            match expr_ty(receiver, f, program, locals)? {
                TirTy::ListInt | TirTy::ListFloat | TirTy::ListClass(_) | TirTy::ListGen(_) => {
                    Ok(if is_remove { TirTy::Bool } else { TirTy::Int })
                }
                _ => Err("method requires a list[int], list[float] or list[class]".to_string()),
            }
        }
        TirExpr::DictKeys(receiver) | TirExpr::DictValues(receiver) => {
            let keys = matches!(expr, TirExpr::DictKeys(_));
            match expr_ty(receiver, f, program, locals)? {
                TirTy::DictStrInt | TirTy::DictStrFloat => Ok(TirTy::ListGen(ObjElem::Str)),
                TirTy::DictIntInt => Ok(TirTy::ListGen(ObjElem::Int)),
                TirTy::DictGen(k, v) => {
                    Ok(TirTy::ListGen(if keys { k } else { v }))
                }
                _ => Err("dict.keys/values requires a dict receiver".to_string()),
            }
        }
        TirExpr::Len(arg) => {
            match expr_ty(arg, f, program, locals)? {
                TirTy::Str | TirTy::ListInt | TirTy::ListFloat | TirTy::ListClass(_)
                | TirTy::DictStrInt | TirTy::DictStrFloat | TirTy::DictIntInt
                | TirTy::ListGen(_) | TirTy::DictGen(_, _) | TirTy::SetGen(_) | TirTy::TupleGen(_) => {
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
                TirTy::ListClass(id) => Ok(TirTy::Class(id)),
                TirTy::ListGen(e) => Ok(tir_of_elem(e)),
                _ => Err("indexing requires a list[int], list[float] or list[class]".to_string()),
            }
        }
        TirExpr::Dict(entries) => {
            if entries.is_empty() {
                return Err("empty dict literal cannot be typed in the core".to_string());
            }
            let mut kty = None;
            let mut vty = None;
            for (key, value) in entries {
                let k = expr_ty(key, f, program, locals)?;
                if k != TirTy::Str && k != TirTy::Int {
                    return Err("dict keys must be strings or ints".to_string());
                }
                if let Some(prev) = kty {
                    if prev != k {
                        return Err("dict keys must all have the same type".to_string());
                    }
                } else {
                    kty = Some(k);
                }
                let t = expr_ty(value, f, program, locals)?;
                if let Some(prev) = vty {
                    if prev != t {
                        vty = Some(TirTy::Obj);
                    }
                } else {
                    vty = Some(t);
                }
            }
            match (kty, vty) {
                (Some(TirTy::Str), Some(TirTy::Int)) => Ok(TirTy::DictStrInt),
                (Some(TirTy::Str), Some(TirTy::Float)) => Ok(TirTy::DictStrFloat),
                (Some(TirTy::Int), Some(TirTy::Int)) => Ok(TirTy::DictIntInt),
                (Some(k @ (TirTy::Str | TirTy::Int)), Some(v)) => {
                    Ok(TirTy::DictGen(elem_of_tir(k), elem_of_tir(v)))
                }
                _ => Err("dict keys must be strings or ints in the core".to_string()),
            }
        }
        TirExpr::DictGet(target, key) => {
            let key_ty = expr_ty(key, f, program, locals)?;
            match expr_ty(target, f, program, locals)? {
                TirTy::DictStrInt if key_ty == TirTy::Str => Ok(TirTy::Int),
                TirTy::DictStrFloat if key_ty == TirTy::Str => Ok(TirTy::Float),
                TirTy::DictIntInt if key_ty == TirTy::Int => Ok(TirTy::Int),
                TirTy::DictGen(k, v) if elem_of_tir(key_ty) == k => Ok(tir_of_elem(v)),
                _ => Err(
                    "indexing requires a dict[str, int], dict[str, float] or dict[int, int]".to_string(),
                ),
            }
        }
        TirExpr::ClassNew { name, args, ret } => {
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
                    (Ok(TirTy::Float), TirTy::Int) => {}                    _ => {
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
            let mut ty = expr_ty(&elements[0], f, program, locals)?;
            for element in &elements[1..] {
                let ety = expr_ty(element, f, program, locals)?;
                if ety != ty {
                    ty = TirTy::Obj;
                }
            }
            if ty == TirTy::Int {
                Ok(TirTy::ListInt)
            } else if ty == TirTy::Float {
                Ok(TirTy::ListFloat)
            } else if let TirTy::Class(id) = ty {
                Ok(TirTy::ListClass(id))
            } else if let TirTy::Lambda(_) = ty {
                Ok(TirTy::ListLambda)
            } else {
                Ok(TirTy::ListGen(elem_of_tir(ty)))
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
            if t == TirTy::Obj || e == TirTy::Obj {
                return Ok(TirTy::Obj);
            }
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
            // Any operand with a dynamic (tagged handle) type goes through the
            // NRT dispatch bridge; the result stays boxed so further dynamic
            // operations can inspect its tag.
            if a == TirTy::Obj || b == TirTy::Obj {
                return Ok(TirTy::Obj);
            }
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
            if a == TirTy::Obj || b == TirTy::Obj {
                return Ok(TirTy::Bool);
            }
            let numeric_pair = matches!((a, b), (TirTy::Int, TirTy::Int) | (TirTy::Int, TirTy::Float) | (TirTy::Float, TirTy::Int) | (TirTy::Float, TirTy::Float));
            if numeric_pair
                || (same_ty(a, b) && !matches!(a, TirTy::ListInt | TirTy::ListFloat | TirTy::ListClass(_) | TirTy::DictStrInt | TirTy::DictStrFloat | TirTy::DictIntInt | TirTy::Class(_) | TirTy::Obj | TirTy::ListGen(_) | TirTy::DictGen(_, _) | TirTy::SetGen(_) | TirTy::TupleGen(_)))
            {
                return Ok(TirTy::Bool);
            }
            Err("comparison operands have incompatible types".to_string())
        }
        TirExpr::Un(op, inner) => {
            let t = expr_ty(inner, f, program, locals)?;
            match op {
                TirUnOp::Neg => match t {
                    TirTy::Obj => Ok(TirTy::Obj),
                    TirTy::Int => Ok(TirTy::Int),
                    TirTy::Float => Ok(TirTy::Float),
                    TirTy::Bool => Err("negation of bool".to_string()),
                    TirTy::Str => Err("negation of str".to_string()),
                    TirTy::Bytes => Err("negation of bytes".to_string()),
                    TirTy::ListInt | TirTy::ListFloat | TirTy::ListClass(_) | TirTy::DictStrInt | TirTy::DictStrFloat
                    | TirTy::DictIntInt | TirTy::Class(_) | TirTy::ErrorObj | TirTy::Lambda(_) | TirTy::ListLambda
                    | TirTy::ListGen(_) | TirTy::DictGen(_, _) | TirTy::SetGen(_) | TirTy::TupleGen(_) => {
                        Err("negation of collection or class instance".to_string())
                    }
                },
                TirUnOp::Not => match t {
                    TirTy::Bool => Ok(TirTy::Bool),
                    TirTy::Int => Ok(TirTy::Int),
                    TirTy::Float => Err("'not' requires a bool or int".to_string()),
                    TirTy::Str => Err("'not' requires a bool or int".to_string()),
                    TirTy::Bytes => Err("'not' requires a bool or int".to_string()),
                    TirTy::ListInt | TirTy::ListFloat | TirTy::ListClass(_) | TirTy::DictStrInt | TirTy::DictStrFloat
                    | TirTy::DictIntInt | TirTy::Class(_) | TirTy::ErrorObj | TirTy::Lambda(_) | TirTy::ListLambda
                    | TirTy::Obj | TirTy::ListGen(_) | TirTy::DictGen(_, _) | TirTy::SetGen(_) | TirTy::TupleGen(_) => {
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
                // Scalars box into a dynamic handle at the call boundary;
                // NRT performs the boxing via `unrt_obj_int/float/bool`.
                let boxes = *param == TirTy::Obj
                    && matches!(arg_ty, TirTy::Int | TirTy::Float | TirTy::Bool | TirTy::Obj);
                if !same_ty(arg_ty, *param) && !boxes {
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
                None => Ok(TirTy::Bool),            }
        }
    }
}

fn param_name(ty: TirTy) -> &'static str {
    match ty {
        TirTy::Int => "int",
        TirTy::Float => "float",
        TirTy::Bool => "bool",
        TirTy::Str => "str",
        TirTy::Bytes => "bytes",
        TirTy::ListInt => "list[int]",
        TirTy::ListFloat => "list[float]",
        TirTy::ListClass(_) => "list[class]",
        TirTy::DictStrInt => "dict[str, int]",
        TirTy::DictStrFloat => "dict[str, float]",
        TirTy::DictIntInt => "dict[int, int]",
        TirTy::Class(_) => "class",
        TirTy::ErrorObj => "error-obj",
        TirTy::Lambda(_) => "lambda",
        TirTy::ListLambda => "list[lambda]",
        TirTy::Obj => "obj",
        TirTy::ListGen(_) => "list",
        TirTy::DictGen(_, _) => "dict",
        TirTy::SetGen(_) => "set",
        TirTy::TupleGen(_) => "tuple",
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
            TirStmt::GlobalSet(_, expr) => {
                let _ = expr_ty(expr, f, program, locals)?;
            }
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
                let boxes = expected == TirTy::Obj
                    && matches!(ty, TirTy::Int | TirTy::Float | TirTy::Bool | TirTy::Obj);
                if ty != expected && !boxes {
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
                    TirTy::ListClass(id) => TirTy::Class(id),
                    TirTy::ListGen(elem) => tir_of_elem(elem),
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
                    TirTy::ListClass(id) => TirTy::Class(id),
                    TirTy::ListGen(elem) => tir_of_elem(elem),
                    _ => return Err("index assignment requires a list".to_string()),
                };
                if expr_ty(value, f, program, locals)? != elem_ty {
                    return Err("list elements must all have the same type".to_string());
                }
            }
            TirStmt::DictSet { dict, key, value } => {
                let dict_ty = expr_ty(dict, f, program, locals)?;
                let key_ty = expr_ty(key, f, program, locals)?;
                let elem_ty = match (dict_ty, key_ty) {
                    (TirTy::DictStrInt, TirTy::Str) => TirTy::Int,
                    (TirTy::DictStrFloat, TirTy::Str) => TirTy::Float,
                    (TirTy::DictIntInt, TirTy::Int) => TirTy::Int,
                    (TirTy::DictGen(k, v), key) if elem_of_tir(key) == k => tir_of_elem(v),
                    _ => return Err(
                        "index assignment requires a dict[str, int], dict[str, float] or dict[int, int]"
                            .to_string(),
                    ),
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
                    TirTy::ListClass(id) => TirTy::Class(id),
                    _ => return Err("list.add requires a list[int], list[float] or list[class]".to_string()),
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
                    TirTy::ListClass(id) => TirTy::Class(id),
                    _ => return Err("list.insert requires a list[int], list[float] or list[class]".to_string()),
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
                    TirTy::ListInt | TirTy::ListFloat | TirTy::ListClass(_) => {}
                    _ => return Err("list.reverse/list.sort requires a list[int], list[float] or list[class]".to_string()),
                }
            }
            TirStmt::ListClear { receiver } => {
                match expr_ty(receiver, f, program, locals)? {
                    TirTy::ListInt | TirTy::ListFloat | TirTy::ListClass(_) | TirTy::ListGen(_) => {}
                    _ => return Err("list.clear requires a list receiver".to_string()),
                }
            }
            TirStmt::SetSlice { list, value, start, end, step } => {
                let list_ty = expr_ty(list, f, program, locals)?;
                if !matches!(
                    list_ty,
                    TirTy::ListInt | TirTy::ListFloat | TirTy::ListClass(_)
                        | TirTy::ListLambda | TirTy::ListGen(_) | TirTy::Obj
                ) {
                    return Err("slice assignment requires a list".to_string());
                }
                for bound in [start, end, step].into_iter().flatten() {
                    if expr_ty(bound, f, program, locals)? != TirTy::Int {
                        return Err("slice bounds must be ints".to_string());
                    }
                }
                let value_ty = expr_ty(value, f, program, locals)?;
                if !matches!(
                    value_ty,
                    TirTy::ListInt | TirTy::ListFloat | TirTy::ListClass(_)
                        | TirTy::ListLambda | TirTy::ListGen(_) | TirTy::Str | TirTy::Obj
                ) {
                    return Err("slice assignment value must be a list or string".to_string());
                }
            }
            TirStmt::DictClear { receiver } => {
                match expr_ty(receiver, f, program, locals)? {
                    TirTy::DictStrInt | TirTy::DictStrFloat | TirTy::DictIntInt | TirTy::DictGen(_, _) => {}
                    _ => return Err("dict.clear requires a dict receiver".to_string()),
                }
            }
            TirStmt::Try { body, var, catch_body, finally_body } => {
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
                        // Scalar <-> dynamic conversions happen at the return
                        // boundary in natcode (`unrt_obj_*` / unbox helpers).
                        let boxes_or_unboxes = (ty == TirTy::Obj
                            && matches!(ret, TirTy::Int | TirTy::Float | TirTy::Bool | TirTy::Obj))
                            || (ret == TirTy::Obj
                                && matches!(ty, TirTy::Int | TirTy::Float | TirTy::Bool));
                        if !same_ty(ty, ret) && !boxes_or_unboxes {
                            return Err(format!(
                                "return of '{}' where '{}' was declared",
                                param_name(ty),
                                param_name(ret)
                            ));
                        }
                    }
                }
            }
            TirStmt::Match { arms, .. } => {
                for arm in arms {
                    if let TirPattern::Bind(bind_slot) = arm.pattern {
                        let ty = f.locals.get(bind_slot as usize).copied()
                            .ok_or_else(|| format!("local slot {} out of range", bind_slot))?;
                        locals[bind_slot as usize] = ty;
                        *slot += 1;
                    }
                    if let TirPattern::Literal(e) = &arm.pattern {
                        let _ = expr_ty(e, f, program, locals)?;
                    }
                    if let Some(guard) = &arm.guard {
                        if expr_ty(guard, f, program, locals)? != TirTy::Bool {
                            return Err("match guard must be a bool".to_string());
                        }
                    }
                    validate_stmt(&arm.body, f, program, locals, slot, loop_depth)?;
                }
            }
            TirStmt::Break | TirStmt::Skip => {
                if loop_depth == 0 {
                    return Err("break/skip outside a loop".to_string());
                }
            }
            TirStmt::ExprStmt(expr) => {
                let inner = match expr {
                    TirExpr::Located { inner, .. } => inner.as_ref(),
                    other => other,
                };
                if !matches!(inner, TirExpr::Call(..) | TirExpr::MethodCall { .. } | TirExpr::CallObj { .. }) {
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
        // The implicit no-match arm always throws, so a match terminates when
        // every explicit arm terminates.
        // A guarded arm may fall through, but the implicit no-match arm always
        // throws, so fallthrough still cannot reach the end of the function.
        TirStmt::Match { arms, .. } => arms.iter().all(|arm| body_terminates(&arm.body)),
        _ => false,
    }
}

impl TirProgram {
    pub fn validate(&self) -> Result<(), TirError> {
        for f in &self.functions {
            validate(f, self).map_err(|e| TirError(format!("{}: {}", f.name, e)))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod abi_tests {
    use super::*;

    /// Exhaustive on purpose: adding a `TirTy` variant without updating this
    /// table (and `abi_kind`) is a compile error, which is the synchronization
    /// check we want.
    fn expected_abi_kind(ty: TirTy) -> u8 {
        match ty {
            TirTy::Int => 0,
            TirTy::Float => 1,
            TirTy::Bool => 2,
            TirTy::Bytes | TirTy::Str | TirTy::ListInt | TirTy::ListFloat
            | TirTy::ListClass(_) | TirTy::DictStrInt | TirTy::DictStrFloat
            | TirTy::DictIntInt | TirTy::Class(_) | TirTy::ErrorObj
            | TirTy::Lambda(_) | TirTy::ListLambda | TirTy::Obj
            | TirTy::ListGen(_) | TirTy::DictGen(_, _) | TirTy::SetGen(_) | TirTy::TupleGen(_) => 3,
        }
    }

    fn all_types() -> Vec<TirTy> {
        vec![
            TirTy::Int,
            TirTy::Float,
            TirTy::Bool,
            TirTy::Str,
            TirTy::Bytes,
            TirTy::ListInt,
            TirTy::ListFloat,
            TirTy::ListClass(0),
            TirTy::DictStrInt,
            TirTy::DictStrFloat,
            TirTy::DictIntInt,
            TirTy::Class(0),
            TirTy::ErrorObj,
            TirTy::Lambda(0),
            TirTy::ListLambda,
            TirTy::Obj,
            TirTy::ListGen(ObjElem::Obj),
            TirTy::DictGen(ObjElem::Str, ObjElem::Int),
            TirTy::SetGen(ObjElem::Bool),
        ]
    }

    #[test]
    fn abi_kind_is_stable_for_every_variant() {
        for ty in all_types() {
            let kind = abi_kind(ty);
            assert!(kind <= 3, "NRT kind out of range for {ty:?}: {kind}");
            assert_eq!(kind, expected_abi_kind(ty), "NRT kind changed for {ty:?}");
            assert_eq!(tir_is_ref(ty), kind == 3, "tir_is_ref drifted for {ty:?}");
        }
    }

    #[test]
    fn arg_kind_from_tir_follows_abi_table() {
        use crate::jit_cache::ArgKind;
        for ty in all_types() {
            let expected = match abi_kind(ty) {
                0 => ArgKind::Int,
                1 => ArgKind::Float,
                2 => ArgKind::Bool,
                _ => ArgKind::Int,
            };
            assert!(ArgKind::from_tir(ty) == expected, "ArgKind drifted for {ty:?}");
        }
    }

    fn program_with_native_stmt(module: &str, member: &str) -> TirProgram {
        TirProgram {
            functions: vec![TirFn {
                name: "main".into(),
                proto_span: (0, 0),
                index: 0,
                params: vec![],
                defaults: vec![],
                ret: None,
                locals: vec![],
                body: vec![TirStmt::NativeCall {
                    module: module.into(),
                    member: member.into(),
                    args: vec![],
                }],
            }],
            class_names: vec![],
            class_fields: vec![],
            class_methods: vec![],
            class_setters: vec![],
            class_field_order: vec![],
            has_global_write: false,
        }
    }

    #[test]
    fn statement_native_calls_share_expression_gate() {
        assert!(!nrt_supported(&program_with_native_stmt("random", "int")));
        // `bytes.*`, `fs.read_bytes` and `fs.write_bytes` are implemented in
        // NRT (nrt/src/bytes_native.rs), so the shared gate now admits them.
        assert!(nrt_supported(&program_with_native_stmt("bytes", "len")));
        assert!(nrt_supported(&program_with_native_stmt("fs", "read_bytes")));
        assert!(nrt_supported(&program_with_native_stmt("fs", "write_bytes")));
        assert!(nrt_supported(&program_with_native_stmt("io", "print")));
    }
}

#[cfg(test)]
mod lower_call_tests {
    use super::*;

    /// FNTYPE-01: `Ty::Func` parameters map to the opaque `TirTy::Lambda` slot
    /// (no synthesized lambda, hence no `lambda_vars` entry). The indirect call
    /// must still lower to `CallObj`, exactly like an `any`-typed callee.
    #[test]
    fn func_typed_parameter_call_lowers_to_call_obj() {
        let source = Source::new("fntir.un", "f(1)");
        let checked = CheckedProgram::default();
        let classes = ClassTable::new(&checked);
        let tir_index = HashMap::new();
        let call_names = HashMap::new();
        let instance_args = HashMap::new();
        let mut lambdas = Vec::new();
        let globals = HashMap::new();
        let mut wrote_global = false;

        // `f` is slot 0, statically typed as a function value with no lambda
        // info — precisely the shape a `f: func(int) -> int` parameter produces.
        let mut ctx = LowerCtx {
            source: &source,
            checked: &checked,
            tir_index: &tir_index,
            call_names: &call_names,
            current_instance: String::new(),
            instance_args: &instance_args,
            slots: HashMap::from([("f".to_string(), 0u16)]),
            locals: vec![TirTy::Lambda(u32::MAX)],
            declared_types: HashMap::new(),
            classes: &classes,
            lambda_vars: HashMap::new(),
            lambdas: &mut lambdas,
            lambda_base: 0,
            globals: &globals,
            wrote_global: &mut wrote_global,
            pending: Vec::new(),
        };

        let callee = Node::new(0, 1, NodeKind::Identifier); // `f`
        let arg = Node::new(2, 1, NodeKind::Integer).with_value(Const::Int(1));
        let args = Node::with_children(2, 1, NodeKind::Tuple, None, vec![arg]);
        let call = Node::with_children(0, 3, NodeKind::Call, None, vec![callee, args]);

        let (expr, cty) = lower_call(&call, &mut ctx).expect("typed function call must lower");
        match expr {
            TirExpr::CallObj { callee, args, ret } => {
                assert!(matches!(*callee, TirExpr::Local(0)));
                assert_eq!(args.len(), 1);
                assert!(matches!(args[0], TirExpr::Int(1)));
                assert_eq!(ret, TirTy::Obj, "the `any` codegen path is reused for now");
            }
            other => panic!("expected CallObj, got {other:?}"),
        }
        assert!(cty == CoreTy::Obj);
        // The opaque lambda slot must not be registered as a synthesized lambda.
        assert!(ctx.lambda_vars.is_empty());
    }

    /// The existing `any` callee path is unchanged (regression guard).
    #[test]
    fn any_typed_parameter_call_still_lowers_to_call_obj() {
        let source = Source::new("fntir.un", "f(1)");
        let checked = CheckedProgram::default();
        let classes = ClassTable::new(&checked);
        let tir_index = HashMap::new();
        let call_names = HashMap::new();
        let instance_args = HashMap::new();
        let mut lambdas = Vec::new();
        let globals = HashMap::new();
        let mut wrote_global = false;
        let mut ctx = LowerCtx {
            source: &source,
            checked: &checked,
            tir_index: &tir_index,
            call_names: &call_names,
            current_instance: String::new(),
            instance_args: &instance_args,
            slots: HashMap::from([("f".to_string(), 0u16)]),
            locals: vec![TirTy::Obj],
            declared_types: HashMap::new(),
            classes: &classes,
            lambda_vars: HashMap::new(),
            lambdas: &mut lambdas,
            lambda_base: 0,
            globals: &globals,
            wrote_global: &mut wrote_global,
            pending: Vec::new(),
        };

        let callee = Node::new(0, 1, NodeKind::Identifier);
        let arg = Node::new(2, 1, NodeKind::Integer).with_value(Const::Int(1));
        let args = Node::with_children(2, 1, NodeKind::Tuple, None, vec![arg]);
        let call = Node::with_children(0, 3, NodeKind::Call, None, vec![callee, args]);

        let (expr, cty) = lower_call(&call, &mut ctx).expect("any-typed call must lower");
        assert!(matches!(expr, TirExpr::CallObj { .. }));
        assert!(cty == CoreTy::Obj);
    }

    /// Ties the new function-type syntax to the lowering branch above.
    #[test]
    fn ty_func_maps_to_opaque_lambda_slot() {
        let checked = CheckedProgram::default();
        let classes = ClassTable::new(&checked);
        let func_ty = Ty::Func {
            params: std::rc::Rc::from(vec![Ty::Int]),
            names: std::rc::Rc::from(vec!["x".to_string()]),
            ret: std::rc::Rc::new(Ty::Int),
            required: 1,
        };
        assert_eq!(tir_ty_from_ty(&classes, &func_ty), Some(TirTy::Lambda(u32::MAX)));
    }
}
