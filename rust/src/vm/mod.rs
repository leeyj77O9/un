
pub(crate) mod transfer;

pub use self::transfer::dump_register_plan;

use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::rc::Rc;

use crate::bytecode::instr::*;
use crate::bytecode::{Constant, ExcKind, Pattern, Program, Proto};
use crate::error::{LangError, Panic};
use crate::module;
use crate::runtime::collections;
use crate::runtime::context::{self, Context, Frame};
use crate::runtime::function::{self, Args};
use crate::runtime::modules;
use crate::runtime::obj;
use crate::runtime::obj::{BinOp, Obj, Payload};
use crate::runtime::scope;
use crate::runtime::types::{self, Types};
use crate::runtime::world;
use crate::source::Source;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Ins {
    pub(crate) op: Op,
    pub(crate) a: u8,
    pub(crate) b: u8,
    pub(crate) c: u8,
    pub(crate) d: i32,
    pub(crate) flags: u16,
}

pub struct ProtoPlan {
    pub(crate) scratch_base: usize,
    pub(crate) reg_count: usize,
    pub(crate) code: Vec<Ins>,
    pub(crate) depths: Vec<u8>,
}

pub struct Plan {
    pub protos: Vec<ProtoPlan>,
}

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
    pub acc_depth: usize,
    pub var_slot: Option<usize>,
}

struct ArgFrame {
    values: Vec<Obj>,
    names: Vec<String>,
}

pub struct VM {
    pub ctx: Context,
    pub ip: usize,
    pub program: Rc<Program>,
    proto_idx: usize,
    plan: Rc<Plan>,
    regs: Vec<Obj>,
    args: Vec<ArgFrame>,
    tries: Vec<TryEntry>,
    slot_scope: Option<scope::Scope>,
    pending: Option<LangError>,
    pending_finally_error: Option<LangError>,
    pending_return: Option<Obj>,
    pending_propagate: Option<Outcome>,
}

pub fn bind_module_functions(program: &Rc<Program>, ctx: &Context) {
    let scope = context::scope(ctx);
    for (index, proto) in program.protos.iter().enumerate() {
        if proto.flags & FLAG_FUNC_BIND == 0 {
            continue;
        }
        let name = program.names[proto.name].clone();
        let arg_count = proto.params.len();
        let mut args = Vec::with_capacity(arg_count);
        for param in &proto.params {
            let pname = program.names[param.name].clone();
            let type_name = match param.type_name {
                Some(i) => Types::single(&program.names[i]),
                None => Types::single(types::ANY),
            };
            let essential = param.flags & PARAM_ESSENTIAL != 0;
            let optional = param.flags & PARAM_OPTIONAL != 0;
            let positional = param.flags & PARAM_POSITIONAL != 0;
            let keyword = param.flags & PARAM_KEYWORD != 0;
            let default = if optional {
                obj::null()
            } else if positional {
                collections::tup(Vec::new(), Vec::new())
            } else if keyword {
                collections::dict(Vec::new())
            } else {
                obj::null()
            };
            args.push(function::ArgData {
                name: pname,
                type_name,
                essential,
                optional,
                positional,
                keyword,
                default: Some(default),
            });
        }
        let created = function::proto_fn(
            Some(name.clone()),
            args,
            Types::single(types::ANY),
            index,
            program.clone(),
            ctx.clone(),
        );
        scope::set(&scope, &name, created);
    }
}

impl VM {
    pub fn new(program: Rc<Program>, source: Rc<Source>) -> Result<Self, String> {
        let root = program.root;
        Self::build(program, source, root)
    }

    pub fn run_proto(
        program: Rc<Program>,
        source: Rc<Source>,
        entry: usize,
    ) -> Result<Outcome, String> {
        let mut vm = Self::build(program, source, entry)?;
        bind_module_functions(&vm.program, &vm.ctx);
        Ok(vm.run())
    }

    fn build(program: Rc<Program>, source: Rc<Source>, entry: usize) -> Result<Self, String> {
        program.validate()?;
        world::set_proto_executor(Some(Rc::new(
            |program: &Rc<Program>, ctx: &Context, proto_index: usize| {
                let plan = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    plan_for(program)
                })) {
                    Ok(plan) => plan,
                    Err(payload) => {
                        let message = panic_payload_message(&payload);
                        return function::Flow::Panic(obj::panic(
                            "panic",
                            format!("internal error (please report): {message}"),
                        ));
                    }
                };
                execute_proto(program, ctx, proto_index, plan)
            },
        )));
        let plan = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| plan_for(&program)))
            .map_err(|payload| {
                let message = panic_payload_message(&payload);
                format!("internal error (please report): {message}")
            })?;
        let root = entry;
        let reg_count = plan.protos[root].reg_count;
        Ok(Self {
            ctx: context::new(world::global_scope(), source, context::new_frames()),
            ip: 0,
            program,
            proto_idx: root,
            plan,
            regs: vec![world::none_obj(); reg_count],
            args: Vec::new(),
            tries: Vec::new(),
            slot_scope: None,
            pending: None,
            pending_finally_error: None,
            pending_return: None,
            pending_propagate: None,
        })
    }

    fn new_proto(
        program: Rc<Program>,
        plan: Rc<Plan>,
        proto_idx: usize,
        ctx: Context,
    ) -> Self {
        let reg_count = plan.protos[proto_idx].reg_count;
        Self {
            ctx,
            ip: 0,
            program,
            proto_idx,
            plan,
            regs: vec![world::none_obj(); reg_count],
            args: Vec::new(),
            tries: Vec::new(),
            slot_scope: None,
            pending: None,
            pending_finally_error: None,
            pending_return: None,
            pending_propagate: None,
        }
    }

    fn set_item_slow(
        &mut self,
        ins: &Ins,
        span: (usize, usize),
    ) -> Result<bool, Outcome> {
        let _ = span;
        let index = self.regs[ins.c as usize].clone();
        let target = self.regs[ins.b as usize].clone();
        let value = self.regs[ins.a as usize].clone();
        self.regs[ins.a as usize] = obj::set_item(&target, &index, &value);
        Ok(true)
    }

    fn cur_slot_scope(&mut self) -> scope::Scope {
        if let Some(s) = &self.slot_scope {
            return s.clone();
        }
        let cur = context::scope(&self.ctx);
        let owner = cur.borrow().slot_scope.clone().unwrap_or_else(|| cur.clone());
        self.slot_scope = Some(owner.clone());
        owner
    }

    fn plan(&self) -> &ProtoPlan {
        &self.plan.protos[self.proto_idx]
    }

    fn proto(&self) -> &Proto {
        &self.program.protos[self.proto_idx]
    }

    pub fn run(&mut self) -> Outcome {
        loop {
            let Some(&ins) = self.plan().code.get(self.ip) else {
                let scratch_base = self.plan().scratch_base;
                let final_depth = self.plan().depths[self.plan().code.len()] as usize;
                let value = if final_depth > 0 {
                    self.regs[scratch_base + final_depth - 1].clone()
                } else {
                    world::none_obj()
                };
                return Outcome::Value(value);
            };
            let depth_before = self.plan().depths[self.ip] as usize;
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.step(&ins, depth_before)
            }));
            let result = match result {
                Ok(result) => result,
                Err(payload) => {
                    let message = panic_payload_message(&payload);
                    Err(Outcome::Panic(obj::panic(
                        "panic",
                        format!("internal error (please report): {message}"),
                    )))
                }
            };
            match result {
                Ok(advance) => {
                    if advance {
                        self.ip += 1;
                    }
                    let depth = self.plan().depths.get(self.ip).copied().unwrap_or(depth_before as u8) as usize;
                    if depth > 0 {
                        let slot = self.plan().scratch_base + depth - 1;
                        if let Some(top) = self.regs.get(slot) {
                            if matches!(obj::payload(top), Payload::Panic { .. }) {
                                let o = top.clone();
                                return Outcome::Panic(o);
                            }
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
}

impl VM {
    pub fn with_context(
        program: Rc<Program>,
        _source: Rc<Source>,
        ctx: Context,
    ) -> Result<Self, String> {
        let plan = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| plan_for(&program)))
            .map_err(|payload| {
                let message = panic_payload_message(&payload);
                format!("internal error (please report): {message}")
            })?;
        let root = program.root;
        let reg_count = plan.protos[root].reg_count;
        Ok(Self {
            ctx,
            ip: 0,
            program,
            proto_idx: root,
            plan,
            regs: vec![world::none_obj(); reg_count],
            args: Vec::new(),
            tries: Vec::new(),
            slot_scope: None,
            pending: None,
            pending_finally_error: None,
            pending_return: None,
            pending_propagate: None,
        })
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
                    self.args.truncate(entry.acc_depth);
                    self.pending = Some(e.clone());
                    self.ip = entry.handler;
                    return true;
                }
                ExcKind::Catch => {
                    self.tries.remove(index);
                    context::clear_trace(&self.ctx);
                    self.args.truncate(entry.acc_depth);

                    let parent = context::scope(&self.ctx);
                    let catch_scope = scope::new(Some(parent.clone()), false);
                    scope::inherit_slot_scope(&parent, &catch_scope);
                    context::set_scope(&self.ctx, catch_scope.clone());
                    self.slot_scope = None;
                    if let Some(slot) = entry.var_slot {
                        let name = self.plan_name(slot);
                        let error = obj::err_with_header(&e.header, &e.message);
                        scope::set(&catch_scope, &name, error);
                    }
                    self.ip = entry.handler;
                    return true;
                }
                ExcKind::Finally => {
                    self.tries.remove(index);
                    self.args.truncate(entry.acc_depth);
                    self.pending_finally_error = Some(e.clone());
                    self.ip = entry.handler;
                    return true;
                }
            }
        }
        false
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

    fn name(&self, index: usize) -> String {
        self.program.name(self.proto(), index).to_string()
    }

    fn error(&self, message: impl Into<String>, span: (usize, usize)) -> Outcome {
        Outcome::Error(
            LangError::new(span.0, span.1, message).with_source(context::source(&self.ctx)),
        )
    }

    fn header_error(&self, err: &Obj, span: (usize, usize)) -> Outcome {
        let source = context::source(&self.ctx);
        match obj::payload(err) {
            Payload::Error { header, message } => {
                Outcome::Error(
                    LangError::new(span.0, span.1, message).with_header(header).with_source(source),
                )
            }
            _ => self.error(obj::display(err), span),
        }
    }

    fn plan_name(&self, slot: usize) -> String {
        self.program.name(self.proto(), slot).to_string()
    }

    fn s(&self, k: usize) -> usize {
        self.plan().scratch_base + k
    }

    fn bin(&mut self, op: BinOp, a: usize, b: usize, c: usize) {
        let l = self.regs[b].clone();
        let r = self.regs[c].clone();
        self.regs[a] = obj::binop(&l, op, &r);
    }

    fn slice_bounds(&mut self, flags: u16) -> (Option<Obj>, Option<Obj>, Option<Obj>) {
        let nb = slice_bounds_count(flags);
        let base = self.depth_of_current();
        let target = base - nb;
        let s_i = target + 1;
        let e_i = if flags & FLAG_SLICE_START != 0 { s_i + 1 } else { s_i };
        let st_i = if flags & FLAG_SLICE_END != 0 { e_i + 1 } else { e_i };
        let read = |i: usize| self.regs[self.s(i)].clone();
        let start = if flags & FLAG_SLICE_START != 0 { Some(read(s_i)) } else { None };
        let end = if flags & FLAG_SLICE_END != 0 { Some(read(e_i)) } else { None };
        let step = if flags & FLAG_SLICE_STEP != 0 { Some(read(st_i)) } else { None };
        (step, end, start)
    }

    fn depth_of_current(&self) -> usize {
        self.plan().depths[self.ip] as usize
    }
}

fn slice_bounds_count(flags: u16) -> usize {
    1 + (flags & FLAG_SLICE_START != 0) as usize
        + (flags & FLAG_SLICE_END != 0) as usize
        + (flags & FLAG_SLICE_STEP != 0) as usize
}

impl VM {
    fn step(&mut self, ins: &Ins, depth: usize) -> Result<bool, Outcome> {
        let src = {
            let base = self.plan().scratch_base;
            move |k: usize| base + k
        };
        let span = self.proto().span(self.ip);
        match ins.op {
            Op::LoadNone => self.regs[src(depth)] = world::none_obj(),
            Op::LoadNull => self.regs[src(depth)] = world::null_obj(),
            Op::LoadTrue => self.regs[src(depth)] = world::bool_obj(true),
            Op::LoadFalse => self.regs[src(depth)] = world::bool_obj(false),
            Op::LoadConst => {
                let c = self.program.constant(self.proto(), ins.d as usize).clone();
                self.regs[src(depth)] = materialize(&c);
            }
            Op::LoadName => {
                let name = self.name(ins.d as usize);
                let scope = context::scope(&self.ctx);
                if let Some(v) = scope::get_opt(&scope, &name) {
                    self.regs[src(depth)] = if function::is_field_ref(&v) { function::field_get(&v) } else { v };
                } else if let Some(v) = world::try_get_global(&name) {
                    self.regs[src(depth)] = v;
                } else if world::get_class(&name).is_some() {
                    self.regs[src(depth)] = world::class_tobj(&name);
                } else {
                    return Err(self.error(format!("undefined variable '{name}'"), span));
                }
            }
            Op::StoreName => {
                let scope_set = ins.flags & FLAG_SCOPE_SET != 0;
                let value = if scope_set {
                    self.regs[src(depth - 1)].clone()
                } else {
                    self.regs[src(depth - 1)].clone()
                };
                let name = self.name(ins.d as usize);
                let local_only = ins.flags & FLAG_LOCAL_ONLY != 0;
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
                let slot = ins.d as usize;
                let owner = self.cur_slot_scope();
                let slot_opt = {
                    let d = owner.borrow();
                    d.slots.get(slot).cloned().filter(|v| !world::is_uninitialized(v))
                };
                if let Some(v) = slot_opt {
                    self.regs[src(depth)] = if function::is_field_ref(&v) { function::field_get(&v) } else { v };
                    return Ok(true);
                }
                let scope = context::scope(&self.ctx);
                let name_idx = self.proto().locals.get(slot).copied().unwrap_or(0);
                let name = self.name(name_idx);
                if let Some(v) = scope::get_opt(&scope, &name) {
                    self.regs[src(depth)] = if function::is_field_ref(&v) { function::field_get(&v) } else { v };
                    return Ok(true);
                }
                if let Some(v) = world::try_get_global(&name) {
                    self.regs[src(depth)] = v;
                    return Ok(true);
                }
                if world::get_class(&name).is_some() {
                    self.regs[src(depth)] = world::class_tobj(&name);
                    return Ok(true);
                }
                return Err(self.error(format!("undefined variable '{name}'"), span));
            }
            Op::StoreLocal => {
                let scope_set = ins.flags & FLAG_SCOPE_SET != 0;
                let value = if scope_set {
                    self.regs[src(depth - 1)].clone()
                } else {
                    self.regs[src(depth - 1)].clone()
                };
                let slot = ins.d as usize;
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
                self.regs[src(depth)] = self.regs[src(depth - 1)].clone();
            }
            Op::Pop => {}
            Op::Swap => {
                self.regs.swap(src(depth - 2), src(depth - 1));
            }
            Op::Unpack => {
                let count = ins.d as usize;
                let value = self.regs[src(depth - 1)].clone();
                let items = obj::unpack_items(&value);
                if let Some(first) = items.first() {
                    if obj::is_err(first) {
                        return Err(self.header_error(first, span));
                    }
                }
                if items.len() != count {
                    return Err(self.error("tuple unpack mismatch", span));
                }
                for (i, item) in items.iter().enumerate() {
                    self.regs[src(depth - 1 + (count - 1 - i))] = item.clone();
                }
            }
            Op::BuildList => {
                let n = ins.d as usize;
                let values: Vec<Obj> = (src(depth - n)..src(depth)).map(|i| self.regs[i].clone()).collect();
                self.args.pop();
                self.regs[src(depth - n)] = collections::list(values);
            }
            Op::BuildSet => {
                let n = ins.d as usize;
                let items: Vec<Obj> = (src(depth - n)..src(depth)).map(|i| self.regs[i].clone()).collect();
                self.args.pop();
                self.regs[src(depth - n)] = collections::set(items);
            }
            Op::BuildTuple => {
                let n = ins.d as usize;
                let values: Vec<Obj> = (src(depth - n)..src(depth)).map(|i| self.regs[i].clone()).collect();
                self.args.pop();
                self.regs[src(depth - n)] = collections::tup(values, Vec::new());
            }
            Op::BuildTupleNamed => {
                let first = ins.flags as usize;
                let n = ins.d as usize;
                let mut names = Vec::with_capacity(n);
                for i in 0..n {
                    names.push(self.program.name(self.proto(), first + i).to_string());
                }
                let values: Vec<Obj> = (src(depth - n)..src(depth)).map(|i| self.regs[i].clone()).collect();
                self.args.pop();
                self.regs[src(depth - n)] = collections::tup(values, names);
            }
            Op::BuildDict => {
                let n = ins.d as usize;
                let flat: Vec<Obj> = (src(depth - 2 * n)..src(depth)).map(|i| self.regs[i].clone()).collect();
                self.args.pop();
                let mut entries = Vec::with_capacity(n);
                for i in 0..n {
                    entries.push((flat[2 * i].clone(), flat[2 * i + 1].clone()));
                }
                self.regs[src(depth - 2 * n)] = collections::dict(entries);
            }
            Op::FString => {
                let n = ins.d as usize;
                let parts: Vec<Obj> = (src(depth - n)..src(depth)).map(|i| self.regs[i].clone()).collect();
                let mut out = String::new();
                for part in &parts {
                    match obj::payload(part) {
                        Payload::Str(s) => out.push_str(&s),
                        _ => return Err(self.error("f-string expression must be a string", span)),
                    }
                }
                self.regs[src(depth - n)] = obj::str(&out);
            }

            Op::Add => self.bin(BinOp::Add, ins.a as usize, ins.b as usize, ins.c as usize),
            Op::Sub => self.bin(BinOp::Sub, ins.a as usize, ins.b as usize, ins.c as usize),
            Op::Mul => self.bin(BinOp::Mul, ins.a as usize, ins.b as usize, ins.c as usize),
            Op::Div => self.bin(BinOp::Div, ins.a as usize, ins.b as usize, ins.c as usize),
            Op::Idiv => self.bin(BinOp::IDiv, ins.a as usize, ins.b as usize, ins.c as usize),
            Op::Mod => self.bin(BinOp::Mod, ins.a as usize, ins.b as usize, ins.c as usize),
            Op::Pow => self.bin(BinOp::Pow, ins.a as usize, ins.b as usize, ins.c as usize),
            Op::Band => self.bin(BinOp::BAnd, ins.a as usize, ins.b as usize, ins.c as usize),
            Op::Bor => self.bin(BinOp::BOr, ins.a as usize, ins.b as usize, ins.c as usize),
            Op::Bxor => self.bin(BinOp::BXor, ins.a as usize, ins.b as usize, ins.c as usize),
            Op::Lsh => self.bin(BinOp::LShift, ins.a as usize, ins.b as usize, ins.c as usize),
            Op::Rsh => self.bin(BinOp::RShift, ins.a as usize, ins.b as usize, ins.c as usize),
            Op::Eq => self.bin(BinOp::Eq, ins.a as usize, ins.b as usize, ins.c as usize),
            Op::Ne => self.bin(BinOp::NEq, ins.a as usize, ins.b as usize, ins.c as usize),
            Op::Lt => self.bin(BinOp::Lt, ins.a as usize, ins.b as usize, ins.c as usize),
            Op::Gt => self.bin(BinOp::Gt, ins.a as usize, ins.b as usize, ins.c as usize),
            Op::Le => self.bin(BinOp::LtEq, ins.a as usize, ins.b as usize, ins.c as usize),
            Op::Ge => self.bin(BinOp::GtEq, ins.a as usize, ins.b as usize, ins.c as usize),

            Op::Range => {
                let right = self.regs[ins.c as usize].clone();
                let left = self.regs[ins.b as usize].clone();
                self.regs[ins.a as usize] = obj::range_to(&left, &right);
            }
            Op::In => {
                let right = self.regs[ins.c as usize].clone();
                let left = self.regs[ins.b as usize].clone();
                self.regs[ins.a as usize] = obj::obj_in(&right, &left);
            }
            Op::Is => {
                let right = self.regs[ins.c as usize].clone();
                let left = self.regs[ins.b as usize].clone();
                self.regs[ins.a as usize] = is_obj(&left, &right);
            }
            Op::IsNot => {
                let right = self.regs[ins.c as usize].clone();
                let left = self.regs[ins.b as usize].clone();
                let result = is_obj(&left, &right);
                self.regs[ins.a as usize] = match obj::payload(&result) {
                    Payload::Bool(b) => world::bool_obj(!b),
                    _ => result,
                };
            }
            Op::Xor => {
                let right = self.regs[ins.c as usize].clone();
                let left = self.regs[ins.b as usize].clone();
                self.regs[ins.a as usize] = obj::xor_obj(&left, &right);
            }
            Op::Not => {
                let v = self.regs[ins.b as usize].clone();
                self.regs[ins.a as usize] = obj::not_obj(&v);
            }
            Op::Pos => {
                let v = self.regs[ins.b as usize].clone();
                self.regs[ins.a as usize] = obj::unary(&v, obj::UnOp::Pos);
            }
            Op::Neg => {
                let v = self.regs[ins.b as usize].clone();
                self.regs[ins.a as usize] = obj::unary(&v, obj::UnOp::Neg);
            }
            Op::Bnot => {
                let v = self.regs[ins.b as usize].clone();
                self.regs[ins.a as usize] = obj::unary(&v, obj::UnOp::BNot);
            }
            Op::Spread => {
                let v = self.regs[ins.b as usize].clone();
                self.regs[ins.a as usize] = obj::unary(&v, obj::UnOp::Spread);
            }
            Op::ToStr => {
                let v = self.regs[ins.b as usize].clone();
                self.regs[ins.a as usize] = obj::to_str(&v);
            }
            Op::ToBool => {
                let v = self.regs[ins.b as usize].clone();
                let b = obj::to_bool(&v);
                if ins.flags & FLAG_STRICT != 0 && obj::is_err(&b) {
                    return Err(self.error(err_message(&b), span));
                }
                self.regs[ins.a as usize] = b;
            }
            Op::FusedIntMulAdd => {
                let dst = ((ins.d >> 20) & 0x3FF) as usize;
                let x = ((ins.d >> 10) & 0x3FF) as usize;
                let y = (ins.d & 0x3FF) as usize;
                let scope = context::scope(&self.ctx);
                let owner = self.cur_slot_scope();

                let a = owner.borrow().slots.get(dst).cloned().filter(|v| !world::is_uninitialized(v));
                let b = owner.borrow().slots.get(x).cloned().filter(|v| !world::is_uninitialized(v));
                let c = owner.borrow().slots.get(y).cloned().filter(|v| !world::is_uninitialized(v));

                let load = |slot: usize| -> Obj {
                    let name_idx = self.proto().locals.get(slot).copied().unwrap_or(0);
                    let name = self.name(name_idx);
                    if let Some(v) = scope::get_opt(&scope, &name) {
                        if function::is_field_ref(&v) { function::field_get(&v) } else { v }
                    } else if let Some(v) = world::try_get_global(&name) {
                        v
                    } else if world::get_class(&name).is_some() {
                        world::class_tobj(&name)
                    } else {
                        world::none_obj()
                    }
                };

                let a = match a {
                    Some(v) => if function::is_field_ref(&v) { function::field_get(&v) } else { v },
                    None => load(dst),
                };
                let b = match b {
                    Some(v) => if function::is_field_ref(&v) { function::field_get(&v) } else { v },
                    None => load(x),
                };
                let c = match c {
                    Some(v) => if function::is_field_ref(&v) { function::field_get(&v) } else { v },
                    None => load(y),
                };

                let fast = match (obj::payload(&a), obj::payload(&b), obj::payload(&c)) {
                    (Payload::Int(acc), Payload::Int(v), Payload::Int(w)) => {
                        Some(obj::int(acc.wrapping_add(v.wrapping_mul(w))))
                    }
                    _ => None,
                };
                let result = match fast {
                    Some(r) => r,
                    None => {
                        let mul = obj::binop(&b, BinOp::Mul, &c);
                        obj::binop(&a, BinOp::Add, &mul)
                    }
                };
                match owner.borrow_mut().slots.get_mut(dst) {
                    Some(slot) => *slot = result.clone(),
                    None => {
                        scope::set_slot(&scope, dst, result.clone());
                    }
                }
                self.regs[ins.a as usize] = result;
            }
            Op::Jmp => {
                self.ip = self.proto().jump_target(self.ip, ins.d);
                return Ok(false);
            }
            Op::JmpIf => {
                let value = self.regs[ins.a as usize].clone();
                let cond = if ins.flags & FLAG_JUMP_NONE != 0 {
                    obj::is_none(&value)
                } else {
                    match obj::payload(&value) {
                        Payload::Bool(b) => b,
                        _ => false,
                    }
                };
                let jump = if ins.flags & FLAG_JUMP_TRUE != 0 { cond } else { !cond };
                if jump {
                    self.ip = self.proto().jump_target(self.ip, ins.d);
                    return Ok(false);
                }
            }
            Op::PushTry | Op::PushTryProp => {
                let entry = self.proto().exc[ins.d as usize];
                self.tries.push(TryEntry {
                    kind: entry.kind,
                    start: entry.start,
                    end: entry.end,
                    handler: entry.handler,
                    acc_depth: self.args.len(),
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
                let top = self.regs[ins.a as usize].clone();
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
            Op::GetAttr => {
                let target = self.regs[ins.b as usize].clone();
                let name = self.name(ins.d as usize);
                self.regs[ins.a as usize] = obj::get_attr(&target, &name);
            }
            Op::SetAttr => {
                let target = self.regs[ins.b as usize].clone();
                let value = self.regs[ins.c as usize].clone();
                let name = self.name(ins.d as usize);
                let outcome = obj::set_attr(&target, &name, &value);
                if obj::is_err(&outcome) {
                    return Err(self.error(err_message(&outcome), span));
                }
                self.regs[ins.a as usize] = outcome;
            }
            Op::GetItem => {
                let key = self.regs[ins.c as usize].clone();
                let target = self.regs[ins.b as usize].clone();
                self.regs[ins.a as usize] = obj::get_item(&target, &key);
            }
            Op::SetItem => {
                let done = {
                    let idx = &self.regs[ins.c as usize];
                    let target = &self.regs[ins.b as usize];
                    let val = &self.regs[ins.a as usize];
                    let key_int_opt = match obj::payload(idx) {
                        crate::runtime::obj::Payload::Int(n) => Some(n),
                        _ => None,
                    };
                    let value_int_opt = match obj::payload(val) {
                        crate::runtime::obj::Payload::Int(n) => Some(n),
                        _ => None,
                    };
                    if let (Some(key_int), Some(value_int)) = (key_int_opt, value_int_opt) {
                        if key_int >= -1024 && key_int <= 100000 && value_int >= -1024 && value_int <= 100000 {
                            let key_obj = world::int_cached(key_int);
                            let value_obj = world::int_cached(value_int);
                            let done = {
                                let target_b = target.borrow();
                                match &target_b.payload {
                                    crate::runtime::obj::Payload::Dict(km) => {
                                        km.borrow_mut().set(&key_obj, &value_obj);
                                        true
                                    }
                                    _ => false,
                                }
                            };
                            done
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                };
                if done {
                    self.regs[ins.a as usize] = self.regs[ins.a as usize].clone();
                    return Ok(true);
                }
                return self.set_item_slow(ins, span);
            }

            Op::Slice => {
                let (step, end, start) = self.slice_bounds(ins.flags);
                let target = self.regs[ins.b as usize].clone();
                self.regs[ins.a as usize] = obj::slice(&target, start, end, step);
            }
            Op::SetSlice => {
                let (step, end, start) = self.slice_bounds(ins.flags);
                let value = self.regs[ins.a as usize].clone();
                let target = self.regs[ins.b as usize].clone();
                self.set_slice(&target, &value, start, end, step, span)?;
                self.regs[ins.a as usize] = value;
            }

            Op::ArgBegin => {
                let n = ins.d as usize;
                self.args.push(ArgFrame {
                    values: Vec::with_capacity(n),
                    names: Vec::with_capacity(n),
                });
            }
            Op::ArgPos => {
                if ins.flags & FLAG_ARG_CHECK != 0 {
                    let v = &self.regs[ins.a as usize];
                    if obj::is_err(v) {
                        let e = v.clone();
                        return Err(self.header_error(&e, span));
                    }
                }
                let v = self.regs[ins.a as usize].clone();
                let Some(frame) = self.args.last_mut() else {
                    return Err(self.error("malformed bytecode: argument instruction without a call frame", span));
                };
                frame.values.push(v);
                frame.names.push(String::new());
            }
            Op::ArgKw => {
                let name = self.name(ins.d as usize);
                let v = self.regs[ins.a as usize].clone();
                let Some(frame) = self.args.last_mut() else {
                    return Err(self.error("malformed bytecode: argument instruction without a call frame", span));
                };
                frame.values.push(v);
                frame.names.push(name);
            }
            Op::ArgSpread => {
                let source = self.regs[ins.a as usize].clone();
                let items = obj::unpack_items(&source);
                if let Some(first) = items.first() {
                    if obj::is_err(first) {
                        return Err(self.header_error(first, span));
                    }
                }
                let Some(frame) = self.args.last_mut() else {
                    return Err(self.error("malformed bytecode: argument instruction without a call frame", span));
                };
                for item in items {
                    frame.values.push(item);
                    frame.names.push(String::new());
                }
            }
            Op::ArgKwSpread => {
                let value = self.regs[ins.a as usize].clone();
                let dict = match obj::payload(&value) {
                    Payload::Dict(map) => map,
                    _ => return Err(self.error("** argument must be dict", span)),
                };
                let pairs: Vec<(Obj, Obj)> = dict
                    .borrow()
                    .iter_pairs()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                let Some(frame) = self.args.last_mut() else {
                    return Err(self.error("malformed bytecode: argument instruction without a call frame", span));
                };
                for (key, item) in pairs {
                    let name = match obj::payload(&key) {
                        Payload::Str(s) => s.to_string(),
                        _ => obj::display(&obj::repr(&key)),
                    };
                    frame.values.push(item.clone());
                    frame.names.push(name);
                }
            }

            Op::Call => {
                let frame = self.args.pop().unwrap_or(ArgFrame {
                    values: Vec::new(),
                    names: Vec::new(),
                });
                let callable = self.regs[ins.a as usize].clone();
                let values = frame.values;
                let names = frame.names;

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
                let crosses_error = match &outcome {
                    Err(Outcome::Error(_)) | Err(Outcome::Propagate(..)) | Err(Outcome::Panic(_)) => true,
                    Ok(result) => obj::is_err(&result) || obj::is_panic(&result),
                    _ => false,
                };
                if crosses_error {
                    if let Ok(frame) = &popped {
                        let name = self
                            .program
                            .names
                            .get(self.proto().name)
                            .cloned()
                            .unwrap_or_default();
                        context::push_trace(
                            &self.ctx,
                            context::TraceEntry {
                                source: frame.source.clone(),
                                start: frame.start,
                                length: frame.length,
                                name,
                            },
                        );
                    }
                }
                match outcome {
                    Ok(result) => {
                        if obj::is_err(&result) {
                            return Err(self.error(err_message(&result), span));
                        }
                        if obj::is_panic(&result) {
                            return Err(Outcome::Panic(result));
                        }
                        if popped.is_err() {
                            return Err(Outcome::Panic(obj::panic("panic", "stack is empty")));
                        }
                        self.regs[ins.a as usize] = result;
                    }
                    Err(outcome) => return Err(outcome),
                }
            }

            Op::NewScope => {
                let cur = context::scope(&self.ctx);
                let is_fn = ins.flags & FLAG_SCOPE_FUNCTION != 0;
                let child = scope::new(Some(cur.clone()), is_fn);
                scope::inherit_slot_scope(&cur, &child);
                context::set_scope(&self.ctx, child);
                self.slot_scope = None;
            }
            Op::PopScope => {
                let cur = context::scope(&self.ctx);
                let parent = scope::parent(&cur).unwrap_or_else(|| cur.clone());
                context::set_scope(&self.ctx, parent);
                self.slot_scope = None;
            }
            Op::PrepIter => {
                let iterable = self.regs[ins.a as usize].clone();
                let iter = obj::unary(&iterable, obj::UnOp::Iter);
                if !matches!(obj::payload(&iter), Payload::Iter(_)) {
                    let msg = self.program.message(ins.d as usize).to_string();
                    return Err(self.error(msg, span));
                }
                self.regs[ins.a as usize] = iter;
            }
            Op::ForNext => {
                let iter = self.regs[ins.b as usize].clone();
                let next = if let Payload::Iter(cell) = obj::payload(&iter) {
                    if cell.borrow().kind == crate::runtime::iters::Kind::Range {
                        if let Some(value) = crate::runtime::iters::range_next(&cell) {
                            world::int_cached(value)
                        } else {
                            obj::err("iteration stopped")
                        }
                    } else {
                        obj::unary(&iter, obj::UnOp::Next)
                    }
                } else {
                    obj::unary(&iter, obj::UnOp::Next)
                };
                match obj::payload(&next) {
                    Payload::Error { message, .. } if message == "iteration stopped" => {
                        self.ip = self.proto().jump_target(self.ip, ins.d);
                        return Ok(false);
                    }
                    Payload::Panic { .. } => {
                        self.regs[ins.a as usize] = next;
                        return Ok(true);
                    }
                    _ => {
                        self.regs[ins.a as usize] = next;
                    }
                }
            }
            Op::Break | Op::Skip => {
                self.ip = self.proto().jump_target(self.ip, ins.d);
                return Ok(false);
            }
            Op::Return => {
                let value = self.regs[ins.a as usize].clone();

                if let Some(index) = self.pending_finally() {
                    let entry = self.tries[index];
                    self.tries.truncate(index);
                    self.pending_return = Some(value);
                    self.ip = entry.handler;
                    return Ok(false);
                }
                return Err(Outcome::Return(value, span.0, span.1));
            }

            Op::MakeFunc => {
                let slot = ins.d as usize;
                let module_index = self.proto().protos[slot];
                let proto = self.program.protos[module_index].clone();

                let defaults = (ins.flags >> 8) as usize;
                let mut values = (src(depth - defaults)..src(depth))
                    .map(|i| self.regs[i].clone())
                    .collect::<Vec<_>>()
                    .into_iter();
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
                if ins.flags & FLAG_FUNC_BIND != 0 {
                    let scope = context::scope(&self.ctx);
                    scope::set(&scope, &name, created.clone());
                }
                if ins.flags & FLAG_FUNC_EXT != 0 {
                    if let Some(ext) = proto.ext_type {
                        let type_name = self.program.names[ext].clone();
                        world::try_set_original_class(&type_name, &name, created.clone());
                    }
                }
                self.regs[ins.a as usize] = created;
            }

            Op::MakeClass => {
                let name_slot = ins.d as usize;
                let name = self.name(name_slot);
                let field_count = (ins.flags >> 8) as usize;
                let mut fields = Vec::with_capacity(field_count);
                for i in 0..field_count {
                    fields.push(self.name(name_slot + 1 + i));
                }
                let created = if ins.flags & FLAG_CLASS_PARAMS != 0 {
                    collections::stru(&name, fields)
                } else {
                    obj::obj_new(&name, Payload::Plain)
                };
                self.regs[ins.a as usize] = created;
            }
            Op::RegisterClass => {
                let name = self.name(ins.d as usize);
                let created = self.regs[ins.a as usize].clone();
                world::set_class(&name, created);
            }
            Op::LoadBaseClass => {
                let name = self.name(ins.d as usize);
                match world::get_class(&name) {
                    Some(class) => self.regs[ins.a as usize] = class,
                    None => {
                        return Err(self.error(format!("unknown base class '{name}'"), span));
                    }
                }
            }
            Op::SetBases => {
                let count = ins.d as usize;
                let mut bases = Vec::with_capacity(count);
                for i in 0..count {
                    bases.push(self.regs[src(depth - 1 - i)].clone());
                }
                bases.reverse();
                let created = self.regs[ins.a as usize].clone();
                for (index, base) in bases.iter().enumerate() {
                    if index == 0 {
                        created.borrow_mut().super_ = Some(base.clone());
                    }
                    created.borrow_mut().parents.push(base.clone());
                    let base_types = base.borrow().types();
                    let merged = Types::create(&[created.borrow().types(), base_types]);
                    created.borrow_mut().set_types(merged);
                }
                if let Some(first) = bases.first() {
                    let derived_name = obj::type_name(&created).to_string();
                    let base_name = obj::type_name(first).to_string();
                    world::register_original_base(&derived_name, &base_name);
                }
            }
            Op::AddMember | Op::AddMethod => {
                let value = self.regs[ins.b as usize].clone();
                let name = self.name(ins.d as usize);
                let created = self.regs[ins.a as usize].clone();
                obj::member_set(&created, &name, &value);
            }
            Op::MakeProperty => {
                let setter = self.regs[src(depth - 1)].clone();
                let getter = self.regs[src(depth - 2)].clone();
                let mut default = self.regs[src(depth - 3)].clone();
                let name = self.name(ins.d as usize);

                if obj::is_none(&default) {
                    let created = self.regs[src(depth - 4)].clone();
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
                self.regs[ins.a as usize] = property;
            }
            Op::MakeEnum => {
                let name = self.name(ins.d as usize);
                let created = collections::enu(&name, 0);
                self.regs[ins.a as usize] = created;
            }
            Op::MatchPattern => {
                let pattern = self.proto().patterns[ins.d as usize].clone();
                let value = self.regs[ins.b as usize].clone();
                let scope = context::scope(&self.ctx);
                let matched = self.match_pattern(&pattern, &value, &scope);
                self.regs[ins.a as usize] = world::bool_obj(matched);
            }
            Op::MakeFuture => {
                let slot = ins.d as usize;
                let module_index = self.proto().protos[slot];
                self.regs[ins.a as usize] = crate::native::flow::future_pending(
                    module_index,
                    self.program.clone(),
                    scope::snapshot(&context::scope(&self.ctx)),
                );
            }
            Op::Await => {
                let value = self.regs[ins.a as usize].clone();
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
                        let (source, frames, trace) = {
                            let c = self.ctx.borrow();
                            (c.source.clone(), c.frames.clone(), c.trace.clone())
                        };
                        let ctx = context::new(scope, source, frames);
                        ctx.borrow_mut().trace = trace;
                        match execute_proto(&program, &ctx, index, plan_for(&program)) {
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
                if obj::is_err(&result) {
                    return Err(self.error(err_message(&result), span));
                }
                if obj::is_panic(&result) {
                    return Err(Outcome::Panic(result));
                }

                state.borrow_mut().value = Some(result.clone());
                self.regs[ins.a as usize] = result;
            }
            Op::PushDefer => {
                let slot = ins.d as usize;
                let module_index = self.proto().protos[slot];
                context::push_defer(
                    &self.ctx,
                    context::Defer::Proto {
                        index: module_index,
                        program: self.program.clone(),
                    },
                );
            }
            Op::PushUsing => {
                let value = self.regs[ins.a as usize].clone();
                let entry = obj::entry(&value);
                if obj::is_err(&entry) {
                    return Err(self.error(err_message(&entry), span));
                }
                context::push_using(&self.ctx, entry);
            }
            Op::Unwrap => {
                let v = &self.regs[ins.a as usize];
                if obj::is_err(v) {
                    let e = v.clone();
                    return Err(self.error(err_message(&e), span));
                }
            }
            Op::Use => {
                let path = self.name(ins.d as usize);
                let mut cursor = ins.d as usize + 1;
                let alias = if ins.flags & 1 != 0 {
                    let a = self.name(cursor);
                    cursor += 1;
                    Some(a)
                } else {
                    None
                };
                let mut imports = Vec::new();
                if ins.flags & 2 != 0 {
                    let count = (ins.flags >> 8) as usize;
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
                    modules::include(&last, alias, imports).map_err(module::ModuleError::Panic)
                } else {
                    let source_path = self.ctx.borrow().source.path.clone();
                    module::import_file(&parts, &source_path, alias, imports, Some(context::traces(&self.ctx)))
                };
                if let Err(error) = result {
                    let name = self
                        .program
                        .names
                        .get(self.proto().name)
                        .cloned()
                        .unwrap_or_default();
                    context::push_trace(
                        &self.ctx,
                        context::TraceEntry {
                            source: context::source(&self.ctx),
                            start: span.0,
                            length: span.1,
                            name,
                        },
                    );
                    return Err(error.into());
                }
            }
            Op::Unsupported => {
                let message = self.program.message(ins.d as usize).to_string();
                if ins.flags & FLAG_HOST_ERROR != 0 {
                    let (name, text) = match message.split_once(": ") {
                        Some((name, text)) => (name.to_owned(), text.to_owned()),
                        None => ("error".to_owned(), message),
                    };
                    return Err(Outcome::Panic(obj::panic(name, text)));
                }
                return Err(self.error(message, span));
            }
            _ => {
                return Err(Outcome::Panic(obj::panic(
                    "panic",
                    format!("vm2: opcode {:?} is not implemented yet", ins.op),
                )))
            }
        }
        Ok(true)
    }
}


impl VM {
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
    if let Some(result) = crate::jit_fast::try_call_jit(&program, proto_index, args, None) {
        return Ok(result);
    }
    let frame = match function::begin_call(callable, args) {
            Ok(frame) => frame,
            Err(error) => return Err(self.header_error(&error, span)),
        };
        let flow = execute_proto(&program, &frame.ctx, proto_index, plan_for(&program));
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
                return Err(self.error("slice assignment with step != 1 not supported", span));
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
}

#[derive(Clone, Copy)]
pub(crate) struct UnJitResult {
    pub(crate) value: i64,
    pub(crate) tag: i64,
}

pub(crate) fn jit_call(
    entry: crate::jit_cache::JitEntry,
    ints: &[i64],
    param_tags: i64,
) -> UnJitResult {
    assert!(ints.len() == entry.arity as usize);
    let temp_mark = crate::runtime::int_class::temp_mark();
    type F0 = unsafe extern "C" fn(*mut i64, i64);
    type F1 = unsafe extern "C" fn(*mut i64, i64, i64);
    type F2 = unsafe extern "C" fn(*mut i64, i64, i64, i64);
    type F3 = unsafe extern "C" fn(*mut i64, i64, i64, i64, i64);
    type F4 = unsafe extern "C" fn(*mut i64, i64, i64, i64, i64, i64);
    type F5 = unsafe extern "C" fn(*mut i64, i64, i64, i64, i64, i64, i64);
    type F6 = unsafe extern "C" fn(*mut i64, i64, i64, i64, i64, i64, i64, i64);
    type F7 = unsafe extern "C" fn(*mut i64, i64, i64, i64, i64, i64, i64, i64, i64);
    type F8 = unsafe extern "C" fn(*mut i64, i64, i64, i64, i64, i64, i64, i64, i64, i64);
    type F9 = unsafe extern "C" fn(*mut i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64);
    type F10 = unsafe extern "C" fn(*mut i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64);
    let mut out = [0i64; 2];
    let ptr = out.as_mut_ptr();
    unsafe {
        match ints.len() {
            0 => std::mem::transmute::<*const u8, F0>(entry.ptr)(ptr, param_tags),
            1 => std::mem::transmute::<*const u8, F1>(entry.ptr)(ptr, param_tags, ints[0]),
            2 => std::mem::transmute::<*const u8, F2>(entry.ptr)(ptr, param_tags, ints[0], ints[1]),
            3 => std::mem::transmute::<*const u8, F3>(entry.ptr)(ptr, param_tags, ints[0], ints[1], ints[2]),
            4 => std::mem::transmute::<*const u8, F4>(entry.ptr)(ptr, param_tags, ints[0], ints[1], ints[2], ints[3]),
            5 => std::mem::transmute::<*const u8, F5>(entry.ptr)(ptr, param_tags, ints[0], ints[1], ints[2], ints[3], ints[4]),
            6 => std::mem::transmute::<*const u8, F6>(entry.ptr)(ptr, param_tags, ints[0], ints[1], ints[2], ints[3], ints[4], ints[5]),
            7 => std::mem::transmute::<*const u8, F7>(entry.ptr)(ptr, param_tags, ints[0], ints[1], ints[2], ints[3], ints[4], ints[5], ints[6]),
            8 => std::mem::transmute::<*const u8, F8>(entry.ptr)(ptr, param_tags, ints[0], ints[1], ints[2], ints[3], ints[4], ints[5], ints[6], ints[7]),
            9 => std::mem::transmute::<*const u8, F9>(entry.ptr)(ptr, param_tags, ints[0], ints[1], ints[2], ints[3], ints[4], ints[5], ints[6], ints[7], ints[8]),
            10 => std::mem::transmute::<*const u8, F10>(entry.ptr)(ptr, param_tags, ints[0], ints[1], ints[2], ints[3], ints[4], ints[5], ints[6], ints[7], ints[8], ints[9]),
            _ => unreachable!("jit_call with more than 10 args"),
        }
    }
    crate::runtime::int_class::temp_truncate(temp_mark);
    UnJitResult { value: out[0], tag: out[1] }
}

thread_local! {
    static PLAN_CACHE: RefCell<(HashMap<u64, (Rc<Program>, Rc<Plan>)>, VecDeque<u64>)> =
        RefCell::new((HashMap::new(), VecDeque::new()));
}

const PLAN_CACHE_LIMIT: usize = 64;

fn seed_class_layouts(plan: &Plan, program: &Rc<Program>) {
    #[inline]
    fn proto_name(program: &Rc<Program>, proto: &crate::bytecode::Proto, slot: usize) -> Option<String> {
        let &global = proto.names.get(slot)?;
        program.names.get(global).cloned()
    }
    let root_proto = &program.protos[program.root];

    let mut class_regs: HashMap<usize, String> = HashMap::new();
    let mut layout_out: HashMap<String, Vec<String>> = HashMap::new();

    if crate::jit_cache::jit_debug() {
        for (i, proto) in program.protos.iter().enumerate() {
            eprintln!("JITDBG seed proto[{}] name_ix={} wlen={}", i, proto.name, proto.code.len());
        }
    }

    for ins in &plan.protos[program.root].code {
        let op = ins.op;
        match op {
            op if op == Op::MakeClass => {
                let Some(name) = proto_name(program, root_proto, ins.d as usize) else { continue };
                class_regs.insert(ins.a as usize, name.clone());
                layout_out.entry(name).or_default();
            }
            _ => {}
        }
    }

    let mut class_template: HashMap<String, Vec<String>> = HashMap::new();
    let mut current: Option<String> = None;
    for ins in &plan.protos[program.root].code {
        let op = ins.op;
        if op == Op::MakeClass {
            current = None;
            if ins.d < 0 { continue; }
            if let Some(name) = proto_name(program, root_proto, ins.d as usize) {
                class_template.entry(name.clone()).or_default();
                current = Some(name);
            }
            continue;
        }
        if op == Op::AddMethod || op == Op::AddMember {
            if ins.d < 0 { continue; }
            let Some(cur) = current.clone() else { continue };
            if let Some(name) = proto_name(program, root_proto, ins.d as usize) {
                class_template.entry(cur).or_default().push(name);
            }
        }
    }

    let collect_fields = |only_init: bool, layout_out: &mut HashMap<String, Vec<String>>| {
        for proto in &program.protos {
            let ext = match proto.ext_type { Some(e) => e, None => continue };
            let Some(class_name) = program.names.get(ext).cloned() else { continue };
            let proto_name = program.names.get(proto.name).cloned().unwrap_or_default();
            let is_init = proto_name == "__init__";
            if is_init != only_init {
                continue;
            }
            let Some(fields) = layout_out.get_mut(&class_name) else { continue };
            for ins in &proto.code {
                if ins.op != Op::SetAttr as u8 {
                    continue;
                }
                let Some(name_ix) = proto.names.get(ins.operand as usize).copied() else { continue; };
                let Some(field) = program.names.get(name_ix).cloned() else { continue };
                if field == "self" || fields.contains(&field) {
                    continue;
                }
                fields.push(field);
            }
        }
    };
    collect_fields(true, &mut layout_out);
    collect_fields(false, &mut layout_out);

    if crate::jit_cache::jit_debug() {
        eprintln!("JITDBG class_regs {:?}", class_regs);
        eprintln!("JITDBG layout_out {:?}", layout_out);
    }
    for (class_name, fields) in layout_out {
        if world::class_has_property(&class_name) {
            if crate::jit_cache::jit_debug() {
                eprintln!("JITDBG skip layout for {}: class has properties", class_name);
            }
            continue;
        }
        let template = class_template.get(&class_name).cloned().unwrap_or_default();
        let mut full = template;
        for f in fields {
            if !full.contains(&f) {
                full.push(f);
            }
        }
        if !full.is_empty() {
            if crate::jit_cache::jit_debug() {
                eprintln!("JITDBG seeding layout {} -> {:?}", class_name, full);
            }
            world::set_class_layout(&class_name, full);
        } else if crate::jit_cache::jit_debug() {
            eprintln!("JITDBG empty layout for {}", class_name);
        }
    }
}

fn plan_for(program: &Rc<Program>) -> Rc<Plan> {
    let key = program.id;
    let found = PLAN_CACHE.with(|cache| cache.borrow().0.get(&key).cloned());
    if let Some((_, cached_plan)) = found {
        return cached_plan;
    }
    let plan = Rc::new(transfer::compile(program).expect(
        "validated program must translate; Program::validate was strengthened
        to enforce the 256-register limit for this reason",
    ));
    seed_class_layouts(&plan, program);
    PLAN_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        while cache.0.len() >= PLAN_CACHE_LIMIT {
            if let Some(oldest) = cache.1.pop_front() {
                cache.0.remove(&oldest);
            }
        }
        cache.1.push_back(key);
        cache.0.insert(key, (program.clone(), plan.clone()));
    });
    plan
}

fn execute_proto(program: &Rc<Program>, ctx: &Context, proto_index: usize, plan: Rc<Plan>) -> function::Flow {
    let mut vm = VM::new_proto(program.clone(), plan, proto_index, ctx.clone());
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

fn panic_payload_message(payload: &(dyn std::any::Any + Send + 'static)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        message.to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic".to_string()
    }
}

pub fn err_message(o: &Obj) -> String {
    match obj::payload(o) {
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
    match obj::payload(right) {
        Payload::TObj(t) => match obj::payload(left) {
            Payload::TObj(t2) => obj::boolean(t == t2),
            _ => {
                let types = obj::types_of(left);
                obj::boolean(t.accepts(&types))
            }
        },
        _ => obj::obj_is(left, right),
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
#[cfg(test)] pub(crate) mod tests;
