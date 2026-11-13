use std::cell::RefCell;
use std::rc::Rc;
use crate::runtime::collections;
use crate::runtime::collections::KeyMap;
use crate::runtime::date;
use crate::runtime::function::{self, FieldCell, FnCell, PropCell};
use crate::runtime::iters::{self, IterCell};
use crate::runtime::prims;
use crate::runtime::types::Types;
use crate::runtime::world;
pub type Obj = Rc<RefCell<ObjData>>;
#[derive(Default)]
pub struct StopwatchState {

    pub elapsed: i64,

    pub started: i64,
    pub running: bool,
    pub lap: i64,
}
#[derive(Clone)]
pub enum Payload {    Plain,

    Error { header: String, message: String },

    Panic { name: String, message: String },
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(Rc<str>),
    Date(i64),

    Stopwatch(Rc<RefCell<StopwatchState>>),

    Future(Rc<RefCell<crate::native::flow::FutureState>>),

    Pool(Rc<RefCell<crate::native::flow::PoolState>>),

    Lock(Rc<RefCell<crate::native::flow::LockState>>),

    AtomicInt(Rc<RefCell<i64>>),

    AtomicBool(Rc<RefCell<bool>>),

    Stream(Rc<RefCell<crate::native::stream::StreamState>>),

    Random(Rc<RefCell<crate::native::random::Rng>>),

    HttpClient(Rc<RefCell<crate::native::net::HttpState>>),

    WebSocket(Rc<RefCell<crate::native::net::WsState>>),

    List(Rc<RefCell<Vec<Obj>>>),
    Dict(Rc<RefCell<KeyMap>>),
    Set(Rc<RefCell<crate::runtime::collections::KeyMap>>),
    Tup(Rc<Vec<Obj>>, Rc<Vec<String>>),
    Spread(Rc<Vec<Obj>>),
    Json(Obj),
    Stru(Rc<Vec<String>>),
    Enu(i64),
    TObj(Types),

    Iter(IterCell),

    Func(FnCell),

    Property(PropCell),

    FieldRef(FieldCell),
}
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
            Payload::Stopwatch(_) => f.write_str("Stopwatch"),
            Payload::Stream(_) => f.write_str("Stream"),
            Payload::Random(_) => f.write_str("Random"),
            Payload::HttpClient(_) => f.write_str("HttpClient"),
            Payload::WebSocket(_) => f.write_str("WebSocket"),
            Payload::Future(_) => f.write_str("Future"),
            Payload::Pool(_) => f.write_str("Pool"),
            Payload::Lock(_) => f.write_str("Lock"),
            Payload::AtomicInt(_) => f.write_str("AtomicInt"),
            Payload::AtomicBool(_) => f.write_str("AtomicBool"),
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
            Payload::Func(c) => write!(
                f,
                "Func({} args)",
                c.borrow().args.borrow().len()
            ),
            Payload::Property(p) => write!(f, "Property({})", p.borrow().name),
            Payload::FieldRef(fr) => {
                let fr = fr.borrow();
                match function::prop_cell(&fr.prop) {
                    Some(p) => write!(f, "FieldRef({})", p.borrow().name),
                    None => f.write_str("FieldRef(<property>)"),
                }
            }
        }
    }
}
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
            (Random(a), Random(b)) => Rc::ptr_eq(a, b),
            (HttpClient(a), HttpClient(b)) => Rc::ptr_eq(a, b),
            (WebSocket(a), WebSocket(b)) => Rc::ptr_eq(a, b),
            (AtomicInt(a), AtomicInt(b)) => Rc::ptr_eq(a, b),
            (AtomicBool(a), AtomicBool(b)) => Rc::ptr_eq(a, b),
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
pub fn key_eq(a: &Obj, b: &Obj) -> bool {
    let pa = payload(a);
    let pb = payload(b);
    match (&pa, &pb) {
        (Payload::Int(x), Payload::Int(y)) => x == y,
        (Payload::Float(x), Payload::Float(y)) => x == y,
        (Payload::Str(x), Payload::Str(y)) => x == y,
        (Payload::Bool(x), Payload::Bool(y)) => x == y,
        (Payload::Enu(x), Payload::Enu(y)) => x == y,
        _ => matches!(payload(&binop(a, BinOp::Eq, b)), Payload::Bool(true)),
    }
}
pub fn hash_code(o: &Obj) -> u64 {
    match payload(o) {
        Payload::Int(i) => i as u64,
        Payload::Str(s) => prims::str_hash(&s) as u64,
        Payload::Float(v) => v.to_bits(),
        Payload::Bool(b) => b as u64,
        Payload::Enu(n) => n as u64,

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

        Payload::Json(inner) => hash_code(&inner),

        _ => prims::str_hash(&type_name(o)) as u64,
    }
}
fn combine_hash(seed: u64, value: u64) -> u64 {
    seed.rotate_left(13) ^ value
}
pub fn types_of(o: &Obj) -> Types {
    o.borrow().types.clone()
}
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
fn has_possible_method(o: &Obj) -> bool {
    let d = o.borrow();
    if !d.members.is_empty() {
        return true;
    }
    match &d.super_ {
        Some(s) => !is_none(s),
        None => false,
    }
}

fn try_method(o: &Obj, name: &str, args: &[Obj]) -> Option<Obj> {
    if !has_possible_method(o) {
        return None;
    }
    let m = member(o, name)?;
    {
        let mut d = m.borrow_mut();
        d.self_ = Some(o.clone());
        d.super_ = o.borrow().super_.clone();
    }
    invoke(&m, args)
}
pub fn invoke(f: &Obj, args: &[Obj]) -> Option<Obj> {
    match payload(f) {
        Payload::Func(..) => Some(function::fn_call(f, &function::Args::positional(args.to_vec()))),
        _ => None,
    }
}
pub fn call(o: &Obj, args: &[Obj]) -> Obj {
    call_args(o, &function::Args::positional(args.to_vec()))
}
pub fn call_args(o: &Obj, args: &function::Args) -> Obj {
    if let Payload::Func(..) = payload(o) {
        return function::fn_call(o, args);
    }
    if let Some(v) = try_method_named(o, "__call__", args) {
        return v;
    }
    if let Some(s) = valid_super(o) {
        return call_args(&s, args);
    }
    if let Payload::TObj(t) = payload(o) {
        return collections::tobj_call(&t, &args.values);
    }
    err(format!("unsupported operand type(s) for (): '{}'", type_name(o)))
}
pub fn invoke_obj(f: &Obj, args: &function::Args) -> Obj {
    match payload(f) {
        Payload::Func(..) => function::fn_call(f, args),
        _ => err(format!(
            "unsupported operand type(s) for (): '{}'",
            type_name(f)
        )),
    }
}
pub fn try_method_named(o: &Obj, name: &str, args: &function::Args) -> Option<Obj> {
    if !has_possible_method(o) {
        return None;
    }
    let m = member(o, name)?;
    {
        let mut d = m.borrow_mut();
        d.self_ = Some(o.clone());
        d.super_ = o.borrow().super_.clone();
    }
    invoke_args(&m, args)
}
pub fn invoke_args(f: &Obj, args: &function::Args) -> Option<Obj> {
    match payload(f) {
        Payload::Func(..) => Some(function::fn_call(f, args)),
        _ => None,
    }
}
pub fn init(o: &Obj, args: &[Obj]) -> Obj {

    match payload(o) {
        Payload::List(..) => return collections::list_init(o, args),
        Payload::Dict(..) => return collections::dict_init(o, args),
        Payload::Set(..) => return collections::set_init(o, args),
        Payload::Tup(..) => return collections::tup_init(o, args),
        Payload::Json(..) => return collections::json_init(o, args),
        Payload::Enu(n) => return collections::enu_init(o, n, args),
        Payload::Stru(_) => return collections::stru_init(o, args),
        Payload::Iter(st) => return iters::iter_init(o, &st, args),
        Payload::Int(_) | Payload::Float(_) | Payload::Bool(_) | Payload::Str(_) | Payload::Date(_) => {
            return prim_init(o, args);
        }

        Payload::AtomicInt(_) => return crate::native::atomic::atomic_int_init(o, args),
        Payload::AtomicBool(_) => return crate::native::atomic::atomic_bool_init(o, args),
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
fn prim_init(o: &Obj, args: &[Obj]) -> Obj {
    let t = type_name(o);
    match payload(o) {

        Payload::Int(_) => match args.len() {
            0 => int(0),
            1 => to_int(&args[0]),
            2 => {
                if let (Payload::Str(s), Payload::Int(b)) = (payload(&args[0]), payload(&args[1])) {
                    return parse_with_base(&s, b);
                }
                err("int: expected (str, int) for base conversion")
            }
            _ => err(format!("cannot convert to '{t}'")),
        },
        Payload::Float(_) => match args.len() {
            0 => float(0.0),
            1 => to_float(&args[0]),
            _ => err(format!("cannot convert to '{t}'")),
        },

        Payload::Bool(_) => match args.len() {
            0 => boolean(false),
            1 => to_bool(&args[0]),
            _ => boolean(false),
        },
        Payload::Str(_) => match args.len() {
            0 => str(""),
            1 => to_str(&args[0]),
            2 => {
                if let (Payload::Str(s), Payload::Int(i)) = (payload(&args[0]), payload(&args[1])) {

                    let n = i.min(i32::MAX as i64);
                    let text = if n > 0 { s.repeat(n as usize) } else { String::new() };
                    return str(&text);
                }
                err("too many arguments")
            }
            _ => err("too many arguments"),
        },

        Payload::Date(_) => match args.len() {
            0 => date_obj(0),
            1 => match payload(&args[0]) {
                Payload::Str(s) => match crate::runtime::date::parse(s.as_ref()) {
                    Some(ticks) => date_obj(ticks),
                    None => err("invalid date str"),
                },
                _ => err(format!("cannot convert '{}' to 'date'", type_name(&args[0]))),
            },
            _ => err("cannot convert to 'date'"),
        },
        _ => o.clone(),
    }
}
fn parse_with_base(text: &str, base: i64) -> Obj {
    let base = base as i32;
    if base < 2 || base > 36 {
        return err("int: base must be between 2 and 36");
    }
    let mut text = text.trim().to_string();
    let lower = text.to_lowercase();
    if base == 2 && lower.starts_with("0b") {
        text = text[2..].to_string();
    } else if base == 8 && lower.starts_with("0o") {
        text = text[2..].to_string();
    } else if base == 16 && lower.starts_with("0x") {
        text = text[2..].to_string();
    }
    let mut sign = 1i64;
    if text.starts_with('-') {
        sign = -1;
        text = text[1..].to_string();
    } else if text.starts_with('+') {
        text = text[1..].to_string();
    }
    if text.is_empty() {
        return err(format!("int: invalid literal for base {base}"));
    }
    match i64::from_str_radix(&text, base as u32) {
        Ok(v) => int(sign * v),
        Err(_) => err("int: Input string was not in a correct format."),
    }
}
pub fn get_attr(o: &Obj, name: &str) -> Obj {
    get_attr_from(o, name, o)
}
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
fn is_bound_fn(o: &Obj) -> bool {
    function::is_func(o)
}
pub fn is_property(o: &Obj) -> bool {
    function::is_property(o)
}
fn property_get(member: &Obj, receiver: &Obj) -> Obj {
    function::prop_get(member, receiver)
}
pub fn property_set(member: &Obj, receiver: &Obj, value: &Obj) -> Obj {
    function::prop_set(member, receiver, value.clone())
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

        Payload::Set(v) => boolean(v.borrow().contains_key(other)),
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

    match payload(o) {
        Payload::Func(_) => return str(&function::fn_repr(o)),
        Payload::Property(_) => return str(&function::prop_repr(o)),
        _ => {}
    }
    str(&type_name(o))
}
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

        Payload::Int(v) => return int(v),
        Payload::Float(v) => return float(v),
        Payload::Bool(b) => return boolean(b),
        Payload::Str(s) => return str_obj(s, world::str_type(), world::obj_type()),
        Payload::Date(t) => return date_obj(t),
        Payload::List(v) => return collections::list_clone(&v),
        Payload::Dict(km) => return collections::dict_clone(&km),
        Payload::Set(s) => return collections::set_clone(&s),
        Payload::Tup(vals, names) => return collections::tup_clone(&vals, &names),
        Payload::Json(inner) => return collections::json_clone(&inner),
        Payload::Stru(_) => return collections::stru_clone(o),
        Payload::Enu(n) => return collections::enu_clone(o, n),

        Payload::Spread(v) => return collections::spread(v.to_vec()),
        Payload::TObj(t) => return collections::tobj(t),
        Payload::Iter(st) => return iters::iter_clone(o, &st),

        Payload::Property(_) | Payload::FieldRef(_) => return o.clone(),
        Payload::HttpClient(_) | Payload::WebSocket(_) => return o.clone(),

        Payload::Future(_) => return o.clone(),

        Payload::Pool(_) => return err("'pool' cannot be cloned"),
        Payload::Lock(_) => return err("'lock' cannot be cloned"),
        Payload::AtomicInt(_) => return err("'atomic_int' cannot be cloned"),
        Payload::AtomicBool(_) => return err("'atomic_bool' cannot be cloned"),
        Payload::Func(_) => return function::fn_clone(o),
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
pub fn unpack_items(o: &Obj) -> Vec<Obj> {
    match payload(o) {
        Payload::Tup(v, _) => v.to_vec(),
        Payload::List(v) => v.borrow().clone(),
        Payload::Spread(v) => v.to_vec(),
        _ => {
            let s = spread(o);
            match payload(&s) {
                Payload::Spread(v) => v.to_vec(),
                _ => vec![err(format!("cannot unpack {}", class_name(o)))],
            }
        }
    }
}
fn is_dotnet_infinity(text: &str) -> bool {
    let text = text.trim().to_ascii_lowercase();
    matches!(
        text.as_str(),
        "infinity" | "+infinity" | "-infinity" | "∞" | "+∞" | "-∞"
    )
}
pub fn class_name(o: &Obj) -> &'static str {
    match payload(o) {
        Payload::Plain => "Obj",
        Payload::Error { .. } => "Err",
        Payload::Panic { .. } => "Panic",
        Payload::Int(..) => "Int",
        Payload::Float(..) => "Float",
        Payload::Bool(..) => "Bool",
        Payload::Str(..) => "Str",
        Payload::Stopwatch(..) => "Stopwatch",
        Payload::Stream(..) => "Stream",
        Payload::Random(..) => "Random",
        Payload::HttpClient(..) => "http_client",
        Payload::WebSocket(..) => "web_socket",
        Payload::Future(..) => "Future",
        Payload::Pool(..) => "Pool",
        Payload::Lock(..) => "Lock",
        Payload::Date(..) => "Date",
        Payload::List(..) => "List",
        Payload::Dict(..) => "Dict",
        Payload::Set(..) => "Set",
        Payload::Tup(..) => "Tup",
        Payload::Spread(..) => "Spreads",
        Payload::Json(..) => "Json",
        Payload::Stru(..) => "Stru",
        Payload::Enu(..) => "Enum",
        Payload::TObj(..) => "TObj",
        Payload::AtomicInt(..) => "AtomicInt",
        Payload::AtomicBool(..) => "AtomicBool",
        Payload::Iter(..) => "Iters",
        Payload::Func(..) => "Fn",
        Payload::Property(..) => "Property",
        Payload::FieldRef(..) => "FieldRef",
    }
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
        Payload::Func(_) => str(&function::fn_to_str(o)),

        Payload::WebSocket(state) => str(&format!("<socket {}>", state.borrow().state)),
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
pub fn range_to(o: &Obj, other: &Obj) -> Obj {
    if let Some(v) = try_method(o, "__range__", &[other.clone()]) {
        return v;
    }
    if let Some(s) = valid_super(o) {
        return range_to(&s, other);
    }
    if range_bound(o).is_some() && range_bound(other).is_some() {

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
        Payload::Str(a) => str_binop(o, a, op, other, &t, &ot),
        Payload::Bool(a) => bool_binop(a, op, other, &t, &ot),
        Payload::Date(a) => date_binop(a, op, other, &t, &ot),
        Payload::List(..) => collections::list_binop(o, op, other, &t, &ot),
        Payload::Tup(..) => collections::tup_binop(o, op, other, &t, &ot),
        Payload::Set(..) => collections::set_binop(o, op, other, &t, &ot),
        Payload::Stru(..) => collections::stru_binop(o, op, other, &t, &ot),
        Payload::Enu(n) => collections::enu_binop(o, n, op, other, &t, &ot),
        Payload::TObj(ty) => collections::tobj_binop(&ty, op, other, &t, &ot),
        Payload::Iter(st) => iters::iter_binop(o, &st, op, other, &t, &ot),

        Payload::Json(inner) => binop(&inner, op, other),
        Payload::Dict(..) | Payload::Spread(..) => base_binop(o, op, other, &t, &ot),
        Payload::Error { .. } | Payload::Panic { .. } | Payload::Plain => base_binop(o, op, other, &t, &ot),

        Payload::Func(_)
        | Payload::Property(_)
        | Payload::FieldRef(_)
        | Payload::Stopwatch(_)
        | Payload::Future(_)
        | Payload::Pool(_)
        | Payload::Lock(_)
        | Payload::Stream(_)
        | Payload::Random(_) => base_binop(o, op, other, &t, &ot),

        Payload::AtomicInt(_) | Payload::AtomicBool(_) => base_binop(o, op, other, &t, &ot),

        Payload::HttpClient(state) => match op {
            BinOp::Eq => boolean(matches!(payload(other), Payload::HttpClient(o) if Rc::ptr_eq(&state, &o))),
            BinOp::NEq => boolean(!matches!(payload(other), Payload::HttpClient(o) if Rc::ptr_eq(&state, &o))),
            _ => base_binop(o, op, other, &t, &ot),
        },
        Payload::WebSocket(state) => match op {
            BinOp::Eq => boolean(matches!(payload(other), Payload::WebSocket(o) if Rc::ptr_eq(&state, &o))),
            BinOp::NEq => boolean(!matches!(payload(other), Payload::WebSocket(o) if Rc::ptr_eq(&state, &o))),
            _ => base_binop(o, op, other, &t, &ot),
        },
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

            UnOp::ToInt => match a.trim().parse::<i64>() {
                Ok(v) => int(v),
                Err(_) => err(format!("cannot convert '{a}' to 'int'")),
            },
            UnOp::ToFloat => match a.trim().parse::<f64>() {

                Ok(v) if v.is_infinite() && !is_dotnet_infinity(&a) => {
                    err(format!("cannot convert '{a}' to 'float'"))
                }
                Ok(v) => float(v),
                Err(_) => err(format!("cannot convert '{a}' to 'float'")),
            },
            UnOp::ToBool => match a.parse::<bool>() {
                Ok(v) => boolean(v),
                Err(_) => boolean(!a.is_empty()),
            },
            UnOp::Hash => int(prims::str_hash(&a) as i64),

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

        Payload::Pool(state) => match op {
            UnOp::Entry => o.clone(),
            UnOp::Exit => {
                state.borrow_mut().closed = true;
                none()
            }
            _ => err(op.message(&t)),
        },
        Payload::Lock(state) => match op {
            UnOp::Entry => o.clone(),
            UnOp::Exit => {
                state.borrow_mut().held = false;
                none()
            }
            _ => err(op.message(&t)),
        },

        Payload::WebSocket(state) => match op {
            UnOp::ToBool => boolean(state.borrow().state == "Open"),
            _ => err(op.message(&t)),
        },

        Payload::AtomicInt(_) | Payload::AtomicBool(_) => match op {
            UnOp::Entry => o.clone(),
            _ => err(op.message(&t)),
        },
        Payload::TObj(..)
        | Payload::Func(_)
        | Payload::Property(_)
        | Payload::FieldRef(_)
        | Payload::Stopwatch(_)
        | Payload::Future(_)
        | Payload::Stream(_)
        | Payload::Random(_)
        | Payload::HttpClient(_) => err(op.message(&t)),
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
fn str_binop(o: &Obj, a: Rc<str>, op: BinOp, other: &Obj, t: &Rc<str>, ot: &Rc<str>) -> Obj {
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

                _ => unsupported("==", t, ot),
            }
        }
        BinOp::Lt => match payload(other) {
            Payload::Str(v) => boolean(a < v),
            _ => unsupported("<", t, ot),
        },

        _ => base_binop(o, op, other, t, ot),
    }
}
fn bool_binop(a: bool, op: BinOp, other: &Obj, t: &Rc<str>, ot: &Rc<str>) -> Obj {
    match op {

        BinOp::Eq => boolean(matches!(payload(other), Payload::Bool(v) if v == a)),
        _ => base_binop_of(op, t, ot),
    }
}
fn date_binop(a: i64, op: BinOp, other: &Obj, t: &Rc<str>, ot: &Rc<str>) -> Obj {
    match op {
        BinOp::Add => match payload(other) {

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

    fn units(o: &Obj) -> Vec<u16> {
        match payload(o) {
            Payload::Str(s) => s.encode_utf16().collect(),
            _ => Vec::new(),
        }
    }

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

        let s = str("\u{D55C}\u{ADF8}\u{1F600}");

        assert!(matches!(payload(&unary(&s, UnOp::Len)), Payload::Int(4)));
        assert_eq!(display(&get_item(&s, &int(0))), "한");

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

        assert_eq!(display(&binop(&str("abab"), BinOp::Sub, &str("ab"))), "");
        assert_eq!(display(&binop(&str("aXb"), BinOp::Sub, &str("X"))), "ab");

        assert_eq!(display(&binop(&str("a"), BinOp::Add, &boolean(true))), "atrue");
        assert_eq!(display(&binop(&str("a"), BinOp::Add, &none())), "anone");
        assert_eq!(display(&binop(&str("aXb"), BinOp::Sub, &boolean(true))), "aXb");

        assert_eq!(
            message(&binop(&str("a"), BinOp::Add, &obj_new("obj", Payload::Plain))),
            "Error: unsupported operand type(s) for str(): 'obj'"
        );
        assert!(matches!(payload(&binop(&str("a"), BinOp::Eq, &none())), Payload::Bool(false)));

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

        let long = "x".repeat(1001);
        assert!(!Rc::ptr_eq(&str(&long), &str(&long)));
    }
    #[test]
    fn none_and_type_identity() {
        assert!(matches!(payload(&binop(&none(), BinOp::Eq, &none())), Payload::Bool(true)));
        assert!(matches!(payload(&binop(&int(1), BinOp::Eq, &none())), Payload::Bool(false)));
        assert!(matches!(payload(&binop(&int(1), BinOp::NEq, &none())), Payload::Bool(true)));

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

        assert_eq!(
            message(&set_attr(&int(5), "x", &int(1))),
            "Error: 'int' object has no attribute 'x'"
        );
        assert!(has(&o, "x"));
        assert!(!has(&o, "y"));
    }
}
