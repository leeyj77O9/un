//! stack VM — the executor of the compiled `Proto` code.
//!
//! the reference is a tree-walking evaluator, so the VM reproduces its observable
//! behaviour opcode by opcode: `Unwrap` at the reference's `Unwrap(..., node)` points,
//! the `and` / `or` short-circuit through `ToBool` + `As<Bool>`, the `Is` / `In`
//! argument-order quirks, and the header-preserving `Error` rethrows.
//!
//! the reference throws .NET exceptions; Rust has no unwinding, so every control
//! transfer is an `Outcome` value. `Panic` and host exceptions (`System.X`) travel as
//! `Payload::Panic`, and `Error` travels as `LangError`.

use std::rc::Rc;

use crate::bytecode::instr::*;
use crate::bytecode::{Constant, ExcKind, Program, Proto};
use crate::error::{LangError, Panic};
use crate::runtime::collections;
use crate::runtime::context::{self, Context, Frame};
use crate::runtime::function::{self, Args};
use crate::runtime::global;
use crate::runtime::obj;
use crate::runtime::obj::{BinOp, Obj, Payload};
use crate::runtime::scope;
use crate::runtime::types::Types;
use crate::runtime::world;
use crate::source::Source;

/// the result of a run, and the control transfer an opcode raises.
/// `Obj` has no `Debug`, so `Outcome` cannot derive it.
#[derive(Clone)]
pub enum Outcome {
    /// the top-level block value, the value the reference `Eval` returns
    Value(Obj),
    Return(Obj, usize, usize),
    Break(usize, usize),
    Skip(usize, usize),
    /// `?` propagation: `frames > 0` means the caller has to handle it
    Propagate(Obj, usize, usize),
    Panic(Obj),
    Error(LangError),
}

/// a live exception-table entry. `depth` is the stack depth at region entry, so an
/// abort restores the depth exactly as the reference's exception unwinding does.
#[derive(Debug, Clone, Copy)]
pub struct TryEntry {
    pub kind: ExcKind,
    pub start: usize,
    pub end: usize,
    pub handler: usize,
    pub depth: usize,
    /// the accumulator frame count at region entry
    pub acc_depth: usize,
}

pub struct VM {
    pub ctx: Context,
    pub program: Rc<Program>,
    pub proto: Rc<Proto>,
    pub ip: usize,
    pub stack: Vec<Obj>,
    /// argument name accumulator, one frame per call / literal site. the values stay
    /// on the operand stack, so a spread can grow the count at run time.
    pub acc: Vec<Vec<String>>,
    /// the runtime mirror of `proto.exc`, innermost last
    pub tries: Vec<TryEntry>,
    /// the `Error` a `?` region caught, rethrown at top level exactly as `throw;`
    pub pending: Option<LangError>,
}

impl VM {
    pub fn new(program: Rc<Program>, source: Rc<Source>) -> Result<Self, String> {
        program.validate()?;
        let proto = program.protos[program.root].clone();

        Ok(Self {
            ctx: context::new(world::global_scope(), source, context::new_frames()),
            program,
            proto: Rc::new(proto),
            ip: 0,
            stack: Vec::new(),
            acc: Vec::new(),
            tries: Vec::new(),
            pending: None,
        })
    }

    /// runs the proto until ip leaves the code, then returns the block value.
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

                    // a `Panic` payload is never a value in the reference: `Int.Mod` by
                    // zero, `List.Slice` past the end, `Global.Load` of a missing module
                    // all throw out of the host. the VM raises the same way at the
                    // instruction that produced the object.
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

    /// `catch (Error e)` inside a `?` region: the region aborts, the error is kept, and
    /// the handler runs. `Panic`, `System.X`, `Break`, `Skip`, `Return` and
    /// `Propagate` are not `Error`, so they pass through untouched.
    fn handle(&mut self, outcome: &Outcome) -> bool {
        let Outcome::Error(e) = outcome else {
            return false;
        };

        // innermost region first, which is the last pushed entry
        for entry in self.tries.iter().rev() {
            if entry.kind != ExcKind::Propagate {
                continue;
            }
            if entry.start <= self.ip && self.ip < entry.end {
                self.stack.truncate(entry.depth);
                self.acc.truncate(entry.acc_depth);
                self.pending = Some(e.clone());
                self.ip = entry.handler;
                return true;
            }
        }

        false
    }

    /// one instruction. `Ok(true)` advances ip, `Ok(false)` means the instruction moved
    /// ip itself. `Err` raises a control transfer.
    fn exec(&mut self, op: Op, flags: u16, operand: i32) -> Result<bool, Outcome> {
        let span = self.proto.span(self.ip);

        match op {
            // ------------------------------------------------------------------
            // value load
            // ------------------------------------------------------------------
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
                    // `EvalIdentifier` builds a fresh `TObj` for a class name, annotations
                    // are phase 14
                    self.push(collections::tobj(Types::single(&name)));
                } else {
                    return Err(self.error(format!("undefined variable '{name}'"), span));
                }
            }
            Op::StoreName => {
                let value = self.pop()?;
                let name = self.name(operand as usize);
                let local_only = flags & FLAG_LOCAL_ONLY != 0;
                let scope = context::scope(&self.ctx);

                // `BindName`: a `FieldRef` binding writes through the field
                if !local_only {
                    if let Some(current) = scope::get_opt(&scope, &name) {
                        if function::is_field_ref(&current) {
                            function::field_set(&current, &value);
                            return Ok(true);
                        }
                    }
                }

                if local_only || scope::is_function_scope(&scope) {
                    scope::set_local(&scope, &name, value);
                } else {
                    scope::set(&scope, &name, value);
                }
            }

            // ------------------------------------------------------------------
            // stack shuffles
            // ------------------------------------------------------------------
            Op::Dup => {
                let v = self.peek()?.clone();
                self.push(v);
            }
            Op::Pop => {
                self.pop()?;
            }
            Op::Unpack => {
                let count = operand as usize;
                let value = self.pop()?;
                let items = obj::unpack_items(&value);

                // `BindPattern` checks `values[0] is Err` before the length
                if let Some(first) = items.first() {
                    if obj::is_err(first) {
                        return Err(self.header_error(first, span));
                    }
                }
                if items.len() != count {
                    return Err(self.error("tuple unpack mismatch", span));
                }

                // `values[0]` ends on top, so the forward binders see it first
                for item in items.iter().rev() {
                    self.push(item.clone());
                }
            }

            // ------------------------------------------------------------------
            // collection literals
            // ------------------------------------------------------------------
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
                // `EvalFString` throws at the `FString` node when an expression part is
                // not a string. literal parts are `Str` constants, so they pass.
                // the parts are plain stack values, `FSTRING` is not an accumulator
                // consumer, so no accumulator frame is popped here
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

            // ------------------------------------------------------------------
            // operators
            // ------------------------------------------------------------------
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
                // `EvalBinary` uses `right.In(left)`
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
                // `EvalFString` converts with `ToStr()`, no unwrap here
                let v = self.pop()?;
                self.push(obj::to_str(&v));
            }
            Op::ToBool => {
                // pop the value, push the bool. `FLAG_STRICT` is the `Unwrap(..., node)`
                // `EvalBinary` performs, `and` / `or` only.
                let v = self.pop()?;
                let b = obj::to_bool(&v);
                if flags & FLAG_STRICT != 0 && obj::is_err(&b) {
                    return Err(self.error(err_message(&b), span));
                }
                self.push(b);
            }

            // ------------------------------------------------------------------
            // jumps
            // ------------------------------------------------------------------
            Op::Jmp => {
                self.ip = self.proto.jump_target(self.ip, operand);
                return Ok(false);
            }
            Op::JmpIf => {
                let value = self.pop()?;
                let cond = if flags & FLAG_JUMP_NONE != 0 {
                    obj::is_none(&value)
                } else {
                    // `As<Bool>` fails for every non-`Bool` value, which is falsy
                    match obj::payload(&value) {
                        Payload::Bool(b) => b,
                        _ => false,
                    }
                };
                let jump = if flags & FLAG_JUMP_TRUE != 0 { cond } else { !cond };

                if flags & FLAG_JUMP_POP != 0 {
                    // the `Bool` temporary is always dropped on both paths
                } else {
                    self.push(value);
                }

                if jump {
                    self.ip = self.proto.jump_target(self.ip, operand);
                    return Ok(false);
                }
            }

            // ------------------------------------------------------------------
            // `?` guard
            // ------------------------------------------------------------------
            Op::PushTryProp => {
                let entry = self.proto.exc[operand as usize];
                self.tries.push(TryEntry {
                    kind: entry.kind,
                    start: entry.start,
                    end: entry.end,
                    handler: entry.handler,
                    depth: self.stack.len(),
                    acc_depth: self.acc.len(),
                });
            }
            Op::Propagate => {
                // the handler is this instruction, so the region ends here. popping the
                // entry first is required: `handle` matches a region at `start <= ip <
                // end`, and the handler ip is inside that range, so leaving the entry
                // live makes `handle` re-enter the same handler and the run loops.
                if self.tries.last().is_some_and(|last| last.handler == self.ip) {
                    self.tries.pop();
                }

                // the region ran to its end. `pending` is the `Error` the region threw,
                // otherwise the region's value is checked for `Err`.
                let frames = context::frames(&self.ctx).borrow().len();

                if let Some(e) = self.pending.take() {
                    if frames > 0 {
                        let err = obj::err_with_header(e.header.clone(), e.message.clone());
                        return Err(Outcome::Propagate(err, span.0, span.1));
                    }
                    // `catch (Error e) { ... } throw;` rethrows the original error
                    return Err(Outcome::Error(e));
                }

                let top = self.peek()?.clone();
                if obj::is_err(&top) {
                    if frames > 0 {
                        return Err(Outcome::Propagate(top, span.0, span.1));
                    }
                    return Err(self.header_error(&top, span));
                }
            }

            // ------------------------------------------------------------------
            // members
            // ------------------------------------------------------------------
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
                // `BindPattern`'s Index branch: `Eval(target)`, `Eval(index)`,
                // `target.SetItem(index, value)`, so the stack is `value`, `target`,
                // `index` and the pops run top down: index, target, value
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
                // `BindPattern`'s slice branch consumes the value and leaves it for the
                // pattern result, so `SET_SLICE` pushes the original value back
                self.push(value);
            }

            // ------------------------------------------------------------------
            // argument accumulator
            // ------------------------------------------------------------------
            Op::ArgBegin => {
                self.acc.push(Vec::new());
                // `operand` is the static count, the runtime count comes from the accumulator
                let n = operand as usize;
                self.acc.last_mut().unwrap().reserve(n);
            }
            Op::ArgPos => {
                // the value stays on the stack; the reference checks `Err` only in
                // `EvalArguments`, the call-site form
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
                // `Unpack(Eval(arg))`, `values` and `names` appended together
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

                for (_, key, item) in dict.borrow().entries.iter() {
                    let name = match obj::payload(key) {
                        Payload::Str(s) => s.to_string(),
                        _ => obj::display(&obj::repr(key)),
                    };
                    self.push(item.clone());
                    self.acc.last_mut().unwrap().push(name);
                }
            }

            // ------------------------------------------------------------------
            // call
            // ------------------------------------------------------------------
            Op::Call => {
                let names = self.acc.pop().unwrap_or_default();
                let n = names.len();

                if self.stack.len() < n + 1 {
                    return Err(Outcome::Panic(obj::panic("panic", "vm stack is empty")));
                }

                let values: Vec<Obj> = self.stack[self.stack.len() - n..].to_vec();
                self.stack.truncate(self.stack.len() - n);
                let callable = self.stack.pop().unwrap();

                // `EvalCall` pushes the call node's source text as the frame name
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

                let result = obj::call_args(&callable, &Args::named(values, names));
                let popped = context::pop_frame(&self.ctx);

                // `Unwrap(callable.Call(args), node)` is inside the frame
                if obj::is_err(&result) {
                    return Err(self.error(err_message(&result), span));
                }
                if popped.is_err() {
                    return Err(Outcome::Panic(obj::panic("panic", "stack is empty")));
                }

                self.push(result);
            }

            // ------------------------------------------------------------------
            // unwrap / use / unsupported
            // ------------------------------------------------------------------
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

                // `EvalUse`'s dispatch: `Global.IsNative(last)` chooses `Global.Include`
                // (native module) over `Global.Import` (file module)
                let parts: Vec<String> = path.split('.').map(|s| s.to_string()).collect();
                let last = parts.last().cloned().unwrap_or_default();

                let result = if world::is_native(&last) {
                    global::include(&last, alias, imports)
                } else {
                    let source_path = self.ctx.borrow().source.path.clone();
                    global::import_file(&parts, &source_path, alias, imports)
                };

                if let Err(e) = result {
                    return Err(self.outcome(e));
                }
            }
            Op::Unsupported => {
                let message = self.program.message(operand as usize).to_string();
                return Err(self.error(message, span));
            }

            // every other opcode belongs to a later phase
            _ => {
                return Err(Outcome::Panic(obj::panic(
                    "panic",
                    format!("vm: opcode {op:?} is not implemented yet"),
                )))
            }
        }

        Ok(true)
    }

    // -----------------------------------------------------------------
    // helpers
    // -----------------------------------------------------------------

    fn bin(&mut self, op: BinOp) -> Result<(), Outcome> {
        let right = self.pop()?;
        let left = self.pop()?;
        self.push(obj::binop(&left, op, &right));
        Ok(())
    }

    /// pops the slice bounds in reverse push order (step, end, start).
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

    /// `BindPattern`'s slice branch (`Evaluator.cs:1252`), the slice assignment form.
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

        // `value.ToList().As<List>` — the value must be a list
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

    /// pops `n` values and returns them in push order.
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

    /// `Unwrap` and every default-header `new Error(msg, node, source)`.
    fn error(&self, message: impl Into<String>, span: (usize, usize)) -> Outcome {
        Outcome::Error(LangError::new(span.0, span.1, message))
    }

    /// `new Error(err.Message, node, source, header: err.Header)`, the header-preserving
    /// rethrow the reference uses for argument, tuple and `?` errors.
    fn header_error(&self, err: &Obj, span: (usize, usize)) -> Outcome {
        match obj::payload(err) {
            Payload::Error { header, message } => {
                Outcome::Error(LangError::new(span.0, span.1, message).with_header(header))
            }
            _ => self.error(obj::display(err), span),
        }
    }

    /// a host-level `Obj` result that ends the run: `Panic` payload aborts, `Err` becomes
    /// a `LangError`.
    fn outcome(&self, obj_value: Obj) -> Outcome {
        match obj::payload(&obj_value) {
            Payload::Panic { .. } => Outcome::Panic(obj_value),
            Payload::Error { header, message } => {
                Outcome::Error(LangError::new(0, 0, message).with_header(header))
            }
            _ => Outcome::Panic(obj_value),
        }
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

/// `PrintError` prints the exception object's `ToString()`, and `Panic.ToString()` is
/// `\npanic : {message}`.
pub fn panic_display(o: &Obj) -> String {
    match obj::payload(o) {
        Payload::Panic { name, message } => Panic::named(name, message).display(),
        _ => obj::display(o),
    }
}

/// `EvalBinary`'s `Is` order: `right.Is(left)` when either side is a type object.
fn is_obj(left: &Obj, right: &Obj) -> Obj {
    if matches!(obj::payload(left), Payload::TObj(..)) || matches!(obj::payload(right), Payload::TObj(..)) {
        obj::obj_is(right, left)
    } else {
        obj::obj_is(left, right)
    }
}

/// `LOAD_CONST` materializes with the world interning rules of D-020, so a constant
/// `int` / `str` shares the reference's cache and pool objects.
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
