//! `Fn` / `LFn` / `NFn` / `Property` / `FieldRef` — the callable object model of
//! `Un.Object.Function` and `Un.Object.Property`.
//!
//! `Fn` is an `Obj` payload, so `self` / `super` / `members` live on the `ObjData` cell
//! exactly as in the reference. `LFn` carries an AST body and needs the VM executor hook
//! (`world::set_executor`), `NFn` carries a host closure. The `Args` list is shared by
//! `Clone()` (the reference assigns the same `List<Arg>`), so it lives in its own cell.

use std::cell::RefCell;
use std::rc::Rc;

use crate::ast::Node;
use crate::runtime::collections;
use crate::runtime::context::{self, Context};
use crate::runtime::obj::{self, Obj, ObjData, Payload};
use crate::runtime::scope::{self, Scope};
use crate::runtime::types::Types;
use crate::runtime::world;
use crate::source::Source;

/// `Global.MAXRECURSIONDEPTH`
pub const MAX_RECURSION_DEPTH: usize = 1000;

/// `Arg` — the parameter record `Fn.GetArgs` builds from a parameter node.
#[derive(Clone)]
pub struct ArgData {
    pub name: String,
    pub type_name: Types,
    pub essential: bool,
    pub optional: bool,
    pub positional: bool,
    pub keyword: bool,
    /// `Arg.DefaultValue` — the `null` object for essential parameters, an empty `Tup`
    /// for `*args` and an empty `Dict` for `**kwargs`.
    pub default: Option<Obj>,
}

impl ArgData {
    /// a plain essential parameter, the shape `CreateAccessor` uses for a `set` accessor
    pub fn essential(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            type_name: Types::single("any"),
            essential: true,
            optional: false,
            positional: false,
            keyword: false,
            default: Some(obj::null()),
        }
    }
}

/// the call arguments of `Tup` — values plus the optional keyword names. names are empty
/// strings for positional arguments (the reference uses `string.IsNullOrEmpty`).
pub struct Args {
    pub values: Vec<Obj>,
    pub names: Vec<String>,
}

impl Args {
    pub fn positional(values: Vec<Obj>) -> Self {
        let names = vec![String::new(); values.len()];
        Self { values, names }
    }

    pub fn named(values: Vec<Obj>, names: Vec<String>) -> Self {
        Self { values, names }
    }

    fn count(&self) -> usize {
        self.values.len()
    }
}

/// what a body evaluation hands back to the caller: `ReturnFlow.Value`,
/// `PropagateFlow.Error`, or "the body finished normally".
pub enum Flow {
    Return(Obj),
    Propagate(Obj),
    Normal,
}

pub type FnCell = Rc<RefCell<FnData>>;

pub enum Body {
    /// `LFn` — the AST body of a user function, run through the VM executor hook
    Nodes(Rc<Vec<Node>>),
    /// `NFn` — a host function that receives the bound scope
    Native(Rc<dyn Fn(&Scope) -> Obj>),
}

impl Clone for Body {
    fn clone(&self) -> Self {
        match self {
            Body::Nodes(n) => Body::Nodes(n.clone()),
            Body::Native(f) => Body::Native(f.clone()),
        }
    }
}

pub struct FnData {
    pub name: Option<String>,
    pub args: Rc<RefCell<Vec<ArgData>>>,
    pub return_type: Types,
    pub closure: Context,
    pub property: Option<Obj>,
    pub body: Body,
}

pub fn fn_cell(o: &Obj) -> Option<FnCell> {
    match obj::payload(o) {
        Payload::Func(c) => Some(c),
        _ => None,
    }
}

/// `new LFn(body, closure)`
pub fn lfn(name: Option<String>, args: Vec<ArgData>, return_type: Types, body: Vec<Node>, closure: Context) -> Obj {
    obj::obj_new(
        "func",
        Payload::Func(Rc::new(RefCell::new(FnData {
            name,
            args: Rc::new(RefCell::new(args)),
            return_type,
            closure,
            property: None,
            body: Body::Nodes(Rc::new(body)),
        }))),
    )
}

/// `new NFn(...)` — the reference binds natives to `Global.GetGlobalScope()`, so the
/// default closure is an empty scope with its own frame stack.
pub fn nfn(name: &str, args: Vec<ArgData>, host: Rc<dyn Fn(&Scope) -> Obj>) -> Obj {
    nfn_in(name, args, default_closure(), host)
}

pub fn nfn_in(name: &str, args: Vec<ArgData>, closure: Context, host: Rc<dyn Fn(&Scope) -> Obj>) -> Obj {
    obj::obj_new(
        "func",
        Payload::Func(Rc::new(RefCell::new(FnData {
            name: Some(name.to_owned()),
            args: Rc::new(RefCell::new(args)),
            return_type: Types::single("any"),
            closure,
            property: None,
            body: Body::Native(host),
        }))),
    )
}

pub fn default_closure() -> Context {
    context::new(scope::empty(), Rc::new(Source::new("", "")), context::new_frames())
}

pub fn set_property(fn_obj: &Obj, property: Option<Obj>) {
    if let Some(cell) = fn_cell(fn_obj) {
        cell.borrow_mut().property = property;
    }
}

pub fn bind_self(fn_obj: &Obj, receiver: &Obj) {
    {
        let mut d = fn_obj.borrow_mut();
        d.self_ = Some(receiver.clone());
        d.super_ = receiver.borrow().super_.clone();
    }
}

/// `Fn.Call` on a `Func` payload. the recursion limit is read from the closure's
/// `CallDepth`, which is shared state of the closure cell.
pub fn fn_call(f: &Obj, args: &Args) -> Obj {
    let Some(cell) = fn_cell(f) else {
        return obj::err(format!(
            "unsupported operand type(s) for (): '{}'",
            obj::type_name(f)
        ));
    };

    let closure = cell.borrow().closure.clone();
    let native = matches!(cell.borrow().body, Body::Native(..));
    let depth = context::call_depth(&closure);

    // `NFn.Call` increments before the limit test, `LFn.Call` tests first. both reach
    // the same limit and decrement on every exit path.
    if native {
        context::set_call_depth(&closure, depth + 1);
        if depth >= MAX_RECURSION_DEPTH {
            context::set_call_depth(&closure, depth);
            return obj::err("maximum recursion depth exceeded");
        }
    } else if depth >= MAX_RECURSION_DEPTH {
        return obj::err("maximum recursion depth exceeded");
    }

    let parent = { closure.borrow().scope.clone() };
    let scope = scope::new(Some(parent), true);

    let error = bind(f, &scope, args);
    if !obj::is_none(&error) {
        if native {
            context::set_call_depth(&closure, depth);
        }
        return error;
    }

    if native {
        let host = match &cell.borrow().body {
            Body::Native(g) => g.clone(),
            Body::Nodes(_) => unreachable!(),
        };
        let value = host(&scope);
        context::set_call_depth(&closure, depth);
        return value;
    }

    context::set_call_depth(&closure, depth + 1);

    let (source, frames) = {
        let c = closure.borrow();
        (c.source.clone(), c.frames.clone())
    };
    let ctx = context::new(scope, source, frames);

    let nodes = match &cell.borrow().body {
        Body::Nodes(n) => n.clone(),
        Body::Native(_) => unreachable!(),
    };

    let mut result = match world::execute(&ctx, &nodes) {
        Flow::Return(v) => v,
        Flow::Propagate(e) => e,
        Flow::Normal => obj::none(),
    };

    // `finally`: defers drain LIFO through the evaluator, then usings drain LIFO
    // through `Exit()`. a flow raised inside the drain escapes the reference's
    // `finally` block, so its value becomes the call result.
    while let Some(node) = context::pop_defer(&ctx) {
        match world::execute(&ctx, std::slice::from_ref(&node)) {
            Flow::Return(v) | Flow::Propagate(v) => result = v,
            Flow::Normal => {}
        }
    }

    while let Some(u) = context::pop_using(&ctx) {
        obj::exit(&u);
    }

    context::set_call_depth(&closure, depth);
    result
}

/// `Fn.Bind` — `self` / `super` / `field` locals, then positional and keyword binding.
pub fn bind(f: &Obj, scope: &Scope, args: &Args) -> Obj {
    let Some(cell) = fn_cell(f) else {
        return obj::none();
    };

    let property = cell.borrow().property.clone();
    let self_ = f.borrow().self_.clone().unwrap_or_else(obj::none);
    let super_ = f.borrow().super_.clone().unwrap_or_else(obj::none);

    scope::set_local(scope, "self", self_.clone());
    scope::set_local(scope, "super", super_.clone());

    if let Some(prop) = property {
        scope::set_local(scope, "field", field_ref(&self_, &prop));
    }

    if let Some(result) = try_bind_simple_positional(f, scope, args) {
        return result;
    }

    let arg_list = cell.borrow().args.clone();

    let (values, names) = unpack_args(args);

    let mut positional: Vec<Obj> = Vec::new();
    let mut keyword: Vec<(String, Obj)> = Vec::new();

    for i in 0..values.len() {
        let name = names[i].clone();
        if name.is_empty() {
            positional.push(values[i].clone());
            continue;
        }
        if keyword.iter().any(|(k, _)| *k == name) {
            return obj::err(format!("multiple values for argument '{name}'"));
        }
        keyword.push((name, values[i].clone()));
    }

    let mut pos_index = 0usize;
    let mut star: Option<ArgData> = None;
    let mut kw: Option<ArgData> = None;
    let mut keyword_only = false;

    {
        let list = arg_list.borrow();
        for arg in list.iter() {
            if arg.name == "self" || arg.name == "super" {
                continue;
            }

            if arg.positional {
                star = Some(arg.clone());
                keyword_only = true;
                continue;
            }

            if arg.keyword {
                kw = Some(arg.clone());
                continue;
            }

            let value = if !keyword_only && pos_index < positional.len() {
                let v = positional[pos_index].clone();
                pos_index += 1;
                v
            } else if let Some(idx) = keyword.iter().position(|(k, _)| *k == arg.name) {
                keyword.remove(idx).1
            } else if !arg.essential {
                arg.default.clone().unwrap_or_else(obj::none)
            } else {
                return obj::err(format!("missing required argument '{}'", arg.name));
            };

            scope::set_local(scope, &arg.name, value);
        }
    }

    match star {
        Some(s) => {
            let rest = positional[pos_index..].to_vec();
            scope::set_local(scope, &s.name, collections::tup(rest, Vec::new()));
        }
        None => {
            if pos_index < positional.len() {
                return obj::err("too many positional arguments");
            }
        }
    }

    match kw {
        Some(k) => {
            let entries: Vec<(Obj, Obj)> =
                keyword.into_iter().map(|(name, v)| (obj::str(&name), v)).collect();
            scope::set_local(scope, &k.name, collections::dict(entries));
        }
        None => {
            if !keyword.is_empty() {
                return obj::err(format!("unexpected keyword argument '{}'", keyword[0].0));
            }
        }
    }

    obj::none()
}

/// `Fn.TryBindSimplePositional` — the fast path for calls that are purely positional
/// against a parameter list with no `*` / `**` parameters. `None` means the fast path
/// does not apply and the general binding runs.
fn try_bind_simple_positional(f: &Obj, scope: &Scope, args: &Args) -> Option<Obj> {
    for i in 0..args.count() {
        if !args.names[i].is_empty() || matches!(obj::payload(&args.values[i]), Payload::Spread(..)) {
            return None;
        }
    }

    let arg_list = match fn_cell(f) {
        Some(c) => c.borrow().args.clone(),
        None => return None,
    };

    let mut bindable = 0usize;

    {
        let list = arg_list.borrow();
        for arg in list.iter() {
            if arg.name == "self" || arg.name == "super" {
                continue;
            }

            if arg.positional || arg.keyword {
                return None;
            }

            if bindable < args.count() {
                scope::set_local(scope, &arg.name, args.values[bindable].clone());
                bindable += 1;
                continue;
            }

            if !arg.essential {
                scope::set_local(scope, &arg.name, arg.default.clone().unwrap_or_else(obj::none));
                bindable += 1;
                continue;
            }

            return Some(obj::err(format!("missing required argument '{}'", arg.name)));
        }
    }

    if args.count() > bindable {
        return Some(obj::err("too many positional arguments"));
    }

    Some(obj::none())
}

/// `Fn.UnpackArgs` — a `Spreads` argument item is flattened, and each unpacked value
/// inherits the name of the argument it came from.
pub fn unpack_args(args: &Args) -> (Vec<Obj>, Vec<String>) {
    let mut values: Vec<Obj> = Vec::new();
    let mut names: Vec<String> = Vec::new();

    for i in 0..args.count() {
        match obj::payload(&args.values[i]) {
            Payload::Spread(items) => {
                for v in items.iter() {
                    values.push(v.clone());
                    names.push(args.names[i].clone());
                }
            }
            _ => {
                values.push(args.values[i].clone());
                names.push(args.names[i].clone());
            }
        }
    }

    (values, names)
}

/// `Fn.Repr()` — `fn name(x: any) -> any`
pub fn fn_repr(f: &Obj) -> String {
    let cell = fn_cell(f).expect("func payload");
    let list = cell.borrow().args.clone();
    let list = list.borrow();
    let args = list
        .iter()
        .map(|a| format!("{}: {}", a.name, a.type_name.display()))
        .collect::<Vec<_>>()
        .join(", ");
    let ret = cell.borrow().return_type.display().to_string();
    let name = cell.borrow().name.clone().unwrap_or_default();
    format!("fn {name}({args}) -> {ret}")
}

/// `Fn.ToStr()` — `fn(any) -> any`
pub fn fn_to_str(f: &Obj) -> String {
    let cell = fn_cell(f).expect("func payload");
    let list = cell.borrow().args.clone();
    let list = list.borrow();
    let args = list
        .iter()
        .map(|a| a.type_name.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let ret = cell.borrow().return_type.display().to_string();
    format!("fn({args}) -> {ret}")
}

/// `LFn.Clone()` / `NFn.Clone()` — a new cell over the same closure and the same
/// `Args` list, the way the reference shares the list object.
pub fn fn_clone(f: &Obj) -> Obj {
    let Some(cell) = fn_cell(f) else {
        return obj::plain(obj::type_name(f), world::obj_type());
    };

    let data = {
        let c = cell.borrow();
        FnData {
            name: c.name.clone(),
            args: c.args.clone(),
            return_type: c.return_type.clone(),
            closure: c.closure.clone(),
            property: c.property.clone(),
            body: c.body.clone(),
        }
    };

    let src = f.borrow();
    let created = ObjData {
        type_name: src.type_name.clone(),
        declared: src.declared.clone(),
        types: src.types.clone(),
        self_: src.self_.clone(),
        super_: src.super_.clone(),
        parents: src.parents.clone(),
        members: src.members.clone(),
        annotations: src.annotations.clone(),
        payload: Payload::Func(Rc::new(RefCell::new(data))),
    };

    Rc::new(RefCell::new(created))
}

pub fn is_func(o: &Obj) -> bool {
    matches!(obj::payload(o), Payload::Func(..))
}

// --- Property / FieldRef ----------------------------------------------------------------

pub type PropCell = Rc<RefCell<PropData>>;

pub struct PropData {
    pub name: String,
    /// `Property.BackingKey` — the member slot the value is stored in
    pub backing: String,
    pub default: Obj,
    pub getter: Option<Obj>,
    pub setter: Option<Obj>,
}

/// `new Property(name)` — the reference types it as `UnType.Obj`
pub fn property(name: &str) -> Obj {
    obj::obj_new(
        "obj",
        Payload::Property(Rc::new(RefCell::new(PropData {
            name: name.to_owned(),
            backing: format!("${name}"),
            default: obj::none(),
            getter: None,
            setter: None,
        }))),
    )
}

pub fn prop_cell(o: &Obj) -> Option<PropCell> {
    match obj::payload(o) {
        Payload::Property(p) => Some(p),
        _ => None,
    }
}

pub fn set_prop_default(prop: &Obj, value: Obj) {
    if let Some(cell) = prop_cell(prop) {
        cell.borrow_mut().default = value;
    }
}

pub fn set_prop_getter(prop: &Obj, getter: Obj) {
    if let Some(cell) = prop_cell(prop) {
        cell.borrow_mut().getter = Some(getter);
    }
}

pub fn set_prop_setter(prop: &Obj, setter: Obj) {
    if let Some(cell) = prop_cell(prop) {
        cell.borrow_mut().setter = Some(setter);
    }
}

/// `Property.Get(instance)`
pub fn prop_get(prop: &Obj, instance: &Obj) -> Obj {
    let cell = prop_cell(prop).expect("property payload");
    let getter = cell.borrow().getter.clone();

    if let Some(getter) = getter {
        accessor_bind(&getter, instance, prop);
        return fn_call(&getter, &Args::positional(Vec::new()));
    }

    prop_read(prop, instance)
}

/// `Property.Set(instance, value)`
pub fn prop_set(prop: &Obj, instance: &Obj, value: Obj) -> Obj {
    let cell = prop_cell(prop).expect("property payload");
    let setter = cell.borrow().setter.clone();

    if let Some(setter) = setter {
        accessor_bind(&setter, instance, prop);
        return fn_call(&setter, &Args::positional(vec![value]));
    }

    prop_write(prop, instance, &value);
    value
}

fn accessor_bind(accessor: &Obj, instance: &Obj, prop: &Obj) {
    bind_self(accessor, instance);
    set_property(accessor, Some(prop.clone()));
}

/// `Property.Read` — lazily materialises the backing member from `Default.Clone()`
pub fn prop_read(prop: &Obj, instance: &Obj) -> Obj {
    let cell = prop_cell(prop).expect("property payload");
    let backing = cell.borrow().backing.clone();
    let default = cell.borrow().default.clone();

    if let Some(v) = obj::member(instance, &backing) {
        return v;
    }

    let initial = obj::clone_obj(&default);
    obj::member_set(instance, &backing, &initial);
    initial
}

/// `Property.Write`
pub fn prop_write(prop: &Obj, instance: &Obj, value: &Obj) {
    let cell = prop_cell(prop).expect("property payload");
    let backing = cell.borrow().backing.clone();
    obj::member_set(instance, &backing, value);
}

pub fn prop_name(prop: &Obj) -> String {
    prop_cell(prop).map(|c| c.borrow().name.clone()).unwrap_or_default()
}

/// `Property.Repr()` — `<property name>`, `Clone` / `Copy` are `this`
pub fn prop_repr(prop: &Obj) -> String {
    format!("<property {}>", prop_name(prop))
}

pub fn is_property(o: &Obj) -> bool {
    matches!(obj::payload(o), Payload::Property(..))
}

pub type FieldCell = Rc<RefCell<FieldData>>;

pub struct FieldData {
    pub owner: Obj,
    pub prop: Obj,
}

/// `new FieldRef(owner, property)` — the reference types it as `UnType.Any`
pub fn field_ref(owner: &Obj, prop: &Obj) -> Obj {
    obj::obj_new(
        "any",
        Payload::FieldRef(Rc::new(RefCell::new(FieldData {
            owner: owner.clone(),
            prop: prop.clone(),
        }))),
    )
}

pub fn field_cell(o: &Obj) -> Option<FieldCell> {
    match obj::payload(o) {
        Payload::FieldRef(f) => Some(f),
        _ => None,
    }
}

/// `FieldRef.Value` get
pub fn field_get(field: &Obj) -> Obj {
    let cell = field_cell(field).expect("field payload");
    let (owner, prop) = {
        let f = cell.borrow();
        (f.owner.clone(), f.prop.clone())
    };
    prop_read(&prop, &owner)
}

/// `FieldRef.Value` set — `BindName` writes through to the backing member
pub fn field_set(field: &Obj, value: &Obj) {
    let cell = field_cell(field).expect("field payload");
    let (owner, prop) = {
        let f = cell.borrow();
        (f.owner.clone(), f.prop.clone())
    };
    prop_write(&prop, &owner, value);
}

pub fn is_field_ref(o: &Obj) -> bool {
    matches!(obj::payload(o), Payload::FieldRef(..))
}

#[cfg(test)]
mod tests;
