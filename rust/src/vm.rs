use std::rc::Rc;
use crate::bytecode::instr::*;
use crate::bytecode::{Constant, ExcKind, Pattern, Program, Proto};
use crate::error::{LangError, Panic};
use crate::module;
use crate::runtime::collections;
use crate::runtime::context::{self, Context, Frame};
use crate::runtime::function::{self, Args};
use crate::runtime::global;
use crate::runtime::obj;
use crate::runtime::obj::{BinOp, Obj, Payload};
use crate::runtime::scope;
use crate::runtime::types::{self, Types};
use crate::runtime::world;
use crate::source::Source;
#[derive(Clone)]
pub enum Outcome {

    Value(Obj),
    Return(Obj, usize, usize),
    Break(usize, usize),
    Skip(usize, usize),

    Propagate(Obj, usize, usize),
    Panic(Obj),
    Error(LangError),
}
#[derive(Debug, Clone, Copy)]
pub struct TryEntry {
    pub kind: ExcKind,
    pub start: usize,
    pub end: usize,
    pub handler: usize,
    pub depth: usize,

    pub acc_depth: usize,

    pub var_slot: Option<usize>,
}
pub struct VM {
    pub ctx: Context,
    pub program: Rc<Program>,
    pub proto: Rc<Proto>,
    pub ip: usize,
    pub stack: Vec<Obj>,

    pub acc: Vec<Vec<String>>,

    pub tries: Vec<TryEntry>,

    pub pending: Option<LangError>,

    pub pending_finally_error: Option<LangError>,

    pub pending_return: Option<Obj>,

    pub pending_propagate: Option<Outcome>,
}
impl VM {
    pub fn new(program: Rc<Program>, source: Rc<Source>) -> Result<Self, String> {
        program.validate()?;
        let proto = program.protos[program.root].clone();

        world::set_current_program(Some(program.clone()));
        world::set_proto_executor(Some(Rc::new(execute_proto)));
        Ok(Self {
            ctx: context::new(world::global_scope(), source, context::new_frames()),
            program,
            proto: Rc::new(proto),
            ip: 0,
            stack: Vec::new(),
            acc: Vec::new(),
            tries: Vec::new(),
            pending: None,
            pending_finally_error: None,
            pending_return: None,
            pending_propagate: None,
        })
    }

    pub fn with_context(program: Rc<Program>, _source: Rc<Source>, ctx: Context) -> Self {
        let proto = program.protos[program.root].clone();
        Self {
            ctx,
            program,
            proto: Rc::new(proto),
            ip: 0,
            stack: Vec::new(),
            acc: Vec::new(),
            tries: Vec::new(),
            pending: None,
            pending_finally_error: None,
            pending_return: None,
            pending_propagate: None,
        }
    }

    fn new_proto(program: Rc<Program>, proto_index: usize, ctx: Context) -> Self {
        let proto = program.protos[proto_index].clone();
        Self {
            ctx,
            program,
            proto: Rc::new(proto),
            ip: 0,
            stack: Vec::new(),
            acc: Vec::new(),
            tries: Vec::new(),
            pending: None,
            pending_finally_error: None,
            pending_return: None,
            pending_propagate: None,
        }
    }

    pub fn run(&mut self) -> Outcome {
        loop {
            let Some((op, flags, operand)) = self.proto.instruction(self.ip) else {
                return Outcome::Value(self.stack.pop().unwrap_or_else(world::none_obj));
            };
            match self.exec(op, flags, operand) {
                Ok(advance) => {
                    if advance {
                        self.ip += 1;
                    }

                    if let Some(top) = self.stack.last() {
                        if matches!(obj::payload(top), Payload::Panic { .. }) {
                            let o = top.clone();
                            return Outcome::Panic(o);
                        }
                    }
                }
                Err(outcome) => {
                    if !self.handle(&outcome) {
                        return outcome;
                    }
                }
            }
        }
    }

    fn handle(&mut self, outcome: &Outcome) -> bool {
        let Outcome::Error(e) = outcome else {
            return false;
        };

        for index in (0..self.tries.len()).rev() {
            let entry = self.tries[index];
            if entry.start > self.ip || self.ip >= entry.end {
                continue;
            }
            match entry.kind {
                ExcKind::Propagate => {
                    self.stack.truncate(entry.depth);
                    self.acc.truncate(entry.acc_depth);
                    self.pending = Some(e.clone());
                    self.ip = entry.handler;
                    return true;
                }
                ExcKind::Catch => {

                    self.tries.remove(index);
                    self.stack.truncate(entry.depth);
                    self.acc.truncate(entry.acc_depth);

                    let parent = context::scope(&self.ctx);
                    let catch_scope = scope::new(Some(parent.clone()), false);
                    scope::inherit_slot_scope(&parent, &catch_scope);
                    context::set_scope(&self.ctx, catch_scope.clone());
                    if let Some(slot) = entry.var_slot {
                        let name = self.name(slot);
                        let error = obj::err_with_header(&e.header, &e.message);
                        scope::set(&catch_scope, &name, error);
                    }
                    self.ip = entry.handler;
                    return true;
                }
                ExcKind::Finally => {

                    self.tries.remove(index);
                    self.stack.truncate(entry.depth);
                    self.acc.truncate(entry.acc_depth);
                    self.pending_finally_error = Some(e.clone());
                    self.ip = entry.handler;
                    return true;
                }
            }
        }
        false
    }

    fn exec(&mut self, op: Op, flags: u16, operand: i32) -> Result<bool, Outcome> {
        let span = self.proto.span(self.ip);
        match op {

            Op::LoadNone => self.push(world::none_obj()),
            Op::LoadNull => self.push(world::null_obj()),
            Op::LoadTrue => self.push(world::bool_obj(true)),
            Op::LoadFalse => self.push(world::bool_obj(false)),
            Op::LoadConst => {
                let c = self.program.constant(&self.proto, operand as usize).clone();
                self.push(materialize(&c));
            }
            Op::LoadName => {
                let name = self.name(operand as usize);
                let scope = context::scope(&self.ctx);
                if let Some(v) = scope::get_opt(&scope, &name) {
                    self.push(if function::is_field_ref(&v) {
                        function::field_get(&v)
                    } else {
                        v
                    });
                } else if let Some(v) = world::try_get_global(&name) {
                    self.push(v);
                } else if world::get_class(&name).is_some() {

                    self.push(collections::tobj(Types::single(&name)));
                } else {
                    return Err(self.error(format!("undefined variable '{name}'"), span));
                }
            }
            Op::StoreName => {
                let scope_set = flags & FLAG_SCOPE_SET != 0;

                let value = if scope_set {
                    self.peek()?.clone()
                } else {
                    self.pop()?
                };
                let name = self.name(operand as usize);
                let local_only = flags & FLAG_LOCAL_ONLY != 0;
                let scope = context::scope(&self.ctx);

                if !local_only {
                    if let Some(current) = scope::get_opt(&scope, &name) {
                        if function::is_field_ref(&current) {
                            function::field_set(&current, &value);
                            return Ok(true);
                        }
                    }
                }
                if scope_set {

                    scope::set(&scope, &name, value);
                } else if local_only || scope::is_function_scope(&scope) {
                    scope::set_local(&scope, &name, value);
                } else {
                    scope::set(&scope, &name, value);
                }
            }

            Op::LoadLocal => {
                let slot = operand as usize;
                let scope = context::scope(&self.ctx);
                if let Some(v) = scope::get_slot(&scope, slot) {
                    self.push(v);
                    return Ok(true);
                }
                let name_idx = self.proto.locals.get(slot).copied().unwrap_or(0);
                let name = self.name(name_idx);
                if let Some(v) = scope::get_opt(&scope, &name) {
                    let v = if function::is_field_ref(&v) { function::field_get(&v) } else { v };
                    self.push(v);
                    return Ok(true);
                }
                if let Some(v) = world::try_get_global(&name) {
                    self.push(v);
                    return Ok(true);
                }
                if world::get_class(&name).is_some() {
                    self.push(collections::tobj(Types::single(&name)));
                    return Ok(true);
                }
                return Err(self.error(format!("undefined variable '{name}'"), span));
            }
            Op::StoreLocal => {
                let scope_set = flags & FLAG_SCOPE_SET != 0;
                let value = if scope_set {
                    self.peek()?.clone()
                } else {
                    self.pop()?
                };
                let slot = operand as usize;
                let scope = context::scope(&self.ctx);
                if let Some(current) = scope::get_slot(&scope, slot) {
                    if function::is_field_ref(&current) {
                        function::field_set(&current, &value);
                        return Ok(true);
                    }
                }
                scope::set_slot(&scope, slot, value);
            }

            Op::Dup => {
                let v = self.peek()?.clone();
                self.push(v);
            }
            Op::Pop => {
                self.pop()?;
            }
            Op::Swap => {

                let top = self.pop()?;
                let below = self.pop()?;
                self.push(top);
                self.push(below);
            }
            Op::Unpack => {
                let count = operand as usize;
                let value = self.pop()?;
                let items = obj::unpack_items(&value);

                if let Some(first) = items.first() {
                    if obj::is_err(first) {
                        return Err(self.header_error(first, span));
                    }
                }
                if items.len() != count {
                    return Err(self.error("tuple unpack mismatch", span));
                }

                for item in items.iter().rev() {
                    self.push(item.clone());
                }
            }

            Op::BuildList => {
                let n = operand as usize;
                let values = self.take(n);
                self.acc.pop();
                self.push(collections::list(values));
            }
            Op::BuildSet => {
                let n = operand as usize;
                let items = self.take(n);
                self.acc.pop();
                self.push(collections::set(items));
            }
            Op::BuildTuple => {
                let n = operand as usize;
                let values = self.take(n);
                self.acc.pop();
                self.push(collections::tup(values, Vec::new()));
            }
            Op::BuildTupleNamed => {
                let first = flags as usize;
                let n = operand as usize;
                let mut names = Vec::with_capacity(n);
                for i in 0..n {
                    names.push(self.program.name(&self.proto, first + i).to_string());
                }
                let values = self.take(n);
                self.acc.pop();
                self.push(collections::tup(values, names));
            }
            Op::BuildDict => {
                let n = operand as usize;
                let flat = self.take(2 * n);
                self.acc.pop();
                let mut entries = Vec::with_capacity(n);
                for i in 0..n {
                    entries.push((flat[2 * i].clone(), flat[2 * i + 1].clone()));
                }
                self.push(collections::dict(entries));
            }
            Op::FString => {

                let n = operand as usize;
                let parts = self.take(n);
                let mut out = String::new();
                for part in &parts {
                    match obj::payload(part) {
                        Payload::Str(s) => out.push_str(&s),
                        _ => {
                            return Err(self.error(
                                "f-string expression must be a string",
                                span,
                            ))
                        }
                    }
                }
                self.push(obj::str(&out));
            }

            Op::Add => self.bin(BinOp::Add)?,
            Op::Sub => self.bin(BinOp::Sub)?,
            Op::Mul => self.bin(BinOp::Mul)?,
            Op::Div => self.bin(BinOp::Div)?,
            Op::Idiv => self.bin(BinOp::IDiv)?,
            Op::Mod => self.bin(BinOp::Mod)?,
            Op::Pow => self.bin(BinOp::Pow)?,
            Op::Band => self.bin(BinOp::BAnd)?,
            Op::Bor => self.bin(BinOp::BOr)?,
            Op::Bxor => self.bin(BinOp::BXor)?,
            Op::Lsh => self.bin(BinOp::LShift)?,
            Op::Rsh => self.bin(BinOp::RShift)?,
            Op::Eq => self.bin(BinOp::Eq)?,
            Op::Ne => self.bin(BinOp::NEq)?,
            Op::Lt => self.bin(BinOp::Lt)?,
            Op::Gt => self.bin(BinOp::Gt)?,
            Op::Le => self.bin(BinOp::LtEq)?,
            Op::Ge => self.bin(BinOp::GtEq)?,
            Op::Range => {
                let right = self.pop()?;
                let left = self.pop()?;
                self.push(obj::range_to(&left, &right));
            }
            Op::In => {

                let right = self.pop()?;
                let left = self.pop()?;
                self.push(obj::obj_in(&right, &left));
            }
            Op::Is => {
                let right = self.pop()?;
                let left = self.pop()?;
                self.push(is_obj(&left, &right));
            }
            Op::IsNot => {
                let right = self.pop()?;
                let left = self.pop()?;
                let result = is_obj(&left, &right);
                match obj::payload(&result) {
                    Payload::Bool(b) => self.push(world::bool_obj(!b)),
                    _ => self.push(result),
                }
            }
            Op::Xor => {
                let right = self.pop()?;
                let left = self.pop()?;
                self.push(obj::xor_obj(&left, &right));
            }
            Op::Not => {
                let v = self.pop()?;
                self.push(obj::not_obj(&v));
            }
            Op::Pos => {
                let v = self.pop()?;
                self.push(obj::unary(&v, obj::UnOp::Pos));
            }
            Op::Neg => {
                let v = self.pop()?;
                self.push(obj::unary(&v, obj::UnOp::Neg));
            }
            Op::Bnot => {
                let v = self.pop()?;
                self.push(obj::unary(&v, obj::UnOp::BNot));
            }
            Op::Spread => {
                let v = self.pop()?;
                self.push(obj::unary(&v, obj::UnOp::Spread));
            }
            Op::ToStr => {

                let v = self.pop()?;
                self.push(obj::to_str(&v));
            }
            Op::ToBool => {

                let v = self.pop()?;
                let b = obj::to_bool(&v);
                if flags & FLAG_STRICT != 0 && obj::is_err(&b) {
                    return Err(self.error(err_message(&b), span));
                }
                self.push(b);
            }

            Op::Jmp => {
                self.ip = self.proto.jump_target(self.ip, operand);
                return Ok(false);
            }
            Op::JmpIf => {
                let value = self.pop()?;
                let cond = if flags & FLAG_JUMP_NONE != 0 {
                    obj::is_none(&value)
                } else {

                    match obj::payload(&value) {
                        Payload::Bool(b) => b,
                        _ => false,
                    }
                };
                let jump = if flags & FLAG_JUMP_TRUE != 0 { cond } else { !cond };
                if flags & FLAG_JUMP_POP != 0 {

                } else {
                    self.push(value);
                }
                if jump {
                    self.ip = self.proto.jump_target(self.ip, operand);
                    return Ok(false);
                }
            }

            Op::PushTryProp => {
                let entry = self.proto.exc[operand as usize];
                self.tries.push(TryEntry {
                    kind: entry.kind,
                    start: entry.start,
                    end: entry.end,
                    handler: entry.handler,
                    depth: self.stack.len(),
                    acc_depth: self.acc.len(),
                    var_slot: entry.var_slot,
                });
            }
            Op::Propagate => {

                if self.tries.last().is_some_and(|last| last.handler == self.ip) {
                    self.tries.pop();
                }

                let frames = context::frames(&self.ctx).borrow().len();
                if let Some(e) = self.pending.take() {
                    if frames > 0 {
                        let err = obj::err_with_header(e.header.clone(), e.message.clone());
                        let outcome = Outcome::Propagate(err, span.0, span.1);
                        if let Some(index) = self.pending_finally() {
                            let entry = self.tries[index];
                            self.tries.truncate(index);
                            self.pending_propagate = Some(outcome);
                            self.ip = entry.handler;
                            return Ok(false);
                        }
                        return Err(outcome);
                    }

                    return Err(Outcome::Error(e));
                }
                let top = self.peek()?.clone();
                if obj::is_err(&top) {
                    if frames > 0 {
                        let outcome = Outcome::Propagate(top, span.0, span.1);
                        if let Some(index) = self.pending_finally() {
                            let entry = self.tries[index];
                            self.tries.truncate(index);
                            self.pending_propagate = Some(outcome);
                            self.ip = entry.handler;
                            return Ok(false);
                        }
                        return Err(outcome);
                    }
                    return Err(self.header_error(&top, span));
                }
            }

            Op::GetAttr => {
                let target = self.pop()?;
                let name = self.name(operand as usize);
                self.push(obj::get_attr(&target, &name));
            }
            Op::SetAttr => {
                let target = self.pop()?;
                let value = self.pop()?;
                let name = self.name(operand as usize);
                self.push(obj::set_attr(&target, &name, &value));
            }
            Op::GetItem => {
                let key = self.pop()?;
                let target = self.pop()?;
                self.push(obj::get_item(&target, &key));
            }
            Op::SetItem => {

                let index = self.pop()?;
                let target = self.pop()?;
                let value = self.pop()?;
                self.push(obj::set_item(&target, &index, &value));
            }
            Op::Slice => {
                let (step, end, start) = self.slice_bounds(flags);
                let target = self.pop()?;
                self.push(obj::slice(&target, start, end, step));
            }
            Op::SetSlice => {
                let (step, end, start) = self.slice_bounds(flags);
                let target = self.pop()?;
                let value = self.pop()?;
                self.set_slice(&target, &value, start, end, step, span)?;

                self.push(value);
            }

            Op::ArgBegin => {
                self.acc.push(Vec::new());

                let n = operand as usize;
                self.acc.last_mut().unwrap().reserve(n);
            }
            Op::ArgPos => {

                if flags & FLAG_ARG_CHECK != 0 {
                    let v = self.peek()?;
                    if obj::is_err(v) {
                        return Err(self.header_error(v, span));
                    }
                }
                self.acc.last_mut().unwrap().push(String::new());
            }
            Op::ArgKw => {
                let name = self.name(operand as usize);
                self.acc.last_mut().unwrap().push(name);
            }
            Op::ArgSpread => {

                let source = self.pop()?;
                let items = obj::unpack_items(&source);
                if let Some(first) = items.first() {
                    if obj::is_err(first) {
                        return Err(self.header_error(first, span));
                    }
                }
                for item in items {
                    self.push(item);
                    self.acc.last_mut().unwrap().push(String::new());
                }
            }
            Op::ArgKwSpread => {
                let value = self.pop()?;
                let dict = match obj::payload(&value) {
                    Payload::Dict(map) => map,
                    _ => {
                        return Err(self.error("** argument must be dict", span));
                    }
                };
                for (key, item) in dict.borrow().iter_pairs() {
                    let name = match obj::payload(key) {
                        Payload::Str(s) => s.to_string(),
                        _ => obj::display(&obj::repr(key)),
                    };
                    self.push(item.clone());
                    self.acc.last_mut().unwrap().push(name);
                }
            }

            Op::Call => {
                let names = self.acc.pop().unwrap_or_default();
                let n = names.len();
                if self.stack.len() < n + 1 {
                    return Err(Outcome::Panic(obj::panic("panic", "vm stack is empty")));
                }
                let values: Vec<Obj> = self.stack[self.stack.len() - n..].to_vec();
                self.stack.truncate(self.stack.len() - n);
                let callable = self.stack.pop().unwrap();

                let source = context::source(&self.ctx);
                context::push_frame(
                    &self.ctx,
                    Frame {
                        name: source.span(span.0, span.1),
                        source,
                        start: span.0,
                        length: span.1,
                    },
                );
                let outcome = self.call_value(&callable, &Args::named(values, names), span);
                let popped = context::pop_frame(&self.ctx);
                match outcome {
                    Ok(result) => {

                        if obj::is_err(&result) {
                            return Err(self.error(err_message(&result), span));
                        }
                        if popped.is_err() {
                            return Err(Outcome::Panic(obj::panic("panic", "stack is empty")));
                        }
                        self.push(result);
                    }
                    Err(outcome) => return Err(outcome),
                }
            }

            Op::NewScope => {
                let cur = context::scope(&self.ctx);
                let is_fn = flags & FLAG_SCOPE_FUNCTION != 0;
                let child = scope::new(Some(cur.clone()), is_fn);
                scope::inherit_slot_scope(&cur, &child);
                context::set_scope(&self.ctx, child);
            }
            Op::PopScope => {
                let cur = context::scope(&self.ctx);
                let parent = scope::parent(&cur).unwrap_or_else(|| cur.clone());
                context::set_scope(&self.ctx, parent);
            }
            Op::PrepIter => {
                let iterable = self.pop()?;
                let iter = obj::unary(&iterable, obj::UnOp::Iter);
                if !matches!(obj::payload(&iter), Payload::Iter(_)) {
                    let msg = self.program.message(operand as usize).to_string();
                    return Err(self.error(msg, span));
                }
                self.push(iter);
            }
            Op::ForNext => {
                let iter = self.peek()?.clone();
                let next = obj::unary(&iter, obj::UnOp::Next);
                match obj::payload(&next) {
                    Payload::Error { message, .. } if message == "iteration stopped" => {
                        self.pop()?;
                        self.ip = self.proto.jump_target(self.ip, operand);
                        return Ok(false);
                    }
                    Payload::Panic { .. } => {
                        self.push(next);
                        self.ip += 1;
                        return Ok(true);
                    }
                    _ => {
                        self.push(next);
                    }
                }
            }
            Op::Break | Op::Skip => {
                self.ip = self.proto.jump_target(self.ip, operand);
                return Ok(false);
            }
            Op::Return => {

                let value = self.pop()?;

                if let Some(index) = self.pending_finally() {
                    let entry = self.tries[index];
                    self.tries.truncate(index);
                    self.pending_return = Some(value);
                    self.ip = entry.handler;
                    return Ok(false);
                }
                return Err(Outcome::Return(value, span.0, span.1));
            }
            Op::PushTry => {
                let entry = self.proto.exc[operand as usize];
                self.tries.push(TryEntry {
                    kind: entry.kind,
                    start: entry.start,
                    end: entry.end,
                    handler: entry.handler,
                    depth: self.stack.len(),
                    acc_depth: self.acc.len(),
                    var_slot: entry.var_slot,
                });
            }
            Op::PopTry => {
                self.tries.pop();
            }
            Op::Raise => {

                if let Some(error) = self.pending_finally_error.take() {
                    return Err(Outcome::Error(error));
                }

                if let Some(value) = self.pending_return.take() {
                    return Err(Outcome::Return(value, span.0, span.1));
                }

                if let Some(outcome) = self.pending_propagate.take() {
                    return Err(outcome);
                }
            }

            Op::MakeFunc => {
                let slot = operand as usize;
                let module_index = self.proto.protos[slot];
                let proto = self.program.protos[module_index].clone();

                let defaults = (flags >> 8) as usize;
                let mut values = self.take(defaults).into_iter();
                let mut args = Vec::with_capacity(proto.params.len());
                for param in &proto.params {
                    let name = self.program.names[param.name].clone();
                    let type_name = match param.type_name {
                        Some(index) => Types::single(&self.program.names[index]),
                        None => Types::single(types::ANY),
                    };
                    let essential = param.flags & PARAM_ESSENTIAL != 0;
                    let optional = param.flags & PARAM_OPTIONAL != 0;
                    let positional = param.flags & PARAM_POSITIONAL != 0;
                    let keyword = param.flags & PARAM_KEYWORD != 0;
                    let default = if optional {
                        values.next().unwrap_or_else(obj::none)
                    } else if positional {

                        collections::tup(Vec::new(), Vec::new())
                    } else if keyword {

                        collections::dict(Vec::new())
                    } else {
                        obj::null()
                    };
                    args.push(function::ArgData {
                        name,
                        type_name,
                        essential,
                        optional,
                        positional,
                        keyword,
                        default: Some(default),
                    });
                }
                let name = self.program.names[proto.name].clone();
                let created = function::proto_fn(
                    Some(name.clone()),
                    args,
                    Types::single(types::ANY),
                    module_index,
                    self.program.clone(),
                    self.ctx.clone(),
                );
                if flags & FLAG_FUNC_BIND != 0 {

                    let scope = context::scope(&self.ctx);
                    scope::set(&scope, &name, created.clone());
                }
                if flags & FLAG_FUNC_EXT != 0 {
                    if let Some(ext) = proto.ext_type {
                        let type_name = self.program.names[ext].clone();
                        world::try_set_original_class(&type_name, &name, created.clone());
                    }
                }
                self.push(created);
            }

            Op::MakeClass => {

                let name_slot = operand as usize;
                let name = self.name(name_slot);
                let field_count = (flags >> 8) as usize;
                let mut fields = Vec::with_capacity(field_count);
                for i in 0..field_count {
                    fields.push(self.name(name_slot + 1 + i));
                }
                let created = if flags & FLAG_CLASS_PARAMS != 0 {

                    collections::stru(&name, fields)
                } else {

                    obj::obj_new(&name, Payload::Plain)
                };
                self.push(created);
            }
            Op::RegisterClass => {

                let name = self.name(operand as usize);
                let created = self.peek()?.clone();
                world::set_class(&name, created);
            }
            Op::LoadBaseClass => {
                let name = self.name(operand as usize);
                match world::get_class(&name) {
                    Some(class) => self.push(class),
                    None => {
                        return Err(self.error(format!("unknown base class '{name}'"), span));
                    }
                }
            }
            Op::SetBases => {

                let count = operand as usize;
                let mut bases = Vec::with_capacity(count);
                for _ in 0..count {
                    bases.push(self.pop()?);
                }

                bases.reverse();
                let created = self.peek()?.clone();
                for (index, base) in bases.iter().enumerate() {
                    if index == 0 {

                        created.borrow_mut().super_ = Some(base.clone());
                    }
                    created.borrow_mut().parents.push(base.clone());
                    let base_types = base.borrow().types.clone();
                    let merged = Types::create(&[created.borrow().types.clone(), base_types]);
                    created.borrow_mut().types = merged;
                }
            }
            Op::AddMember => {
                let value = self.pop()?;
                let name = self.name(operand as usize);
                let created = self.peek()?.clone();
                obj::member_set(&created, &name, &value);
            }
            Op::AddMethod => {
                let value = self.pop()?;
                let name = self.name(operand as usize);
                let created = self.peek()?.clone();
                obj::member_set(&created, &name, &value);
            }
            Op::MakeProperty => {

                let setter = self.pop()?;
                let getter = self.pop()?;
                let mut default = self.pop()?;
                let name = self.name(operand as usize);

                if obj::is_none(&default) {
                    let created = self.peek()?.clone();
                    let base = created.borrow().super_.clone();
                    if let Some(base) = base {
                        if let Some(member) = obj::member(&base, &name) {
                            if let Some(cell) = function::prop_cell(&member) {
                                default = cell.borrow().default.clone();
                            }
                        }
                    }
                }
                let property = function::property_with(
                    &name,
                    if obj::is_none(&getter) { None } else { Some(getter) },
                    if obj::is_none(&setter) { None } else { Some(setter) },
                    default,
                );
                self.push(property);
            }
            Op::MakeEnum => {
                let name = self.name(operand as usize);
                let created = collections::enu(&name, 0);
                self.push(created);
            }
            Op::MatchPattern => {

                let pattern = self.proto.patterns[operand as usize].clone();
                let value = self.peek()?.clone();
                let scope = context::scope(&self.ctx);
                let matched = self.match_pattern(&pattern, &value, &scope);
                self.push(world::bool_obj(matched));
            }
            Op::MakeFuture => {

                let slot = operand as usize;
                let module_index = self.proto.protos[slot];
                self.push(crate::native::flow::future_pending(
                    module_index,
                    self.program.clone(),
                    scope::snapshot(&context::scope(&self.ctx)),
                ));
            }
            Op::Await => {

                let value = self.pop()?;
                let state = match obj::payload(&value) {
                    Payload::Future(state) => state.clone(),
                    _ => return Err(self.error("'wait' requires Future", span)),
                };
                let result = match state.borrow().value.clone() {
                    Some(v) => v,
                    None => {
                        let (index, program, captured) = state
                            .borrow()
                            .pending
                            .clone()
                            .expect("pending work");
                        let scope = scope::new(Some(captured), false);
                        let (source, frames) = {
                            let c = self.ctx.borrow();
                            (c.source.clone(), c.frames.clone())
                        };
                        let ctx = context::new(scope, source, frames);
                        match world::execute_proto(&program, &ctx, index) {
                            function::Flow::Return(v) => v,

                            function::Flow::Propagate(v) => {
                                return Err(Outcome::Propagate(v, span.0, span.1))
                            }
                            function::Flow::Error(e) => return Err(Outcome::Error(e)),
                            function::Flow::Panic(p) => return Err(Outcome::Panic(p)),
                            function::Flow::Break(s, l) => return Err(Outcome::Break(s, l)),
                            function::Flow::Skip(s, l) => return Err(Outcome::Skip(s, l)),
                            function::Flow::Normal => obj::none(),
                        }
                    }
                };

                state.borrow_mut().value = Some(result.clone());
                self.push(result);
            }
            Op::PushDefer => {

                let slot = operand as usize;
                let module_index = self.proto.protos[slot];
                context::push_defer(
                    &self.ctx,
                    context::Defer::Proto {
                        index: module_index,
                        program: self.program.clone(),
                    },
                );
            }
            Op::PushUsing => {

                let value = self.pop()?;
                let entry = obj::entry(&value);
                if obj::is_err(&entry) {
                    return Err(self.error(err_message(&entry), span));
                }
                context::push_using(&self.ctx, entry);
            }

            Op::Unwrap => {
                let v = self.peek()?;
                if obj::is_err(v) {
                    return Err(self.error(err_message(v), span));
                }
            }
            Op::Use => {
                let path = self.name(operand as usize);
                let mut cursor = operand as usize + 1;
                let alias = if flags & 1 != 0 {
                    let a = self.name(cursor);
                    cursor += 1;
                    Some(a)
                } else {
                    None
                };
                let mut imports = Vec::new();
                if flags & 2 != 0 {
                    let count = (flags >> 8) as usize;
                    for _ in 0..count {
                        let name = self.name(cursor);
                        cursor += 1;
                        let alias = self.name(cursor);
                        cursor += 1;
                        imports.push((name, alias));
                    }
                }
                let imports = if imports.is_empty() { None } else { Some(imports) };

                let parts: Vec<String> = path.split('.').map(|s| s.to_string()).collect();
                let last = parts.last().cloned().unwrap_or_default();
                let result = if world::is_native(&last) {
                    global::include(&last, alias, imports).map_err(module::ModuleError::Panic)
                } else {
                    let source_path = self.ctx.borrow().source.path.clone();
                    module::import_file(&parts, &source_path, alias, imports)
                };
                if let Err(error) = result {
                    return Err(error.into());
                }
            }
            Op::Unsupported => {
                let message = self.program.message(operand as usize).to_string();
                if flags & FLAG_HOST_ERROR != 0 {

                    let (name, text) = match message.split_once(": ") {
                        Some((name, text)) => (name.to_owned(), text.to_owned()),
                        None => ("System.Exception".to_owned(), message),
                    };
                    return Err(Outcome::Panic(obj::panic(name, text)));
                }
                return Err(self.error(message, span));
            }

            _ => {
                return Err(Outcome::Panic(obj::panic(
                    "panic",
                    format!("vm: opcode {op:?} is not implemented yet"),
                )))
            }
        }
        Ok(true)
    }

    fn call_value(
        &mut self,
        callable: &Obj,
        args: &Args,
        span: (usize, usize),
    ) -> Result<Obj, Outcome> {
        let proto = function::fn_cell(callable).and_then(|cell| match &cell.borrow().body {
            function::Body::Proto { index, program } => Some((*index, program.clone())),
            _ => None,
        });
        match proto {
            Some((index, program)) => self.call_proto(callable, program, index, args, span),
            None => Ok(obj::call_args(callable, args)),
        }
    }

    fn call_proto(
        &mut self,
        callable: &Obj,
        program: Rc<Program>,
        proto_index: usize,
        args: &Args,
        span: (usize, usize),
    ) -> Result<Obj, Outcome> {
        let frame = match function::begin_call(callable, args) {
            Ok(frame) => frame,
            Err(error) => return Err(self.header_error(&error, span)),
        };
        let flow = world::execute_proto(&program, &frame.ctx, proto_index);
        match flow {
            function::Flow::Return(value) => Ok(function::end_call(&frame, value)),

            function::Flow::Propagate(error) => Ok(function::end_call(&frame, error)),
            function::Flow::Normal => Ok(function::end_call(&frame, obj::none())),
            function::Flow::Error(error) => {
                function::end_call(&frame, obj::none());
                Err(Outcome::Error(error))
            }
            function::Flow::Panic(value) => {
                function::end_call(&frame, obj::none());
                Err(Outcome::Panic(value))
            }
            function::Flow::Break(start, length) => {
                function::end_call(&frame, obj::none());
                Err(Outcome::Break(start, length))
            }
            function::Flow::Skip(start, length) => {
                function::end_call(&frame, obj::none());
                Err(Outcome::Skip(start, length))
            }
        }
    }

    fn pending_finally(&self) -> Option<usize> {
        for index in (0..self.tries.len()).rev() {
            let entry = self.tries[index];
            if entry.kind == ExcKind::Finally && entry.start <= self.ip && self.ip < entry.end {
                return Some(index);
            }
        }
        None
    }

    fn match_pattern(&self, pattern: &Pattern, value: &Obj, scope: &scope::Scope) -> bool {
        match pattern {
            Pattern::Wildcard => true,
            Pattern::Bind(name) => {
                let name = self.program.names[*name].clone();
                scope::set(scope, &name, value.clone());
                true
            }
            Pattern::Literal(index) => {
                let literal = materialize(&self.program.consts[*index]);
                let equal = obj::binop(&literal, obj::BinOp::Eq, value);
                matches!(obj::payload(&equal), Payload::Bool(true))
            }
            Pattern::Tuple(items) => {
                let values = obj::unpack_items(value);
                if values.is_empty() {
                    return false;
                }
                if let Some(first) = values.first() {
                    if obj::is_err(first) {
                        return false;
                    }
                }
                if values.len() != items.len() {
                    return false;
                }
                for (item, item_value) in items.iter().zip(values.iter()) {
                    if !self.match_pattern(item, item_value, scope) {
                        return false;
                    }
                }
                true
            }
        }
    }
    fn bin(&mut self, op: BinOp) -> Result<(), Outcome> {
        let right = self.pop()?;
        let left = self.pop()?;
        self.push(obj::binop(&left, op, &right));
        Ok(())
    }

    fn slice_bounds(&mut self, flags: u16) -> (Option<Obj>, Option<Obj>, Option<Obj>) {
        let step = if flags & FLAG_SLICE_STEP != 0 {
            Some(self.pop().unwrap_or_else(|_| world::none_obj()))
        } else {
            None
        };
        let end = if flags & FLAG_SLICE_END != 0 {
            Some(self.pop().unwrap_or_else(|_| world::none_obj()))
        } else {
            None
        };
        let start = if flags & FLAG_SLICE_START != 0 {
            Some(self.pop().unwrap_or_else(|_| world::none_obj()))
        } else {
            None
        };
        (step, end, start)
    }

    fn set_slice(
        &mut self,
        target: &Obj,
        value: &Obj,
        start: Option<Obj>,
        end: Option<Obj>,
        step: Option<Obj>,
        span: (usize, usize),
    ) -> Result<(), Outcome> {
        let list = match obj::payload(target) {
            Payload::List(v) => v,
            _ => return Err(self.error("slice assignment only supports list", span)),
        };
        let count = list.borrow().len() as i64;
        let (mut a, mut b) = (0i64, count);
        if let Some(o) = &start {
            let i = match obj::payload(o) {
                Payload::Int(i) => i,
                _ => return Err(self.error("slice start must be int", span)),
            };
            let i = i as i32 as i64;
            a = if i < 0 { i + count } else { i };
            a = a.clamp(0, count);
        }
        if let Some(o) = &end {
            let i = match obj::payload(o) {
                Payload::Int(i) => i,
                _ => return Err(self.error("slice end must be int", span)),
            };
            let i = i as i32 as i64;
            b = if i < 0 { i + count } else { i };
            b = b.clamp(0, count);
        }
        if let Some(o) = &step {
            let i = match obj::payload(o) {
                Payload::Int(i) => i,
                _ => return Err(self.error("slice step must be int", span)),
            };
            let i = i as i32 as i64;
            if i != 1 {
                return Err(self.error(
                    "slice assignment with step != 1 not supported",
                    span,
                ));
            }
        }
        if b < a {
            b = a;
        }
        let slice_len = b - a;

        let converted = obj::to_list(value);
        let rhs = match obj::payload(&converted) {
            Payload::List(v) => v.borrow().clone(),
            _ => return Err(self.error("slice assignment value must be list", span)),
        };
        let common = std::cmp::min(slice_len, rhs.len() as i64);
        for i in 0..common {
            let result = collections::list_set_item(&list, &obj::int(a + i), &rhs[i as usize]);
            if obj::is_err(&result) {
                return Err(self.error(err_message(&result), span));
            }
        }
        for i in common..rhs.len() as i64 {
            let r = collections::list_insert(&list, &rhs[i as usize], &obj::int(a + i));
            if let Err(e) = r {
                return Err(self.header_error(&e, span));
            }
        }
        for _ in rhs.len() as i64..slice_len {
            let r = collections::list_remove_at(&list, &obj::int(a + rhs.len() as i64));
            if let Err(e) = r {
                return Err(self.header_error(&e, span));
            }
        }
        Ok(())
    }
    fn push(&mut self, value: Obj) {
        self.stack.push(value);
    }
    fn peek(&self) -> Result<&Obj, Outcome> {
        self.stack
            .last()
            .ok_or_else(|| Outcome::Panic(obj::panic("panic", "vm stack is empty")))
    }
    fn pop(&mut self) -> Result<Obj, Outcome> {
        self.stack
            .pop()
            .ok_or_else(|| Outcome::Panic(obj::panic("panic", "vm stack is empty")))
    }

    fn take(&mut self, n: usize) -> Vec<Obj> {
        if self.stack.len() < n {
            return Vec::new();
        }
        let values = self.stack[self.stack.len() - n..].to_vec();
        self.stack.truncate(self.stack.len() - n);
        values
    }
    fn name(&self, index: usize) -> String {
        self.program.name(&self.proto, index).to_string()
    }

    fn error(&self, message: impl Into<String>, span: (usize, usize)) -> Outcome {
        Outcome::Error(LangError::new(span.0, span.1, message))
    }

    fn header_error(&self, err: &Obj, span: (usize, usize)) -> Outcome {
        match obj::payload(err) {
            Payload::Error { header, message } => {
                Outcome::Error(LangError::new(span.0, span.1, message).with_header(header))
            }
            _ => self.error(obj::display(err), span),
        }
    }
}
fn execute_proto(program: &Rc<Program>, ctx: &Context, proto_index: usize) -> function::Flow {
    let mut vm = VM::new_proto(program.clone(), proto_index, ctx.clone());
    match vm.run() {
        Outcome::Value(value) => function::Flow::Return(value),
        Outcome::Return(value, _, _) => function::Flow::Return(value),
        Outcome::Propagate(error, _, _) => function::Flow::Propagate(error),
        Outcome::Error(error) => function::Flow::Error(error),
        Outcome::Panic(value) => function::Flow::Panic(value),
        Outcome::Break(start, length) => function::Flow::Break(start, length),
        Outcome::Skip(start, length) => function::Flow::Skip(start, length),
    }
}
pub fn err_message(o: &Obj) -> String {    match obj::payload(o) {
        Payload::Error { message, .. } => message,
        Payload::Panic { name, message } => format!("{name}: {message}"),
        _ => obj::display(o),
    }
}
pub fn err_header(o: &Obj) -> String {
    match obj::payload(o) {
        Payload::Error { header, .. } => header,
        _ => "error".to_string(),
    }
}
pub fn panic_display(o: &Obj) -> String {
    match obj::payload(o) {
        Payload::Panic { name, message } => Panic::named(name, message).display(),
        _ => obj::display(o),
    }
}
fn is_obj(left: &Obj, right: &Obj) -> Obj {
    if matches!(obj::payload(left), Payload::TObj(..)) || matches!(obj::payload(right), Payload::TObj(..)) {
        obj::obj_is(right, left)
    } else {
        obj::obj_is(left, right)
    }
}
fn materialize(c: &Constant) -> Obj {
    match c {
        Constant::Int(v) => world::int_cached(*v),
        Constant::Float(v) => obj::float(*v),
        Constant::Str(s) => world::str_pooled(s),
        Constant::Bool(b) => world::bool_obj(*b),
        Constant::None => world::none_obj(),
        Constant::Null => world::null_obj(),
    }
}
#[cfg(test)]
pub(crate) mod tests;
