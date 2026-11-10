use std::cell::RefCell;
use std::rc::Rc;

use crate::runtime::collections;
use crate::runtime::collections::KeyMap;
use crate::runtime::date;
use crate::runtime::iters::{self, IterCell};
use crate::runtime::prims;
use crate::runtime::types::Types;
use crate::runtime::world;

/// a reference object. the reference model is class-based and mutable
/// (`Obj.Members`, `Self`, `Super`, `Type`), so every object is a shared,
/// interior-mutable cell.
pub type Obj = Rc<RefCell<ObjData>>;

#[derive(Clone)]
pub enum Payload {
    Plain,
    /// `Err` — `ToStr()` renders `"{header}: {message}"`
    Error { header: String, message: String },
    /// uncaught host exception (`System.X`, `Panic`); the VM aborts when it reaches
    /// the top of a statement. the reference throws, Rust has no exception
    /// unwinding, so it travels as a value.
    Panic { name: String, message: String },
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(Rc<str>),
    Date(i64),
    /// every collection payload is a shared cell, so `b = a` keeps `b` and `a`
    /// looking at the same items, exactly like the reference's `Ref<T>` cells.
    List(Rc<RefCell<Vec<Obj>>>),
    Dict(Rc<RefCell<KeyMap>>),
    Set(Rc<RefCell<Vec<Obj>>>),
    Tup(Rc<Vec<Obj>>, Rc<Vec<String>>),
    Spread(Rc<Vec<Obj>>),
    Json(Obj),
    Stru(Rc<Vec<String>>),
    Enu(i64),
    TObj(Types),
    /// `Iters` and its subclasses share one cell: the sequence definition is
    /// immutable and the enumeration position lives inside it, so `Iter()` returns
    /// the same cell and `Clone()` rebuilds fresh cursors over the same definition.
    Iter(IterCell),
}

/// `Payload` is printed as a summary: an `Obj` payload cannot be printed by
/// recursion (a list holds objects that hold lists), so the count is shown instead.
impl std::fmt::Debug for Payload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Payload::Plain => f.write_str("Plain"),
            Payload::Error { header, message } => {
                write!(f, "Error({header}: {message})")
            }
            Payload::Panic { name, message } => {
                write!(f, "Panic({name}: {message})")
            }
            Payload::Int(v) => write!(f, "Int({v})"),
            Payload::Float(v) => write!(f, "Float({v})"),
            Payload::Bool(v) => write!(f, "Bool({v})"),
            Payload::Str(s) => write!(f, "Str({s})"),
            Payload::Date(t) => write!(f, "Date({t})"),
            Payload::List(v) => write!(f, "List({} items)", v.borrow().len()),
            Payload::Dict(km) => write!(f, "Dict({} entries)", km.borrow().len()),
            Payload::Set(s) => write!(f, "Set({} items)", s.borrow().len()),
            Payload::Tup(v, n) => write!(f, "Tup({} values, {} names)", v.len(), n.len()),
            Payload::Spread(v) => write!(f, "Spread({} values)", v.len()),
            Payload::Json(_) => f.write_str("Json(<value>)"),
            Payload::Stru(n) => write!(f, "Stru({} names)", n.len()),
            Payload::Enu(n) => write!(f, "Enu({n})"),
            Payload::TObj(t) => write!(f, "TObj({})", t.display()),
            Payload::Iter(st) => write!(f, "Iter({})", st.borrow().kind.name()),
        }
    }
}

/// payload identity, not Un equality: Un's `==` is the `Eq()` dunder. object payloads
/// compare by cell identity, the same way the reference's `ReferenceEquals` does.
impl PartialEq for Payload {
    fn eq(&self, other: &Self) -> bool {
        use Payload::*;
        match (self, other) {
            (Plain, Plain) => true,
            (Error { header: a, message: x }, Error { header: b, message: y }) => a == b && x == y,
            (Panic { name: a, message: x }, Panic { name: b, message: y }) => a == b && x == y,
            (Int(a), Int(b)) => a == b,
            (Float(a), Float(b)) => a == b,
            (Bool(a), Bool(b)) => a == b,
            (Str(a), Str(b)) => a == b,
            (Date(a), Date(b)) => a == b,
            (List(a), List(b)) => Rc::ptr_eq(a, b),
            (Dict(a), Dict(b)) => Rc::ptr_eq(a, b),
            (Set(a), Set(b)) => Rc::ptr_eq(a, b),
            (Tup(a, na), Tup(b, nb)) => Rc::ptr_eq(a, b) && Rc::ptr_eq(na, nb),
            (Spread(a), Spread(b)) => Rc::ptr_eq(a, b),
            (Json(a), Json(b)) => Rc::ptr_eq(a, b),
            (Stru(a), Stru(b)) => Rc::ptr_eq(a, b),
            (Enu(a), Enu(b)) => a == b,
            (TObj(a), TObj(b)) => a == b,
            (Iter(a), Iter(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

pub struct ObjData {
    pub type_name: Rc<str>,
    pub declared: Rc<str>,
    pub types: Types,
    pub self_: Option<Obj>,
    pub super_: Option<Obj>,
    pub parents: Vec<Obj>,
    pub members: Vec<(String, Obj)>,
    pub annotations: Vec<(String, Obj)>,
    pub payload: Payload,
}

impl ObjData {
    fn new(type_name: Rc<str>, obj_type: Rc<str>, payload: Payload) -> Self {
        Self {
            types: Types::create(&[Types::Single(obj_type), Types::Single(type_name.clone())]),
            declared: type_name.clone(),
            type_name,
            self_: None,
            super_: None,
            parents: Vec::new(),
            members: Vec::new(),
            annotations: Vec::new(),
            payload,
        }
    }
}

pub fn obj_new(type_name: &str, payload: Payload) -> Obj {
    Rc::new(RefCell::new(ObjData::new(
        world::intern_type(type_name),
        world::obj_type(),
        payload,
    )))
}

pub fn plain(type_name: Rc<str>, obj_type: Rc<str>) -> Obj {
    Rc::new(RefCell::new(ObjData::new(type_name, obj_type, Payload::Plain)))
}

pub fn int_obj(value: i64, type_name: Rc<str>, obj_type: Rc<str>) -> Obj {
    Rc::new(RefCell::new(ObjData::new(type_name, obj_type, Payload::Int(value))))
}

pub fn str_obj(value: Rc<str>, type_name: Rc<str>, obj_type: Rc<str>) -> Obj {
    Rc::new(RefCell::new(ObjData::new(type_name, obj_type, Payload::Str(value))))
}

pub fn bool_obj_new(value: bool, type_name: Rc<str>, obj_type: Rc<str>) -> Obj {
    Rc::new(RefCell::new(ObjData::new(type_name, obj_type, Payload::Bool(value))))
}

pub fn none() -> Obj {
    world::none_obj()
}

pub fn null() -> Obj {
    world::null_obj()
}

pub fn int(value: i64) -> Obj {
    world::int_cached(value)
}

pub fn float(value: f64) -> Obj {
    obj_new("float", Payload::Float(value))
}

pub fn boolean(value: bool) -> Obj {
    world::bool_obj(value)
}

pub fn str(value: &str) -> Obj {
    world::str_pooled(value)
}

/// `new Str(value)` — bypasses the pool, so identity is observable
pub fn str_new(value: &str) -> Obj {
    obj_new("str", Payload::Str(Rc::from(value)))
}

pub fn date_obj(ticks: i64) -> Obj {
    obj_new("date", Payload::Date(ticks))
}

pub fn err(message: impl Into<String>) -> Obj {
    obj_new(
        "obj",
        Payload::Error {
            header: "Error".to_owned(),
            message: message.into(),
        },
    )
}

pub fn err_with_header(header: impl Into<String>, message: impl Into<String>) -> Obj {
    obj_new(
        "obj",
        Payload::Error {
            header: header.into(),
            message: message.into(),
        },
    )
}

pub fn panic(name: impl Into<String>, message: impl Into<String>) -> Obj {
    obj_new(
        "obj",
        Payload::Panic {
            name: name.into(),
            message: message.into(),
        },
    )
}

pub fn divide_by_zero() -> Obj {
    panic("System.DivideByZeroException", "Attempted to divide by zero.")
}

pub fn type_name(o: &Obj) -> Rc<str> {
    o.borrow().type_name.clone()
}

pub fn payload(o: &Obj) -> Payload {
    o.borrow().payload.clone()
}

pub fn is_none(o: &Obj) -> bool {
    &*type_name(o) == "none"
}

pub fn is_err(o: &Obj) -> bool {
    matches!(payload(o), Payload::Error { .. })
}

pub fn is_panic(o: &Obj) -> bool {
    matches!(payload(o), Payload::Panic { .. })
}

pub fn is_bool(o: &Obj) -> bool {
    matches!(payload(o), Payload::Bool(_))
}

/// `Obj.Equals` as a collection key test: `Eq()` coerced to a boolean. a non-boolean
/// result (an `Err`) is `false`, exactly like the reference's `As<Bool>` pattern.
pub fn key_eq(a: &Obj, b: &Obj) -> bool {
    matches!(payload(&binop(a, BinOp::Eq, b)), Payload::Bool(true))
}

/// `Obj.GetHashCode()`. the reference overrides it in `Val<T>` / `Ref<T>` (the CLR value
/// hash, so an int key hashes to its own value, a `Date` to its tick count), `Str`,
/// `Enu`, `List` and `Tup`. `Dict`, `Set`, `Stru`, `Spreads` and `Json` keep the default
/// object hash, and .NET string / reference hashing is process-randomised, so string-key
/// iteration order is not a parity target (D-025). the type-name FNV stand-in is used for
/// those, which keeps int keys hashing to their value and int-key dictionaries iterating
/// in insertion order in the reference too.
pub fn hash_code(o: &Obj) -> u64 {
    match payload(o) {
        Payload::Int(i) => i as u64,
        Payload::Str(s) => prims::str_hash(&s) as u64,
        Payload::Float(v) => v.to_bits(),
        Payload::Bool(b) => b as u64,
        Payload::Enu(n) => n as u64,
        // `Val<T>.GetHashCode() => Value.GetHashCode()`, so `Date` hashes its tick count
        Payload::Date(t) => t as u64,
        Payload::List(v) => {
            let mut h = 0u64;
            for e in v.borrow().iter() {
                h = combine_hash(h, hash_code(e));
            }
            h
        }
        Payload::Tup(vals, _) => {
            let mut h = 0u64;
            for e in vals.iter() {
                h = combine_hash(h, hash_code(e));
            }
            h
        }
        // `Json` delegates to its wrapped value
        Payload::Json(inner) => hash_code(&inner),
        // `Ref<T>.GetHashCode() => Value.GetHashCode()`, so an `Iters` hash is the
        // host enumerator's identity hash. that is process-randomised in .NET, so the
        // type-name fallback is used instead (D-025: str-key order is not a parity target).
        _ => prims::str_hash(&type_name(o)) as u64,
    }
}

fn combine_hash(seed: u64, value: u64) -> u64 {
    seed.rotate_left(13) ^ value
}

pub fn types_of(o: &Obj) -> Types {
    o.borrow().types.clone()
}

/// `Members[name]` on a missing key throws inside the host; the inner .NET message is
/// reproduced verbatim and the `TargetInvocationException` wrapper is dropped (D-024).
pub fn key_not_found(name: &str) -> Obj {
    panic(
        "System.Collections.Generic.KeyNotFoundException",
        format!("The given key '{name}' was not present in the dictionary."),
    )
}

pub fn bool_value(o: &Obj) -> bool {
    matches!(payload(o), Payload::Bool(true))
}

fn valid_super(o: &Obj) -> Option<Obj> {
    let s = o.borrow().super_.clone();
    match s {
        Some(s) if !is_none(&s) => Some(s),
        _ => None,
    }
}

pub fn member(o: &Obj, name: &str) -> Option<Obj> {
    o.borrow()
        .members
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.clone())
}

fn has_member(o: &Obj, name: &str) -> bool {
    o.borrow().members.iter().any(|(k, _)| k == name)
}

pub fn member_set(o: &Obj, name: &str, value: &Obj) {
    let mut d = o.borrow_mut();
    for slot in &mut d.members {
        if slot.0 == name {
            slot.1 = value.clone();
            return;
        }
    }
    d.members.push((name.to_owned(), value.clone()));
}

pub fn parents(o: &Obj) -> Vec<Obj> {
    o.borrow().parents.clone()
}

/// `Obj.BaseChain()` — super first, then `Parents[1..]`, de-duplicated by identity
pub fn base_chain(o: &Obj) -> Vec<Obj> {
    let mut seen: Vec<Obj> = Vec::new();
    let mut queue: Vec<Obj> = Vec::new();

    fn push(b: &Obj, seen: &mut Vec<Obj>, queue: &mut Vec<Obj>) {
        if is_none(b) {
            return;
        }
        if seen.iter().any(|o| Rc::ptr_eq(o, b)) {
            return;
        }
        seen.push(b.clone());
        queue.push(b.clone());
        for p in parents(b).iter().skip(1) {
            push(p, seen, queue);
        }
        if let Some(s) = b.borrow().super_.clone() {
            push(&s, seen, queue);
        }
    }

    if let Some(s) = o.borrow().super_.clone() {
        push(&s, &mut seen, &mut queue);
    }
    for p in parents(o).iter().skip(1) {
        push(p, &mut seen, &mut queue);
    }

    queue
}

/// `Obj.TryMethod` — bind `Self`/`Super` on the member and call it.
/// phase 7 makes members callable; until then this always reports "not found".
fn try_method(o: &Obj, name: &str, args: &[Obj]) -> Option<Obj> {
    let m = member(o, name)?;
    {
        let mut d = m.borrow_mut();
        d.self_ = Some(o.clone());
        d.super_ = o.borrow().super_.clone();
    }
    invoke(&m, args)
}

/// the reference dispatches through `Obj.Call`; function payloads arrive in phase 7.
pub fn invoke(f: &Obj, args: &[Obj]) -> Option<Obj> {
    let _ = (f, args);
    None
}

pub fn call(o: &Obj, args: &[Obj]) -> Obj {
    if let Some(v) = try_method(o, "__call__", args) {
        return v;
    }
    if let Some(s) = valid_super(o) {
        return call(&s, args);
    }
    if let Payload::TObj(t) = payload(o) {
        return collections::tobj_call(&t, args);
    }
    err(format!("unsupported operand type(s) for (): '{}'", type_name(o)))
}

/// `Obj.Init`
pub fn init(o: &Obj, args: &[Obj]) -> Obj {
    // the builtin collections override `Init`, so their rule runs before the base chain
    match payload(o) {
        Payload::List(..) => return collections::list_init(o, args),
        Payload::Dict(..) => return collections::dict_init(o, args),
        Payload::Set(..) => return collections::set_init(o, args),
        Payload::Tup(..) => return collections::tup_init(o, args),
        Payload::Json(..) => return collections::json_init(o, args),
        Payload::Enu(n) => return collections::enu_init(o, n, args),
        Payload::Stru(_) => return collections::stru_init(o, args),
        Payload::Iter(st) => return iters::iter_init(o, &st, args),
        _ => {}
    }
    if try_method(o, "__init__", args).is_some() {
        return o.clone();
    }
    if let Some(s) = valid_super(o) {
        let r = init(&s, args);
        if is_err(&r) {
            return r;
        }
    }
    o.clone()
}

pub fn get_attr(o: &Obj, name: &str) -> Obj {
    get_attr_from(o, name, o)
}

/// `Obj.GetAttrFrom` — property, `__getattr__`, members, super, global original
/// values, `Parents[1..]`, then the missing-attribute error.
fn get_attr_from(o: &Obj, name: &str, receiver: &Obj) -> Obj {
    if let Some(m) = member(o, name) {
        if is_property(&m) {
            return property_get(&m, receiver);
        }
    }
    if let Some(v) = try_method(o, "__getattr__", &[str(name)]) {
        return bind_self(v, receiver);
    }
    if let Some(v) = member(o, name) {
        return bind_self(v, receiver);
    }
    if let Some(s) = valid_super(o) {
        if has(&s, name) {
            return bind_self(get_attr_from(&s, name, receiver), receiver);
        }
    }
    if let Some(v) = world::try_get_original_value(&type_name(o), name) {
        return bind_self(v, receiver);
    }
    for p in parents(o).iter().skip(1) {
        if has(p, name) {
            return bind_self(get_attr_from(p, name, receiver), receiver);
        }
    }
    err(format!(
        "'{}' object has no attribute '{}'",
        type_name(o),
        name
    ))
}

fn bind_self(value: Obj, receiver: &Obj) -> Obj {
    if is_bound_fn(&value) {
        let mut d = value.borrow_mut();
        d.self_ = Some(receiver.clone());
        d.super_ = receiver.borrow().super_.clone();
    }
    value
}

/// phase 7: `value is Function.Fn`
fn is_bound_fn(o: &Obj) -> bool {
    let _ = o;
    false
}

/// phase 7: `member is Property`
pub fn is_property(o: &Obj) -> bool {
    let _ = o;
    false
}

/// phase 7: `Property.Get(receiver)`
fn property_get(member: &Obj, receiver: &Obj) -> Obj {
    let _ = (member, receiver);
    none()
}

/// phase 7: `Property.Set(receiver, value)`
pub fn property_set(member: &Obj, receiver: &Obj, value: &Obj) -> Obj {
    let _ = (member, receiver, value);
    value.clone()
}

pub fn set_attr(o: &Obj, name: &str, value: &Obj) -> Obj {
    set_attr_from(o, name, value, o)
}

pub fn set_attr_from(o: &Obj, name: &str, value: &Obj, receiver: &Obj) -> Obj {
    if let Some(m) = member(o, name) {
        if is_property(&m) {
            return property_set(&m, receiver, value);
        }
    }
    if try_method(o, "__setattr__", &[str(name), value.clone()]).is_some() {
        return value.clone();
    }
    if has_member(o, name) {
        member_set(o, name, value);
        return value.clone();
    }
    if let Some(s) = valid_super(o) {
        return set_attr_from(&s, name, value, receiver);
    }
    let tn = type_name(o);
    if matches!(&*tn, "int" | "float" | "str" | "bool" | "date" | "none" | "null") {
        return err(format!("'{tn}' object has no attribute '{name}'"));
    }
    member_set(o, name, value);
    value.clone()
}

pub fn has(o: &Obj, name: &str) -> bool {
    if has_member(o, name) {
        return true;
    }
    if let Some(s) = valid_super(o) {
        if has(&s, name) {
            return true;
        }
    }
    parents(o).iter().skip(1).any(|p| has(p, name))
}

pub fn obj_is(o: &Obj, other: &Obj) -> Obj {
    if let Some(v) = try_method(o, "__is__", &[other.clone()]) {
        return v;
    }
    if let Payload::TObj(t) = payload(o) {
        return collections::tobj_is(&t, other);
    }
    let types = o.borrow().types.clone();
    let other_type = Types::Single(type_name(other));
    if types.contains(&other_type) {
        return boolean(true);
    }
    match valid_super(o) {
        Some(s) => obj_is(&s, other),
        None => boolean(false),
    }
}

pub fn obj_in(o: &Obj, other: &Obj) -> Obj {
    if let Some(v) = try_method(o, "__in__", &[other.clone()]) {
        return v;
    }
    if let Some(s) = valid_super(o) {
        return obj_in(&s, other);
    }
    match payload(o) {
        Payload::List(..) => collections::list_in(o, other),
        Payload::Dict(..) => collections::dict_in(o, other),
        Payload::Tup(vals, _) => collections::tup_in(&vals, other),
        Payload::TObj(t) => collections::tobj_in(&t, other),
        Payload::Iter(st) => iters::iter_in(o, &st, other),
        _ => err(format!(
            "unsupported operand type(s) for in: '{}'",
            type_name(o)
        )),
    }
}

pub fn repr(o: &Obj) -> Obj {
    if let Some(v) = try_method(o, "__repr__", &[]) {
        return v;
    }
    if let Some(s) = valid_super(o) {
        let r = repr(&s);
        if let Payload::Str(rs) = payload(&r) {
            if rs == type_name(&s) {
                return str(&type_name(o));
            }
            return r;
        }
    }
    str(&type_name(o))
}

/// `io.write` rendering: `ToStr()` first, `Repr()` as the fallback
pub fn display(o: &Obj) -> String {
    let t = to_str(o);
    if let Payload::Str(s) = payload(&t) {
        return s.to_string();
    }
    let r = repr(o);
    if let Payload::Str(s) = payload(&r) {
        return s.to_string();
    }
    String::new()
}

pub fn clone_obj(o: &Obj) -> Obj {
    if let Some(v) = try_method(o, "__clone__", &[]) {
        return v;
    }
    if let Some(s) = valid_super(o) {
        if has(&s, "__clone__") {
            return clone_obj(&s);
        }
    }

    match payload(o) {
        Payload::List(v) => return collections::list_clone(&v),
        Payload::Dict(km) => return collections::dict_clone(&km),
        Payload::Set(s) => return collections::set_clone(&s),
        Payload::Tup(vals, names) => return collections::tup_clone(&vals, &names),
        Payload::Json(inner) => return collections::json_clone(&inner),
        Payload::Stru(_) => return collections::stru_clone(o),
        Payload::Enu(n) => return collections::enu_clone(o, n),
        // `Spreads` and `TObj` have no `Clone` override, so the base clone rebuilds the
        // payload cell with the same content.
        Payload::Spread(v) => return collections::spread(v.to_vec()),
        Payload::TObj(t) => return collections::tobj(t),
        Payload::Iter(st) => return iters::iter_clone(o, &st),
        _ => {}
    }

    let (type_name, types, members, annotations, self_, parents, super_) = {
        let d = o.borrow();
        (
            d.type_name.clone(),
            d.types.clone(),
            d.members.clone(),
            d.annotations.clone(),
            d.self_.clone(),
            d.parents.clone(),
            d.super_.clone(),
        )
    };

    let mut created = ObjData::new(type_name, world::obj_type(), Payload::Plain);
    created.types = types;
    created.members = members;
    created.annotations = annotations;
    created.self_ = self_;
    created.parents = parents;
    created.super_ = match &super_ {
        Some(s) if is_none(s) => Some(s.clone()),
        Some(s) => Some(clone_obj(s)),
        None => Some(none()),
    };

    Rc::new(RefCell::new(created))
}

pub fn copy_obj(o: &Obj) -> Obj {
    if let Some(v) = try_method(o, "__copy__", &[]) {
        return v;
    }
    match payload(o) {
        Payload::Int(v) => int(v),
        Payload::Float(v) => float(v),
        Payload::Bool(b) => boolean(b),
        Payload::Str(s) => str_obj(s, world::str_type(), world::obj_type()),
        Payload::Date(t) => date_obj(t),
        Payload::Tup(vals, names) => collections::tup_copy(&vals, &names),
        Payload::Stru(_) => collections::stru_copy(o),
        // `List.Copy` / `Dict.Copy` / `Set.Copy` / `Json.Copy` / `Enu.Copy` / `Iters.Copy` are all `this`
        Payload::List(..)
        | Payload::Dict(..)
        | Payload::Set(..)
        | Payload::Iter(..)
        | Payload::Json(..)
        | Payload::Enu(..)
        | Payload::Spread(..) => o.clone(),
        _ => o.clone(),
    }
}

pub fn compare_to(o: &Obj, other: &Obj) -> Result<i64, Obj> {
    let eq = binop(o, BinOp::Eq, other);
    if is_bool(&eq) && bool_value(&eq) {
        return Ok(0);
    }
    let lt = binop(o, BinOp::Lt, other);
    if is_bool(&lt) && bool_value(&lt) {
        return Ok(-1);
    }
    let gt = binop(o, BinOp::Gt, other);
    if is_bool(&gt) && bool_value(&gt) {
        return Ok(1);
    }
    Err(panic("panic", "types that are not comparable to each other."))
}

pub fn not_obj(o: &Obj) -> Obj {
    if let Payload::Bool(b) = payload(o) {
        return boolean(!b);
    }
    let v = to_bool(o);
    match payload(&v) {
        Payload::Bool(b) => boolean(!b),
        _ => err("operand must be a boolean"),
    }
}

pub fn xor_obj(o: &Obj, other: &Obj) -> Obj {
    if let Payload::Bool(b) = payload(o) {
        return if b { not_obj(other) } else { o.clone() };
    }
    let left = to_bool(o);
    if !is_bool(&left) {
        return err("left operand must be a boolean");
    }
    let right = to_bool(other);
    if !is_bool(&right) {
        return err("right operand must be a boolean");
    }
    boolean(bool_value(&left) ^ bool_value(&right))
}

pub fn len(o: &Obj) -> Obj {
    unary(o, UnOp::Len)
}

pub fn hash(o: &Obj) -> Obj {
    unary(o, UnOp::Hash)
}

pub fn iter(o: &Obj) -> Obj {
    unary(o, UnOp::Iter)
}

pub fn next(o: &Obj) -> Obj {
    unary(o, UnOp::Next)
}

pub fn entry(o: &Obj) -> Obj {
    unary(o, UnOp::Entry)
}

pub fn exit(o: &Obj) -> Obj {
    unary(o, UnOp::Exit)
}

pub fn spread(o: &Obj) -> Obj {
    unary(o, UnOp::Spread)
}

pub fn to_int(o: &Obj) -> Obj {
    unary(o, UnOp::ToInt)
}

pub fn to_float(o: &Obj) -> Obj {
    unary(o, UnOp::ToFloat)
}

pub fn to_bool(o: &Obj) -> Obj {
    unary(o, UnOp::ToBool)
}

pub fn to_list(o: &Obj) -> Obj {
    unary(o, UnOp::ToList)
}

pub fn to_tuple(o: &Obj) -> Obj {
    unary(o, UnOp::ToTuple)
}

pub fn to_str(o: &Obj) -> Obj {
    // `Obj.ToStr` answers `none` before any dunder lookup
    if is_none(o) {
        return str("none");
    }
    if let Some(v) = try_method(o, "__str__", &[]) {
        return v;
    }
    if let Some(s) = valid_super(o) {
        return to_str(&s);
    }
    match payload(o) {
        Payload::Str(s) => str_obj(s, world::str_type(), world::obj_type()),
        Payload::Int(v) => str(&v.to_string()),
        Payload::Float(v) => str(&prims::fmt_double(v)),
        Payload::Bool(b) => str(if b { "true" } else { "false" }),
        Payload::Date(t) => str(&date::format(t)),
        Payload::Error { header, message } => str(&format!("{header}: {message}")),
        Payload::List(v) => str(&collections::list_to_str(&v)),
        Payload::Dict(km) => str(&collections::dict_to_str(&km)),
        Payload::Set(s) => str(&collections::set_to_str(&s)),
        Payload::Tup(vals, _) => str(&collections::tup_to_str(&vals)),
        Payload::Json(inner) => str(&collections::json_to_str(&inner)),
        Payload::Stru(_) => collections::stru_to_str(o),
        Payload::Enu(n) => collections::enu_to_str(o, n),
        Payload::TObj(t) => collections::tobj_to_str(&t),
        Payload::Iter(st) => iters::iter_to_str(o, &st),
        _ => err(format!("unsupported operand type(s) for str(): '{}'", type_name(o))),
    }
}

pub fn get_item(o: &Obj, key: &Obj) -> Obj {
    if let Some(v) = try_method(o, "__getitem__", &[key.clone()]) {
        return v;
    }
    if let Some(s) = valid_super(o) {
        return get_item(&s, key);
    }
    match payload(o) {
        Payload::Str(s) => match payload(key) {
            Payload::Int(i) => {
                let len = s.encode_utf16().count() as i64;
                let index = i as i32 as i64;
                if 0 > index || index >= len {
                    let wrapped = (i + len) as i32 as i64;
                    if 0 > wrapped || wrapped >= len {
                        return err("list index out of range");
                    }
                    return str(&date::utf16_char(&s, wrapped as usize));
                }
                str(&date::utf16_char(&s, index as usize))
            }
            _ => err("invalid index type"),
        },
        Payload::List(v) => collections::list_get_item(&v, key),
        Payload::Dict(km) => collections::dict_get_item(&km, key),
        Payload::Set(s) => collections::set_get_item(&s, key),
        Payload::Tup(vals, names) => collections::tup_get_item(&vals, &names, o, key),
        Payload::Json(inner) => get_item(&inner, key),
        _ => err(format!("unsupported operand type(s) for []: '{}'", type_name(o))),
    }
}

pub fn set_item(o: &Obj, key: &Obj, value: &Obj) -> Obj {
    if try_method(o, "__setitem__", &[key.clone(), value.clone()]).is_some() {
        return value.clone();
    }
    if let Some(s) = valid_super(o) {
        return set_item(&s, key, value);
    }
    match payload(o) {
        Payload::List(v) => return collections::list_set_item(&v, key, value),
        Payload::Dict(km) => return collections::dict_set_item(&km, key, value),
        Payload::Json(inner) => return set_item(&inner, key, value),
        _ => {}
    }
    err(format!(
        "unsupported operand type(s) for [] = 'value': '{}'",
        type_name(o)
    ))
}

pub fn slice(o: &Obj, start: Option<Obj>, end: Option<Obj>, step: Option<Obj>) -> Obj {
    let args = [
        start.clone().unwrap_or_else(none),
        end.clone().unwrap_or_else(none),
        step.clone().unwrap_or_else(none),
    ];
    if let Some(v) = try_method(o, "__slice__", &args) {
        return v;
    }
    if let Some(s) = valid_super(o) {
        return slice(&s, start, end, step);
    }

    match payload(o) {
        Payload::Str(s) => {
            let units: Vec<u16> = s.encode_utf16().collect();
            let len = units.len() as i64;
            let (mut a, mut b, mut st) = (0usize, units.len(), 1i64);

            if let Some(start) = start {
                let i = match payload(&start) {
                    Payload::Int(i) => i,
                    _ => return err("slice start must be int"),
                };
                let mut i = i as i32 as i64;
                if i < 0 {
                    i += len;
                }
                a = i.clamp(0, len) as usize;
            }
            if let Some(end) = end {
                let i = match payload(&end) {
                    Payload::Int(i) => i,
                    _ => return err("slice end must be int"),
                };
                let mut i = i as i32 as i64;
                if i < 0 {
                    i += len;
                }
                b = i.clamp(0, len) as usize;
            }
            if let Some(step) = step {
                let i = match payload(&step) {
                    Payload::Int(i) => i,
                    _ => return err("slice step must be int"),
                };
                st = i as i32 as i64;
                if st == 0 {
                    return err("slice step cannot be zero");
                }
            }

            if st == 1 {
                if b < a {
                    b = a;
                }
                return str(&date::utf16_join(&units[a..b]));
            }

            let mut out: Vec<u16> = Vec::new();
            if st > 0 {
                let mut i = a as i64;
                while i < b as i64 {
                    out.push(units[i as usize]);
                    i += st;
                }
            } else {
                let mut i = a as i64;
                while i > b as i64 {
                    out.push(units[i as usize]);
                    i += st;
                }
            }
            str(&date::utf16_join(&out))
        }
        Payload::List(v) => collections::list_slice(&v, &start, &end, &step),
        Payload::Tup(vals, names) => collections::tup_slice(&vals, &names, &start, &end, &step),
        _ => err(format!("unsupported slice for '{}'", type_name(o))),
    }
}

/// `Obj.RangeTo` — `..`. the reference builds an `Iter.Range`; phase 7 supplies it.
pub fn range_to(o: &Obj, other: &Obj) -> Obj {
    if let Some(v) = try_method(o, "__range__", &[other.clone()]) {
        return v;
    }
    if let Some(s) = valid_super(o) {
        return range_to(&s, other);
    }

    if range_bound(o).is_some() && range_bound(other).is_some() {
        // `new Iter.Range(start, stop)`, an inclusive-lower / exclusive-upper range with step 1
        return iters::range_new(
            range_bound(o).unwrap(),
            range_bound(other).unwrap(),
            1,
        );
    }

    err(format!(
        "unsupported operand type(s) for ..: '{}' and '{}'",
        type_name(o),
        type_name(other)
    ))
}

fn range_bound(o: &Obj) -> Option<i64> {
    match payload(o) {
        Payload::Int(v) => Some(v),
        Payload::Float(v) => {
            if v.floor() == v && v >= i64::MIN as f64 && v <= i64::MAX as f64 {
                Some(v as i64)
            } else {
                None
            }
        }
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    IDiv,
    Mod,
    Pow,
    BAnd,
    BOr,
    BXor,
    LShift,
    RShift,
    Eq,
    NEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    Range,
    GetItem,
}

impl BinOp {
    fn symbol(&self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::IDiv => "//",
            BinOp::Mod => "%",
            BinOp::Pow => "**",
            BinOp::BAnd => "&",
            BinOp::BOr => "|",
            BinOp::BXor => "^",
            BinOp::LShift => "<<",
            BinOp::RShift => ">>",
            BinOp::Eq => "==",
            BinOp::NEq => "!=",
            BinOp::Lt => "<",
            BinOp::Gt => ">",
            BinOp::LtEq => "<=",
            BinOp::GtEq => ">=",
            BinOp::Range => "..",
            BinOp::GetItem => "[]",
        }
    }

    fn dunder(&self) -> &'static str {
        match self {
            BinOp::Add => "__add__",
            BinOp::Sub => "__sub__",
            BinOp::Mul => "__mul__",
            BinOp::Div => "__div__",
            BinOp::IDiv => "__idiv__",
            BinOp::Mod => "__mod__",
            BinOp::Pow => "__pow__",
            BinOp::BAnd => "__band__",
            BinOp::BOr => "__bor__",
            BinOp::BXor => "__bxor__",
            BinOp::LShift => "__lsh__",
            BinOp::RShift => "__rsh__",
            BinOp::Eq => "__eq__",
            BinOp::NEq => "__neq__",
            BinOp::Lt => "__lt__",
            BinOp::Gt => "__gt__",
            BinOp::LtEq => "__ltoreq__",
            BinOp::GtEq => "__gtoreq__",
            BinOp::Range => "__range__",
            BinOp::GetItem => "__getitem__",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Pos,
    Neg,
    BNot,
    Len,
    Hash,
    ToInt,
    ToFloat,
    ToBool,
    ToList,
    ToTuple,
    Iter,
    Next,
    Entry,
    Exit,
    Spread,
}

impl UnOp {
    fn dunder(&self) -> &'static str {
        match self {
            UnOp::Pos => "__pos__",
            UnOp::Neg => "__neg__",
            UnOp::BNot => "__bnot__",
            UnOp::Len => "__len__",
            UnOp::Hash => "__hash__",
            UnOp::ToInt => "__int__",
            UnOp::ToFloat => "__float__",
            UnOp::ToBool => "__bool__",
            UnOp::ToList => "__list__",
            UnOp::ToTuple => "__tuple__",
            UnOp::Iter => "__iter__",
            UnOp::Next => "__next__",
            UnOp::Entry => "__entry__",
            UnOp::Exit => "__exit__",
            UnOp::Spread => "__spread__",
        }
    }

    pub fn message(&self, t: &str) -> String {
        match self {
            UnOp::Len => "unsupported operand type(s) for len()".to_owned(),
            UnOp::Hash => "cannot hashable object".to_owned(),
            UnOp::Entry => format!("unsupported operand type(s) for using entry: '{t}'"),
            UnOp::Exit => format!("unsupported operand type(s) for using exit: '{t}'"),
            UnOp::Spread => format!("unsupported operand type(s) for *: '{t}'"),
            _ => {
                let name = match self {
                    UnOp::ToInt => "int()",
                    UnOp::ToFloat => "float()",
                    UnOp::ToBool => "bool()",
                    UnOp::ToList => "list()",
                    UnOp::ToTuple => "tuple()",
                    UnOp::Iter => "iter()",
                    UnOp::Next => "next()",
                    UnOp::Pos => "+",
                    UnOp::Neg => "-",
                    UnOp::BNot => "~",
                    _ => "",
                };
                format!("unsupported operand type(s) for {name}: '{t}'")
            }
        }
    }
}

pub fn binop(o: &Obj, op: BinOp, other: &Obj) -> Obj {
    if let Some(v) = try_method(o, op.dunder(), &[other.clone()]) {
        return v;
    }
    if let Some(s) = valid_super(o) {
        return binop(&s, op, other);
    }
    builtin_binop(o, op, other)
}

pub fn unary(o: &Obj, op: UnOp) -> Obj {
    if let Some(v) = try_method(o, op.dunder(), &[]) {
        return v;
    }
    if let Some(s) = valid_super(o) {
        return unary(&s, op);
    }
    builtin_unop(o, op)
}

fn builtin_binop(o: &Obj, op: BinOp, other: &Obj) -> Obj {
    let t = type_name(o);
    let ot = type_name(other);

    match payload(o) {
        Payload::Int(a) => int_binop(a, op, other, &ot),
        Payload::Float(a) => float_binop(a, op, other, &ot),
        Payload::Str(a) => str_binop(a, op, other, &t, &ot),
        Payload::Bool(a) => bool_binop(a, op, other, &t, &ot),
        Payload::Date(a) => date_binop(a, op, other, &t, &ot),
        Payload::List(..) => collections::list_binop(o, op, other, &t, &ot),
        Payload::Tup(..) => collections::tup_binop(o, op, other, &t, &ot),
        Payload::Set(..) => collections::set_binop(o, op, other, &t, &ot),
        Payload::Stru(..) => collections::stru_binop(o, op, other, &t, &ot),
        Payload::Enu(n) => collections::enu_binop(o, n, op, other, &t, &ot),
        Payload::TObj(ty) => collections::tobj_binop(&ty, op, other, &t, &ot),
        Payload::Iter(st) => iters::iter_binop(o, &st, op, other, &t, &ot),
        // `Json` delegates every operation to its wrapped value; the dict / spread
        // payloads have no operator override, so they fall through to the base chain.
        Payload::Json(inner) => binop(&inner, op, other),
        Payload::Dict(..) | Payload::Spread(..) => base_binop(o, op, other, &t, &ot),
        Payload::Error { .. } | Payload::Panic { .. } | Payload::Plain => base_binop(o, op, other, &t, &ot),
    }
}

fn builtin_unop(o: &Obj, op: UnOp) -> Obj {
    let t = type_name(o);

    match payload(o) {
        Payload::Int(a) => match op {
            UnOp::Pos => int(a),
            UnOp::Neg => int(a.wrapping_neg()),
            UnOp::BNot => int(!a),
            UnOp::Len => int(1),
            UnOp::ToInt => int(a),
            UnOp::ToFloat => float(a as f64),
            UnOp::ToBool => boolean(a != 0),
            UnOp::Spread => err(format!("unsupported operand type(s) for *: '{t}'")),
            _ => err(op.message(&t)),
        },
        Payload::Float(a) => match op {
            UnOp::Pos => float(a),
            UnOp::Neg => float(-a),
            UnOp::ToInt => int(prims::cast_long(a)),
            UnOp::ToFloat => float(a),
            UnOp::ToBool => boolean(a != 0.0),
            _ => err(op.message(&t)),
        },
        Payload::Str(a) => match op {
            UnOp::Len => int(a.encode_utf16().count() as i64),
            UnOp::ToInt => match a.parse::<i64>() {
                Ok(v) => int(v),
                Err(_) => err(format!("cannot convert '{a}' to 'int'")),
            },
            UnOp::ToFloat => match a.parse::<f64>() {
                Ok(v) => float(v),
                Err(_) => err(format!("cannot convert '{a}' to 'float'")),
            },
            UnOp::ToBool => match a.parse::<bool>() {
                Ok(v) => boolean(v),
                Err(_) => boolean(!a.is_empty()),
            },
            UnOp::Hash => int(prims::str_hash(&a) as i64),
            // one `Str` per UTF-16 code unit, like `Value.Select(c => From($"{c}"))`
            UnOp::Iter => iters::str_iter(&a),
            UnOp::ToList => iters::str_list(&a),
            UnOp::ToTuple => iters::str_tuple(&a),
            _ => err(op.message(&t)),
        },
        Payload::Bool(a) => match op {
            UnOp::ToBool => boolean(a),
            _ => err(op.message(&t)),
        },
        Payload::List(v) => collections::list_unop(&v, op, &t),
        Payload::Dict(km) => collections::dict_unop(&km, op, &t),
        Payload::Set(s) => collections::set_unop(&s, op, &t),
        Payload::Tup(vals, names) => collections::tup_unop(&vals, &names, op, &t),
        Payload::Spread(_v) => collections::spread_unop(o, op, &t),
        Payload::Json(inner) => collections::json_unop(&inner, op, &t),
        Payload::Stru(_) => collections::stru_unop(o, op, &t),
        Payload::Enu(n) => collections::enu_unop(n, op, &t),
        Payload::Iter(st) => iters::iter_unop(o, &st, op, &t),
        // `TObj` has no unop override, so only the base message applies
        Payload::TObj(..) => err(op.message(&t)),
        Payload::Date(_) | Payload::Error { .. } | Payload::Panic { .. } | Payload::Plain => {
            err(op.message(&t))
        }
    }
}

fn int_binop(a: i64, op: BinOp, other: &Obj, ot: &Rc<str>) -> Obj {
    let b = payload(other);

    match op {
        BinOp::Add => match b {
            Payload::Int(v) => int(a.wrapping_add(v)),
            Payload::Float(v) => float(a as f64 + v),
            Payload::Str(v) => str(&format!("{a}{v}")),
            _ => unsupported("+", "int", ot),
        },
        BinOp::Sub => match b {
            Payload::Int(v) => int(a.wrapping_sub(v)),
            Payload::Float(v) => float(a as f64 - v),
            _ => unsupported("-", "int", ot),
        },
        BinOp::Mul => match b {
            Payload::Int(v) => int(a.wrapping_mul(v)),
            Payload::Float(v) => float(a as f64 * v),
            _ => unsupported("*", "int", ot),
        },
        BinOp::Div => match b {
            Payload::Int(v) => {
                if v == 0 {
                    return div_zero("int");
                }
                float(a as f64 / v as f64)
            }
            Payload::Float(v) => {
                if v == 0.0 {
                    return div_zero("int");
                }
                float(a as f64 / v)
            }
            _ => unsupported("/", "int", ot),
        },
        BinOp::IDiv => match b {
            Payload::Int(v) => {
                if v == 0 {
                    return div_zero("int");
                }
                int(a.wrapping_div(v))
            }
            Payload::Float(v) => {
                if v == 0.0 {
                    return div_zero("int");
                }
                int(prims::cast_long(a as f64 / v))
            }
            _ => unsupported("//", "int", ot),
        },
        BinOp::Mod => match b {
            Payload::Int(v) => {
                if v == 0 {
                    return divide_by_zero();
                }
                int(a.wrapping_rem(v))
            }
            Payload::Float(v) => float(a as f64 % v),
            _ => unsupported("%", "int", ot),
        },
        BinOp::Pow => match b {
            Payload::Int(v) => {
                if v < 0 {
                    return err(format!("int pow: negative exponent {v}"));
                }
                let d = (a as f64).powf(v as f64);
                if d.is_infinite() || d.is_nan() || d > i64::MAX as f64 || d < i64::MIN as f64 {
                    return err(format!("int pow overflow: {a} ** {v}"));
                }
                int(d as i64)
            }
            Payload::Float(v) => {
                let d = (a as f64).powf(v);
                if d.is_infinite() || d.is_nan() {
                    return err(format!("int pow overflow: {a} ** {}", prims::fmt_double(v)));
                }
                float(d)
            }
            _ => unsupported("**", "int", ot),
        },
        BinOp::BAnd => bit(a, op, b, ot),
        BinOp::BOr => bit(a, op, b, ot),
        BinOp::BXor => bit(a, op, b, ot),
        BinOp::LShift => match b {
            Payload::Int(v) => int(a << (v as i32 & 63)),
            _ => unsupported("<<", "int", ot),
        },
        BinOp::RShift => match b {
            Payload::Int(v) => int(a >> (v as i32 & 63)),
            _ => unsupported(">>", "int", ot),
        },
        BinOp::Eq => {
            if is_none(other) {
                return boolean(false);
            }
            match b {
                Payload::Int(v) => boolean(a == v),
                Payload::Float(v) => boolean(a as f64 == v),
                _ => unsupported("==", "int", ot),
            }
        }
        BinOp::NEq => {
            if is_none(other) {
                return boolean(true);
            }
            match b {
                Payload::Int(v) => boolean(a != v),
                Payload::Float(v) => boolean(a as f64 != v),
                _ => unsupported("!=", "int", ot),
            }
        }
        BinOp::Lt => numeric("int", a as f64, op, b, ot),
        BinOp::Gt => numeric("int", a as f64, op, b, ot),
        BinOp::LtEq => numeric("int", a as f64, op, b, ot),
        BinOp::GtEq => numeric("int", a as f64, op, b, ot),
        _ => base_binop_of(op, "int", ot),
    }
}

fn float_binop(a: f64, op: BinOp, other: &Obj, ot: &Rc<str>) -> Obj {
    let b = payload(other);

    match op {
        BinOp::Add => match b {
            Payload::Int(v) => float(a + v as f64),
            Payload::Float(v) => float(a + v),
            Payload::Str(v) => str(&format!("{}{v}", prims::fmt_double(a))),
            _ => unsupported("+", "float", ot),
        },
        BinOp::Sub => match b {
            Payload::Int(v) => float(a - v as f64),
            Payload::Float(v) => float(a - v),
            _ => unsupported("-", "float", ot),
        },
        BinOp::Mul => match b {
            Payload::Int(v) => float(a * v as f64),
            Payload::Float(v) => float(a * v),
            _ => unsupported("*", "float", ot),
        },
        BinOp::Div => match b {
            Payload::Int(v) => {
                if v == 0 {
                    return div_zero("float");
                }
                float(a / v as f64)
            }
            Payload::Float(v) => {
                if v == 0.0 {
                    return div_zero("float");
                }
                float(a / v)
            }
            _ => unsupported("/", "float", ot),
        },
        BinOp::IDiv => match b {
            Payload::Int(v) => {
                if v == 0 {
                    return div_zero("float");
                }
                int(prims::cast_long(a / v as f64))
            }
            Payload::Float(v) => {
                if v == 0.0 {
                    return div_zero("float");
                }
                int(prims::cast_long(a / v))
            }
            _ => unsupported("//", "float", ot),
        },
        BinOp::Mod => match b {
            Payload::Int(v) => float(a % v as f64),
            Payload::Float(v) => float(a % v),
            _ => unsupported("%", "float", ot),
        },
        BinOp::Pow => {
            let exp = match b {
                Payload::Int(v) => v as f64,
                Payload::Float(v) => v,
                _ => return unsupported("**", "float", ot),
            };
            let d = a.powf(exp);
            if d.is_infinite() || d.is_nan() {
                return err(format!(
                    "float pow overflow: {} ** {}",
                    prims::fmt_double(a),
                    prims::fmt_double(exp)
                ));
            }
            float(d)
        }
        BinOp::Eq => {
            if is_none(other) {
                return boolean(false);
            }
            match b {
                Payload::Int(v) => boolean(a == v as f64),
                Payload::Float(v) => boolean(a == v),
                _ => unsupported("==", "float", ot),
            }
        }
        BinOp::NEq => {
            if is_none(other) {
                return boolean(true);
            }
            match b {
                Payload::Int(v) => boolean(a != v as f64),
                Payload::Float(v) => boolean(a != v),
                _ => unsupported("!=", "float", ot),
            }
        }
        BinOp::Lt => numeric("float", a, op, b, ot),
        BinOp::Gt => numeric("float", a, op, b, ot),
        BinOp::LtEq => numeric("float", a, op, b, ot),
        BinOp::GtEq => numeric("float", a, op, b, ot),
        _ => base_binop_of(op, "float", ot),
    }
}

fn str_binop(a: Rc<str>, op: BinOp, other: &Obj, t: &Rc<str>, ot: &Rc<str>) -> Obj {
    match op {
        BinOp::Add | BinOp::Sub => {
            let s = to_str(other);
            if is_err(&s) {
                return s;
            }
            match payload(&s) {
                Payload::Str(v) => {
                    if op == BinOp::Add {
                        str(&format!("{a}{v}"))
                    } else {
                        str(&a.replace(v.as_ref(), ""))
                    }
                }
                _ => err(format!("cannot add '{ot}' to '{t}'")),
            }
        }
        BinOp::Eq => {
            if is_none(other) {
                return boolean(false);
            }
            match payload(other) {
                Payload::Str(v) => boolean(a == v),
                _ => unsupported("==", t, ot),
            }
        }
        BinOp::NEq => {
            if is_none(other) {
                return boolean(true);
            }
            match payload(other) {
                Payload::Str(v) => boolean(a != v),
                // the reference reuses the `==` wording in `Str.NEq`
                _ => unsupported("==", t, ot),
            }
        }
        BinOp::Lt => match payload(other) {
            Payload::Str(v) => boolean(a < v),
            _ => unsupported("<", t, ot),
        },
        _ => base_binop_of(op, t, ot),
    }
}

fn bool_binop(a: bool, op: BinOp, other: &Obj, t: &Rc<str>, ot: &Rc<str>) -> Obj {
    match op {
        // `Bool.Eq` answers false for every non-boolean, never an error
        BinOp::Eq => boolean(matches!(payload(other), Payload::Bool(v) if v == a)),
        _ => base_binop_of(op, t, ot),
    }
}

fn date_binop(a: i64, op: BinOp, other: &Obj, t: &Rc<str>, ot: &Rc<str>) -> Obj {
    match op {
        BinOp::Add => match payload(other) {
            // `Date.Add` adds `other.Day`, the day-of-month of the right operand
            // (a reference quirk kept for parity), in 100 ns tick units.
            Payload::Date(d) => date_obj(a + date::day(d) * date::TICKS_PER_DAY),
            Payload::Str(s) => str(&format!("{}{s}", date::format_date(a))),
            _ => unsupported("+", t, ot),
        },
        BinOp::Sub => match payload(other) {
            Payload::Date(d) => date_obj(a - d),
            _ => unsupported("-", t, ot),
        },
        _ => base_binop_of(op, t, ot),
    }
}

pub fn base_binop(o: &Obj, op: BinOp, other: &Obj, t: &Rc<str>, ot: &Rc<str>) -> Obj {
    match op {
        BinOp::Eq => {
            if is_none(o) && is_none(other) {
                return boolean(true);
            }
            if is_none(o) || is_none(other) {
                return boolean(false);
            }
            unsupported("==", t, ot)
        }
        BinOp::NEq => {
            let eq = binop(o, BinOp::Eq, other);
            match payload(&eq) {
                Payload::Bool(v) => boolean(!v),
                _ => err("operands must be booleans"),
            }
        }
        BinOp::Gt => {
            let lt = binop(o, BinOp::Lt, other);
            if !is_bool(&lt) {
                return unsupported(">", t, ot);
            }
            let eq = binop(o, BinOp::Eq, other);
            if !is_bool(&eq) {
                return unsupported(">", t, ot);
            }
            boolean(!bool_value(&lt) && !bool_value(&eq))
        }
        BinOp::LtEq => {
            let lt = binop(o, BinOp::Lt, other);
            if !is_bool(&lt) {
                return unsupported("<=", t, ot);
            }
            let eq = binop(o, BinOp::Eq, other);
            if !is_bool(&eq) {
                return unsupported("<=", t, ot);
            }
            boolean(bool_value(&lt) || bool_value(&eq))
        }
        BinOp::GtEq => {
            let lt = binop(o, BinOp::Lt, other);
            if !is_bool(&lt) {
                return unsupported(">=", t, ot);
            }
            boolean(!bool_value(&lt))
        }
        op => base_binop_of(op, t, ot),
    }
}

pub fn base_binop_of(op: BinOp, t: &str, ot: &str) -> Obj {
    unsupported(op.symbol(), t, ot)
}

fn unsupported(op: &str, a: &str, b: &str) -> Obj {
    err(format!(
        "unsupported operand type(s) for {op}: '{a}' and '{b}'"
    ))
}

fn div_zero(t: &str) -> Obj {
    err(format!("{t} division by zero"))
}

fn bit(a: i64, op: BinOp, b: Payload, ot: &Rc<str>) -> Obj {
    match b {
        Payload::Int(v) => match op {
            BinOp::BAnd => int(a & v),
            BinOp::BOr => int(a | v),
            BinOp::BXor => int(a ^ v),
            _ => unsupported("&", "int", ot),
        },
        _ => unsupported(op.symbol(), "int", ot),
    }
}

fn numeric(t: &str, a: f64, op: BinOp, b: Payload, ot: &Rc<str>) -> Obj {
    let symbol = op.symbol();
    match b {
        Payload::Int(v) => compare(op, a, v as f64),
        Payload::Float(v) => compare(op, a, v),
        _ => unsupported(symbol, t, ot),
    }
}

fn compare(op: BinOp, a: f64, b: f64) -> Obj {
    match op {
        BinOp::Lt => boolean(a < b),
        BinOp::Gt => boolean(a > b),
        BinOp::LtEq => boolean(a <= b),
        BinOp::GtEq => boolean(a >= b),
        _ => boolean(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(o: &Obj) -> String {
        match payload(o) {
            Payload::Error { header, message } => format!("{header}: {message}"),
            Payload::Panic { name, message } => {
                if name == "panic" {
                    format!("\npanic : {message}")
                } else {
                    format!("{name}: {message}")
                }
            }
            _ => display(o),
        }
    }

    /// the UTF-16 code units of a string payload, so surrogate cases can be asserted
    /// without depending on the console encoding
    fn units(o: &Obj) -> Vec<u16> {
        match payload(o) {
            Payload::Str(s) => s.encode_utf16().collect(),
            _ => Vec::new(),
        }
    }

    /// every user-visible failure string measured from the reference CLI
    #[test]
    fn error_strings_match_reference() {
        assert_eq!(message(&binop(&int(1), BinOp::Div, &int(0))), "Error: int division by zero");
        assert_eq!(message(&binop(&float(1.0), BinOp::Div, &float(0.0))), "Error: float division by zero");
        assert_eq!(
            message(&binop(&int(2), BinOp::Pow, &int(-1))),
            "Error: int pow: negative exponent -1"
        );
        assert_eq!(
            message(&binop(&int(2), BinOp::Pow, &int(100))),
            "Error: int pow overflow: 2 ** 100"
        );
        assert_eq!(
            message(&binop(&float(1e300), BinOp::Pow, &float(10.0))),
            "Error: float pow overflow: 1E+300 ** 10"
        );
        assert_eq!(
            message(&binop(&boolean(true), BinOp::Add, &int(1))),
            "Error: unsupported operand type(s) for +: 'bool' and 'int'"
        );
        assert_eq!(message(&unary(&int(5), UnOp::Hash)), "Error: cannot hashable object");
        assert_eq!(
            message(&unary(&boolean(true), UnOp::Len)),
            "Error: unsupported operand type(s) for len()"
        );
        assert_eq!(
            message(&get_attr(&int(5), "x")),
            "Error: 'int' object has no attribute 'x'"
        );
        assert_eq!(message(&get_item(&int(5), &int(0))), "Error: unsupported operand type(s) for []: 'int'");
        assert_eq!(
            message(&unary(&str("abc"), UnOp::ToInt)),
            "Error: cannot convert 'abc' to 'int'"
        );
    }

    #[test]
    fn host_exceptions_travel_as_panic_values() {
        assert_eq!(
            message(&binop(&int(5), BinOp::Mod, &int(0))),
            "System.DivideByZeroException: Attempted to divide by zero."
        );
    }

    #[test]
    fn int_arithmetic_is_wrapping_at_runtime() {
        let max = int(i64::MAX);
        assert!(matches!(payload(&binop(&max, BinOp::Add, &int(1))), Payload::Int(i64::MIN)));
        assert!(matches!(payload(&binop(&max, BinOp::Mul, &int(2))), Payload::Int(-2)));
        // `1 << 64` masks the shift count to 6 bits in C#, so it wraps to 1
        assert!(matches!(payload(&binop(&int(1), BinOp::LShift, &int(64))), Payload::Int(1)));
        assert!(matches!(payload(&binop(&int(7), BinOp::IDiv, &int(2))), Payload::Int(3)));
        assert!(matches!(payload(&binop(&int(7), BinOp::Mod, &int(2))), Payload::Int(1)));
    }

    #[test]
    fn int_pow_uses_double_and_truncates() {
        assert!(matches!(payload(&binop(&int(2), BinOp::Pow, &int(10))), Payload::Int(1024)));
        assert!(matches!(payload(&binop(&int(10), BinOp::Pow, &float(0.5))), Payload::Float(v) if v == 3.1622776601683795));
    }

    #[test]
    fn display_covers_every_primitive() {
        assert_eq!(display(&int(42)), "42");
        assert_eq!(display(&float(1.5)), "1.5");
        assert_eq!(display(&str("hi")), "hi");
        assert_eq!(display(&boolean(true)), "true");
        assert_eq!(display(&boolean(false)), "false");
        assert_eq!(display(&none()), "none");
        assert_eq!(display(&date_obj(date::ticks_from_civil(2024, 1, 5, 10, 20, 30, 123))), "2024-01-05 10:20:30.123");
        assert_eq!(display(&err("boom")), "Error: boom");
        assert_eq!(display(&repr(&int(5))), "int");
        assert_eq!(display(&to_str(&int(5))), "5");
    }

    #[test]
    fn string_index_and_slice_use_utf16_units() {
        // escapes keep the source ASCII-only: the two Hangul syllables and the
        // surrogate pair of the emoji
        let s = str("\u{D55C}\u{ADF8}\u{1F600}");
        // Rust stores `Str` as UTF-8, so a slice that cuts a surrogate pair cannot
        // keep the lone surrogate. It becomes U+FFFD here, while .NET keeps the lone
        // surrogate in the `string` and prints it as '?'. Documented divergence, so
        // the tests compare the UTF-16 unit sequences of the lossy result.
        assert!(matches!(payload(&unary(&s, UnOp::Len)), Payload::Int(4)));
        assert_eq!(display(&get_item(&s, &int(0))), "한");
        // `s[-1]` is normalized to index 3, the low surrogate of the emoji
        assert_eq!(units(&get_item(&s, &int(-1))), [0xFFFD]);
        assert_eq!(message(&get_item(&s, &int(9))), "Error: list index out of range");
        assert_eq!(units(&slice(&s, Some(int(1)), Some(int(3)), None)), [0xADF8, 0xFFFD]);
        assert_eq!(units(&slice(&s, Some(int(2)), Some(int(4)), None)), [0xD83D, 0xDE00]);
        assert_eq!(units(&slice(&s, None, None, Some(int(2)))), [0xD55C, 0xFFFD]);
        assert_eq!(
            message(&slice(&s, None, None, Some(int(0)))),
            "Error: slice step cannot be zero"
        );
        assert_eq!(message(&slice(&s, Some(float(1.0)), None, None)), "Error: slice start must be int");
        assert_eq!(display(&slice(&str("abc"), Some(int(1)), Some(int(2)), Some(int(-1)))), "");
        assert_eq!(display(&slice(&str("abc"), Some(int(2)), Some(int(0)), None)), "");
    }

    #[test]
    fn string_add_sub_and_compare() {
        assert_eq!(display(&binop(&str("a"), BinOp::Add, &int(1))), "a1");
        assert_eq!(display(&binop(&int(1), BinOp::Add, &str("a"))), "1a");
        // `Str.Sub` removes every occurrence (`Replace(old, "")`), so "abab" - "ab" is empty
        assert_eq!(display(&binop(&str("abab"), BinOp::Sub, &str("ab"))), "");
        assert_eq!(display(&binop(&str("aXb"), BinOp::Sub, &str("X"))), "ab");
        // `Str.Add` calls `other.ToStr()`, so a bool operand concatenates as "true"
        assert_eq!(display(&binop(&str("a"), BinOp::Add, &boolean(true))), "atrue");
        assert_eq!(display(&binop(&str("a"), BinOp::Add, &none())), "anone");
        assert_eq!(display(&binop(&str("aXb"), BinOp::Sub, &boolean(true))), "aXb");
        // `cannot add 'T' to 'Type'` needs a ToStr() result that is neither Str nor Err
        assert_eq!(
            message(&binop(&str("a"), BinOp::Add, &obj_new("obj", Payload::Plain))),
            "Error: unsupported operand type(s) for str(): 'obj'"
        );
        assert!(matches!(payload(&binop(&str("a"), BinOp::Eq, &none())), Payload::Bool(false)));
        // `Str.NEq` reuses the `==` wording, a reference quirk
        assert_eq!(
            message(&binop(&str("a"), BinOp::NEq, &int(1))),
            "Error: unsupported operand type(s) for ==: 'str' and 'int'"
        );
        assert_eq!(
            message(&binop(&int(1), BinOp::NEq, &boolean(true))),
            "Error: unsupported operand type(s) for !=: 'int' and 'bool'"
        );
    }

    #[test]
    fn interning_is_observable() {
        assert!(Rc::ptr_eq(&int(100), &int(100)));
        assert!(!Rc::ptr_eq(&int(1000), &int(1000)));
        assert!(Rc::ptr_eq(&int(1), &int(1)));
        assert!(Rc::ptr_eq(&boolean(true), &boolean(true)));
        assert!(Rc::ptr_eq(&none(), &none()));
        assert!(Rc::ptr_eq(&str("abc"), &str("abc")));
        assert!(!Rc::ptr_eq(&str_new("abc"), &str("abc")));
        // the pool is keyed by UTF-16 length, and long strings bypass it
        let long = "x".repeat(1001);
        assert!(!Rc::ptr_eq(&str(&long), &str(&long)));
    }

    #[test]
    fn none_and_type_identity() {
        assert!(matches!(payload(&binop(&none(), BinOp::Eq, &none())), Payload::Bool(true)));
        assert!(matches!(payload(&binop(&int(1), BinOp::Eq, &none())), Payload::Bool(false)));
        assert!(matches!(payload(&binop(&int(1), BinOp::NEq, &none())), Payload::Bool(true)));
        // `Bool.Eq` never fails: any non-boolean compares false
        assert!(matches!(payload(&binop(&boolean(true), BinOp::Eq, &int(1))), Payload::Bool(false)));
        assert_eq!(message(&binop(&boolean(true), BinOp::Gt, &int(1))), "Error: unsupported operand type(s) for >: 'bool' and 'int'");
    }

    #[test]
    fn comparison_falls_back_to_lt_then_eq() {
        let a = int(1);
        let b = int(2);
        assert!(matches!(payload(&binop(&a, BinOp::GtEq, &b)), Payload::Bool(false)));
        assert!(matches!(payload(&binop(&b, BinOp::GtEq, &a)), Payload::Bool(true)));
        assert!(matches!(payload(&binop(&a, BinOp::LtEq, &a)), Payload::Bool(true)));
        assert_eq!(message(&binop(&boolean(true), BinOp::LtEq, &int(1))), "Error: unsupported operand type(s) for <=: 'bool' and 'int'");
    }

    #[test]
    fn types_canonicalize_unions() {
        let t = Types::create(&[Types::single("int"), Types::single("str"), Types::single("int")]);
        assert_eq!(t.display(), "int | str");
        let u = Types::create(&[Types::single("any"), Types::single("int")]);
        assert_eq!(u.display(), "any");
        assert_eq!(Types::create(&[]).display(), "none");
        assert!(Types::create(&[Types::single("int")]).contains(&Types::single("int")));
    }

    #[test]
    fn attribute_lookup_follows_the_member_super_chain() {
        let o = obj_new("myclass", Payload::Plain);
        set_attr(&o, "x", &int(7));
        assert!(matches!(payload(&get_attr(&o, "x")), Payload::Int(7)));
        assert_eq!(message(&get_attr(&o, "y")), "Error: 'myclass' object has no attribute 'y'");
        // assigning on a primitive type name fails with the same wording
        assert_eq!(
            message(&set_attr(&int(5), "x", &int(1))),
            "Error: 'int' object has no attribute 'x'"
        );
        assert!(has(&o, "x"));
        assert!(!has(&o, "y"));
    }
}
